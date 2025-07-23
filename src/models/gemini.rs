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

pub struct GeminiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    circuit_breaker: CircuitBreaker,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| Error::model_provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            api_key,
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            circuit_breaker: CircuitBreaker::new(
                "gemini".to_string(),
                5,
                Duration::from_secs(30),
            ),
        })
    }

    fn convert_messages(&self, messages: &[Message]) -> Result<Vec<GeminiContent>> {
        let mut gemini_contents = Vec::new();
        let mut system_parts = Vec::new();

        for message in messages {
            match message.role {
                MessageRole::System => {
                    system_parts.push(GeminiPart {
                        text: message.content.clone(),
                    });
                }
                MessageRole::User => {
                    gemini_contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart {
                            text: message.content.clone(),
                        }],
                    });
                }
                MessageRole::Assistant => {
                    gemini_contents.push(GeminiContent {
                        role: "model".to_string(), // Gemini uses "model" instead of "assistant"
                        parts: vec![GeminiPart {
                            text: message.content.clone(),
                        }],
                    });
                }
            }
        }

        // If we have system instructions, we'll need to prepend them to the first user message
        // or create a system message at the beginning
        if !system_parts.is_empty() {
            let system_text = system_parts
                .into_iter()
                .map(|p| p.text)
                .collect::<Vec<_>>()
                .join("\n");
            
            if let Some(first_user) = gemini_contents
                .iter_mut()
                .find(|c| c.role == "user") 
            {
                // Prepend system instructions to the first user message
                first_user.parts.insert(0, GeminiPart {
                    text: format!("System instructions: {}\n\nUser: ", system_text),
                });
            } else {
                // If no user messages, create one with just the system instructions
                gemini_contents.insert(0, GeminiContent {
                    role: "user".to_string(),
                    parts: vec![GeminiPart { text: system_text }],
                });
            }
        }

        Ok(gemini_contents)
    }

    fn calculate_cost(&self, model: &str, usage: &GeminiUsage) -> Decimal {
        // Google Gemini pricing (as of 2024)
        let (input_cost_per_1k, output_cost_per_1k) = match model {
            "gemini-1.5-pro" => (Decimal::from_f32_retain(1.25).unwrap() / Decimal::from(1000), Decimal::from_f32_retain(5.0).unwrap() / Decimal::from(1000)),
            "gemini-1.5-flash" => (Decimal::from_f32_retain(0.075).unwrap() / Decimal::from(1000), Decimal::from_f32_retain(0.3).unwrap() / Decimal::from(1000)),
            "gemini-pro" => (Decimal::from_f32_retain(0.5).unwrap() / Decimal::from(1000), Decimal::from_f32_retain(1.5).unwrap() / Decimal::from(1000)),
            _ => {
                warn!("Unknown model for cost calculation: {}", model);
                return Decimal::ZERO;
            }
        };

        let input_cost = input_cost_per_1k * Decimal::from(usage.prompt_token_count) / Decimal::from(1000);
        let output_cost = output_cost_per_1k * Decimal::from(usage.candidates_token_count) / Decimal::from(1000);

        input_cost + output_cost
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse> {
        let start_time = Instant::now();
        
        let contents = self.convert_messages(&request.messages)?;

        let gemini_request = GeminiRequest {
            contents,
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

        let response = self.circuit_breaker.call(|| {
            let request_body = gemini_request.clone();
            let client = self.client.clone();
            let api_key = self.api_key.clone();
            let base_url = self.base_url.clone();
            let model = request.model.clone();
            
            async move {
                let url = format!(
                    "{}/models/{}:generateContent?key={}",
                    base_url, model, api_key
                );

                let response = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| Error::model_provider(format!("Request failed: {}", e)))?;

                if response.status().is_success() {
                    let gemini_response: GeminiResponse = response
                        .json()
                        .await
                        .map_err(|e| Error::model_provider(format!("Failed to parse response: {}", e)))?;
                    
                    debug!("Received successful response from Gemini API");
                    Ok(gemini_response)
                } else {
                    let status = response.status();
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    
                    warn!("Gemini API error: {} - {}", status, error_text);
                    Err(Error::model_provider(format!("API error {}: {}", status, error_text)))
                }
            }
        }).await?;

        let content = response.candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .unwrap_or_default();

        let usage = response.usage_metadata.as_ref().map(|u| TokenUsage::new(
            u.prompt_token_count,
            u.candidates_token_count,
        ));

        let cost = response.usage_metadata
            .as_ref()
            .map(|u| self.calculate_cost(&request.model, u))
            .unwrap_or(Decimal::ZERO);

        Ok(ChatResponse {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: request.id,
            model: request.model,
            content,
            role: MessageRole::Assistant,
            created_at: Utc::now(),
            usage,
            finish_reason: response.candidates
                .first()
                .and_then(|c| c.finish_reason.clone()),
            provider_metadata: serde_json::json!({
                "provider": "gemini",
                "cost": cost,
                "response_time_ms": start_time.elapsed().as_millis()
            }),
        })
    }

    async fn stream_message(&self, _request: ChatRequest) -> Result<Box<dyn ChatStream>> {
        Err(Error::model_provider("Streaming not yet implemented for Gemini provider".to_string()))
    }

    fn get_pricing(&self) -> Option<PricingInfo> {
        Some(PricingInfo {
            provider: "gemini".to_string(),
            model: "gemini-1.5-flash".to_string(),
            input_price_per_1k_tokens: Decimal::from_f32_retain(0.075).unwrap() / Decimal::from(1000),
            output_price_per_1k_tokens: Decimal::from_f32_retain(0.3).unwrap() / Decimal::from(1000),
            effective_date: Utc::now(),
        })
    }

    fn get_capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            max_tokens: 8192,
            supports_streaming: false, // Will be true once implemented
            supports_function_calling: true,
            supports_vision: true,
            context_window: 1048576, // 1M tokens for Gemini 1.5
            supported_formats: vec!["text".to_string(), "image".to_string()],
        }
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        let start_time = Instant::now();
        
        let test_request = GeminiRequest {
            contents: vec![GeminiContent {
                role: "user".to_string(),
                parts: vec![GeminiPart {
                    text: "Hi".to_string(),
                }],
            }],
            generation_config: Some(GeminiGenerationConfig {
                temperature: Some(0.1),
                max_output_tokens: Some(10),
            }),
        };

        let url = format!(
            "{}/models/gemini-1.5-flash:generateContent?key={}",
            self.base_url, self.api_key
        );

        match self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&test_request)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                Ok(HealthStatus::healthy(start_time.elapsed().as_millis() as u64))
            }
            Ok(response) => {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Ok(HealthStatus::unhealthy(error_text, 1))
            }
            Err(e) => Ok(HealthStatus::unhealthy(e.to_string(), 1)),
        }
    }

    fn get_rate_limits(&self) -> RateLimits {
        RateLimits {
            requests_per_minute: Some(60),
            tokens_per_minute: Some(32000),
            requests_per_day: Some(1500),
            concurrent_requests: Some(5),
        }
    }

    fn supports_streaming(&self) -> bool {
        false // Will be true once streaming is implemented
    }

    fn get_provider_name(&self) -> &str {
        "gemini"
    }
}

#[derive(Debug, Clone, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Clone, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: u32,
    #[serde(rename = "totalTokenCount")]
    #[allow(dead_code)]
    total_token_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::Message;

    #[test]
    fn test_provider_creation() {
        let provider = GeminiProvider::new("test-key".to_string());
        assert!(provider.is_ok());
        
        let provider = provider.unwrap();
        assert_eq!(provider.get_provider_name(), "gemini");
    }

    #[test]
    fn test_capabilities() {
        let provider = GeminiProvider::new("test-key".to_string()).unwrap();
        let capabilities = provider.get_capabilities();
        
        assert_eq!(capabilities.context_window, 1048576);
        assert!(capabilities.supports_vision);
        assert!(capabilities.supports_function_calling);
        assert!(!capabilities.supports_streaming);
    }

    #[test]
    fn test_message_conversion() {
        let provider = GeminiProvider::new("test-key".to_string()).unwrap();
        
        let messages = vec![
            Message::system("You are a helpful assistant.".to_string()),
            Message::user("Hello!".to_string()),
            Message::assistant("Hi there!".to_string()),
        ];

        let converted = provider.convert_messages(&messages).unwrap();
        assert_eq!(converted.len(), 2); // System message merged with first user message
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "model");
        assert_eq!(converted[1].parts[0].text, "Hi there!");
    }

    #[test]
    fn test_cost_calculation() {
        let provider = GeminiProvider::new("test-key".to_string()).unwrap();
        
        let usage = GeminiUsage {
            prompt_token_count: 1000,
            candidates_token_count: 500,
            total_token_count: 1500,
        };

        let cost = provider.calculate_cost("gemini-1.5-flash", &usage);
        assert!(cost > Decimal::ZERO);
    }

    #[test]
    fn test_rate_limits() {
        let provider = GeminiProvider::new("test-key".to_string()).unwrap();
        let limits = provider.get_rate_limits();
        
        assert_eq!(limits.requests_per_minute, Some(60));
        assert_eq!(limits.tokens_per_minute, Some(32000));
    }
}