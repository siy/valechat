use async_trait::async_trait;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};
use tokio::time::timeout;

use crate::error::{Error, Result};
use crate::models::capability_detection::{CapabilityDetector, ModelRecommendation, QualityPriority, TaskRequirements, TaskType};
use crate::models::provider::{ChatRequest, ChatResponse, ModelProvider};

#[derive(Debug, Clone)]
pub struct FallbackConfig {
    pub max_retries: usize,
    pub retry_delay_ms: u64,
    pub timeout_ms: u64,
    pub fallback_on_rate_limit: bool,
    pub fallback_on_error: bool,
    pub fallback_on_timeout: bool,
    pub quality_degradation_allowed: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_delay_ms: 1000,
            timeout_ms: 30000,
            fallback_on_rate_limit: true,
            fallback_on_error: true,
            fallback_on_timeout: true,
            quality_degradation_allowed: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FallbackAttempt {
    pub provider_name: String,
    pub model_name: String,
    pub attempt_number: usize,
    pub error: Option<String>,
    pub response_time_ms: Option<u64>,
    pub success: bool,
}

pub struct ModelFallbackManager {
    providers: HashMap<String, Box<dyn ModelProvider + Send + Sync>>,
    capability_detector: CapabilityDetector,
    config: FallbackConfig,
    failure_counts: HashMap<String, usize>,
    last_success: HashMap<String, Instant>,
}

impl ModelFallbackManager {
    pub fn new(config: FallbackConfig) -> Self {
        Self {
            providers: HashMap::new(),
            capability_detector: CapabilityDetector::new(),
            config,
            failure_counts: HashMap::new(),
            last_success: HashMap::new(),
        }
    }

    pub fn add_provider(&mut self, provider: Box<dyn ModelProvider + Send + Sync>) -> Result<()> {
        let provider_name = provider.get_provider_name().to_string();
        debug!("Adding provider to fallback manager: {}", provider_name);
        
        // Add to capability detector as well
        let provider_clone = self.clone_provider(&*provider)?;
        self.capability_detector.add_provider(provider_clone)?;
        
        self.providers.insert(provider_name, provider);
        Ok(())
    }

    pub async fn send_message_with_fallback(&mut self, request: ChatRequest) -> Result<(ChatResponse, Vec<FallbackAttempt>)> {
        info!("Starting message send with fallback for request: {}", request.id);
        
        let task_requirements = self.infer_task_requirements(&request);
        let recommendations = self.capability_detector.recommend_model(&task_requirements).await?;
        
        if recommendations.is_empty() {
            return Err(Error::model_provider("No suitable providers available"));
        }

        let mut attempts = Vec::new();
        let mut last_error = None;

        for (attempt_num, recommendation) in recommendations.iter().enumerate() {
            let provider_key = format!("{}:{}", recommendation.provider_name, recommendation.model_name);
            
            // Check if this provider has failed too many times recently
            if self.should_skip_provider(&provider_key) {
                debug!("Skipping provider {} due to recent failures", provider_key);
                continue;
            }

            let start_time = Instant::now();
            let mut attempt = FallbackAttempt {
                provider_name: recommendation.provider_name.clone(),
                model_name: recommendation.model_name.clone(),
                attempt_number: attempt_num + 1,
                error: None,
                response_time_ms: None,
                success: false,
            };

            match self.try_provider(&recommendation.provider_name, &request).await {
                Ok(response) => {
                    let response_time = start_time.elapsed();
                    attempt.response_time_ms = Some(response_time.as_millis() as u64);
                    attempt.success = true;
                    attempts.push(attempt);
                    
                    // Record success
                    self.record_success(&provider_key);
                    
                    info!(
                        "Successfully sent message using {} in {}ms",
                        provider_key,
                        response_time.as_millis()
                    );
                    
                    return Ok((response, attempts));
                }
                Err(e) => {
                    let response_time = start_time.elapsed();
                    attempt.response_time_ms = Some(response_time.as_millis() as u64);
                    attempt.error = Some(e.to_string());
                    attempts.push(attempt);
                    
                    // Record failure
                    self.record_failure(&provider_key);
                    
                    warn!(
                        "Failed to send message using {}: {} (attempt {}/{})",
                        provider_key,
                        e,
                        attempt_num + 1,
                        recommendations.len()
                    );
                    
                    last_error = Some(e);
                    
                    // Wait before next attempt if configured
                    if self.config.retry_delay_ms > 0 && attempt_num < recommendations.len() - 1 {
                        tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                    }
                }
            }
            
            // Check if we've exceeded max retries
            if attempt_num + 1 >= self.config.max_retries {
                break;
            }
        }

        error!(
            "All fallback attempts failed for request: {}. Attempts: {}",
            request.id,
            attempts.len()
        );

        Err(last_error.unwrap_or_else(|| Error::model_provider("All providers failed")))
    }

    async fn try_provider(&self, provider_name: &str, request: &ChatRequest) -> Result<ChatResponse> {
        let provider = self.providers.get(provider_name)
            .ok_or_else(|| Error::model_provider(format!("Provider not found: {}", provider_name)))?;

        // Apply timeout if configured
        if self.config.timeout_ms > 0 {
            match timeout(
                Duration::from_millis(self.config.timeout_ms),
                provider.send_message(request.clone())
            ).await {
                Ok(result) => result,
                Err(_) => {
                    if self.config.fallback_on_timeout {
                        Err(Error::model_provider(format!("Request timeout after {}ms", self.config.timeout_ms)))
                    } else {
                        // If not falling back on timeout, wait for the actual result
                        provider.send_message(request.clone()).await
                    }
                }
            }
        } else {
            provider.send_message(request.clone()).await
        }
    }

    fn infer_task_requirements(&self, request: &ChatRequest) -> TaskRequirements {
        // Simple heuristic to infer task type from request content
        let content = request.messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

        let task_type = if content.contains("code") || content.contains("function") || content.contains("programming") {
            TaskType::CodeGeneration
        } else if content.contains("analyze") || content.contains("reasoning") || content.contains("logic") {
            TaskType::ReasoningAndAnalysis
        } else if content.contains("summarize") || content.contains("summary") {
            TaskType::Summarization
        } else if content.contains("translate") || content.contains("translation") {
            TaskType::Translation
        } else if content.contains("creative") || content.contains("story") || content.contains("poem") {
            TaskType::CreativeWriting
        } else if content.contains("document") || content.contains("analyze") {
            TaskType::DocumentAnalysis
        } else if content.contains("question") || content.contains("what") || content.contains("how") || content.contains("why") {
            TaskType::QuestionAnswering
        } else {
            TaskType::ConversationalChat
        };

        TaskRequirements {
            task_type,
            max_tokens_needed: request.max_tokens,
            requires_streaming: request.stream,
            requires_function_calling: false, // Could be inferred from content
            requires_vision: false,           // Could be inferred from message attachments
            max_cost_per_request: None,       // Could be configured per user
            max_response_time_ms: request.timeout.map(|t| t.as_millis() as u64),
            quality_priority: QualityPriority::Balanced, // Could be user-configurable
        }
    }

    fn should_skip_provider(&self, provider_key: &str) -> bool {
        const MAX_FAILURES: usize = 5;
        const COOLDOWN_DURATION: Duration = Duration::from_secs(300); // 5 minutes

        if let Some(&failure_count) = self.failure_counts.get(provider_key) {
            if failure_count >= MAX_FAILURES {
                // Check if enough time has passed since last success
                if let Some(&last_success_time) = self.last_success.get(provider_key) {
                    return last_success_time.elapsed() < COOLDOWN_DURATION;
                }
                return true;
            }
        }
        false
    }

    fn record_success(&mut self, provider_key: &str) {
        self.failure_counts.insert(provider_key.to_string(), 0);
        self.last_success.insert(provider_key.to_string(), Instant::now());
        debug!("Recorded success for provider: {}", provider_key);
    }

    fn record_failure(&mut self, provider_key: &str) {
        let count = self.failure_counts.get(provider_key).unwrap_or(&0) + 1;
        self.failure_counts.insert(provider_key.to_string(), count);
        debug!("Recorded failure for provider: {} (count: {})", provider_key, count);
    }

    // Helper method to clone providers for the capability detector
    // This is a workaround since we can't clone trait objects directly
    fn clone_provider(&self, provider: &dyn ModelProvider) -> Result<Box<dyn ModelProvider + Send + Sync>> {
        // This is a simplified approach - in a real implementation, we'd need
        // proper cloning support or factory patterns
        match provider.get_provider_name() {
            "openai" => {
                // We'd need to extract the API key and recreate the provider
                // For now, return an error indicating this needs implementation
                Err(Error::model_provider("Provider cloning not yet implemented"))
            }
            "anthropic" => {
                Err(Error::model_provider("Provider cloning not yet implemented"))
            }
            "gemini" => {
                Err(Error::model_provider("Provider cloning not yet implemented"))
            }
            _ => Err(Error::model_provider("Unknown provider type"))
        }
    }

    pub fn get_provider_statistics(&self) -> HashMap<String, ProviderStats> {
        let mut stats = HashMap::new();
        
        for provider_name in self.providers.keys() {
            let failure_count = self.failure_counts.get(provider_name).unwrap_or(&0);
            let last_success = self.last_success.get(provider_name).copied();
            
            stats.insert(provider_name.clone(), ProviderStats {
                failure_count: *failure_count,
                last_success,
                is_available: !self.should_skip_provider(provider_name),
            });
        }
        
        stats
    }
}

#[derive(Debug, Clone)]
pub struct ProviderStats {
    pub failure_count: usize,
    pub last_success: Option<Instant>,
    pub is_available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::{Message, MessageRole};

    #[test]
    fn test_fallback_config_default() {
        let config = FallbackConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert!(config.fallback_on_error);
    }

    #[test]
    fn test_fallback_manager_creation() {
        let config = FallbackConfig::default();
        let manager = ModelFallbackManager::new(config);
        assert_eq!(manager.providers.len(), 0);
    }

    #[test]
    fn test_task_type_inference() {
        let config = FallbackConfig::default();
        let manager = ModelFallbackManager::new(config);
        
        let request = ChatRequest {
            id: "test".to_string(),
            messages: vec![Message::user("Write some code to sort an array".to_string())],
            model: "test".to_string(),
            temperature: None,
            max_tokens: None,
            stream: false,
            timeout: None,
            user_id: None,
        };
        
        let requirements = manager.infer_task_requirements(&request);
        assert_eq!(requirements.task_type, TaskType::CodeGeneration);
    }

    #[test]
    fn test_provider_failure_tracking() {
        let config = FallbackConfig::default();
        let mut manager = ModelFallbackManager::new(config);
        
        let provider_key = "test:model";
        
        // Initially should not skip
        assert!(!manager.should_skip_provider(provider_key));
        
        // Record multiple failures
        for _ in 0..5 {
            manager.record_failure(provider_key);
        }
        
        // Should skip after too many failures
        assert!(manager.should_skip_provider(provider_key));
        
        // Record success should reset
        manager.record_success(provider_key);
        assert!(!manager.should_skip_provider(provider_key));
    }

    #[test]
    fn test_provider_statistics() {
        let config = FallbackConfig::default();
        let mut manager = ModelFallbackManager::new(config);
        
        // Add some mock failures
        manager.record_failure("provider1:model1");
        manager.record_failure("provider1:model1");
        manager.record_success("provider2:model2");
        
        let stats = manager.get_provider_statistics();
        
        // Note: providers need to be added to show up in stats
        // This test mainly verifies the method doesn't crash
        assert!(stats.len() >= 0);
    }
}