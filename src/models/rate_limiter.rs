use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, warn, info};

use crate::error::{Error, Result};
use crate::models::provider::RateLimits;

#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    pub enable_rate_limiting: bool,
    pub token_bucket_refill_rate: f64, // tokens per second
    pub burst_allowance_multiplier: f64, // multiply base limit for burst capacity
    pub backoff_base_delay_ms: u64,
    pub backoff_max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            enable_rate_limiting: true,
            token_bucket_refill_rate: 1.0,
            burst_allowance_multiplier: 2.0,
            backoff_base_delay_ms: 1000,
            backoff_max_delay_ms: 60000,
            backoff_multiplier: 2.0,
        }
    }
}

#[derive(Debug)]
pub struct TokenBucket {
    capacity: u32,
    tokens: AtomicU32,
    refill_rate: f64, // tokens per second
    last_refill: AtomicU64, // timestamp in milliseconds
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            capacity,
            tokens: AtomicU32::new(capacity),
            refill_rate,
            last_refill: AtomicU64::new(now),
        }
    }

    pub fn try_consume(&self, tokens_needed: u32) -> bool {
        self.refill();
        
        let current_tokens = self.tokens.load(Ordering::Acquire);
        if current_tokens >= tokens_needed {
            // Try to consume tokens atomically
            let new_tokens = current_tokens - tokens_needed;
            match self.tokens.compare_exchange_weak(
                current_tokens,
                new_tokens,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    debug!("Consumed {} tokens, {} remaining", tokens_needed, new_tokens);
                    true
                }
                Err(_) => {
                    // Another thread modified the tokens count, retry
                    self.try_consume(tokens_needed)
                }
            }
        } else {
            debug!("Insufficient tokens: need {}, have {}", tokens_needed, current_tokens);
            false
        }
    }

    fn refill(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let last_refill = self.last_refill.load(Ordering::Acquire);
        let time_passed_ms = now.saturating_sub(last_refill);
        
        if time_passed_ms > 0 {
            let tokens_to_add = ((time_passed_ms as f64 / 1000.0) * self.refill_rate) as u32;
            
            if tokens_to_add > 0 {
                let current_tokens = self.tokens.load(Ordering::Acquire);
                let new_tokens = (current_tokens + tokens_to_add).min(self.capacity);
                
                if self.tokens.compare_exchange_weak(
                    current_tokens,
                    new_tokens,
                    Ordering::Release,
                    Ordering::Relaxed,
                ).is_ok() {
                    self.last_refill.store(now, Ordering::Release);
                    if tokens_to_add > 0 {
                        debug!("Refilled {} tokens, total: {}", tokens_to_add, new_tokens);
                    }
                }
            }
        }
    }

    pub fn available_tokens(&self) -> u32 {
        self.refill();
        self.tokens.load(Ordering::Acquire)
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

#[derive(Debug)]
pub struct ProviderRateLimiter {
    provider_name: String,
    request_bucket: TokenBucket,
    token_bucket: TokenBucket,
    daily_request_count: AtomicU32,
    daily_reset_time: AtomicU64,
    concurrent_requests: AtomicU32,
    max_concurrent: u32,
    last_request_time: AtomicU64,
    consecutive_rate_limit_hits: AtomicU32,
    config: RateLimiterConfig,
}

impl ProviderRateLimiter {
    pub fn new(provider_name: String, limits: &RateLimits, config: RateLimiterConfig) -> Self {
        let requests_per_minute = limits.requests_per_minute.unwrap_or(60);
        let tokens_per_minute = limits.tokens_per_minute.unwrap_or(10000);
        let max_concurrent = limits.concurrent_requests.unwrap_or(5);

        // Convert per-minute limits to per-second for token bucket
        let request_rate = requests_per_minute as f64 / 60.0;
        let token_rate = tokens_per_minute as f64 / 60.0;

        // Apply burst multiplier
        let request_capacity = (requests_per_minute as f64 * config.burst_allowance_multiplier) as u32;
        let token_capacity = (tokens_per_minute as f64 * config.burst_allowance_multiplier) as u32;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            provider_name: provider_name.clone(),
            request_bucket: TokenBucket::new(request_capacity, request_rate),
            token_bucket: TokenBucket::new(token_capacity, token_rate),
            daily_request_count: AtomicU32::new(0),
            daily_reset_time: AtomicU64::new(now + 86400000), // 24 hours from now
            concurrent_requests: AtomicU32::new(0),
            max_concurrent,
            last_request_time: AtomicU64::new(0),
            consecutive_rate_limit_hits: AtomicU32::new(0),
            config,
        }
    }

    pub async fn acquire_permit(&self, estimated_tokens: u32) -> Result<RateLimitPermit> {
        if !self.config.enable_rate_limiting {
            return Ok(RateLimitPermit::new(self.provider_name.clone()));
        }

        info!("Acquiring rate limit permit for {} (estimated {} tokens)", self.provider_name, estimated_tokens);

        // Check daily limits
        self.check_daily_limits()?;

        // Check concurrent request limits
        self.acquire_concurrent_slot().await?;

        // Check rate limits with backoff
        self.acquire_rate_limited_permit(estimated_tokens).await?;

        debug!("Rate limit permit acquired for {}", self.provider_name);
        Ok(RateLimitPermit::new(self.provider_name.clone()))
    }

    fn check_daily_limits(&self) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let reset_time = self.daily_reset_time.load(Ordering::Acquire);
        
        // Reset daily counter if a day has passed
        if now >= reset_time {
            self.daily_request_count.store(0, Ordering::Release);
            self.daily_reset_time.store(now + 86400000, Ordering::Release);
            debug!("Reset daily request count for {}", self.provider_name);
        }

        // For now, we don't enforce daily limits since they're provider-specific
        // This is a placeholder for future implementation
        Ok(())
    }

    async fn acquire_concurrent_slot(&self) -> Result<()> {
        let mut retries = 0;
        const MAX_RETRIES: u32 = 10;

        while retries < MAX_RETRIES {
            let current = self.concurrent_requests.load(Ordering::Acquire);
            if current < self.max_concurrent {
                match self.concurrent_requests.compare_exchange_weak(
                    current,
                    current + 1,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        debug!("Acquired concurrent slot for {} ({}/{})", 
                               self.provider_name, current + 1, self.max_concurrent);
                        return Ok(());
                    }
                    Err(_) => {
                        // Retry on contention
                        retries += 1;
                        continue;
                    }
                }
            } else {
                warn!("Too many concurrent requests for {} ({}/{})", 
                      self.provider_name, current, self.max_concurrent);
                
                // Wait a bit before retrying
                sleep(Duration::from_millis(100)).await;
                retries += 1;
            }
        }

        Err(Error::model_provider(format!(
            "Too many concurrent requests for provider: {}",
            self.provider_name
        )))
    }

    async fn acquire_rate_limited_permit(&self, estimated_tokens: u32) -> Result<()> {
        let mut backoff_delay = self.config.backoff_base_delay_ms;
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 5;

        while attempts < MAX_ATTEMPTS {
            // Try to consume tokens from both buckets
            let request_ok = self.request_bucket.try_consume(1);
            let token_ok = estimated_tokens == 0 || self.token_bucket.try_consume(estimated_tokens);

            if request_ok && token_ok {
                // Reset consecutive failures on success
                self.consecutive_rate_limit_hits.store(0, Ordering::Release);
                self.last_request_time.store(
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
                    Ordering::Release,
                );
                return Ok(());
            }

            // Record rate limit hit
            let consecutive_hits = self.consecutive_rate_limit_hits.fetch_add(1, Ordering::AcqRel);
            
            warn!(
                "Rate limit hit for {} (attempt {}/{}): request_ok={}, token_ok={}, consecutive_hits={}",
                self.provider_name, attempts + 1, MAX_ATTEMPTS, request_ok, token_ok, consecutive_hits + 1
            );

            // Calculate backoff delay
            let delay = backoff_delay.min(self.config.backoff_max_delay_ms);
            
            info!("Backing off for {}ms before retry", delay);
            sleep(Duration::from_millis(delay)).await;

            // Increase backoff for next attempt
            backoff_delay = ((backoff_delay as f64) * self.config.backoff_multiplier) as u64;
            attempts += 1;
        }

        Err(Error::model_provider(format!(
            "Rate limit exceeded for provider: {} after {} attempts",
            self.provider_name, attempts
        )))
    }

    pub fn release_concurrent_slot(&self) {
        let current = self.concurrent_requests.fetch_sub(1, Ordering::AcqRel);
        debug!("Released concurrent slot for {} ({}/{})", 
               self.provider_name, current.saturating_sub(1), self.max_concurrent);
    }

    pub fn get_status(&self) -> RateLimiterStatus {
        RateLimiterStatus {
            provider_name: self.provider_name.clone(),
            available_requests: self.request_bucket.available_tokens(),
            request_capacity: self.request_bucket.capacity(),
            available_tokens: self.token_bucket.available_tokens(),
            token_capacity: self.token_bucket.capacity(),
            concurrent_requests: self.concurrent_requests.load(Ordering::Acquire),
            max_concurrent: self.max_concurrent,
            daily_request_count: self.daily_request_count.load(Ordering::Acquire),
            consecutive_rate_limit_hits: self.consecutive_rate_limit_hits.load(Ordering::Acquire),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimiterStatus {
    pub provider_name: String,
    pub available_requests: u32,
    pub request_capacity: u32,
    pub available_tokens: u32,
    pub token_capacity: u32,
    pub concurrent_requests: u32,
    pub max_concurrent: u32,
    pub daily_request_count: u32,
    pub consecutive_rate_limit_hits: u32,
}

pub struct RateLimitPermit {
    provider_name: String,
    acquired_at: Instant,
}

impl RateLimitPermit {
    fn new(provider_name: String) -> Self {
        Self {
            provider_name,
            acquired_at: Instant::now(),
        }
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub fn duration_held(&self) -> Duration {
        self.acquired_at.elapsed()
    }
}

pub struct MultiProviderRateLimiter {
    limiters: Arc<Mutex<HashMap<String, ProviderRateLimiter>>>,
    config: RateLimiterConfig,
}

impl MultiProviderRateLimiter {
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            limiters: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    pub async fn add_provider(&self, provider_name: String, limits: &RateLimits) -> Result<()> {
        let mut limiters = self.limiters.lock().await;
        
        let limiter = ProviderRateLimiter::new(
            provider_name.clone(),
            limits,
            self.config.clone(),
        );
        
        limiters.insert(provider_name.clone(), limiter);
        info!("Added rate limiter for provider: {}", provider_name);
        
        Ok(())
    }

    pub async fn acquire_permit(&self, provider_name: &str, estimated_tokens: u32) -> Result<RateLimitPermit> {
        let limiters = self.limiters.lock().await;
        
        if let Some(limiter) = limiters.get(provider_name) {
            limiter.acquire_permit(estimated_tokens).await
        } else {
            warn!("No rate limiter found for provider: {}", provider_name);
            // If no limiter is configured, allow the request
            Ok(RateLimitPermit::new(provider_name.to_string()))
        }
    }

    pub async fn release_concurrent_slot(&self, provider_name: &str) {
        let limiters = self.limiters.lock().await;
        
        if let Some(limiter) = limiters.get(provider_name) {
            limiter.release_concurrent_slot();
        }
    }

    pub async fn get_all_status(&self) -> HashMap<String, RateLimiterStatus> {
        let limiters = self.limiters.lock().await;
        
        limiters
            .iter()
            .map(|(name, limiter)| (name.clone(), limiter.get_status()))
            .collect()
    }
}

impl Drop for RateLimitPermit {
    fn drop(&mut self) {
        debug!("Rate limit permit for {} held for {:?}", 
               self.provider_name, self.duration_held());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(100, 10.0);
        assert_eq!(bucket.capacity(), 100);
        assert_eq!(bucket.available_tokens(), 100);
    }

    #[test]
    fn test_token_bucket_consume() {
        let bucket = TokenBucket::new(100, 10.0);
        
        assert!(bucket.try_consume(50));
        assert_eq!(bucket.available_tokens(), 50);
        
        assert!(bucket.try_consume(50));
        assert_eq!(bucket.available_tokens(), 0);
        
        assert!(!bucket.try_consume(1));
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let bucket = TokenBucket::new(10, 10.0); // 10 tokens per second
        
        // Consume all tokens
        assert!(bucket.try_consume(10));
        assert_eq!(bucket.available_tokens(), 0);
        
        // Wait for refill
        tokio::time::sleep(Duration::from_millis(1100)).await;
        
        // Should have refilled at least 10 tokens
        assert!(bucket.available_tokens() >= 10);
    }

    #[test]
    fn test_rate_limiter_config_default() {
        let config = RateLimiterConfig::default();
        assert!(config.enable_rate_limiting);
        assert_eq!(config.token_bucket_refill_rate, 1.0);
    }

    #[tokio::test]
    async fn test_provider_rate_limiter_creation() {
        let limits = RateLimits {
            requests_per_minute: Some(60),
            tokens_per_minute: Some(1000),
            requests_per_day: Some(1000),
            concurrent_requests: Some(5),
        };
        
        let config = RateLimiterConfig::default();
        let limiter = ProviderRateLimiter::new("test".to_string(), &limits, config);
        
        let status = limiter.get_status();
        assert_eq!(status.provider_name, "test");
        assert_eq!(status.max_concurrent, 5);
    }

    #[tokio::test]
    async fn test_multi_provider_rate_limiter() {
        let config = RateLimiterConfig::default();
        let multi_limiter = MultiProviderRateLimiter::new(config);
        
        let limits = RateLimits::default();
        multi_limiter.add_provider("test".to_string(), &limits).await.unwrap();
        
        let permit = multi_limiter.acquire_permit("test", 10).await.unwrap();
        assert_eq!(permit.provider_name(), "test");
    }

    #[tokio::test]
    async fn test_concurrent_slot_management() {
        let limits = RateLimits {
            requests_per_minute: Some(60),
            tokens_per_minute: Some(1000),
            requests_per_day: Some(1000),
            concurrent_requests: Some(2), // Small limit for testing
        };
        
        let config = RateLimiterConfig::default();
        let limiter = ProviderRateLimiter::new("test".to_string(), &limits, config);
        
        // Should be able to acquire 2 permits
        let _permit1 = limiter.acquire_permit(10).await.unwrap();
        let _permit2 = limiter.acquire_permit(10).await.unwrap();
        
        // Third should fail quickly (within reasonable time)
        let start = Instant::now();
        let result = limiter.acquire_permit(10).await;
        let elapsed = start.elapsed();
        
        assert!(result.is_err());
        assert!(elapsed < Duration::from_secs(2)); // Should fail relatively quickly
    }
}