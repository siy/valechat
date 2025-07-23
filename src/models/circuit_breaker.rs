use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use tracing::{warn, info, debug};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
    failure_count: Arc<AtomicU32>,
    name: String,
}

#[derive(Debug, Clone)]
pub enum CircuitState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(name: String, failure_threshold: u32, recovery_timeout: Duration) -> Self {
        info!("Creating circuit breaker '{}' with threshold {} and timeout {:?}", 
              name, failure_threshold, recovery_timeout);

        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_threshold,
            recovery_timeout,
            failure_count: Arc::new(AtomicU32::new(0)),
            name,
        }
    }

    pub async fn call<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Check if circuit is open
        {
            let state = self.state.read();
            match *state {
                CircuitState::Open { opened_at } => {
                    if opened_at.elapsed() < self.recovery_timeout {
                        debug!("Circuit breaker '{}' is open, rejecting call", self.name);
                        return Err(Error::CircuitBreakerOpen);
                    }
                    // Timeout expired, transition to half-open
                    drop(state);
                    let mut state = self.state.write();
                    *state = CircuitState::HalfOpen;
                    info!("Circuit breaker '{}' transitioning to half-open", self.name);
                }
                CircuitState::HalfOpen => {
                    debug!("Circuit breaker '{}' is half-open, allowing single test call", self.name);
                }
                CircuitState::Closed => {
                    debug!("Circuit breaker '{}' is closed, allowing call", self.name);
                }
            }
        }

        // Execute the operation
        let result = operation().await;

        // Update circuit state based on result
        match result {
            Ok(value) => {
                self.on_success();
                Ok(value)
            }
            Err(error) => {
                self.on_failure();
                Err(error)
            }
        }
    }

    fn on_success(&self) {
        let previous_count = self.failure_count.swap(0, Ordering::SeqCst);
        
        let mut state = self.state.write();
        match *state {
            CircuitState::HalfOpen => {
                *state = CircuitState::Closed;
                info!("Circuit breaker '{}' recovered, transitioning to closed", self.name);
            }
            CircuitState::Open { .. } => {
                // This shouldn't happen if logic is correct
                *state = CircuitState::Closed;
                warn!("Circuit breaker '{}' was open during success, forcing to closed", self.name);
            }
            CircuitState::Closed => {
                if previous_count > 0 {
                    debug!("Circuit breaker '{}' reset failure count from {}", self.name, previous_count);
                }
            }
        }
    }

    fn on_failure(&self) {
        let failure_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        debug!("Circuit breaker '{}' failure count: {}/{}", self.name, failure_count, self.failure_threshold);

        if failure_count >= self.failure_threshold {
            let mut state = self.state.write();
            match *state {
                CircuitState::Closed => {
                    *state = CircuitState::Open { opened_at: Instant::now() };
                    warn!("Circuit breaker '{}' opened due to {} consecutive failures", 
                          self.name, failure_count);
                }
                CircuitState::HalfOpen => {
                    *state = CircuitState::Open { opened_at: Instant::now() };
                    warn!("Circuit breaker '{}' re-opened during half-open test", self.name);
                }
                CircuitState::Open { .. } => {
                    // Already open, just update the timestamp
                    *state = CircuitState::Open { opened_at: Instant::now() };
                }
            }
        }
    }

    pub fn get_state(&self) -> CircuitState {
        self.state.read().clone()
    }

    pub fn get_failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::SeqCst)
    }

    pub fn is_open(&self) -> bool {
        matches!(*self.state.read(), CircuitState::Open { .. })
    }

    pub fn is_closed(&self) -> bool {
        matches!(*self.state.read(), CircuitState::Closed)
    }

    pub fn is_half_open(&self) -> bool {
        matches!(*self.state.read(), CircuitState::HalfOpen)
    }

    pub fn force_open(&self) {
        let mut state = self.state.write();
        *state = CircuitState::Open { opened_at: Instant::now() };
        warn!("Circuit breaker '{}' manually opened", self.name);
    }

    pub fn force_close(&self) {
        let mut state = self.state.write();
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        info!("Circuit breaker '{}' manually closed and reset", self.name);
    }

    pub fn get_stats(&self) -> CircuitBreakerStats {
        let state = self.state.read().clone();
        let failure_count = self.failure_count.load(Ordering::SeqCst);

        CircuitBreakerStats {
            name: self.name.clone(),
            state: state.clone(),
            failure_count,
            failure_threshold: self.failure_threshold,
            recovery_timeout: self.recovery_timeout,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub name: String,
    pub state: CircuitState,
    pub failure_count: u32,
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_circuit_breaker_closed_to_open() {
        let cb = CircuitBreaker::new("test".to_string(), 3, Duration::from_millis(100));

        // Initially closed
        assert!(cb.is_closed());

        // Cause failures
        for i in 0..3 {
            let result = cb.call(|| async { 
                Err::<(), _>(Error::unknown("test failure"))
            }).await;
            assert!(result.is_err());
            
            if i < 2 {
                assert!(cb.is_closed());
            } else {
                assert!(cb.is_open());
            }
        }

        // Should be open now
        assert!(cb.is_open());
        assert_eq!(cb.get_failure_count(), 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let cb = CircuitBreaker::new("test".to_string(), 2, Duration::from_millis(50));

        // Cause failures to open circuit
        for _ in 0..2 {
            let _ = cb.call(|| async { 
                Err::<(), _>(Error::unknown("test failure"))
            }).await;
        }
        assert!(cb.is_open());

        // Wait for recovery timeout
        sleep(Duration::from_millis(60)).await;

        // Next call should transition to half-open
        let result = cb.call(|| async {
            Ok::<(), Error>(())
        }).await;
        assert!(result.is_ok());
        assert!(cb.is_closed());
        assert_eq!(cb.get_failure_count(), 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure() {
        let cb = CircuitBreaker::new("test".to_string(), 1, Duration::from_millis(50));

        // Cause failure to open circuit
        let _ = cb.call(|| async { 
            Err::<(), _>(Error::unknown("test failure"))
        }).await;
        assert!(cb.is_open());

        // Wait for recovery timeout
        sleep(Duration::from_millis(60)).await;

        // Next call should transition to half-open, but fail
        let result = cb.call(|| async { 
            Err::<(), _>(Error::unknown("test failure"))
        }).await;
        assert!(result.is_err());
        assert!(cb.is_open()); // Should go back to open
    }
}