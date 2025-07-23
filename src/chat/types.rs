use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error;

/// Represents a single chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
    pub tool_invocations: Vec<ToolInvocation>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ChatMessage {
    pub fn new(session_id: String, role: MessageRole, content: MessageContent) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            timestamp: Utc::now(),
            tool_invocations: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_tool_invocations(mut self, invocations: Vec<ToolInvocation>) -> Self {
        self.tool_invocations = invocations;
        self
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn add_tool_invocation(&mut self, invocation: ToolInvocation) {
        self.tool_invocations.push(invocation);
    }
}

/// Role of the message sender
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    MultiModal {
        text: Option<String>,
        attachments: Vec<MessageAttachment>,
    },
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
        call_id: String,
    },
    ToolResult {
        call_id: String,
        result: serde_json::Value,
        is_error: bool,
    },
}

impl MessageContent {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }

    pub fn tool_call(tool_name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self::ToolCall {
            tool_name: tool_name.into(),
            arguments,
            call_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn tool_result(call_id: impl Into<String>, result: serde_json::Value) -> Self {
        Self::ToolResult {
            call_id: call_id.into(),
            result,
            is_error: false,
        }
    }

    pub fn tool_error(call_id: impl Into<String>, error: serde_json::Value) -> Self {
        Self::ToolResult {
            call_id: call_id.into(),
            result: error,
            is_error: true,
        }
    }

    pub fn get_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::MultiModal { text, .. } => text.as_deref(),
            _ => None,
        }
    }
}

/// Attachment in a multimodal message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachment {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub data: AttachmentData,
}

/// Data for message attachments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttachmentData {
    Base64(String),
    FilePath(String),
    Url(String),
}

/// Information about a tool invocation within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub id: String,
    pub tool_name: String,
    pub server_name: String,
    pub arguments: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub timestamp: DateTime<Utc>,
}

impl ToolInvocation {
    pub fn new(tool_name: String, server_name: String, arguments: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            tool_name,
            server_name,
            arguments,
            result: None,
            error: None,
            duration_ms: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_result(mut self, result: serde_json::Value, duration_ms: u64) -> Self {
        self.result = Some(result);
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_error(mut self, error: String, duration_ms: u64) -> Self {
        self.error = Some(error);
        self.duration_ms = Some(duration_ms);
        self
    }
}

/// Represents a chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub model_provider: String,
    pub model_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: SessionStatus,
    pub metrics: SessionMetrics,
    pub system_prompt: Option<String>,
    pub settings: SessionSettings,
}

impl ChatSession {
    pub fn new(title: impl Into<String>, model_provider: impl Into<String>, model_name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.into(),
            model_provider: model_provider.into(),
            model_name: model_name.into(),
            created_at: now,
            updated_at: now,
            status: SessionStatus::Active,
            metrics: SessionMetrics::default(),
            system_prompt: None,
            settings: SessionSettings::default(),
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn update_activity(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn increment_message_count(&mut self) {
        self.metrics.message_count += 1;
        self.update_activity();
    }

    pub fn add_tool_usage(&mut self, tool_name: &str, duration_ms: u64) {
        self.metrics.total_tool_calls += 1;
        self.metrics.total_tool_time_ms += duration_ms;
        *self.metrics.tools_used.entry(tool_name.to_string()).or_insert(0) += 1;
        self.update_activity();
    }

    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
        self.update_activity();
    }
}

/// Status of a chat session
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Paused,
    Archived,
    Error(String),
}

/// Metrics for a chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub message_count: u64,
    pub total_tokens_used: u64,
    pub total_cost: f64,
    pub total_tool_calls: u64,
    pub total_tool_time_ms: u64,
    pub tools_used: HashMap<String, u64>,
    pub average_response_time_ms: f64,
}

impl Default for SessionMetrics {
    fn default() -> Self {
        Self {
            message_count: 0,
            total_tokens_used: 0,
            total_cost: 0.0,
            total_tool_calls: 0,
            total_tool_time_ms: 0,
            tools_used: HashMap::new(),
            average_response_time_ms: 0.0,
        }
    }
}

/// Settings for a chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSettings {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub enable_tools: bool,
    pub allowed_servers: Vec<String>,
    pub tool_timeout_ms: u64,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            temperature: None,
            max_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            enable_tools: true,
            allowed_servers: Vec::new(), // Empty means all servers allowed
            tool_timeout_ms: 30000, // 30 second default timeout
        }
    }
}

/// Response from the chat service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: ChatMessage,
    pub usage: ResponseUsage,
    pub tool_calls: Vec<ToolInvocation>,
    pub processing_time_ms: u64,
}

impl ChatResponse {
    pub fn new(message: ChatMessage, usage: ResponseUsage, processing_time_ms: u64) -> Self {
        Self {
            message,
            usage,
            tool_calls: Vec::new(),
            processing_time_ms,
        }
    }

    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolInvocation>) -> Self {
        self.tool_calls = tool_calls;
        self
    }
}

/// Usage information for a response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub estimated_cost: f64,
}

impl ResponseUsage {
    pub fn new(input_tokens: u32, output_tokens: u32, estimated_cost: f64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            estimated_cost,
        }
    }
}

impl Default for ResponseUsage {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            estimated_cost: 0.0,
        }
    }
}

/// Errors specific to chat operations
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("Session not found: {session_id}")]
    SessionNotFound { session_id: String },

    #[error("Message not found: {message_id}")]
    MessageNotFound { message_id: String },

    #[error("Tool call failed: {tool_name} on server {server_name}: {error}")]
    ToolCallFailed {
        tool_name: String,
        server_name: String,
        error: String,
    },

    #[error("Model provider not available: {provider}")]
    ModelProviderUnavailable { provider: String },

    #[error("Invalid message format: {reason}")]
    InvalidMessageFormat { reason: String },

    #[error("Rate limit exceeded for session: {session_id}")]
    RateLimitExceeded { session_id: String },

    #[error("Tool timeout: {tool_name} took longer than {timeout_ms}ms")]
    ToolTimeout { tool_name: String, timeout_ms: u64 },

    #[error("Server not ready: {server_name}")]
    ServerNotReady { server_name: String },
}

impl From<ChatError> for Error {
    fn from(err: ChatError) -> Self {
        Error::Chat(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_chat_message_creation() {
        let session_id = "test_session".to_string();
        let message = ChatMessage::new(
            session_id.clone(),
            MessageRole::User,
            MessageContent::text("Hello, world!"),
        );

        assert_eq!(message.session_id, session_id);
        assert_eq!(message.role, MessageRole::User);
        assert!(message.tool_invocations.is_empty());
        assert!(message.metadata.is_empty());
    }

    #[test]
    fn test_message_content_types() {
        let text_content = MessageContent::text("Hello");
        assert_eq!(text_content.get_text(), Some("Hello"));

        let tool_call = MessageContent::tool_call("test_tool", json!({"key": "value"}));
        match tool_call {
            MessageContent::ToolCall { tool_name, .. } => {
                assert_eq!(tool_name, "test_tool");
            }
            _ => panic!("Expected ToolCall variant"),
        }

        let tool_result = MessageContent::tool_result("call_123", json!({"result": "success"}));
        match tool_result {
            MessageContent::ToolResult { call_id, is_error, .. } => {
                assert_eq!(call_id, "call_123");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult variant"),
        }
    }

    #[test]
    fn test_tool_invocation() {
        let invocation = ToolInvocation::new(
            "test_tool".to_string(),
            "test_server".to_string(),
            json!({"arg": "value"}),
        );

        assert_eq!(invocation.tool_name, "test_tool");
        assert_eq!(invocation.server_name, "test_server");
        assert!(invocation.result.is_none());
        assert!(invocation.error.is_none());

        let invocation_with_result = invocation.with_result(json!({"status": "ok"}), 100);
        assert!(invocation_with_result.result.is_some());
        assert_eq!(invocation_with_result.duration_ms, Some(100));
    }

    #[test]
    fn test_chat_session() {
        let mut session = ChatSession::new("Test Chat", "openai", "gpt-4")
            .with_system_prompt("You are a helpful assistant");

        assert_eq!(session.title, "Test Chat");
        assert_eq!(session.model_provider, "openai");
        assert_eq!(session.model_name, "gpt-4");
        assert!(session.system_prompt.is_some());
        assert_eq!(session.status, SessionStatus::Active);

        session.increment_message_count();
        assert_eq!(session.metrics.message_count, 1);

        session.add_tool_usage("test_tool", 150);
        assert_eq!(session.metrics.total_tool_calls, 1);
        assert_eq!(session.metrics.total_tool_time_ms, 150);
        assert_eq!(session.metrics.tools_used.get("test_tool"), Some(&1));
    }

    #[test]
    fn test_response_usage() {
        let usage = ResponseUsage::new(100, 50, 0.002);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.estimated_cost, 0.002);
    }

    #[test]
    fn test_session_settings_default() {
        let settings = SessionSettings::default();
        assert!(settings.enable_tools);
        assert!(settings.allowed_servers.is_empty());
        assert_eq!(settings.tool_timeout_ms, 30000);
    }

    #[test]
    fn test_chat_error_variants() {
        let error = ChatError::SessionNotFound {
            session_id: "test_123".to_string(),
        };
        assert!(error.to_string().contains("test_123"));

        let tool_error = ChatError::ToolCallFailed {
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            error: "Connection failed".to_string(),
        };
        assert!(tool_error.to_string().contains("test_tool"));
        assert!(tool_error.to_string().contains("test_server"));
    }
}