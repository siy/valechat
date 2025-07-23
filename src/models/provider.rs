use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::error::Result;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn stream_message(&self, request: ChatRequest) -> Result<Box<dyn ChatStream>>;
    fn get_pricing(&self) -> Option<PricingInfo>;
    fn get_capabilities(&self) -> ModelCapabilities;
    async fn health_check(&self) -> Result<HealthStatus>;
    fn get_rate_limits(&self) -> RateLimits;
    fn supports_streaming(&self) -> bool;
    fn get_provider_name(&self) -> &str;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub id: String,
    pub messages: Vec<Message>,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub timeout: Option<Duration>,
    pub user_id: Option<String>,
}

impl ChatRequest {
    pub fn new(messages: Vec<Message>, model: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            messages,
            model,
            temperature: None,
            max_tokens: None,
            stream: false,
            timeout: Some(Duration::from_secs(60)),
            user_id: None,
        }
    }

    pub fn with_streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub request_id: String,
    pub model: String,
    pub content: String,
    pub role: MessageRole,
    pub created_at: DateTime<Utc>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
    pub provider_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

impl Message {
    pub fn new(role: MessageRole, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    pub fn user(content: String) -> Self {
        Self::new(MessageRole::User, content)
    }

    pub fn assistant(content: String) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    pub fn system(content: String) -> Self {
        Self::new(MessageRole::System, content)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        }
    }
}

#[async_trait]
pub trait ChatStream: Send {
    async fn next_chunk(&mut self) -> Result<Option<StreamChunk>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub delta: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingInfo {
    pub provider: String,
    pub model: String,
    pub input_price_per_1k_tokens: Decimal,
    pub output_price_per_1k_tokens: Decimal,
    pub effective_date: DateTime<Utc>,
}

impl PricingInfo {
    pub fn calculate_cost(&self, usage: &TokenUsage) -> Decimal {
        let input_cost = Decimal::from(usage.input_tokens) * self.input_price_per_1k_tokens / Decimal::from(1000);
        let output_cost = Decimal::from(usage.output_tokens) * self.output_price_per_1k_tokens / Decimal::from(1000);
        input_cost + output_cost
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub max_tokens: u32,
    pub supports_streaming: bool,
    pub supports_function_calling: bool,
    pub supports_vision: bool,
    pub context_window: u32,
    pub supported_formats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub last_check: DateTime<Utc>,
    pub response_time_ms: Option<u64>,
    pub error_message: Option<String>,
    pub consecutive_failures: u32,
}

impl HealthStatus {
    pub fn healthy(response_time_ms: u64) -> Self {
        Self {
            is_healthy: true,
            last_check: Utc::now(),
            response_time_ms: Some(response_time_ms),
            error_message: None,
            consecutive_failures: 0,
        }
    }

    pub fn unhealthy(error: String, consecutive_failures: u32) -> Self {
        Self {
            is_healthy: false,
            last_check: Utc::now(),
            response_time_ms: None,
            error_message: Some(error),
            consecutive_failures,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub requests_per_minute: Option<u32>,
    pub tokens_per_minute: Option<u32>,
    pub requests_per_day: Option<u32>,
    pub concurrent_requests: Option<u32>,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            requests_per_minute: Some(60),
            tokens_per_minute: Some(10000),
            requests_per_day: Some(1000),
            concurrent_requests: Some(10),
        }
    }
}