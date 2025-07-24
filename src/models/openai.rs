use async_trait::async_trait;
use chrono::Utc;
use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tracing::{debug, error};

use crate::error::{Error, Result};
use crate::models::{
    ModelProvider, ChatRequest, ChatResponse, ChatStream,
    Message, MessageRole, TokenUsage, PricingInfo, ModelCapabilities, 
    HealthStatus, RateLimits
};
use crate::models::circuit_breaker::CircuitBreaker;

pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    base_url: String,
    circuit_breaker: CircuitBreaker,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Result<Self> {
        let base_url = "https://api.openai.com".to_string();
        Self::with_base_url(api_key, base_url)
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::model_provider(format!("Failed to create HTTP client: {}", e)))?;

        let circuit_breaker = CircuitBreaker::new(
            "openai".to_string(),
            5, // failure threshold
            Duration::from_secs(30), // recovery timeout
        );

        Ok(Self {
            client,
            api_key,
            base_url,
            circuit_breaker,
        })
    }

    fn create_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        
        let auth_value = HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|e| Error::model_provider(format!("Invalid API key format: {}", e)))?;
        headers.insert(AUTHORIZATION, auth_value);
        
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        Ok(headers)
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAIMessage> {
        messages.iter().map(|msg| OpenAIMessage {
            role: match msg.role {
                MessageRole::User => "user".to_string(),
                MessageRole::Assistant => "assistant".to_string(),
                MessageRole::System => "system".to_string(),
            },
            content: msg.content.clone(),
        }).collect()
    }

    async fn make_request(&self, request: &ChatRequest) -> Result<OpenAIResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let headers = self.create_headers()?;
        
        let openai_request = OpenAIRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(false),
            user: request.user_id.clone(),
        };

        debug!("Sending request to OpenAI: model={}, messages={}", 
               request.model, request.messages.len());

        let start_time = Instant::now();
        
        let response = self.client
            .post(&url)
            .headers(headers)
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| Error::model_provider(format!("HTTP request failed: {}", e)))?;

        let elapsed = start_time.elapsed();
        debug!("OpenAI request completed in {:?}", elapsed);

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("OpenAI API error: {} - {}", status, error_text);
            return Err(Error::model_provider(format!("API error {}: {}", status, error_text)));
        }

        let openai_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| Error::model_provider(format!("Failed to parse response: {}", e)))?;

        debug!("Received response from OpenAI: id={}", openai_response.id);
        Ok(openai_response)
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse> {
        let response = self.circuit_breaker.call(|| async {
            self.make_request(&request).await
        }).await?;

        let choice = response.choices.into_iter().next()
            .ok_or_else(|| Error::model_provider("No choices in response"))?;

        let usage = response.usage.map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

        Ok(ChatResponse {
            id: response.id,
            request_id: request.id,
            model: response.model,
            content: choice.message.content,
            role: MessageRole::Assistant,
            created_at: Utc::now(),
            usage,
            finish_reason: choice.finish_reason,
            provider_metadata: serde_json::json!({
                "provider": "openai",
                "created": response.created,
                "object": response.object
            }),
        })
    }

    async fn stream_message(&self, request: ChatRequest) -> Result<Box<dyn ChatStream>> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let headers = self.create_headers()?;
        
        let openai_request = OpenAIRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(true), // Enable streaming
            user: request.user_id.clone(),
        };

        debug!("Starting streaming request to OpenAI: model={}, messages={}", 
               request.model, request.messages.len());

        let response = self.client
            .post(&url)
            .headers(headers)
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| Error::model_provider(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(Error::model_provider(format!("OpenAI API error {}: {}", status, error_body)));
        }

        let stream = OpenAIStream::new(response).await?;
        Ok(Box::new(stream))
    }

    fn get_pricing(&self) -> Option<PricingInfo> {
        // Default GPT-4 pricing as of 2024 - this should be configurable/updatable
        Some(PricingInfo {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            input_price_per_1k_tokens: Decimal::from_str("0.03").unwrap(),
            output_price_per_1k_tokens: Decimal::from_str("0.06").unwrap(),
            effective_date: Utc::now(),
        })
    }

    fn get_capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            max_tokens: 4096,
            supports_streaming: true, // Will be implemented in Phase 2
            supports_function_calling: true,
            supports_vision: false, // Model-dependent
            context_window: 128000, // GPT-4 Turbo context window
            supported_formats: vec!["text".to_string()],
        }
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        let start_time = Instant::now();
        
        // Simple health check with a minimal request
        let health_request = ChatRequest {
            id: "health-check".to_string(),
            messages: vec![Message::user("Hello".to_string())],
            model: "gpt-3.5-turbo".to_string(), // Use cheaper model for health checks
            temperature: Some(0.1),
            max_tokens: Some(5),
            stream: false,
            timeout: Some(Duration::from_secs(10)),
            user_id: None,
        };

        match self.make_request(&health_request).await {
            Ok(_) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                Ok(HealthStatus::healthy(response_time))
            }
            Err(e) => {
                let failure_count = self.circuit_breaker.get_failure_count();
                Ok(HealthStatus::unhealthy(e.to_string(), failure_count))
            }
        }
    }

    fn get_rate_limits(&self) -> RateLimits {
        // OpenAI rate limits (approximate, varies by tier)
        RateLimits {
            requests_per_minute: Some(3500),
            tokens_per_minute: Some(90000),
            requests_per_day: Some(10000),
            concurrent_requests: Some(100),
        }
    }

    fn supports_streaming(&self) -> bool {
        true // Will be implemented in Phase 2
    }

    fn get_provider_name(&self) -> &str {
        "openai"
    }
}

// OpenAI API request/response structures
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// Streaming response structures
#[derive(Debug, Deserialize)]
struct OpenAIStreamResponse {
    id: String,
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    content: Option<String>,
}

pub struct OpenAIStream {
    response: reqwest::Response,
    buffer: String,
    finished: bool,
}

impl OpenAIStream {
    pub async fn new(response: reqwest::Response) -> Result<Self> {
        Ok(Self {
            response,
            buffer: String::new(),
            finished: false,
        })
    }

    async fn read_next_line(&mut self) -> Result<Option<String>> {
        if self.finished {
            return Ok(None);
        }

        // Read more data from the response
        while !self.buffer.contains('\n') {
            let chunk = match self.response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => {
                    self.finished = true;
                    if self.buffer.is_empty() {
                        return Ok(None);
                    }
                    let line = self.buffer.clone();
                    self.buffer.clear();
                    return Ok(Some(line));
                }
                Err(e) => {
                    return Err(Error::model_provider(format!("Stream read error: {}", e)));
                }
            };

            match std::str::from_utf8(&chunk) {
                Ok(text) => self.buffer.push_str(text),
                Err(e) => {
                    return Err(Error::model_provider(format!("Invalid UTF-8 in stream: {}", e)));
                }
            }
        }

        if let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();
            Ok(Some(line))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl ChatStream for OpenAIStream {
    async fn next_chunk(&mut self) -> Result<Option<crate::models::provider::StreamChunk>> {
        loop {
            match self.read_next_line().await? {
                Some(line) => {
                    let line = line.trim();
                    
                    // Skip empty lines and comments
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    
                    // Handle server-sent events format
                    if let Some(data) = line.strip_prefix("data: ") {
                        // Check for end of stream
                        if data == "[DONE]" {
                            return Ok(None);
                        }
                        
                        // Parse JSON response
                        match serde_json::from_str::<OpenAIStreamResponse>(data) {
                            Ok(stream_response) => {
                                if let Some(choice) = stream_response.choices.first() {
                                    if let Some(content) = &choice.delta.content {
                                        return Ok(Some(crate::models::provider::StreamChunk {
                                            id: stream_response.id,
                                            delta: content.clone(),
                                            finish_reason: choice.finish_reason.clone(),
                                            usage: stream_response.usage.map(|u| TokenUsage {
                                                input_tokens: u.prompt_tokens,
                                                output_tokens: u.completion_tokens,
                                                total_tokens: u.total_tokens,
                                            }),
                                        }));
                                    } else if choice.finish_reason.is_some() {
                                        // End of generation, return empty chunk with finish reason
                                        return Ok(Some(crate::models::provider::StreamChunk {
                                            id: stream_response.id,
                                            delta: String::new(),
                                            finish_reason: choice.finish_reason.clone(),
                                            usage: stream_response.usage.map(|u| TokenUsage {
                                                input_tokens: u.prompt_tokens,
                                                output_tokens: u.completion_tokens,
                                                total_tokens: u.total_tokens,
                                            }),
                                        }));
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Failed to parse stream response: {} (data: {})", e, data);
                                continue;
                            }
                        }
                    }
                }
                None => return Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAIProvider::new("test-key".to_string());
        assert!(provider.is_ok());
        
        let provider = provider.unwrap();
        assert_eq!(provider.get_provider_name(), "openai");
        assert!(provider.supports_streaming());
        assert!(provider.get_pricing().is_some());
    }

    #[test]
    fn test_message_conversion() {
        let provider = OpenAIProvider::new("test-key".to_string()).unwrap();
        
        let messages = vec![
            Message::system("You are a helpful assistant".to_string()),
            Message::user("Hello".to_string()),
        ];
        
        let converted = provider.convert_messages(&messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
    }

    #[test]
    fn test_capabilities() {
        let provider = OpenAIProvider::new("test-key".to_string()).unwrap();
        let caps = provider.get_capabilities();
        
        assert!(caps.supports_streaming);
        assert!(caps.supports_function_calling);
        assert_eq!(caps.context_window, 128000);
    }
}