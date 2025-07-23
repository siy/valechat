use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::error::{Error, Result};
use crate::mcp::transport::{Transport, StdioTransport, TransportStatus};
use crate::mcp::types::{
    ServerCapabilities, Tool, Resource, Prompt, InitializeParams, InitializeResult,
    ClientCapabilities, Implementation, ProtocolVersion, SamplingCapability
};
use crate::mcp::protocol::{ProtocolHandler, JsonRpcError};
use crate::app::config::MCPServerConfig;

/// Represents the lifecycle state of an MCP server
#[derive(Debug, Clone, PartialEq)]
pub enum ServerState {
    NotStarted,
    Starting,
    Initializing,
    Ready,
    Error(String),
    Stopping,
    Stopped,
}

/// Health status of an MCP server
#[derive(Debug, Clone)]
pub struct ServerHealth {
    pub is_healthy: bool,
    pub last_check: Instant,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub response_time_ms: Option<u64>,
}

impl Default for ServerHealth {
    fn default() -> Self {
        Self {
            is_healthy: false,
            last_check: Instant::now(),
            consecutive_failures: 0,
            last_error: None,
            response_time_ms: None,
        }
    }
}

/// Instance of an MCP server with its transport and state
pub struct MCPServerInstance {
    pub name: String,
    pub config: MCPServerConfig,
    transport: Option<Box<dyn Transport>>,
    protocol_handler: ProtocolHandler,
    state: Arc<RwLock<ServerState>>,
    health: Arc<RwLock<ServerHealth>>,
    capabilities: Arc<RwLock<Option<ServerCapabilities>>>,
    tools: Arc<RwLock<Vec<Tool>>>,
    resources: Arc<RwLock<Vec<Resource>>>,
    prompts: Arc<RwLock<Vec<Prompt>>>,
    #[allow(dead_code)]
    last_activity: Arc<RwLock<Instant>>,
}

impl MCPServerInstance {
    pub fn new(name: String, config: MCPServerConfig) -> Self {
        Self {
            name,
            config,
            transport: None,
            protocol_handler: ProtocolHandler::new(),
            state: Arc::new(RwLock::new(ServerState::NotStarted)),
            health: Arc::new(RwLock::new(ServerHealth::default())),
            capabilities: Arc::new(RwLock::new(None)),
            tools: Arc::new(RwLock::new(Vec::new())),
            resources: Arc::new(RwLock::new(Vec::new())),
            prompts: Arc::new(RwLock::new(Vec::new())),
            last_activity: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Start the MCP server process and initialize the connection
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting MCP server: {}", self.name);
        
        // Update state to starting
        {
            let mut state = self.state.write().await;
            *state = ServerState::Starting;
        }

        // Create transport based on configuration
        let transport = match &self.config.transport_type {
            crate::app::config::TransportType::Stdio => {
                let working_dir = std::env::current_dir().ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()));

                StdioTransport::new(
                    &self.config.command,
                    &self.config.args,
                    &self.config.env_vars,
                    working_dir.as_deref(),
                ).await?
            }
            crate::app::config::TransportType::WebSocket { url: _ } => {
                return Err(Error::mcp("WebSocket transport not yet implemented".to_string()));
            }
        };

        self.transport = Some(Box::new(transport));

        // Update state to initializing
        {
            let mut state = self.state.write().await;
            *state = ServerState::Initializing;
        }

        // Initialize the MCP connection
        self.initialize().await?;

        // Update state to ready
        {
            let mut state = self.state.write().await;
            *state = ServerState::Ready;
        }

        info!("MCP server {} started successfully", self.name);
        Ok(())
    }

    /// Initialize the MCP protocol connection
    async fn initialize(&mut self) -> Result<()> {
        debug!("Initializing MCP protocol for server: {}", self.name);

        let transport = self.transport.as_ref()
            .ok_or_else(|| Error::mcp("Transport not available".to_string()))?;

        // Create initialization request
        let init_params = InitializeParams {
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

        let init_request = self.protocol_handler.create_request(
            "initialize".to_string(),
            Some(serde_json::to_value(&init_params)?),
        );

        // Send initialization request
        let message = crate::mcp::protocol::ProtocolMessage::Request(init_request.clone());
        transport.send(&message).await?;

        // Wait for initialization response
        if let Some(response_message) = transport.receive().await? {
            match response_message {
                crate::mcp::protocol::ProtocolMessage::Response(response) => {
                    if response.is_success() {
                        let init_result: InitializeResult = serde_json::from_value(
                            response.result.unwrap_or(serde_json::Value::Null)
                        ).map_err(|e| Error::mcp(format!("Invalid initialization response: {}", e)))?;

                        // Store server capabilities
                        {
                            let mut capabilities = self.capabilities.write().await;
                            *capabilities = Some(init_result.capabilities);
                        }

                        debug!("MCP server {} initialized with protocol version: {:?}", 
                               self.name, init_result.protocol_version);
                        
                        // Send initialized notification
                        let initialized_notification = self.protocol_handler.create_notification(
                            "notifications/initialized".to_string(),
                            None,
                        );
                        let notif_message = crate::mcp::protocol::ProtocolMessage::Request(initialized_notification);
                        transport.send(&notif_message).await?;

                        Ok(())
                    } else {
                        let error = response.error.unwrap_or(JsonRpcError::internal_error("Unknown error"));
                        Err(Error::mcp(format!("Initialization failed: {}", error.message)))
                    }
                }
                _ => Err(Error::mcp("Expected response to initialization request".to_string())),
            }
        } else {
            Err(Error::mcp("No response received for initialization".to_string()))
        }
    }

    /// Stop the MCP server and clean up resources
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping MCP server: {}", self.name);

        // Update state to stopping
        {
            let mut state = self.state.write().await;
            *state = ServerState::Stopping;
        }

        // Close transport if available
        if let Some(transport) = &self.transport {
            transport.close().await?;
        }
        self.transport = None;

        // Clear cached data
        {
            let mut tools = self.tools.write().await;
            tools.clear();
        }
        {
            let mut resources = self.resources.write().await;
            resources.clear();
        }
        {
            let mut prompts = self.prompts.write().await;
            prompts.clear();
        }
        {
            let mut capabilities = self.capabilities.write().await;
            *capabilities = None;
        }

        // Update state to stopped
        {
            let mut state = self.state.write().await;
            *state = ServerState::Stopped;
        }

        info!("MCP server {} stopped", self.name);
        Ok(())
    }

    /// Restart the MCP server
    pub async fn restart(&mut self) -> Result<()> {
        info!("Restarting MCP server: {}", self.name);
        
        self.stop().await?;
        tokio::time::sleep(Duration::from_millis(1000)).await; // Brief pause
        self.start().await?;
        
        Ok(())
    }

    /// Get the current state of the server
    pub async fn get_state(&self) -> ServerState {
        let state = self.state.read().await;
        state.clone()
    }

    /// Get the current health status
    pub async fn get_health(&self) -> ServerHealth {
        let health = self.health.read().await;
        health.clone()
    }

    /// Get server capabilities
    pub async fn get_capabilities(&self) -> Option<ServerCapabilities> {
        let capabilities = self.capabilities.read().await;
        capabilities.clone()
    }

    /// Get available tools from the server
    pub async fn get_tools(&self) -> Vec<Tool> {
        let tools = self.tools.read().await;
        tools.clone()
    }

    /// Get available resources from the server  
    pub async fn get_resources(&self) -> Vec<Resource> {
        let resources = self.resources.read().await;
        resources.clone()
    }

    /// Get available prompts from the server
    pub async fn get_prompts(&self) -> Vec<Prompt> {
        let prompts = self.prompts.read().await;
        prompts.clone()
    }

    /// Check if the transport is connected
    pub async fn is_connected(&self) -> bool {
        if let Some(transport) = &self.transport {
            transport.is_connected().await
        } else {
            false
        }
    }

    /// Get transport status
    pub async fn get_transport_status(&self) -> Option<TransportStatus> {
        if let Some(transport) = &self.transport {
            Some(transport.get_status().await)
        } else {
            None
        }
    }

    /// Perform health check on the server
    pub async fn health_check(&self) -> Result<()> {
        let start_time = Instant::now();
        
        if let Some(_transport) = &self.transport {
            // For now, just check if transport is connected
            // In a full implementation, we would send a ping or list request
            let is_connected = self.is_connected().await;
            let response_time = start_time.elapsed().as_millis() as u64;

            let mut health = self.health.write().await;
            health.last_check = Instant::now();
            health.response_time_ms = Some(response_time);

            if is_connected {
                health.is_healthy = true;
                health.consecutive_failures = 0;
                health.last_error = None;
            } else {
                health.is_healthy = false;
                health.consecutive_failures += 1;
                health.last_error = Some("Transport not connected".to_string());
            }

            if is_connected {
                Ok(())
            } else {
                Err(Error::mcp("Server not healthy".to_string()))
            }
        } else {
            let mut health = self.health.write().await;
            health.is_healthy = false;
            health.consecutive_failures += 1;
            health.last_error = Some("No transport available".to_string());
            
            Err(Error::mcp("No transport available for health check".to_string()))
        }
    }
}

/// Manager for multiple MCP server instances
pub struct MCPServerManager {
    servers: Arc<RwLock<HashMap<String, MCPServerInstance>>>,
    health_check_interval: Duration,
    _health_check_handle: Option<tokio::task::JoinHandle<()>>,
}

impl MCPServerManager {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            health_check_interval: Duration::from_secs(30),
            _health_check_handle: None,
        }
    }

    /// Add a new MCP server configuration
    pub async fn add_server(&mut self, name: String, config: MCPServerConfig) -> Result<()> {
        info!("Adding MCP server: {}", name);

        let instance = MCPServerInstance::new(name.clone(), config);
        
        let mut servers = self.servers.write().await;
        servers.insert(name.clone(), instance);

        debug!("MCP server {} added to manager", name);
        Ok(())
    }

    /// Remove an MCP server
    pub async fn remove_server(&mut self, name: &str) -> Result<()> {
        info!("Removing MCP server: {}", name);

        let mut servers = self.servers.write().await;
        if let Some(mut instance) = servers.remove(name) {
            // Stop the server if it's running
            instance.stop().await?;
        }

        debug!("MCP server {} removed from manager", name);
        Ok(())
    }

    /// Start a specific MCP server
    pub async fn start_server(&mut self, name: &str) -> Result<()> {
        info!("Starting MCP server: {}", name);

        let mut servers = self.servers.write().await;
        if let Some(instance) = servers.get_mut(name) {
            instance.start().await?;
            Ok(())
        } else {
            Err(Error::mcp(format!("MCP server not found: {}", name)))
        }
    }

    /// Stop a specific MCP server
    pub async fn stop_server(&mut self, name: &str) -> Result<()> {
        info!("Stopping MCP server: {}", name);

        let mut servers = self.servers.write().await;
        if let Some(instance) = servers.get_mut(name) {
            instance.stop().await?;
            Ok(())
        } else {
            Err(Error::mcp(format!("MCP server not found: {}", name)))
        }
    }

    /// Restart a specific MCP server
    pub async fn restart_server(&mut self, name: &str) -> Result<()> {
        info!("Restarting MCP server: {}", name);

        let mut servers = self.servers.write().await;
        if let Some(instance) = servers.get_mut(name) {
            instance.restart().await?;
            Ok(())
        } else {
            Err(Error::mcp(format!("MCP server not found: {}", name)))
        }
    }

    /// Start all configured MCP servers
    pub async fn start_all_servers(&mut self) -> Result<()> {
        info!("Starting all MCP servers");

        let server_names: Vec<String> = {
            let servers = self.servers.read().await;
            servers.keys().cloned().collect()
        };

        for name in server_names {
            if let Err(e) = self.start_server(&name).await {
                error!("Failed to start MCP server {}: {}", name, e);
                // Continue starting other servers even if one fails
            }
        }

        Ok(())
    }

    /// Stop all MCP servers
    pub async fn stop_all_servers(&mut self) -> Result<()> {
        info!("Stopping all MCP servers");

        let server_names: Vec<String> = {
            let servers = self.servers.read().await;
            servers.keys().cloned().collect()
        };

        for name in server_names {
            if let Err(e) = self.stop_server(&name).await {
                error!("Failed to stop MCP server {}: {}", name, e);
                // Continue stopping other servers even if one fails
            }
        }

        Ok(())
    }

    /// Get status of all servers
    pub async fn get_server_status(&self) -> HashMap<String, (ServerState, ServerHealth)> {
        let mut status_map = HashMap::new();
        
        let servers = self.servers.read().await;
        for (name, instance) in servers.iter() {
            let state = instance.get_state().await;
            let health = instance.get_health().await;
            status_map.insert(name.clone(), (state, health));
        }

        status_map
    }

    /// Get a specific server instance (read-only access)
    pub async fn get_server(&self, name: &str) -> Option<ServerState> {
        let servers = self.servers.read().await;
        if let Some(instance) = servers.get(name) {
            Some(instance.get_state().await)
        } else {
            None
        }
    }

    /// Start background health checking
    pub async fn start_health_monitoring(&mut self) {
        info!("Starting MCP server health monitoring");

        let servers = self.servers.clone();
        let check_interval = self.health_check_interval;

        let handle = tokio::spawn(async move {
            let mut interval = interval(check_interval);

            loop {
                interval.tick().await;
                
                let server_names: Vec<String> = {
                    let servers_guard = servers.read().await;
                    servers_guard.keys().cloned().collect()
                };

                for name in server_names {
                    let servers_guard = servers.read().await;
                    if let Some(instance) = servers_guard.get(&name) {
                        if let Err(e) = instance.health_check().await {
                            warn!("Health check failed for MCP server {}: {}", name, e);
                        }
                    }
                }
            }
        });

        self._health_check_handle = Some(handle);
    }
}

impl Default for MCPServerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::config::TransportType;

    fn create_test_config() -> MCPServerConfig {
        MCPServerConfig {
            name: "test_server".to_string(),
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            transport_type: TransportType::Stdio,
            env_vars: std::collections::HashMap::new(),
            enabled: true,
            auto_start: false,
            timeout_seconds: 30,
        }
    }

    #[test]
    fn test_server_instance_creation() {
        let config = create_test_config();
        let instance = MCPServerInstance::new("test".to_string(), config);
        
        assert_eq!(instance.name, "test");
        assert_eq!(instance.config.name, "test_server");
    }

    #[tokio::test]
    async fn test_server_manager_add_remove() {
        let mut manager = MCPServerManager::new();
        let config = create_test_config();

        // Add server
        manager.add_server("test".to_string(), config).await.unwrap();
        
        let status = manager.get_server_status().await;
        assert!(status.contains_key("test"));
        
        // Remove server
        manager.remove_server("test").await.unwrap();
        
        let status = manager.get_server_status().await;
        assert!(!status.contains_key("test"));
    }

    #[tokio::test]
    async fn test_server_state_transitions() {
        let config = create_test_config();
        let instance = MCPServerInstance::new("test".to_string(), config);
        
        // Initial state should be NotStarted
        let state = instance.get_state().await;
        assert_eq!(state, ServerState::NotStarted);
        
        // Health should be unhealthy initially
        let health = instance.get_health().await;
        assert!(!health.is_healthy);
    }

    #[test]
    fn test_server_health_default() {
        let health = ServerHealth::default();
        assert!(!health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
        assert!(health.last_error.is_none());
    }
}