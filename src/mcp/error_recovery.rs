use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};

/// Type alias for fallback handler references
type FallbackHandlerRef = Arc<dyn FallbackHandler + Send + Sync>;


/// Error recovery manager for MCP operations
pub struct MCPErrorRecovery {
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    retry_policies: Arc<RwLock<HashMap<String, RetryPolicy>>>,
    fallback_handlers: Arc<RwLock<HashMap<String, FallbackHandlerRef>>>,
    config: ErrorRecoveryConfig,
    stats: Arc<RwLock<ErrorRecoveryStats>>,
}

/// Configuration for error recovery
#[derive(Debug, Clone)]
pub struct ErrorRecoveryConfig {
    pub default_retry_attempts: u32,
    pub default_retry_delay: Duration,
    pub circuit_breaker_failure_threshold: u32,
    pub circuit_breaker_timeout: Duration,
    pub circuit_breaker_success_threshold: u32,
    pub enable_graceful_degradation: bool,
    pub max_concurrent_retries: usize,
    pub health_check_interval: Duration,
}

impl Default for ErrorRecoveryConfig {
    fn default() -> Self {
        Self {
            default_retry_attempts: 3,
            default_retry_delay: Duration::from_secs(1),
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_timeout: Duration::from_secs(60),
            circuit_breaker_success_threshold: 3,
            enable_graceful_degradation: true,
            max_concurrent_retries: 10,
            health_check_interval: Duration::from_secs(30),
        }
    }
}

/// Circuit breaker implementation
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: Option<Instant>,
    pub config: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    Closed,    // Normal operation
    Open,      // Failing fast
    HalfOpen,  // Testing if service is back
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub timeout: Duration,
    pub success_threshold: u32,
}

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub jitter: bool,
    pub retryable_errors: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
            retryable_errors: vec![
                "connection_failed".to_string(),
                "timeout".to_string(),
                "server_error".to_string(),
                "rate_limited".to_string(),
            ],
        }
    }
}

/// Fallback handler trait
pub trait FallbackHandler {
    fn handle_fallback(&self, operation: &str, error: &Error) -> Result<FallbackResult>;
    fn is_applicable(&self, operation: &str, error: &Error) -> bool;
}

/// Fallback result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FallbackResult {
    Success(serde_json::Value),
    Degraded(serde_json::Value),
    Failed(String),
}

/// Error recovery statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecoveryStats {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub retry_operations: u64,
    pub circuit_breaker_trips: u64,
    pub fallback_activations: u64,
    pub operations_by_server: HashMap<String, ServerOperationStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerOperationStats {
    pub total_operations: u64,
    pub successful_operations: u64,
    pub failed_operations: u64,
    pub avg_response_time_ms: f64,
    pub circuit_breaker_state: CircuitBreakerState,
}

/// Retry context for tracking retry attempts
#[derive(Debug, Clone)]
pub struct RetryContext {
    pub operation: String,
    pub server_name: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub last_error: Option<String>,
    pub start_time: Instant,
}

/// Recovery operation result
#[derive(Debug)]
pub enum RecoveryResult<T> {
    Success(T),
    Degraded(T),
    Failed(Error),
}

impl MCPErrorRecovery {
    /// Create a new error recovery manager
    pub fn new(config: ErrorRecoveryConfig) -> Self {
        Self {
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            retry_policies: Arc::new(RwLock::new(HashMap::new())),
            fallback_handlers: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(ErrorRecoveryStats {
                total_operations: 0,
                successful_operations: 0,
                failed_operations: 0,
                retry_operations: 0,
                circuit_breaker_trips: 0,
                fallback_activations: 0,
                operations_by_server: HashMap::new(),
            })),
        }
    }

    /// Execute an operation with error recovery (simplified version)
    pub async fn execute_with_recovery_simple(
        &self,
        operation_name: &str,
        server_name: &str,
    ) -> RecoveryResult<String> {
        let start_time = Instant::now();
        
        // Update total operations count
        {
            let mut stats = self.stats.write().await;
            stats.total_operations += 1;
            stats.operations_by_server
                .entry(server_name.to_string())
                .or_insert_with(|| ServerOperationStats {
                    total_operations: 0,
                    successful_operations: 0,
                    failed_operations: 0,
                    avg_response_time_ms: 0.0,
                    circuit_breaker_state: CircuitBreakerState::Closed,
                })
                .total_operations += 1;
        }

        // Check circuit breaker
        if !self.is_circuit_breaker_closed(server_name).await {
            warn!("Circuit breaker open for server: {}", server_name);
            return self.handle_circuit_breaker_open(operation_name, server_name).await;
        }

        // For demonstration, just record success
        self.record_success(server_name, start_time.elapsed()).await;
        RecoveryResult::Success("Operation completed successfully".to_string())
    }

    /// Register a retry policy for a server
    pub async fn register_retry_policy(&self, server_name: String, policy: RetryPolicy) {
        let mut policies = self.retry_policies.write().await;
        policies.insert(server_name, policy);
    }

    /// Register a fallback handler
    pub async fn register_fallback_handler(
        &self,
        operation_name: String,
        handler: FallbackHandlerRef,
    ) {
        let mut handlers = self.fallback_handlers.write().await;
        handlers.insert(operation_name, handler);
    }

    /// Check if circuit breaker is closed (allows operations)
    async fn is_circuit_breaker_closed(&self, server_name: &str) -> bool {
        let circuit_breakers = self.circuit_breakers.read().await;
        
        if let Some(cb) = circuit_breakers.get(server_name) {
            match cb.state {
                CircuitBreakerState::Closed => true,
                CircuitBreakerState::Open => {
                    // Check if timeout has passed
                    if let Some(last_failure) = cb.last_failure_time {
                        if last_failure.elapsed() >= cb.config.timeout {
                            // Move to half-open state
                            drop(circuit_breakers);
                            self.set_circuit_breaker_state(server_name, CircuitBreakerState::HalfOpen).await;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                CircuitBreakerState::HalfOpen => true, // Allow limited operations
            }
        } else {
            // No circuit breaker exists, create one
            drop(circuit_breakers);
            self.initialize_circuit_breaker(server_name).await;
            true
        }
    }

    /// Initialize circuit breaker for a server
    async fn initialize_circuit_breaker(&self, server_name: &str) {
        let mut circuit_breakers = self.circuit_breakers.write().await;
        circuit_breakers.insert(
            server_name.to_string(),
            CircuitBreaker {
                state: CircuitBreakerState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                config: CircuitBreakerConfig {
                    failure_threshold: self.config.circuit_breaker_failure_threshold,
                    timeout: self.config.circuit_breaker_timeout,
                    success_threshold: self.config.circuit_breaker_success_threshold,
                },
            },
        );
    }

    /// Set circuit breaker state
    async fn set_circuit_breaker_state(&self, server_name: &str, state: CircuitBreakerState) {
        let state_clone = state.clone();
        let mut circuit_breakers_dropped = false;
        
        {
            let mut circuit_breakers = self.circuit_breakers.write().await;
            if let Some(cb) = circuit_breakers.get_mut(server_name) {
                cb.state = state.clone();
                
                // Reset counters when moving to different states
                match state {
                    CircuitBreakerState::Closed => {
                        cb.failure_count = 0;
                        cb.success_count = 0;
                    }
                    CircuitBreakerState::Open => {
                        cb.last_failure_time = Some(Instant::now());
                        
                        // Update global stats - need to drop circuit_breakers first
                        drop(circuit_breakers);
                        circuit_breakers_dropped = true;
                        
                        let mut stats = self.stats.write().await;
                        stats.circuit_breaker_trips += 1;
                    }
                    CircuitBreakerState::HalfOpen => {
                        cb.success_count = 0;
                    }
                }
            }
        }
        
        // Update server stats if we haven't already dropped circuit_breakers
        if !circuit_breakers_dropped {
            let mut stats = self.stats.write().await;
            if let Some(server_stats) = stats.operations_by_server.get_mut(server_name) {
                server_stats.circuit_breaker_state = state_clone;
            }
        }
    }

    /// Record successful operation
    async fn record_success(&self, server_name: &str, duration: Duration) {
        // Update circuit breaker
        {
            let mut circuit_breakers = self.circuit_breakers.write().await;
            if let Some(cb) = circuit_breakers.get_mut(server_name) {
                match cb.state {
                    CircuitBreakerState::Closed => {
                        cb.failure_count = 0; // Reset failure count on success
                    }
                    CircuitBreakerState::HalfOpen => {
                        cb.success_count += 1;
                        if cb.success_count >= cb.config.success_threshold {
                            cb.state = CircuitBreakerState::Closed;
                            cb.failure_count = 0;
                            cb.success_count = 0;
                            info!("Circuit breaker closed for server: {}", server_name);
                        }
                    }
                    CircuitBreakerState::Open => {
                        // This shouldn't happen, but handle gracefully
                        warn!("Success recorded while circuit breaker is open for server: {}", server_name);
                    }
                }
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.successful_operations += 1;
            
            if let Some(server_stats) = stats.operations_by_server.get_mut(server_name) {
                server_stats.successful_operations += 1;
                
                // Update average response time (simple moving average)
                let duration_ms = duration.as_millis() as f64;
                server_stats.avg_response_time_ms = 
                    (server_stats.avg_response_time_ms + duration_ms) / 2.0;
            }
        }
    }

    /// Record failed operation
    async fn record_failure(&self, server_name: &str, error: &Error) {
        debug!("Recording failure for server '{}': {}", server_name, error);

        // Update circuit breaker
        {
            let mut circuit_breakers = self.circuit_breakers.write().await;
            if let Some(cb) = circuit_breakers.get_mut(server_name) {
                match cb.state {
                    CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen => {
                        cb.failure_count += 1;
                        cb.success_count = 0; // Reset success count
                        
                        if cb.failure_count >= cb.config.failure_threshold {
                            cb.state = CircuitBreakerState::Open;
                            cb.last_failure_time = Some(Instant::now());
                            warn!("Circuit breaker opened for server: {} (failures: {})", 
                                  server_name, cb.failure_count);
                        }
                    }
                    CircuitBreakerState::Open => {
                        // Already open, just update timestamp
                        cb.last_failure_time = Some(Instant::now());
                    }
                }
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.failed_operations += 1;
            
            if let Some(server_stats) = stats.operations_by_server.get_mut(server_name) {
                server_stats.failed_operations += 1;
            }
        }
    }

    /// Get retry policy for a server
    async fn get_retry_policy(&self, server_name: &str) -> RetryPolicy {
        let policies = self.retry_policies.read().await;
        policies.get(server_name).cloned().unwrap_or_else(|| {
            RetryPolicy {
                max_attempts: self.config.default_retry_attempts,
                base_delay: self.config.default_retry_delay,
                ..Default::default()
            }
        })
    }

    /// Check if an error is retryable
    fn is_retryable_error(&self, error: &Error, policy: &RetryPolicy) -> bool {
        let error_str = error.to_string().to_lowercase();
        
        policy.retryable_errors.iter().any(|retryable| {
            error_str.contains(&retryable.to_lowercase())
        })
    }

    /// Calculate retry delay with exponential backoff and jitter
    fn calculate_retry_delay(&self, policy: &RetryPolicy, attempt: u32) -> Duration {
        let base_delay_ms = policy.base_delay.as_millis() as f64;
        let delay_ms = base_delay_ms * policy.backoff_multiplier.powi((attempt - 1) as i32);
        
        let mut final_delay_ms = delay_ms.min(policy.max_delay.as_millis() as f64);
        
        // Add jitter if enabled
        if policy.jitter {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let jitter_factor = rng.gen_range(0.5..1.5);
            final_delay_ms *= jitter_factor;
        }
        
        Duration::from_millis(final_delay_ms as u64)
    }

    /// Handle circuit breaker open state
    async fn handle_circuit_breaker_open<T>(
        &self,
        operation_name: &str,
        server_name: &str,
    ) -> RecoveryResult<T> {
        info!("Circuit breaker open for server '{}', attempting fallback for operation '{}'", 
              server_name, operation_name);

        if self.config.enable_graceful_degradation {
            return self.try_fallback(operation_name, server_name, 
                                   Error::mcp("Circuit breaker open".to_string())).await;
        }

        RecoveryResult::Failed(Error::mcp(format!(
            "Circuit breaker open for server '{}' and graceful degradation disabled", 
            server_name
        )))
    }

    /// Handle final failure after all retries exhausted
    async fn handle_final_failure<T>(
        &self,
        operation_name: &str,
        server_name: &str,
        error: Error,
    ) -> RecoveryResult<T> {
        error!("All retry attempts exhausted for operation '{}' on server '{}': {}", 
               operation_name, server_name, error);

        if self.config.enable_graceful_degradation {
            return self.try_fallback(operation_name, server_name, error).await;
        }

        RecoveryResult::Failed(error)
    }

    /// Try fallback handlers
    async fn try_fallback<T>(
        &self,
        operation_name: &str,
        server_name: &str,
        error: Error,
    ) -> RecoveryResult<T> {
        let handlers = self.fallback_handlers.read().await;
        
        if let Some(handler) = handlers.get(operation_name) {
            if handler.is_applicable(operation_name, &error) {
                info!("Attempting fallback for operation '{}' on server '{}'", 
                      operation_name, server_name);

                // Update fallback stats
                {
                    let mut stats = self.stats.write().await;
                    stats.fallback_activations += 1;
                }

                match handler.handle_fallback(operation_name, &error) {
                    Ok(FallbackResult::Success(value)) => {
                        info!("Fallback succeeded for operation '{}'", operation_name);
                        // We can't convert serde_json::Value to T without more type information
                        // In a real implementation, we'd need a more sophisticated approach
                        return RecoveryResult::Failed(Error::mcp(
                            "Fallback succeeded but type conversion not implemented".to_string()
                        ));
                    }
                    Ok(FallbackResult::Degraded(value)) => {
                        warn!("Fallback provided degraded result for operation '{}'", operation_name);
                        return RecoveryResult::Failed(Error::mcp(
                            "Fallback degraded but type conversion not implemented".to_string()
                        ));
                    }
                    Ok(FallbackResult::Failed(msg)) => {
                        error!("Fallback failed for operation '{}': {}", operation_name, msg);
                    }
                    Err(fallback_error) => {
                        error!("Fallback error for operation '{}': {}", operation_name, fallback_error);
                    }
                }
            }
        }

        RecoveryResult::Failed(error)
    }

    /// Get current statistics
    pub async fn get_statistics(&self) -> ErrorRecoveryStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Reset statistics
    pub async fn reset_statistics(&self) {
        let mut stats = self.stats.write().await;
        *stats = ErrorRecoveryStats {
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            retry_operations: 0,
            circuit_breaker_trips: 0,
            fallback_activations: 0,
            operations_by_server: HashMap::new(),
        };
    }

    /// Force circuit breaker state for testing
    #[cfg(test)]
    pub async fn force_circuit_breaker_state(&self, server_name: &str, state: CircuitBreakerState) {
        self.set_circuit_breaker_state(server_name, state).await;
    }
}

/// Default fallback handler implementation
pub struct DefaultFallbackHandler;

impl FallbackHandler for DefaultFallbackHandler {
    fn handle_fallback(&self, operation: &str, error: &Error) -> Result<FallbackResult> {
        warn!("Default fallback activated for operation '{}': {}", operation, error);
        
        // Return a generic failure response
        Ok(FallbackResult::Failed(format!(
            "Operation '{}' failed and no specific fallback available: {}", 
            operation, error
        )))
    }

    fn is_applicable(&self, _operation: &str, _error: &Error) -> bool {
        true // Default handler applies to all operations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_error_recovery_creation() {
        let config = ErrorRecoveryConfig::default();
        let recovery = MCPErrorRecovery::new(config);
        
        let stats = recovery.get_statistics().await;
        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.successful_operations, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_states() {
        let config = ErrorRecoveryConfig {
            circuit_breaker_failure_threshold: 2,
            ..Default::default()
        };
        let recovery = MCPErrorRecovery::new(config);
        
        let server_name = "test_server";
        
        // Initially closed
        assert!(recovery.is_circuit_breaker_closed(server_name).await);
        
        // Record failures to trip circuit breaker
        let error = Error::mcp("test error".to_string());
        recovery.record_failure(server_name, &error).await;
        recovery.record_failure(server_name, &error).await;
        
        // Should be open now
        assert!(!recovery.is_circuit_breaker_closed(server_name).await);
    }

    #[tokio::test]
    async fn test_retry_policy() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            jitter: false,
            ..Default::default()
        };
        
        let recovery = MCPErrorRecovery::new(ErrorRecoveryConfig::default());
        
        // Test delay calculation
        let delay1 = recovery.calculate_retry_delay(&policy, 1);
        let delay2 = recovery.calculate_retry_delay(&policy, 2);
        
        assert_eq!(delay1, Duration::from_millis(100));
        assert_eq!(delay2, Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_retryable_error_check() {
        let policy = RetryPolicy::default();
        let recovery = MCPErrorRecovery::new(ErrorRecoveryConfig::default());
        
        let retryable_error = Error::mcp("connection_failed: unable to connect".to_string());
        let non_retryable_error = Error::mcp("authentication_failed: invalid token".to_string());
        
        assert!(recovery.is_retryable_error(&retryable_error, &policy));
        assert!(!recovery.is_retryable_error(&non_retryable_error, &policy));
    }

    #[tokio::test]
    async fn test_fallback_handler() {
        let handler = DefaultFallbackHandler;
        let error = Error::mcp("test error".to_string());
        
        assert!(handler.is_applicable("test_operation", &error));
        
        let result = handler.handle_fallback("test_operation", &error);
        assert!(result.is_ok());
        
        if let Ok(FallbackResult::Failed(_)) = result {
            // Expected
        } else {
            panic!("Expected fallback to return Failed result");
        }
    }

    #[tokio::test]
    async fn test_statistics_tracking() {
        let recovery = MCPErrorRecovery::new(ErrorRecoveryConfig::default());
        let server_name = "test_server";
        
        // Record some operations
        recovery.record_success(server_name, Duration::from_millis(100)).await;
        recovery.record_failure(server_name, &Error::mcp("test".to_string())).await;
        
        let stats = recovery.get_statistics().await;
        assert_eq!(stats.successful_operations, 1);
        assert_eq!(stats.failed_operations, 1);
        assert!(stats.operations_by_server.contains_key(server_name));
    }
}