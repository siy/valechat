use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use serde::{Serialize, Deserialize};
use tracing::{info, error, debug};
use tokio::sync::mpsc;

use crate::error::Result;

/// Alert types for cost tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CostAlertType {
    /// Daily spending limit approaching or exceeded
    DailyLimit { current: Decimal, limit: Decimal },
    /// Monthly spending limit approaching or exceeded
    MonthlyLimit { current: Decimal, limit: Decimal },
    /// Per-provider spending limit
    ProviderLimit { provider: String, current: Decimal, limit: Decimal },
    /// Unusual spending spike detected
    SpendingSpike { current_rate: Decimal, baseline_rate: Decimal },
    /// Cost per request is unusually high
    HighCostRequest { cost: Decimal, average_cost: Decimal },
    /// Running low on budget
    BudgetWarning { remaining: Decimal, days_left: u32 },
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Cost alert notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAlert {
    pub id: String,
    pub alert_type: CostAlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
    /// Action to take (if any)
    pub suggested_action: Option<String>,
    /// Related provider/model context
    pub context: AlertContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertContext {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub conversation_id: Option<String>,
    pub billing_period: String,
}

/// Configuration for cost alerting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAlertConfig {
    /// Enable/disable alerting system
    pub enabled: bool,
    /// Daily spending limit for warnings
    pub daily_warning_limit: Option<Decimal>,
    /// Daily spending limit for critical alerts
    pub daily_critical_limit: Option<Decimal>,
    /// Monthly spending limit for warnings
    pub monthly_warning_limit: Option<Decimal>,
    /// Monthly spending limit for critical alerts
    pub monthly_critical_limit: Option<Decimal>,
    /// Per-provider spending limits
    pub provider_limits: HashMap<String, Decimal>,
    /// Spending spike detection threshold (multiplier of baseline)
    pub spike_threshold: f32,
    /// High cost request threshold (multiplier of average)
    pub high_cost_threshold: f32,
    /// Days to look back for baseline calculations
    pub baseline_days: u32,
    /// Minimum budget warning threshold
    pub budget_warning_days: u32,
}

impl Default for CostAlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            daily_warning_limit: Some(Decimal::new(50, 0)), // $50/day
            daily_critical_limit: Some(Decimal::new(100, 0)), // $100/day
            monthly_warning_limit: Some(Decimal::new(1000, 0)), // $1000/month
            monthly_critical_limit: Some(Decimal::new(2000, 0)), // $2000/month
            provider_limits: HashMap::new(),
            spike_threshold: 3.0, // 3x baseline
            high_cost_threshold: 5.0, // 5x average
            baseline_days: 7,
            budget_warning_days: 3,
        }
    }
}

/// Cost alert monitoring service
pub struct CostAlertSystem {
    config: CostAlertConfig,
    alert_sender: mpsc::UnboundedSender<CostAlert>,
    /// Recent alerts to avoid spam
    recent_alerts: HashMap<String, DateTime<Utc>>,
    /// Cached baseline metrics
    baseline_cache: HashMap<String, BaselineMetrics>,
    /// Alert history for analysis
    alert_history: Vec<CostAlert>,
}

#[derive(Debug, Clone)]
struct BaselineMetrics {
    average_daily_cost: Decimal,
    average_request_cost: Decimal,
    last_updated: DateTime<Utc>,
    sample_period: Duration,
}

impl CostAlertSystem {
    /// Create new cost alert system
    pub fn new(config: CostAlertConfig) -> (Self, mpsc::UnboundedReceiver<CostAlert>) {
        let (alert_sender, alert_receiver) = mpsc::unbounded_channel();
        
        let system = Self {
            config,
            alert_sender,
            recent_alerts: HashMap::new(),
            baseline_cache: HashMap::new(),
            alert_history: Vec::new(),
        };
        
        (system, alert_receiver)
    }

    /// Check spending against configured limits and generate alerts
    pub async fn check_spending_alerts(
        &mut self,
        current_daily: Decimal,
        current_monthly: Decimal,
        provider_spending: &HashMap<String, Decimal>,
        recent_cost: Decimal,
        context: AlertContext,
    ) -> Result<Vec<CostAlert>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let mut alerts = Vec::new();
        let now = Utc::now();

        // Check daily limits
        if let Some(daily_warning) = self.config.daily_warning_limit {
            if current_daily >= daily_warning {
                let severity = if let Some(daily_critical) = self.config.daily_critical_limit {
                    if current_daily >= daily_critical {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    }
                } else {
                    AlertSeverity::Warning
                };

                let alert_key = format!("daily_limit_{}", context.billing_period);
                if self.should_send_alert(&alert_key, severity.clone()) {
                    let alert = CostAlert {
                        id: uuid::Uuid::new_v4().to_string(),
                        alert_type: CostAlertType::DailyLimit {
                            current: current_daily,
                            limit: daily_warning,
                        },
                        severity,
                        message: format!(
                            "Daily spending ${:.2} {} ${:.2} limit",
                            current_daily,
                            if current_daily >= self.config.daily_critical_limit.unwrap_or(Decimal::MAX) {
                                "exceeded"
                            } else {
                                "approaching"
                            },
                            daily_warning
                        ),
                        timestamp: now,
                        acknowledged: false,
                        suggested_action: Some("Consider reviewing recent API usage or adjusting daily limits".to_string()),
                        context: context.clone(),
                    };
                    
                    alerts.push(alert);
                    self.recent_alerts.insert(alert_key, now);
                }
            }
        }

        // Check monthly limits
        if let Some(monthly_warning) = self.config.monthly_warning_limit {
            if current_monthly >= monthly_warning {
                let severity = if let Some(monthly_critical) = self.config.monthly_critical_limit {
                    if current_monthly >= monthly_critical {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    }
                } else {
                    AlertSeverity::Warning
                };

                let alert_key = format!("monthly_limit_{}", context.billing_period);
                if self.should_send_alert(&alert_key, severity.clone()) {
                    let alert = CostAlert {
                        id: uuid::Uuid::new_v4().to_string(),
                        alert_type: CostAlertType::MonthlyLimit {
                            current: current_monthly,
                            limit: monthly_warning,
                        },
                        severity,
                        message: format!(
                            "Monthly spending ${:.2} {} ${:.2} limit",
                            current_monthly,
                            if current_monthly >= self.config.monthly_critical_limit.unwrap_or(Decimal::MAX) {
                                "exceeded"
                            } else {
                                "approaching"
                            },
                            monthly_warning
                        ),
                        timestamp: now,
                        acknowledged: false,
                        suggested_action: Some("Review monthly usage patterns and consider budget adjustments".to_string()),
                        context: context.clone(),
                    };
                    
                    alerts.push(alert);
                    self.recent_alerts.insert(alert_key, now);
                }
            }
        }

        // Check provider-specific limits
        for (provider, current_spending) in provider_spending {
            if let Some(provider_limit) = self.config.provider_limits.get(provider) {
                if current_spending >= provider_limit {
                    let alert_key = format!("provider_limit_{}_{}", provider, context.billing_period);
                    if self.should_send_alert(&alert_key, AlertSeverity::Warning) {
                        let alert = CostAlert {
                            id: uuid::Uuid::new_v4().to_string(),
                            alert_type: CostAlertType::ProviderLimit {
                                provider: provider.clone(),
                                current: *current_spending,
                                limit: *provider_limit,
                            },
                            severity: AlertSeverity::Warning,
                            message: format!(
                                "Provider {} spending ${:.2} exceeded ${:.2} limit",
                                provider, current_spending, provider_limit
                            ),
                            timestamp: now,
                            acknowledged: false,
                            suggested_action: Some(format!("Consider switching to alternative providers or adjusting {} limits", provider)),
                            context: context.clone(),
                        };
                        
                        alerts.push(alert);
                        self.recent_alerts.insert(alert_key, now);
                    }
                }
            }
        }

        // Check for spending spikes
        if let Some(baseline) = self.get_baseline_metrics(&context).await {
            if recent_cost > baseline.average_request_cost * Decimal::from_f32_retain(self.config.high_cost_threshold).unwrap_or(Decimal::from(5)) {
                let alert_key = format!("high_cost_{}", context.provider.as_deref().unwrap_or("unknown"));
                if self.should_send_alert(&alert_key, AlertSeverity::Info) {
                    let alert = CostAlert {
                        id: uuid::Uuid::new_v4().to_string(),
                        alert_type: CostAlertType::HighCostRequest {
                            cost: recent_cost,
                            average_cost: baseline.average_request_cost,
                        },
                        severity: AlertSeverity::Info,
                        message: format!(
                            "High cost request: ${:.4} ({}x average of ${:.4})",
                            recent_cost,
                            recent_cost / baseline.average_request_cost,
                            baseline.average_request_cost
                        ),
                        timestamp: now,
                        acknowledged: false,
                        suggested_action: Some("Review request parameters and model selection".to_string()),
                        context: context.clone(),
                    };
                    
                    alerts.push(alert);
                    self.recent_alerts.insert(alert_key, now);
                }
            }
        }

        // Send alerts
        for alert in &alerts {
            if let Err(e) = self.alert_sender.send(alert.clone()) {
                error!("Failed to send cost alert: {}", e);
            } else {
                debug!("Sent cost alert: {:?}", alert.alert_type);
            }
        }

        // Store in history
        self.alert_history.extend(alerts.clone());

        // Clean up old alerts from history (keep last 1000)
        if self.alert_history.len() > 1000 {
            self.alert_history.drain(0..self.alert_history.len() - 1000);
        }

        Ok(alerts)
    }

    /// Determine if an alert should be sent (avoid spam)
    fn should_send_alert(&self, alert_key: &str, severity: AlertSeverity) -> bool {
        if let Some(last_sent) = self.recent_alerts.get(alert_key) {
            let cooldown = match severity {
                AlertSeverity::Emergency => Duration::minutes(5),
                AlertSeverity::Critical => Duration::minutes(15),
                AlertSeverity::Warning => Duration::hours(1),
                AlertSeverity::Info => Duration::hours(4),
            };
            
            Utc::now() - *last_sent > cooldown
        } else {
            true
        }
    }

    /// Get baseline metrics for anomaly detection
    async fn get_baseline_metrics(&self, context: &AlertContext) -> Option<BaselineMetrics> {
        let cache_key = format!(
            "{}:{}",
            context.provider.as_deref().unwrap_or("all"),
            context.model.as_deref().unwrap_or("all")
        );

        if let Some(cached) = self.baseline_cache.get(&cache_key) {
            if Utc::now() - cached.last_updated < Duration::hours(1) {
                return Some(cached.clone());
            }
        }

        // In a real implementation, this would query the usage database
        // For now, return a placeholder
        None
    }

    /// Get recent alert history
    pub fn get_recent_alerts(&self, limit: Option<usize>) -> Vec<&CostAlert> {
        let limit = limit.unwrap_or(50);
        self.alert_history
            .iter()
            .rev()
            .take(limit)
            .collect()
    }

    /// Acknowledge an alert
    pub fn acknowledge_alert(&mut self, alert_id: &str) -> Result<()> {
        if let Some(alert) = self.alert_history.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
            info!("Acknowledged cost alert: {}", alert_id);
        }
        Ok(())
    }

    /// Update alert configuration
    pub fn update_config(&mut self, new_config: CostAlertConfig) {
        self.config = new_config;
        info!("Cost alert configuration updated");
    }

    /// Get current configuration
    pub fn get_config(&self) -> &CostAlertConfig {
        &self.config
    }

    /// Clean up old alert history and cache
    pub fn cleanup(&mut self) {
        let cutoff = Utc::now() - Duration::hours(24);
        
        // Clean recent alerts cache
        self.recent_alerts.retain(|_, timestamp| *timestamp > cutoff);
        
        // Clean baseline cache
        self.baseline_cache.retain(|_, metrics| metrics.last_updated > cutoff);
        
        debug!("Cleaned up cost alert system caches");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daily_limit_alert() {
        let config = CostAlertConfig {
            enabled: true,
            daily_warning_limit: Some(Decimal::new(50, 0)),
            daily_critical_limit: Some(Decimal::new(100, 0)),
            ..Default::default()
        };
        
        let (mut system, _receiver) = CostAlertSystem::new(config);
        
        let context = AlertContext {
            provider: Some("openai".to_string()),
            model: Some("gpt-4".to_string()),
            conversation_id: None,
            billing_period: "2024-01".to_string(),
        };
        
        let alerts = system.check_spending_alerts(
            Decimal::new(75, 0), // $75 daily
            Decimal::new(300, 0), // $300 monthly
            &HashMap::new(),
            Decimal::new(5, 0), // $5 recent cost
            context,
        ).await.unwrap();
        
        assert_eq!(alerts.len(), 1);
        assert!(matches!(alerts[0].alert_type, CostAlertType::DailyLimit { .. }));
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
    }

    #[tokio::test]
    async fn test_provider_limit_alert() {
        let mut config = CostAlertConfig::default();
        config.provider_limits.insert("openai".to_string(), Decimal::new(200, 0));
        
        let (mut system, _receiver) = CostAlertSystem::new(config);
        
        let context = AlertContext {
            provider: Some("openai".to_string()),
            model: Some("gpt-4".to_string()),
            conversation_id: None,
            billing_period: "2024-01".to_string(),
        };
        
        let mut provider_spending = HashMap::new();
        provider_spending.insert("openai".to_string(), Decimal::new(250, 0));
        
        let alerts = system.check_spending_alerts(
            Decimal::new(25, 0),
            Decimal::new(300, 0),
            &provider_spending,
            Decimal::new(5, 0),
            context,
        ).await.unwrap();
        
        assert_eq!(alerts.len(), 1);
        assert!(matches!(alerts[0].alert_type, CostAlertType::ProviderLimit { .. }));
    }

    #[test]
    fn test_alert_cooldown() {
        let config = CostAlertConfig::default();
        let (system, _receiver) = CostAlertSystem::new(config);
        
        // First alert should be sent
        assert!(system.should_send_alert("test_key", AlertSeverity::Warning));
        
        // After marking as sent, should respect cooldown
        // (This would need system to be mutable in real test)
    }

    #[test]
    fn test_alert_acknowledgment() {
        let config = CostAlertConfig::default();
        let (mut system, _receiver) = CostAlertSystem::new(config);
        
        // Add a test alert to history
        let alert = CostAlert {
            id: "test-alert-123".to_string(),
            alert_type: CostAlertType::DailyLimit {
                current: Decimal::new(75, 0),
                limit: Decimal::new(50, 0),
            },
            severity: AlertSeverity::Warning,
            message: "Test alert".to_string(),
            timestamp: Utc::now(),
            acknowledged: false,
            suggested_action: None,
            context: AlertContext {
                provider: None,
                model: None,
                conversation_id: None,
                billing_period: "2024-01".to_string(),
            },
        };
        
        system.alert_history.push(alert);
        
        // Acknowledge the alert
        system.acknowledge_alert("test-alert-123").unwrap();
        
        // Verify it's acknowledged
        assert!(system.alert_history[0].acknowledged);
    }
}