use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::error::{Error, Result};
use crate::mcp::protocol::{ProtocolHandler, JsonRpcRequest, JsonRpcResponse};

/// Simple pending request tracking
#[derive(Debug)]
struct PendingRequest {
    timestamp: Instant,
}
use crate::mcp::server_manager::{MCPServerManager, ServerState};
use crate::mcp::types::{Tool, ToolResult, Resource, Prompt, Content, ToolCall};
use crate::mcp::validation::{InputValidator, ValidationConfig};

/// MCP client for communicating with servers and executing tools
pub struct MCPClient {
    server_manager: Arc<Mutex<MCPServerManager>>,
    protocol_handler: ProtocolHandler,
    request_timeout: Duration,
    pending_requests: Arc<RwLock<HashMap<String, PendingRequest>>>,
    validator: Arc<Mutex<InputValidator>>,
}


/// Configuration for the MCP client
#[derive(Debug, Clone)]
pub struct MCPClientConfig {
    pub request_timeout: Duration,
    pub max_concurrent_requests: usize,
    pub validation_config: ValidationConfig,
}

impl Default for MCPClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            max_concurrent_requests: 100,
            validation_config: ValidationConfig::default(),
        }
    }
}

impl MCPClient {
    /// Create a new MCP client
    pub fn new(server_manager: MCPServerManager, config: MCPClientConfig) -> Result<Self> {
        let validator = InputValidator::new(config.validation_config)?;
        
        Ok(Self {
            server_manager: Arc::new(Mutex::new(server_manager)),
            protocol_handler: ProtocolHandler::new(),
            request_timeout: config.request_timeout,
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            validator: Arc::new(Mutex::new(validator)),
        })
    }

    /// List all available tools from all servers
    pub async fn list_tools(&self) -> Result<HashMap<String, Vec<Tool>>> {
        debug!("Listing tools from all servers");
        
        let mut all_tools = HashMap::new();
        let server_manager = self.server_manager.lock().await;
        let server_status = server_manager.get_server_status().await;
        
        for (server_name, (state, _health)) in server_status {
            if state == ServerState::Ready {
                match self.list_tools_from_server(&server_name).await {
                    Ok(tools) => {
                        debug!("Found {} tools from server: {}", tools.len(), server_name);
                        all_tools.insert(server_name, tools);
                    }
                    Err(e) => {
                        warn!("Failed to list tools from server {}: {}", server_name, e);
                        all_tools.insert(server_name, Vec::new());
                    }
                }
            }
        }
        
        Ok(all_tools)
    }

    /// List tools from a specific server
    pub async fn list_tools_from_server(&self, server_name: &str) -> Result<Vec<Tool>> {
        debug!("Listing tools from server: {}", server_name);
        
        let request = self.protocol_handler.create_request(
            "tools/list".to_string(),
            None,
        );
        
        let response = self.send_request_to_server(server_name, request).await?;
        
        if let Some(result) = response.result {
            if let Some(tools_array) = result.get("tools") {
                let tools: Vec<Tool> = serde_json::from_value(tools_array.clone())
                    .map_err(|e| Error::mcp(format!("Failed to parse tools from server {}: {}", server_name, e)))?;
                
                debug!("Successfully retrieved {} tools from server: {}", tools.len(), server_name);
                Ok(tools)
            } else {
                Ok(Vec::new())
            }
        } else if let Some(error) = response.error {
            Err(Error::mcp(format!("Server {} returned error: {}", server_name, error.message)))
        } else {
            Err(Error::mcp(format!("Invalid response from server: {}", server_name)))
        }
    }

    /// Execute a tool on a specific server
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        client_id: Option<&str>,
    ) -> Result<ToolResult> {
        info!("Calling tool '{}' on server '{}' with client_id: {:?}", tool_name, server_name, client_id);
        
        // Validate input parameters
        {
            let mut validator = self.validator.lock().await;
            validator.validate_tool_input(tool_name, &arguments, client_id, None)?;
        }
        
        let tool_call = ToolCall {
            name: tool_name.to_string(),
            arguments: Some(arguments),
        };
        
        let request = self.protocol_handler.create_request(
            "tools/call".to_string(),
            Some(serde_json::to_value(&tool_call)?),
        );
        
        let response = self.send_request_to_server(server_name, request).await?;
        
        if let Some(result) = response.result {
            let tool_result: ToolResult = serde_json::from_value(result)
                .map_err(|e| Error::mcp(format!("Failed to parse tool result: {}", e)))?;
            
            info!("Tool '{}' executed successfully on server '{}'", tool_name, server_name);
            Ok(tool_result)
        } else if let Some(error) = response.error {
            Err(Error::mcp(format!("Tool execution failed: {}", error.message)))
        } else {
            Err(Error::mcp("Invalid tool execution response".to_string()))
        }
    }

    /// List all available resources from all servers
    pub async fn list_resources(&self) -> Result<HashMap<String, Vec<Resource>>> {
        debug!("Listing resources from all servers");
        
        let mut all_resources = HashMap::new();
        let server_manager = self.server_manager.lock().await;
        let server_status = server_manager.get_server_status().await;
        
        for (server_name, (state, _health)) in server_status {
            if state == ServerState::Ready {
                match self.list_resources_from_server(&server_name).await {
                    Ok(resources) => {
                        debug!("Found {} resources from server: {}", resources.len(), server_name);
                        all_resources.insert(server_name, resources);
                    }
                    Err(e) => {
                        warn!("Failed to list resources from server {}: {}", server_name, e);
                        all_resources.insert(server_name, Vec::new());
                    }
                }
            }
        }
        
        Ok(all_resources)
    }

    /// List resources from a specific server
    pub async fn list_resources_from_server(&self, server_name: &str) -> Result<Vec<Resource>> {
        debug!("Listing resources from server: {}", server_name);
        
        let request = self.protocol_handler.create_request(
            "resources/list".to_string(),
            None,
        );
        
        let response = self.send_request_to_server(server_name, request).await?;
        
        if let Some(result) = response.result {
            if let Some(resources_array) = result.get("resources") {
                let resources: Vec<Resource> = serde_json::from_value(resources_array.clone())
                    .map_err(|e| Error::mcp(format!("Failed to parse resources from server {}: {}", server_name, e)))?;
                
                debug!("Successfully retrieved {} resources from server: {}", resources.len(), server_name);
                Ok(resources)
            } else {
                Ok(Vec::new())
            }
        } else if let Some(error) = response.error {
            Err(Error::mcp(format!("Server {} returned error: {}", server_name, error.message)))
        } else {
            Err(Error::mcp(format!("Invalid response from server: {}", server_name)))
        }
    }

    /// Read a resource from a specific server
    pub async fn read_resource(
        &self,
        server_name: &str,
        resource_uri: &str,
    ) -> Result<Vec<Content>> {
        debug!("Reading resource '{}' from server: {}", resource_uri, server_name);
        
        let params = serde_json::json!({
            "uri": resource_uri
        });
        
        let request = self.protocol_handler.create_request(
            "resources/read".to_string(),
            Some(params),
        );
        
        let response = self.send_request_to_server(server_name, request).await?;
        
        if let Some(result) = response.result {
            if let Some(contents_array) = result.get("contents") {
                let contents: Vec<Content> = serde_json::from_value(contents_array.clone())
                    .map_err(|e| Error::mcp(format!("Failed to parse resource contents: {}", e)))?;
                
                debug!("Successfully read resource '{}' from server: {}", resource_uri, server_name);
                Ok(contents)
            } else {
                Ok(Vec::new())
            }
        } else if let Some(error) = response.error {
            Err(Error::mcp(format!("Resource read failed: {}", error.message)))
        } else {
            Err(Error::mcp("Invalid resource read response".to_string()))
        }
    }

    /// List all available prompts from all servers
    pub async fn list_prompts(&self) -> Result<HashMap<String, Vec<Prompt>>> {
        debug!("Listing prompts from all servers");
        
        let mut all_prompts = HashMap::new();
        let server_manager = self.server_manager.lock().await;
        let server_status = server_manager.get_server_status().await;
        
        for (server_name, (state, _health)) in server_status {
            if state == ServerState::Ready {
                match self.list_prompts_from_server(&server_name).await {
                    Ok(prompts) => {
                        debug!("Found {} prompts from server: {}", prompts.len(), server_name);
                        all_prompts.insert(server_name, prompts);
                    }
                    Err(e) => {
                        warn!("Failed to list prompts from server {}: {}", server_name, e);
                        all_prompts.insert(server_name, Vec::new());
                    }
                }
            }
        }
        
        Ok(all_prompts)
    }

    /// List prompts from a specific server
    pub async fn list_prompts_from_server(&self, server_name: &str) -> Result<Vec<Prompt>> {
        debug!("Listing prompts from server: {}", server_name);
        
        let request = self.protocol_handler.create_request(
            "prompts/list".to_string(),
            None,
        );
        
        let response = self.send_request_to_server(server_name, request).await?;
        
        if let Some(result) = response.result {
            if let Some(prompts_array) = result.get("prompts") {
                let prompts: Vec<Prompt> = serde_json::from_value(prompts_array.clone())
                    .map_err(|e| Error::mcp(format!("Failed to parse prompts from server {}: {}", server_name, e)))?;
                
                debug!("Successfully retrieved {} prompts from server: {}", prompts.len(), server_name);
                Ok(prompts)
            } else {
                Ok(Vec::new())
            }
        } else if let Some(error) = response.error {
            Err(Error::mcp(format!("Server {} returned error: {}", server_name, error.message)))
        } else {
            Err(Error::mcp(format!("Invalid response from server: {}", server_name)))
        }
    }

    /// Get a prompt from a specific server
    pub async fn get_prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<Vec<Content>> {
        debug!("Getting prompt '{}' from server: {}", prompt_name, server_name);
        
        let mut params = serde_json::json!({
            "name": prompt_name
        });
        
        if let Some(args) = arguments {
            params["arguments"] = args;
        }
        
        let request = self.protocol_handler.create_request(
            "prompts/get".to_string(),
            Some(params),
        );
        
        let response = self.send_request_to_server(server_name, request).await?;
        
        if let Some(result) = response.result {
            if let Some(messages_array) = result.get("messages") {
                let messages: Vec<Content> = serde_json::from_value(messages_array.clone())
                    .map_err(|e| Error::mcp(format!("Failed to parse prompt messages: {}", e)))?;
                
                debug!("Successfully retrieved prompt '{}' from server: {}", prompt_name, server_name);
                Ok(messages)
            } else {
                Ok(Vec::new())
            }
        } else if let Some(error) = response.error {
            Err(Error::mcp(format!("Prompt get failed: {}", error.message)))
        } else {
            Err(Error::mcp("Invalid prompt get response".to_string()))
        }
    }

    /// Send a request to a specific server and wait for response
    async fn send_request_to_server(
        &self,
        server_name: &str,
        request: JsonRpcRequest,
    ) -> Result<JsonRpcResponse> {
        let request_id = request.id.as_ref()
            .and_then(|id| id.as_str())
            .unwrap_or("unknown")
            .to_string();
            
        debug!("Sending request {} to server: {}", request_id, server_name);
        
        // No longer need a channel since we're sending directly through the server manager
        
        // Store the pending request
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(request_id.clone(), PendingRequest {
                timestamp: Instant::now(),
            });
        }
        
        // Send the request through the server manager
        let server_manager = self.server_manager.lock().await;
        let server_status = server_manager.get_server_status().await;
        
        if let Some((state, _health)) = server_status.get(server_name) {
            if *state != ServerState::Ready {
                // Clean up pending request
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                
                return Err(Error::mcp(format!("Server {} is not ready (state: {:?})", server_name, state)));
            }
        } else {
            // Clean up pending request
            let mut pending = self.pending_requests.write().await;
            pending.remove(&request_id);
            
            return Err(Error::mcp(format!("Server {} not found", server_name)));
        }
        
        // Send the request through the server manager with timeout
        let response_result = timeout(
            self.request_timeout,
            server_manager.send_request_to_server(server_name, request)
        ).await;
        
        // Clean up pending request and handle response
        match response_result {
            Ok(Ok(response)) => {
                debug!("Received response for request {} from server: {}", request_id, server_name);
                // Clean up pending request
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                Ok(response)
            }
            Ok(Err(e)) => {
                // Request failed
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                Err(e)
            }
            Err(_) => {
                // Timeout occurred
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                Err(Error::mcp(format!("Request timeout after {:?}", self.request_timeout)))
            }
        }
    }

    /// Get client statistics
    pub async fn get_statistics(&self) -> MCPClientStatistics {
        let pending = self.pending_requests.read().await;
        
        MCPClientStatistics {
            pending_requests: pending.len(),
            total_servers: {
                let server_manager = self.server_manager.lock().await;
                server_manager.get_server_status().await.len()
            },
            ready_servers: {
                let server_manager = self.server_manager.lock().await;
                server_manager.get_server_status().await.iter()
                    .filter(|(_, (state, _))| *state == ServerState::Ready)
                    .count()
            },
        }
    }

    /// Check if a specific server is ready
    pub async fn is_server_ready(&self, server_name: &str) -> bool {
        let server_manager = self.server_manager.lock().await;
        if let Some(state) = server_manager.get_server(server_name).await {
            state == ServerState::Ready
        } else {
            false
        }
    }

    /// Cancel a pending request
    pub async fn cancel_request(&self, request_id: &str) -> bool {
        let mut pending = self.pending_requests.write().await;
        pending.remove(request_id).is_some()
    }

    /// Clean up old pending requests that have timed out
    pub async fn cleanup_old_requests(&self) {
        let now = Instant::now();
        let mut pending = self.pending_requests.write().await;
        
        let old_request_ids: Vec<String> = pending.iter()
            .filter(|(_, req)| now.duration_since(req.timestamp) > self.request_timeout)
            .map(|(id, _)| id.clone())
            .collect();
        
        for request_id in old_request_ids {
            if let Some(_request) = pending.remove(&request_id) {
                warn!("Cleaning up timed out request: {}", request_id);
                // The response channel will be automatically closed when dropped
            }
        }
        
        if !pending.is_empty() {
            debug!("Cleaned up old requests, {} requests still pending", pending.len());
        }
    }
}

/// Statistics about the MCP client
#[derive(Debug, Clone)]
pub struct MCPClientStatistics {
    pub pending_requests: usize,
    pub total_servers: usize,
    pub ready_servers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    fn create_test_server_manager() -> MCPServerManager {
        MCPServerManager::new()
    }

    #[tokio::test]
    async fn test_mcp_client_creation() {
        let server_manager = create_test_server_manager();
        let config = MCPClientConfig::default();
        
        let client = MCPClient::new(server_manager, config);
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_client_statistics() {
        let server_manager = create_test_server_manager();
        let config = MCPClientConfig::default();
        let client = MCPClient::new(server_manager, config).unwrap();
        
        let stats = client.get_statistics().await;
        assert_eq!(stats.pending_requests, 0);
        assert_eq!(stats.total_servers, 0);
        assert_eq!(stats.ready_servers, 0);
    }

    #[tokio::test]
    async fn test_server_ready_check() {
        let server_manager = create_test_server_manager();
        let config = MCPClientConfig::default();
        let client = MCPClient::new(server_manager, config).unwrap();
        
        let is_ready = client.is_server_ready("nonexistent_server").await;
        assert!(!is_ready);
    }

    #[test]
    fn test_client_config_default() {
        let config = MCPClientConfig::default();
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.max_concurrent_requests, 100);
    }

    #[tokio::test]
    async fn test_cleanup_old_requests() {
        let server_manager = create_test_server_manager();
        let config = MCPClientConfig {
            request_timeout: Duration::from_millis(1), // Very short timeout for testing
            ..MCPClientConfig::default()
        };
        let client = MCPClient::new(server_manager, config).unwrap();
        
        // Add a fake pending request manually for testing
        {
            let (_tx, _rx) = oneshot::channel::<()>();
            let mut pending = client.pending_requests.write().await;
            pending.insert("test_request".to_string(), PendingRequest {
                timestamp: Instant::now() - Duration::from_secs(1), // Old timestamp
            });
        }
        
        // Give it a moment to be "old"
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Clean up old requests
        client.cleanup_old_requests().await;
        
        // Check that the old request was removed
        let pending = client.pending_requests.read().await;
        assert!(!pending.contains_key("test_request"));
    }
}