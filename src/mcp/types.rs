use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use chrono::{DateTime, Utc};

use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse};

/// MCP protocol version
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProtocolVersion {
    #[serde(rename = "2024-11-05")]
    V2024_11_05,
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::V2024_11_05
    }
}

/// Server capabilities structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    #[serde(default)]
    pub subscribe: bool,
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingCapability {
    #[serde(default)]
    pub level: String, // "debug", "info", "notice", "warning", "error", "critical", "alert", "emergency"
}

/// Client capabilities structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<HashMap<String, JsonValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingCapability {}

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<JsonValue>, // JSON Schema for input validation
}

/// Tool execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<JsonValue>,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<Content>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Content structure for tool results and messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { 
        data: String, // base64 encoded
        mime_type: String 
    },
    #[serde(rename = "resource")]
    Resource { 
        resource: ResourceReference 
    },
}

/// Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Resource reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReference {
    pub uri: String,
}

/// Resource contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContents {
    pub uri: String,
    pub mime_type: String,
    pub content: Vec<Content>,
}

/// Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Prompt argument definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// Prompt message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: MessageRole,
    pub content: Content,
}

/// Message role for prompts and chat
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Initialization request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersion,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: Implementation,
}

/// Initialization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: ProtocolVersion,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: Implementation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Implementation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

/// Log level for MCP logging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: LogLevel,
    pub data: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
}

/// MCP message types
#[derive(Debug, Clone)]
pub enum MCPMessage {
    Request(MCPRequest),
    Response(MCPResponse),
    Notification(MCPNotification),
}

/// MCP request types
#[derive(Debug, Clone)]
pub enum MCPRequest {
    Initialize(InitializeParams),
    ListTools,
    CallTool(ToolCall),
    ListResources,
    ReadResource(ResourceReference),
    ListPrompts,
    GetPrompt { name: String, arguments: Option<HashMap<String, String>> },
}

/// MCP response types
#[derive(Debug, Clone)]
pub enum MCPResponse {
    Initialize(InitializeResult),
    ListTools(Vec<Tool>),
    CallTool(ToolResult),
    ListResources(Vec<Resource>),
    ReadResource(ResourceContents),
    ListPrompts(Vec<Prompt>),
    GetPrompt(Vec<PromptMessage>),
}

/// MCP notification types
#[derive(Debug, Clone)]
pub enum MCPNotification {
    Initialized,
    ToolsListChanged,
    ResourcesListChanged,
    PromptsListChanged,
    LogMessage(LogEntry),
    Progress { 
        progress_token: String, 
        progress: f32, 
        total: Option<f32> 
    },
}

impl MCPMessage {
    pub fn from_json_rpc(message: JsonRpcRequest) -> Result<Self, crate::error::Error> {
        match message.method.as_str() {
            "initialize" => {
                let params: InitializeParams = serde_json::from_value(
                    message.params.unwrap_or(JsonValue::Null)
                ).map_err(|e| crate::error::Error::mcp(format!("Invalid initialize params: {}", e)))?;
                Ok(MCPMessage::Request(MCPRequest::Initialize(params)))
            }
            "tools/list" => Ok(MCPMessage::Request(MCPRequest::ListTools)),
            "tools/call" => {
                let tool_call: ToolCall = serde_json::from_value(
                    message.params.unwrap_or(JsonValue::Null)
                ).map_err(|e| crate::error::Error::mcp(format!("Invalid tool call params: {}", e)))?;
                Ok(MCPMessage::Request(MCPRequest::CallTool(tool_call)))
            }
            "resources/list" => Ok(MCPMessage::Request(MCPRequest::ListResources)),
            "resources/read" => {
                let resource_ref: ResourceReference = serde_json::from_value(
                    message.params.unwrap_or(JsonValue::Null)
                ).map_err(|e| crate::error::Error::mcp(format!("Invalid resource reference: {}", e)))?;
                Ok(MCPMessage::Request(MCPRequest::ReadResource(resource_ref)))
            }
            "prompts/list" => Ok(MCPMessage::Request(MCPRequest::ListPrompts)),
            "prompts/get" => {
                let params: serde_json::Map<String, JsonValue> = serde_json::from_value(
                    message.params.unwrap_or(JsonValue::Null)
                ).map_err(|e| crate::error::Error::mcp(format!("Invalid prompt params: {}", e)))?;
                
                let name = params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::error::Error::mcp("Missing prompt name".to_string()))?
                    .to_string();
                
                let arguments = params.get("arguments")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                
                Ok(MCPMessage::Request(MCPRequest::GetPrompt { name, arguments }))
            }
            // Notifications
            "notifications/initialized" => Ok(MCPMessage::Notification(MCPNotification::Initialized)),
            "notifications/tools/list_changed" => Ok(MCPMessage::Notification(MCPNotification::ToolsListChanged)),
            "notifications/resources/list_changed" => Ok(MCPMessage::Notification(MCPNotification::ResourcesListChanged)),
            "notifications/prompts/list_changed" => Ok(MCPMessage::Notification(MCPNotification::PromptsListChanged)),
            "notifications/message" => {
                let log_entry: LogEntry = serde_json::from_value(
                    message.params.unwrap_or(JsonValue::Null)
                ).map_err(|e| crate::error::Error::mcp(format!("Invalid log entry: {}", e)))?;
                Ok(MCPMessage::Notification(MCPNotification::LogMessage(log_entry)))
            }
            _ => Err(crate::error::Error::mcp(format!("Unknown MCP method: {}", message.method))),
        }
    }

    pub fn to_json_rpc(&self) -> Result<JsonRpcRequest, crate::error::Error> {
        match self {
            MCPMessage::Request(req) => req.to_json_rpc(),
            MCPMessage::Notification(notif) => notif.to_json_rpc(),
            MCPMessage::Response(_) => Err(crate::error::Error::mcp("Responses cannot be converted to requests".to_string())),
        }
    }
}

impl MCPRequest {
    pub fn to_json_rpc(&self) -> Result<JsonRpcRequest, crate::error::Error> {
        let (method, params) = match self {
            MCPRequest::Initialize(params) => {
                ("initialize", Some(serde_json::to_value(params)?))
            }
            MCPRequest::ListTools => ("tools/list", None),
            MCPRequest::CallTool(tool_call) => {
                ("tools/call", Some(serde_json::to_value(tool_call)?))
            }
            MCPRequest::ListResources => ("resources/list", None),
            MCPRequest::ReadResource(resource_ref) => {
                ("resources/read", Some(serde_json::to_value(resource_ref)?))
            }
            MCPRequest::ListPrompts => ("prompts/list", None),
            MCPRequest::GetPrompt { name, arguments } => {
                let mut params = serde_json::Map::new();
                params.insert("name".to_string(), JsonValue::String(name.clone()));
                if let Some(args) = arguments {
                    params.insert("arguments".to_string(), serde_json::to_value(args)?);
                }
                ("prompts/get", Some(JsonValue::Object(params)))
            }
        };

        Ok(JsonRpcRequest::new(method.to_string(), params))
    }
}

impl MCPNotification {
    pub fn to_json_rpc(&self) -> Result<JsonRpcRequest, crate::error::Error> {
        let (method, params) = match self {
            MCPNotification::Initialized => ("notifications/initialized", None),
            MCPNotification::ToolsListChanged => ("notifications/tools/list_changed", None),
            MCPNotification::ResourcesListChanged => ("notifications/resources/list_changed", None),
            MCPNotification::PromptsListChanged => ("notifications/prompts/list_changed", None),
            MCPNotification::LogMessage(log_entry) => {
                ("notifications/message", Some(serde_json::to_value(log_entry)?))
            }
            MCPNotification::Progress { progress_token, progress, total } => {
                let mut params = serde_json::Map::new();
                params.insert("progressToken".to_string(), JsonValue::String(progress_token.clone()));
                params.insert("progress".to_string(), JsonValue::Number(serde_json::Number::from_f64(*progress as f64).unwrap()));
                if let Some(total_val) = total {
                    params.insert("total".to_string(), JsonValue::Number(serde_json::Number::from_f64(*total_val as f64).unwrap()));
                }
                ("notifications/progress", Some(JsonValue::Object(params)))
            }
        };

        Ok(JsonRpcRequest::notification(method.to_string(), params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_serialization() {
        let tool = Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "param": { "type": "string" }
                }
            })),
        };

        let serialized = serde_json::to_string(&tool).unwrap();
        let deserialized: Tool = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.name, "test_tool");
        assert_eq!(deserialized.description, "A test tool");
        assert!(deserialized.input_schema.is_some());
    }

    #[test]
    fn test_initialize_params() {
        let params = InitializeParams {
            protocol_version: ProtocolVersion::V2024_11_05,
            capabilities: ClientCapabilities {
                sampling: Some(SamplingCapability {}),
                experimental: None,
            },
            client_info: Implementation {
                name: "ValeChat".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        let serialized = serde_json::to_string(&params).unwrap();
        let deserialized: InitializeParams = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.protocol_version, ProtocolVersion::V2024_11_05);
        assert_eq!(deserialized.client_info.name, "ValeChat");
    }

    #[test]
    fn test_content_types() {
        let text_content = Content::Text {
            text: "Hello, world!".to_string(),
        };

        let image_content = Content::Image {
            data: "base64encodeddata".to_string(),
            mime_type: "image/png".to_string(),
        };

        let resource_content = Content::Resource {
            resource: ResourceReference {
                uri: "file://test.txt".to_string(),
            },
        };

        let contents = vec![text_content, image_content, resource_content];
        let serialized = serde_json::to_string(&contents).unwrap();
        let deserialized: Vec<Content> = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.len(), 3);
    }

    #[test]
    fn test_mcp_message_conversion() {
        let tool_call = ToolCall {
            name: "test_tool".to_string(),
            arguments: Some(json!({"param": "value"})),
        };

        let mcp_request = MCPRequest::CallTool(tool_call);
        let json_rpc = mcp_request.to_json_rpc().unwrap();

        assert_eq!(json_rpc.method, "tools/call");
        assert!(json_rpc.params.is_some());
        assert!(json_rpc.id.is_some());
    }

    #[test]
    fn test_notification_conversion() {
        let notification = MCPNotification::ToolsListChanged;
        let json_rpc = notification.to_json_rpc().unwrap();

        assert_eq!(json_rpc.method, "notifications/tools/list_changed");
        assert!(json_rpc.id.is_none()); // Notifications don't have IDs
    }
}