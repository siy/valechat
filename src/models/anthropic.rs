use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::models::circuit_breaker::CircuitBreaker;
use crate::models::provider::{
    ChatRequest, ChatResponse, ChatStream, HealthStatus, Message, MessageRole, ModelCapabilities,
    ModelProvider, PricingInfo, RateLimits, TokenUsage,
};

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    circuit_breaker: CircuitBreaker,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| Error::model_provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            api_key,
            base_url: "https://api.anthropic.com/v1".to_string(),
            circuit_breaker: CircuitBreaker::new(
                "anthropic".to_string(),
                5,
                Duration::from_secs(30),
            ),
        })
    }

    fn convert_messages(&self, messages: &[Message]) -> Result<Vec<AnthropicMessage>> {
        let mut anthropic_messages = Vec::new();
        let mut system_message = String::new();

        for message in messages {
            match message.role {
                MessageRole::System => {
                    if !system_message.is_empty() {
                        system_message.push('\n');
                    }
                    system_message.push_str(&message.content);
                }
                MessageRole::User => {
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: message.content.clone(),
                    });
                }
                MessageRole::Assistant => {
                    anthropic_messages.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: message.content.clone(),
                    });
                }
            }
        }

        // If we have system content but no messages, create a user message
        if anthropic_messages.is_empty() && !system_message.is_empty() {
            anthropic_messages.push(AnthropicMessage {
                role: "user".to_string(),
                content: system_message,
            });
        }

        Ok(anthropic_messages)
    }

    async fn make_request(&self, request: AnthropicRequest) -> Result<AnthropicResponse> {
        debug!("Making Anthropic API request to model: {}", request.model);

        let response = self
            .client
            .post(&format!("{}/messages", self.base_url))
            .header("Content-Type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::model_provider(format!("Request failed: {}", e)))?;

        if response.status().is_success() {
            let anthropic_response: AnthropicResponse = response
                .json()
                .await
                .map_err(|e| Error::model_provider(format!("Failed to parse response: {}", e)))?;
            
            debug!("Received successful response from Anthropic API");
            Ok(anthropic_response)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            warn!("Anthropic API error: {} - {}", status, error_text);
            Err(Error::model_provider(format!("API error {}: {}", status, error_text)))
        }
    }

    fn calculate_cost(&self, model: &str, usage: &AnthropicUsage) -> Decimal {
        // Anthropic pricing (as of 2024)
        let (input_cost_per_1k, output_cost_per_1k) = match model {
            "claude-3-opus-20240229" => (Decimal::from(15) / Decimal::from(1000), Decimal::from(75) / Decimal::from(1000)),
            "claude-3-sonnet-20240229" => (Decimal::from(3) / Decimal::from(1000), Decimal::from(15) / Decimal::from(1000)),
            "claude-3-haiku-20240307" => (Decimal::from_f32_retain(0.25).unwrap() / Decimal::from(1000), Decimal::from_f32_retain(1.25).unwrap() / Decimal::from(1000)),
            "claude-3-5-sonnet-20241022" => (Decimal::from(3) / Decimal::from(1000), Decimal::from(15) / Decimal::from(1000)),
            _ => {
                warn!("Unknown model for cost calculation: {}", model);
                return Decimal::ZERO;
            }
        };

        let input_cost = input_cost_per_1k * Decimal::from(usage.input_tokens) / Decimal::from(1000);
        let output_cost = output_cost_per_1k * Decimal::from(usage.output_tokens) / Decimal::from(1000);

        input_cost + output_cost
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse> {
        let start_time = Instant::now();
        
        let anthropic_messages = self.convert_messages(&request.messages)?;
        
        // Extract system message if present
        let system = request.messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(1024),
            messages: anthropic_messages,
            system,
            temperature: request.temperature.unwrap_or(0.7),
            stream: false,
        };

        let response = self.circuit_breaker.call(|| {
            let request = anthropic_request.clone();
            let client = self.client.clone();
            let api_key = self.api_key.clone();
            let base_url = self.base_url.clone();
            
            async move {
                let response = client
                    .post(&format!("{}/messages", base_url))
                    .header("Content-Type", "application/json")
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| Error::model_provider(format!("Request failed: {}", e)))?;

                if response.status().is_success() {
                    let anthropic_response: AnthropicResponse = response
                        .json()
                        .await
                        .map_err(|e| Error::model_provider(format!("Failed to parse response: {}", e)))?;
                    
                    debug!("Received successful response from Anthropic API");
                    Ok(anthropic_response)
                } else {
                    let status = response.status();
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    
                    warn!("Anthropic API error: {} - {}", status, error_text);
                    Err(Error::model_provider(format!("API error {}: {}", status, error_text)))
                }
            }
        }).await?;
        
        let content = response.content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let usage = TokenUsage::new(response.usage.input_tokens, response.usage.output_tokens);
        let cost = self.calculate_cost(&request.model, &response.usage);

        Ok(ChatResponse {
            id: response.id,
            request_id: request.id,
            model: response.model,
            content,
            role: MessageRole::Assistant,
            created_at: Utc::now(),
            usage: Some(usage),
            finish_reason: Some("stop".to_string()),
            provider_metadata: serde_json::json!({
                "provider": "anthropic",
                "cost": cost,
                "response_time_ms": start_time.elapsed().as_millis()
            }),
        })
    }

    async fn stream_message(&self, _request: ChatRequest) -> Result<Box<dyn ChatStream>> {
        Err(Error::model_provider("Streaming not yet implemented for Anthropic provider".to_string()))
    }

    fn get_pricing(&self) -> Option<PricingInfo> {
        Some(PricingInfo {
            provider: "anthropic".to_string(),
            model: "claude-3-sonnet-20240229".to_string(),
            input_price_per_1k_tokens: Decimal::from(3) / Decimal::from(1000),
            output_price_per_1k_tokens: Decimal::from(15) / Decimal::from(1000),
            effective_date: Utc::now(),
        })
    }

    fn get_capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            max_tokens: 4096,
            supports_streaming: false, // Will be true once implemented
            supports_function_calling: false,
            supports_vision: false,
            context_window: 200000,
            supported_formats: vec!["text".to_string()],
        }
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        let start_time = Instant::now();
        
        // Test with a minimal request
        let test_messages = vec![AnthropicMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }];

        let request = AnthropicRequest {
            model: "claude-3-haiku-20240307".to_string(),
            max_tokens: 10,
            messages: test_messages,
            system: None,
            temperature: 0.1,
            stream: false,
        };

        match self.make_request(request).await {
            Ok(_) => Ok(HealthStatus::healthy(start_time.elapsed().as_millis() as u64)),
            Err(e) => Ok(HealthStatus::unhealthy(e.to_string(), 1)),
        }
    }

    fn get_rate_limits(&self) -> RateLimits {
        RateLimits {
            requests_per_minute: Some(50),
            tokens_per_minute: Some(40000),
            requests_per_day: Some(1000),
            concurrent_requests: Some(5),
        }
    }

    fn supports_streaming(&self) -> bool {
        false // Will be true once streaming is implemented
    }

    fn get_provider_name(&self) -> &str {
        "anthropic"
    }
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    content: Vec<AnthropicContent>,
    model: String,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
    #[allow(dead_code)]
    r#type: String,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::Message;

    #[test]
    fn test_provider_creation() {
        let provider = AnthropicProvider::new("test-key".to_string());
        assert!(provider.is_ok());
        
        let provider = provider.unwrap();
        assert_eq!(provider.get_provider_name(), "anthropic");
    }

    #[test]
    fn test_capabilities() {
        let provider = AnthropicProvider::new("test-key".to_string()).unwrap();
        let capabilities = provider.get_capabilities();
        
        assert_eq!(capabilities.context_window, 200000);
        assert!(!capabilities.supports_streaming);
    }

    #[test]
    fn test_message_conversion() {
        let provider = AnthropicProvider::new("test-key".to_string()).unwrap();
        
        let messages = vec![
            Message::system("You are a helpful assistant.".to_string()),
            Message::user("Hello!".to_string()),
        ];

        let converted = provider.convert_messages(&messages).unwrap();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content, "Hello!");
    }

    #[test]
    fn test_cost_calculation() {
        let provider = AnthropicProvider::new("test-key".to_string()).unwrap();
        
        let usage = AnthropicUsage {
            input_tokens: 1000,
            output_tokens: 500,
        };

        let cost = provider.calculate_cost("claude-3-haiku-20240307", &usage);
        assert!(cost > Decimal::ZERO);
    }

    #[test]
    fn test_rate_limits() {
        let provider = AnthropicProvider::new("test-key".to_string()).unwrap();
        let limits = provider.get_rate_limits();
        
        assert_eq!(limits.requests_per_minute, Some(50));
        assert_eq!(limits.tokens_per_minute, Some(40000));
    }
}