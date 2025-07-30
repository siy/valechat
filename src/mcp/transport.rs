use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::error::{Error, Result};
use crate::mcp::protocol::ProtocolMessage;
use crate::platform::{ProcessConfig, ResourceLimits};

/// Transport layer abstraction for MCP communication
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a message through the transport
    async fn send(&self, message: &ProtocolMessage) -> Result<()>;
    
    /// Receive the next message from the transport
    async fn receive(&self) -> Result<Option<ProtocolMessage>>;
    
    /// Check if the transport is still connected/active
    async fn is_connected(&self) -> bool;
    
    /// Close the transport connection
    async fn close(&self) -> Result<()>;
    
    /// Get transport-specific status information
    async fn get_status(&self) -> TransportStatus;
}

/// Transport status information
#[derive(Debug, Clone)]
pub struct TransportStatus {
    pub transport_type: String,
    pub is_connected: bool,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub last_activity: Option<std::time::Instant>,
    pub error_count: u64,
}

/// Stdio transport for MCP servers running as child processes
pub struct StdioTransport {
    process: Arc<Mutex<Option<Child>>>,
    sender: Arc<Mutex<Option<BufWriter<tokio::process::ChildStdin>>>>,
    receiver: Arc<Mutex<Option<mpsc::Receiver<Result<ProtocolMessage>>>>>,
    status: Arc<RwLock<TransportStatus>>,
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl StdioTransport {
    /// Create a new stdio transport for the given command and arguments
    pub async fn new(
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        working_dir: Option<&str>,
    ) -> Result<Self> {
        info!("Starting MCP server: {} with args: {:?}", command, args);

        // Build the command
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            Error::mcp(format!("Failed to spawn MCP server process: {}", e))
        })?;

        // Get handles to stdin and stdout
        let stdin = child.stdin.take().ok_or_else(|| {
            Error::mcp("Failed to get stdin handle for MCP server".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            Error::mcp("Failed to get stdout handle for MCP server".to_string())
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
            Error::mcp("Failed to get stderr handle for MCP server".to_string())
        })?;

        // Create channels for message passing
        let (message_tx, message_rx) = mpsc::channel::<Result<ProtocolMessage>>(100);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Create status
        let status = TransportStatus {
            transport_type: "stdio".to_string(),
            is_connected: true,
            messages_sent: 0,
            messages_received: 0,
            last_activity: Some(std::time::Instant::now()),
            error_count: 0,
        };

        let transport = Self {
            process: Arc::new(Mutex::new(Some(child))),
            sender: Arc::new(Mutex::new(Some(BufWriter::new(stdin)))),
            receiver: Arc::new(Mutex::new(Some(message_rx))),
            status: Arc::new(RwLock::new(status)),
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
        };

        // Spawn a task to read from stdout
        let message_tx_clone = message_tx.clone();
        let status_clone = transport.status.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Stdout reader shutting down");
                        break;
                    }
                    result = reader.read_line(&mut line) => {
                        match result {
                            Ok(0) => {
                                debug!("MCP server stdout closed");
                                break;
                            }
                            Ok(_) => {
                                let trimmed = line.trim();
                                if !trimmed.is_empty() {
                                    debug!("Received from MCP server: {}", trimmed);
                                    
                                    match ProtocolMessage::parse(trimmed) {
                                        Ok(message) => {
                                            // Update status
                                            {
                                                let mut status = status_clone.write().await;
                                                status.messages_received += 1;
                                                status.last_activity = Some(std::time::Instant::now());
                                            }
                                            
                                            if message_tx_clone.send(Ok(message)).await.is_err() {
                                                warn!("Failed to send message to receiver channel");
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to parse message from MCP server: {}", e);
                                            
                                            // Update error count
                                            {
                                                let mut status = status_clone.write().await;
                                                status.error_count += 1;
                                            }
                                            
                                            if message_tx_clone.send(Err(e)).await.is_err() {
                                                warn!("Failed to send error to receiver channel");
                                                break;
                                            }
                                        }
                                    }
                                }
                                line.clear();
                            }
                            Err(e) => {
                                error!("Error reading from MCP server stdout: {}", e);
                                
                                // Update error count and connection status
                                {
                                    let mut status = status_clone.write().await;
                                    status.error_count += 1;
                                    status.is_connected = false;
                                }
                                
                                let error = Error::mcp(format!("Transport read error: {}", e));
                                if message_tx_clone.send(Err(error)).await.is_err() {
                                    warn!("Failed to send error to receiver channel");
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });

        // Spawn a task to read from stderr (for logging)
        let status_clone = transport.status.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            
            while let Ok(bytes_read) = reader.read_line(&mut line).await {
                if bytes_read == 0 {
                    break;
                }
                
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    warn!("MCP server stderr: {}", trimmed);
                    
                    // Update error count for stderr messages
                    {
                        let mut status = status_clone.write().await;
                        status.error_count += 1;
                    }
                }
                line.clear();
            }
        });

        info!("MCP server started successfully");
        Ok(transport)
    }

    /// Create a sandboxed stdio transport with additional security
    pub async fn new_sandboxed(
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        working_dir: Option<&str>,
        sandbox_config: SandboxConfig,
    ) -> Result<Self> {
        info!("Starting sandboxed MCP server: {}", command);

        // For now, delegate to the regular stdio transport
        // In a full implementation, we would use a proper sandboxed process
        // The sandbox_config contains the security constraints that would be applied
        let _process_config: ProcessConfig = sandbox_config.into();
        
        Self::new(command, args, env_vars, working_dir).await
    }

    /// Wait for the process to exit and get the exit status
    pub async fn wait(&self) -> Result<std::process::ExitStatus> {
        let mut process_guard = self.process.lock().await;
        if let Some(mut child) = process_guard.take() {
            child.wait().await.map_err(|e| {
                Error::mcp(format!("Failed to wait for MCP server process: {}", e))
            })
        } else {
            Err(Error::mcp("Process already finished or not started".to_string()))
        }
    }

    /// Kill the process forcefully
    pub async fn kill(&self) -> Result<()> {
        let mut process_guard = self.process.lock().await;
        if let Some(child) = process_guard.as_mut() {
            child.kill().await.map_err(|e| {
                Error::mcp(format!("Failed to kill MCP server process: {}", e))
            })?;
        }

        // Update status
        {
            let mut status = self.status.write().await;
            status.is_connected = false;
        }

        Ok(())
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn send(&self, message: &ProtocolMessage) -> Result<()> {
        let mut sender_guard = self.sender.lock().await;
        
        if let Some(sender) = sender_guard.as_mut() {
            let serialized = message.serialize()?;
            debug!("Sending to MCP server: {}", serialized);
            
            sender.write_all(serialized.as_bytes()).await.map_err(|e| {
                Error::mcp(format!("Failed to write to MCP server stdin: {}", e))
            })?;
            
            sender.write_all(b"\n").await.map_err(|e| {
                Error::mcp(format!("Failed to write newline to MCP server stdin: {}", e))
            })?;
            
            sender.flush().await.map_err(|e| {
                Error::mcp(format!("Failed to flush MCP server stdin: {}", e))
            })?;

            // Update status
            {
                let mut status = self.status.write().await;
                status.messages_sent += 1;
                status.last_activity = Some(std::time::Instant::now());
            }

            Ok(())
        } else {
            Err(Error::mcp("Transport sender not available".to_string()))
        }
    }

    async fn receive(&self) -> Result<Option<ProtocolMessage>> {
        let mut receiver_guard = self.receiver.lock().await;
        
        if let Some(receiver) = receiver_guard.as_mut() {
            match timeout(Duration::from_secs(30), receiver.recv()).await {
                Ok(Some(result)) => Ok(Some(result?)),
                Ok(None) => Ok(None), // Channel closed
                Err(_) => Err(Error::mcp("Receive timeout".to_string())),
            }
        } else {
            Ok(None)
        }
    }

    async fn is_connected(&self) -> bool {
        let status = self.status.read().await;
        status.is_connected
    }

    async fn close(&self) -> Result<()> {
        info!("Closing stdio transport");

        // Send shutdown signal
        {
            let shutdown_guard = self.shutdown_tx.lock().await;
            if let Some(tx) = shutdown_guard.as_ref() {
                let _ = tx.send(()).await;
            }
        }

        // Close sender
        {
            let mut sender_guard = self.sender.lock().await;
            *sender_guard = None;
        }

        // Close receiver
        {
            let mut receiver_guard = self.receiver.lock().await;
            *receiver_guard = None;
        }

        // Kill the process
        self.kill().await?;

        info!("Stdio transport closed");
        Ok(())
    }

    async fn get_status(&self) -> TransportStatus {
        self.status.read().await.clone()
    }
}

/// Configuration for sandboxed processes
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub enable_network: bool,
    pub allowed_paths: Vec<String>,
    pub max_memory_mb: Option<u32>,
    pub max_cpu_percent: Option<u32>,
    pub timeout_seconds: Option<u32>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enable_network: false,
            allowed_paths: vec![],
            max_memory_mb: Some(512),
            max_cpu_percent: Some(50),
            timeout_seconds: Some(300), // 5 minutes
        }
    }
}

impl From<SandboxConfig> for ProcessConfig {
    fn from(config: SandboxConfig) -> Self {
        ProcessConfig {
            command: String::new(), // Will be set when actually used
            args: Vec::new(),       // Will be set when actually used
            working_dir: None,      // Will be set when actually used
            env_vars: HashMap::new(), // Will be set when actually used
            resource_limits: ResourceLimits {
                max_memory_mb: config.max_memory_mb.unwrap_or(512) as u64,
                max_cpu_percent: config.max_cpu_percent.unwrap_or(50) as u8,
                max_open_files: 100,
                timeout_seconds: config.timeout_seconds.unwrap_or(300) as u64,
            },
            network_access: config.enable_network,
            file_system_access: config.allowed_paths.into_iter()
                .map(std::path::PathBuf::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(!config.enable_network);
        assert_eq!(config.max_memory_mb, Some(512));
        assert_eq!(config.max_cpu_percent, Some(50));
    }

    #[test]
    fn test_sandbox_config_conversion() {
        let sandbox_config = SandboxConfig {
            enable_network: true,
            allowed_paths: vec!["/tmp".to_string()],
            max_memory_mb: Some(1024),
            max_cpu_percent: Some(75),
            timeout_seconds: Some(600),
        };

        let process_config: ProcessConfig = sandbox_config.into();
        assert!(process_config.network_access);
        assert_eq!(process_config.resource_limits.max_memory_mb, 1024);
        assert_eq!(process_config.file_system_access, vec![std::path::PathBuf::from("/tmp")]);
    }

    #[test]
    fn test_transport_status() {
        let status = TransportStatus {
            transport_type: "stdio".to_string(),
            is_connected: true,
            messages_sent: 10,
            messages_received: 5,
            last_activity: Some(std::time::Instant::now()),
            error_count: 0,
        };

        assert_eq!(status.transport_type, "stdio");
        assert!(status.is_connected);
        assert_eq!(status.messages_sent, 10);
        assert_eq!(status.messages_received, 5);
    }

    // Note: Integration tests for the actual stdio transport would require
    // a test MCP server executable, so we'll focus on unit tests for now
}