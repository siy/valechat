use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use rust_decimal::Decimal;
use chrono::{DateTime, Utc, Datelike};
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};
use crate::storage::billing::{BillingSystem, SpendingCheckResult, SpendingLimitType, BillingPeriod};

/// Spending limits enforcement service
pub struct SpendingEnforcement {
    billing: Arc<BillingSystem>,
    state: Arc<RwLock<EnforcementState>>,
}

/// Internal state for enforcement tracking
#[derive(Debug, Clone)]
struct EnforcementState {
    /// Cache of recent spending checks to avoid database hits
    check_cache: std::collections::HashMap<String, CachedCheck>,
    /// Emergency stop flag
    emergency_stop: bool,
    /// Global enforcement enabled/disabled
    enforcement_enabled: bool,
    /// Last reset time for rate limiting
    last_reset: DateTime<Utc>,
    /// Current rate limit counters
    rate_counters: std::collections::HashMap<String, RateCounter>,
}

#[derive(Debug, Clone)]
struct CachedCheck {
    result: SpendingCheckResult,
    timestamp: DateTime<Utc>,
    ttl_seconds: u64,
}

#[derive(Debug, Clone)]
struct RateCounter {
    count: u32,
    window_start: DateTime<Utc>,
    limit: u32,
    window_duration_seconds: u64,
}

/// Result of enforcement check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub action_taken: EnforcementAction,
    pub current_spending: Option<Decimal>,
    pub limit_info: Option<LimitInfo>,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementAction {
    Allow,
    Block,
    Warning,
    EmergencyStop,
    RateLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitInfo {
    pub limit_type: String,
    pub current: Decimal,
    pub maximum: Decimal,
    pub percentage_used: f32,
}

/// Configuration for enforcement behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementConfig {
    pub enabled: bool,
    pub check_cache_ttl_seconds: u64,
    pub rate_limit_window_seconds: u64,
    pub max_requests_per_window: u32,
    pub emergency_stop_threshold: f32, // Percentage of global limit
    pub warning_threshold: f32, // Percentage to issue warnings
    pub grace_period_seconds: u64, // Allow requests during brief overages
}

impl Default for EnforcementConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_cache_ttl_seconds: 30,
            rate_limit_window_seconds: 60,
            max_requests_per_window: 100,
            emergency_stop_threshold: 0.95,
            warning_threshold: 0.8,
            grace_period_seconds: 300, // 5 minutes
        }
    }
}

impl SpendingEnforcement {
    /// Create a new spending enforcement service
    pub fn new(billing: Arc<BillingSystem>) -> Self {
        let state = Arc::new(RwLock::new(EnforcementState {
            check_cache: std::collections::HashMap::new(),
            emergency_stop: false,
            enforcement_enabled: true,
            last_reset: Utc::now(),
            rate_counters: std::collections::HashMap::new(),
        }));

        Self { billing, state }
    }

    /// Check if a request should be allowed based on spending limits
    pub async fn check_request(
        &self,
        provider: &str,
        model: &str,
        estimated_cost: Decimal,
        config: &EnforcementConfig,
    ) -> Result<EnforcementResult> {
        debug!(
            "Checking spending enforcement: provider={}, model={}, cost=${}",
            provider, model, estimated_cost
        );

        // Check if enforcement is disabled
        let state = self.state.read().await;
        if !state.enforcement_enabled || !config.enabled {
            return Ok(EnforcementResult {
                allowed: true,
                reason: Some("Enforcement disabled".to_string()),
                action_taken: EnforcementAction::Allow,
                current_spending: None,
                limit_info: None,
                retry_after_seconds: None,
            });
        }

        // Check emergency stop
        if state.emergency_stop {
            return Ok(EnforcementResult {
                allowed: false,
                reason: Some("Emergency stop activated".to_string()),
                action_taken: EnforcementAction::EmergencyStop,
                current_spending: None,
                limit_info: None,
                retry_after_seconds: Some(3600), // 1 hour
            });
        }
        drop(state);

        // Check rate limits
        if let Some(rate_result) = self.check_rate_limits(provider, model, config).await? {
            return Ok(rate_result);
        }

        // Check cached result first
        let cache_key = format!("{}:{}:{}", provider, model, estimated_cost);
        let cached_result = {
            let state = self.state.read().await;
            state.check_cache.get(&cache_key).and_then(|cached| {
                if cached.timestamp.timestamp() + cached.ttl_seconds as i64 > Utc::now().timestamp() {
                    Some(cached.result.clone())
                } else {
                    None
                }
            })
        };

        if let Some(cached) = cached_result {
            debug!("Using cached spending check result");
            return Ok(EnforcementResult {
                allowed: cached.allowed,
                reason: cached.reason.clone(),
                action_taken: if cached.allowed { EnforcementAction::Allow } else { EnforcementAction::Block },
                current_spending: Some(cached.current_spending),
                limit_info: cached.limit.map(|limit| LimitInfo {
                    limit_type: "cached".to_string(),
                    current: cached.current_spending,
                    maximum: limit,
                    percentage_used: cached.percentage_used.unwrap_or(0.0),
                }),
                retry_after_seconds: None,
            });
        }

        // Perform actual spending check
        let spending_check = self.billing.check_spending_limits(provider, model, estimated_cost).await?;

        // Cache the result
        {
            let mut state = self.state.write().await;
            state.check_cache.insert(cache_key, CachedCheck {
                result: spending_check.clone(),
                timestamp: Utc::now(),
                ttl_seconds: config.check_cache_ttl_seconds,
            });

            // Clean old cache entries
            let now = Utc::now().timestamp();
            state.check_cache.retain(|_, cached| {
                cached.timestamp.timestamp() + cached.ttl_seconds as i64 > now
            });
        }

        // Determine action based on spending check
        let action = if !spending_check.allowed {
            EnforcementAction::Block
        } else if let Some(percentage) = spending_check.percentage_used {
            if percentage >= config.emergency_stop_threshold * 100.0 {
                // Activate emergency stop
                let mut state = self.state.write().await;
                state.emergency_stop = true;
                warn!("Emergency stop activated due to high spending: {}%", percentage);
                EnforcementAction::EmergencyStop
            } else if percentage >= config.warning_threshold * 100.0 {
                EnforcementAction::Warning
            } else {
                EnforcementAction::Allow
            }
        } else {
            EnforcementAction::Allow
        };

        let limit_info = spending_check.limit.map(|limit| LimitInfo {
            limit_type: "global".to_string(), // TODO: Determine actual limit type
            current: spending_check.current_spending,
            maximum: limit,
            percentage_used: spending_check.percentage_used.unwrap_or(0.0),
        });

        Ok(EnforcementResult {
            allowed: spending_check.allowed && !matches!(action, EnforcementAction::EmergencyStop),
            reason: spending_check.reason,
            action_taken: action,
            current_spending: Some(spending_check.current_spending),
            limit_info,
            retry_after_seconds: None,
        })
    }

    /// Check rate limits for the provider/model combination
    async fn check_rate_limits(
        &self,
        provider: &str,
        model: &str,
        config: &EnforcementConfig,
    ) -> Result<Option<EnforcementResult>> {
        let rate_key = format!("{}:{}", provider, model);
        let now = Utc::now();

        let mut state = self.state.write().await;

        // Clean up expired rate counters
        state.rate_counters.retain(|_, counter| {
            now.timestamp() - counter.window_start.timestamp() < counter.window_duration_seconds as i64
        });

        // Get or create rate counter
        let counter = state.rate_counters.entry(rate_key.clone()).or_insert(RateCounter {
            count: 0,
            window_start: now,
            limit: config.max_requests_per_window,
            window_duration_seconds: config.rate_limit_window_seconds,
        });

        // Check if we need to reset the window
        if now.timestamp() - counter.window_start.timestamp() >= counter.window_duration_seconds as i64 {
            counter.count = 0;
            counter.window_start = now;
        }

        // Check rate limit
        if counter.count >= counter.limit {
            let retry_after = counter.window_duration_seconds - 
                (now.timestamp() - counter.window_start.timestamp()) as u64;

            warn!("Rate limit exceeded for {}: {}/{}", rate_key, counter.count, counter.limit);
            
            return Ok(Some(EnforcementResult {
                allowed: false,
                reason: Some(format!("Rate limit exceeded: {}/{} requests", counter.count, counter.limit)),
                action_taken: EnforcementAction::RateLimit,
                current_spending: None,
                limit_info: None,
                retry_after_seconds: Some(retry_after),
            }));
        }

        // Increment counter
        counter.count += 1;

        Ok(None)
    }

    /// Manually set emergency stop
    pub async fn set_emergency_stop(&self, enabled: bool) -> Result<()> {
        let mut state = self.state.write().await;
        state.emergency_stop = enabled;
        
        if enabled {
            error!("Emergency stop manually activated");
        } else {
            info!("Emergency stop manually deactivated");
        }
        
        Ok(())
    }

    /// Enable or disable enforcement
    pub async fn set_enforcement_enabled(&self, enabled: bool) -> Result<()> {
        let mut state = self.state.write().await;
        state.enforcement_enabled = enabled;
        
        if enabled {
            info!("Spending enforcement enabled");
        } else {
            warn!("Spending enforcement disabled");
        }
        
        Ok(())
    }

    /// Clear all cached checks (force fresh checks)
    pub async fn clear_cache(&self) -> Result<()> {
        let mut state = self.state.write().await;
        state.check_cache.clear();
        info!("Spending check cache cleared");
        Ok(())
    }

    /// Reset rate limits
    pub async fn reset_rate_limits(&self) -> Result<()> {
        let mut state = self.state.write().await;
        state.rate_counters.clear();
        info!("Rate limits reset");
        Ok(())
    }

    /// Get current enforcement status
    pub async fn get_status(&self) -> Result<EnforcementStatus> {
        let state = self.state.read().await;
        
        Ok(EnforcementStatus {
            enforcement_enabled: state.enforcement_enabled,
            emergency_stop: state.emergency_stop,
            cached_checks: state.check_cache.len(),
            active_rate_limits: state.rate_counters.len(),
            last_reset: state.last_reset,
        })
    }

    /// Record successful request for tracking
    pub async fn record_successful_request(
        &self,
        provider: &str,
        model: &str,
        actual_cost: Decimal,
    ) -> Result<()> {
        debug!(
            "Recording successful request: provider={}, model={}, cost=${}",
            provider, model, actual_cost
        );

        // Clear any cached checks for this provider/model since costs may have changed
        let cache_prefix = format!("{}:{}", provider, model);
        let mut state = self.state.write().await;
        state.check_cache.retain(|key, _| !key.starts_with(&cache_prefix));

        Ok(())
    }

    /// Estimate cost for a request (placeholder implementation)
    pub fn estimate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u32,
        estimated_output_tokens: u32,
    ) -> Decimal {
        // Simple cost estimation - in reality this would use pricing tables
        let input_cost_per_1k = match (provider, model) {
            ("openai", model) if model.contains("gpt-4") => Decimal::new(30, 3), // $0.030
            ("openai", model) if model.contains("gpt-3.5") => Decimal::new(5, 4), // $0.0005
            ("anthropic", model) if model.contains("claude-3") => Decimal::new(25, 3), // $0.025
            ("anthropic", model) if model.contains("claude-2") => Decimal::new(11, 3), // $0.011  
            _ => Decimal::new(10, 3), // Default $0.010
        };

        let output_cost_per_1k = input_cost_per_1k * Decimal::new(2, 0); // Output typically 2x input cost

        let input_cost = input_cost_per_1k * Decimal::from(input_tokens) / Decimal::from(1000);
        let output_cost = output_cost_per_1k * Decimal::from(estimated_output_tokens) / Decimal::from(1000);

        input_cost + output_cost
    }
}

/// Status information about enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementStatus {
    pub enforcement_enabled: bool,
    pub emergency_stop: bool,
    pub cached_checks: usize,
    pub active_rate_limits: usize,
    pub last_reset: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Database, BillingSystem};
    use crate::platform::AppPaths;
    use tempfile::TempDir;

    async fn create_test_enforcement() -> (SpendingEnforcement, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::with_data_dir(temp_dir.path()).unwrap();
        let db = Database::new(&paths).await.unwrap();
        let billing = Arc::new(BillingSystem::new(db.pool().clone()));
        let enforcement = SpendingEnforcement::new(billing);
        (enforcement, temp_dir)
    }

    #[tokio::test]
    async fn test_enforcement_basic_allow() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;
        let config = EnforcementConfig::default();

        let result = enforcement.check_request(
            "openai",
            "gpt-4",
            Decimal::new(100, 3), // $0.100
            &config,
        ).await.unwrap();

        assert!(result.allowed);
        assert!(matches!(result.action_taken, EnforcementAction::Allow));
    }

    #[tokio::test]
    async fn test_enforcement_disabled() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;
        let mut config = EnforcementConfig::default();
        config.enabled = false;

        let result = enforcement.check_request(
            "openai",
            "gpt-4",
            Decimal::new(100000, 2), // $1000.00
            &config,
        ).await.unwrap();

        assert!(result.allowed);
        assert!(matches!(result.action_taken, EnforcementAction::Allow));
        assert!(result.reason.unwrap().contains("disabled"));
    }

    #[tokio::test]
    async fn test_emergency_stop() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;
        let config = EnforcementConfig::default();

        // Activate emergency stop
        enforcement.set_emergency_stop(true).await.unwrap();

        let result = enforcement.check_request(
            "openai",
            "gpt-4",
            Decimal::new(1, 3), // $0.001
            &config,
        ).await.unwrap();

        assert!(!result.allowed);
        assert!(matches!(result.action_taken, EnforcementAction::EmergencyStop));
        assert!(result.retry_after_seconds.is_some());
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;
        let mut config = EnforcementConfig::default();
        config.max_requests_per_window = 2; // Very low limit for testing

        // First two requests should succeed
        for i in 0..2 {
            let result = enforcement.check_request(
                "openai",
                "gpt-4",
                Decimal::new(1, 3),
                &config,
            ).await.unwrap();
            
            assert!(result.allowed, "Request {} should be allowed", i);
        }

        // Third request should be rate limited
        let result = enforcement.check_request(
            "openai",
            "gpt-4",
            Decimal::new(1, 3),
            &config,
        ).await.unwrap();

        assert!(!result.allowed);
        assert!(matches!(result.action_taken, EnforcementAction::RateLimit));
        assert!(result.retry_after_seconds.is_some());
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;
        let config = EnforcementConfig::default();

        // First request
        let result1 = enforcement.check_request(
            "openai",
            "gpt-4",
            Decimal::new(100, 3),
            &config,
        ).await.unwrap();

        // Second identical request should use cache
        let result2 = enforcement.check_request(
            "openai",
            "gpt-4", 
            Decimal::new(100, 3),
            &config,
        ).await.unwrap();

        assert_eq!(result1.allowed, result2.allowed);
        assert_eq!(result1.current_spending, result2.current_spending);
    }

    #[tokio::test]
    async fn test_cost_estimation() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;

        let cost = enforcement.estimate_cost("openai", "gpt-4", 1000, 500);
        assert!(cost > Decimal::ZERO);

        let cost_3_5 = enforcement.estimate_cost("openai", "gpt-3.5-turbo", 1000, 500);
        assert!(cost_3_5 < cost); // GPT-3.5 should be cheaper than GPT-4
    }

    #[tokio::test]
    async fn test_status_reporting() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;

        let status = enforcement.get_status().await.unwrap();
        assert!(status.enforcement_enabled);
        assert!(!status.emergency_stop);
        assert_eq!(status.cached_checks, 0);
        assert_eq!(status.active_rate_limits, 0);
    }

    #[tokio::test]
    async fn test_successful_request_recording() {
        let (enforcement, _temp_dir) = create_test_enforcement().await;

        let result = enforcement.record_successful_request(
            "openai",
            "gpt-4",
            Decimal::new(125, 3), // $0.125
        ).await;

        assert!(result.is_ok());
    }
}