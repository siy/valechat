use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{Error, Result};

/// JSON-RPC 2.0 protocol version
pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<JsonValue>,
    pub method: String,
    pub params: Option<JsonValue>,
}

impl JsonRpcRequest {
    pub fn new(method: String, params: Option<JsonValue>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(JsonValue::String(Uuid::new_v4().to_string())),
            method,
            params,
        }
    }

    pub fn notification(method: String, params: Option<JsonValue>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: None, // Notifications have no ID
            method,
            params,
        }
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    pub fn get_id(&self) -> Option<&JsonValue> {
        self.id.as_ref()
    }
}

/// JSON-RPC response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: JsonValue, result: JsonValue) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: JsonValue, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// JSON-RPC error structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

impl JsonRpcError {
    // Standard JSON-RPC error codes
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // MCP-specific error codes
    pub const MCP_TRANSPORT_ERROR: i32 = -32000;
    pub const MCP_TIMEOUT_ERROR: i32 = -32001;
    pub const MCP_VALIDATION_ERROR: i32 = -32002;
    pub const MCP_SANDBOX_VIOLATION: i32 = -32003;
    pub const MCP_RESOURCE_LIMIT: i32 = -32004;

    pub fn parse_error() -> Self {
        Self {
            code: Self::PARSE_ERROR,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            code: Self::INVALID_REQUEST,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: Self::METHOD_NOT_FOUND,
            message: format!("Method not found: {}", method),
            data: None,
        }
    }

    pub fn invalid_params(message: &str) -> Self {
        Self {
            code: Self::INVALID_PARAMS,
            message: format!("Invalid params: {}", message),
            data: None,
        }
    }

    pub fn internal_error(message: &str) -> Self {
        Self {
            code: Self::INTERNAL_ERROR,
            message: format!("Internal error: {}", message),
            data: None,
        }
    }

    pub fn transport_error(message: &str) -> Self {
        Self {
            code: Self::MCP_TRANSPORT_ERROR,
            message: format!("Transport error: {}", message),
            data: None,
        }
    }

    pub fn timeout_error(timeout_ms: u64) -> Self {
        Self {
            code: Self::MCP_TIMEOUT_ERROR,
            message: format!("Operation timed out after {}ms", timeout_ms),
            data: Some(serde_json::json!({ "timeout_ms": timeout_ms })),
        }
    }

    pub fn validation_error(message: &str) -> Self {
        Self {
            code: Self::MCP_VALIDATION_ERROR,
            message: format!("Validation error: {}", message),
            data: None,
        }
    }

    pub fn sandbox_violation(violation: &str) -> Self {
        Self {
            code: Self::MCP_SANDBOX_VIOLATION,
            message: format!("Sandbox violation: {}", violation),
            data: Some(serde_json::json!({ "violation": violation })),
        }
    }

    pub fn resource_limit(resource: &str) -> Self {
        Self {
            code: Self::MCP_RESOURCE_LIMIT,
            message: format!("Resource limit exceeded: {}", resource),
            data: Some(serde_json::json!({ "resource": resource })),
        }
    }
}

/// Protocol message that can be either a request or response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProtocolMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
}

impl ProtocolMessage {
    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            Error::mcp(format!("Failed to parse protocol message: {}", e))
        })
    }

    pub fn serialize(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| {
            Error::mcp(format!("Failed to serialize protocol message: {}", e))
        })
    }

    pub fn is_request(&self) -> bool {
        matches!(self, ProtocolMessage::Request(_))
    }

    pub fn is_response(&self) -> bool {
        matches!(self, ProtocolMessage::Response(_))
    }

    pub fn get_id(&self) -> Option<&JsonValue> {
        match self {
            ProtocolMessage::Request(req) => req.get_id(),
            ProtocolMessage::Response(resp) => Some(&resp.id),
        }
    }
}

/// Protocol handler for managing JSON-RPC communication
pub struct ProtocolHandler {
    pending_requests: HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>,
}

impl ProtocolHandler {
    pub fn new() -> Self {
        Self {
            pending_requests: HashMap::new(),
        }
    }

    pub fn create_request(&self, method: String, params: Option<JsonValue>) -> JsonRpcRequest {
        JsonRpcRequest::new(method, params)
    }

    pub fn create_notification(&self, method: String, params: Option<JsonValue>) -> JsonRpcRequest {
        JsonRpcRequest::notification(method, params)
    }

    pub fn register_pending_request(
        &mut self,
        id: &JsonValue,
        sender: tokio::sync::oneshot::Sender<JsonRpcResponse>,
    ) -> Result<()> {
        let id_str = match id {
            JsonValue::String(s) => s.clone(),
            JsonValue::Number(n) => n.to_string(),
            _ => return Err(Error::mcp("Invalid request ID format".to_string())),
        };

        self.pending_requests.insert(id_str, sender);
        Ok(())
    }

    pub fn handle_response(&mut self, response: JsonRpcResponse) -> Result<()> {
        let id_str = match &response.id {
            JsonValue::String(s) => s.clone(),
            JsonValue::Number(n) => n.to_string(),
            _ => return Err(Error::mcp("Invalid response ID format".to_string())),
        };

        if let Some(sender) = self.pending_requests.remove(&id_str) {
            sender.send(response).map_err(|_| {
                Error::mcp("Failed to send response to waiting request".to_string())
            })?;
        }

        Ok(())
    }

    pub fn create_error_response(&self, id: JsonValue, error: JsonRpcError) -> JsonRpcResponse {
        JsonRpcResponse::error(id, error)
    }

    pub fn create_success_response(&self, id: JsonValue, result: JsonValue) -> JsonRpcResponse {
        JsonRpcResponse::success(id, result)
    }
}

impl Default for ProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_rpc_request_creation() {
        let request = JsonRpcRequest::new(
            "test_method".to_string(),
            Some(json!({"param": "value"})),
        );

        assert_eq!(request.jsonrpc, JSONRPC_VERSION);
        assert_eq!(request.method, "test_method");
        assert!(request.id.is_some());
        assert!(!request.is_notification());
    }

    #[test]
    fn test_json_rpc_notification() {
        let notification = JsonRpcRequest::notification(
            "test_notification".to_string(),
            Some(json!({"param": "value"})),
        );

        assert_eq!(notification.jsonrpc, JSONRPC_VERSION);
        assert_eq!(notification.method, "test_notification");
        assert!(notification.id.is_none());
        assert!(notification.is_notification());
    }

    #[test]
    fn test_json_rpc_response_success() {
        let response = JsonRpcResponse::success(
            json!("test-id"),
            json!({"result": "success"}),
        );

        assert_eq!(response.jsonrpc, JSONRPC_VERSION);
        assert_eq!(response.id, json!("test-id"));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert!(response.is_success());
        assert!(!response.is_error());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let error = JsonRpcError::method_not_found("unknown_method");
        let response = JsonRpcResponse::error(json!("test-id"), error);

        assert_eq!(response.jsonrpc, JSONRPC_VERSION);
        assert_eq!(response.id, json!("test-id"));
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert!(!response.is_success());
        assert!(response.is_error());
    }

    #[test]
    fn test_protocol_message_serialization() {
        let request = JsonRpcRequest::new(
            "test_method".to_string(),
            Some(json!({"param": "value"})),
        );
        let message = ProtocolMessage::Request(request);

        let serialized = message.serialize().unwrap();
        let deserialized = ProtocolMessage::parse(&serialized).unwrap();

        assert!(deserialized.is_request());
        match deserialized {
            ProtocolMessage::Request(req) => {
                assert_eq!(req.method, "test_method");
            }
            _ => panic!("Expected request"),
        }
    }

    #[test]
    fn test_json_rpc_error_codes() {
        assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
        assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
        assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
        assert_eq!(JsonRpcError::INVALID_PARAMS, -32602);
        assert_eq!(JsonRpcError::INTERNAL_ERROR, -32603);
    }

    #[test]
    fn test_mcp_specific_errors() {
        let transport_error = JsonRpcError::transport_error("Connection failed");
        assert_eq!(transport_error.code, JsonRpcError::MCP_TRANSPORT_ERROR);
        assert!(transport_error.message.contains("Connection failed"));

        let timeout_error = JsonRpcError::timeout_error(5000);
        assert_eq!(timeout_error.code, JsonRpcError::MCP_TIMEOUT_ERROR);
        assert!(timeout_error.data.is_some());

        let sandbox_error = JsonRpcError::sandbox_violation("File access denied");
        assert_eq!(sandbox_error.code, JsonRpcError::MCP_SANDBOX_VIOLATION);
        assert!(sandbox_error.data.is_some());
    }

    #[test]
    fn test_protocol_handler() {
        let handler = ProtocolHandler::new();
        
        let request = handler.create_request(
            "test_method".to_string(),
            Some(json!({"param": "value"})),
        );
        assert_eq!(request.method, "test_method");
        assert!(!request.is_notification());

        let notification = handler.create_notification(
            "test_notification".to_string(),
            None,
        );
        assert_eq!(notification.method, "test_notification");
        assert!(notification.is_notification());
    }
}