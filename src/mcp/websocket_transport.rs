use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use std::time::Instant;
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream, MaybeTlsStream};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::error::{Error, Result};
use crate::mcp::protocol::ProtocolMessage;
use crate::mcp::transport::{Transport, TransportStatus};

/// WebSocket transport for MCP communication
pub struct WebSocketTransport {
    sender: Arc<Mutex<Option<mpsc::Sender<Message>>>>,
    receiver: Arc<Mutex<Option<mpsc::Receiver<Result<ProtocolMessage>>>>>,
    status: Arc<RwLock<TransportStatus>>,
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    connection_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    config: WebSocketConfig,
}

/// Configuration for WebSocket transport
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub url: String,
    pub connection_timeout: Duration,
    pub ping_interval: Duration,
    pub pong_timeout: Duration,
    pub max_message_size: usize,
    pub compression: bool,
    pub headers: std::collections::HashMap<String, String>,
    pub reconnect_attempts: u32,
    pub reconnect_delay: Duration,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            connection_timeout: Duration::from_secs(30),
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
            max_message_size: 16 * 1024 * 1024, // 16MB
            compression: true,
            headers: std::collections::HashMap::new(),
            reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(5),
        }
    }
}

/// Connection state for WebSocket
#[derive(Debug, Clone, Copy, PartialEq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Closed,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport
    pub async fn new(config: WebSocketConfig) -> Result<Self> {
        info!("Creating WebSocket transport for: {}", config.url);

        // Validate URL
        let url = Url::parse(&config.url).map_err(|e| {
            Error::mcp(format!("Invalid WebSocket URL '{}': {}", config.url, e))
        })?;

        if !matches!(url.scheme(), "ws" | "wss") {
            return Err(Error::mcp(format!(
                "Invalid WebSocket scheme '{}'. Must be 'ws' or 'wss'",
                url.scheme()
            )));
        }

        // Create channels
        let (message_tx, message_rx) = mpsc::channel::<Result<ProtocolMessage>>(100);
        let (ws_tx, mut ws_rx) = mpsc::channel::<Message>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        // Create status
        let status = TransportStatus {
            transport_type: "websocket".to_string(),
            is_connected: false,
            messages_sent: 0,
            messages_received: 0,
            last_activity: Some(Instant::now()),
            error_count: 0,
        };

        let transport = Self {
            sender: Arc::new(Mutex::new(Some(ws_tx))),
            receiver: Arc::new(Mutex::new(Some(message_rx))),
            status: Arc::new(RwLock::new(status)),
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
            connection_task: Arc::new(Mutex::new(None)),
            config,
        };

        // Start connection task
        transport.start_connection_task(message_tx, ws_rx, shutdown_rx).await?;

        Ok(transport)
    }

    /// Start the connection management task
    async fn start_connection_task(
        &self,
        message_tx: mpsc::Sender<Result<ProtocolMessage>>,
        mut ws_rx: mpsc::Receiver<Message>,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<()> {
        let config = self.config.clone();
        let status = Arc::clone(&self.status);

        let task = tokio::spawn(async move {
            let mut connection_state = ConnectionState::Disconnected;
            let mut reconnect_attempts = 0;
            let mut ws_stream: Option<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>> = None;

            loop {
                match connection_state {
                    ConnectionState::Disconnected | ConnectionState::Reconnecting => {
                        // Update status
                        {
                            let mut status_lock = status.write().await;
                            status_lock.is_connected = false;
                        }

                        // Attempt to connect
                        connection_state = ConnectionState::Connecting;
                        info!("Connecting to WebSocket: {}", config.url);

                        match timeout(config.connection_timeout, connect_async(&config.url)).await {
                            Ok(Ok((stream, response))) => {
                                info!("WebSocket connected successfully. Response: {:?}", response.status());
                                ws_stream = Some(stream);
                                connection_state = ConnectionState::Connected;
                                reconnect_attempts = 0;

                                // Update status
                                {
                                    let mut status_lock = status.write().await;
                                    status_lock.is_connected = true;
                                    status_lock.last_activity = Some(Instant::now());
                                }
                            }
                            Ok(Err(e)) => {
                                error!("WebSocket connection failed: {}", e);
                                connection_state = Self::handle_connection_error(
                                    &mut reconnect_attempts,
                                    config.reconnect_attempts,
                                    &status,
                                ).await;
                                
                                if connection_state == ConnectionState::Reconnecting {
                                    tokio::time::sleep(config.reconnect_delay).await;
                                }
                            }
                            Err(_) => {
                                error!("WebSocket connection timed out");
                                connection_state = Self::handle_connection_error(
                                    &mut reconnect_attempts,
                                    config.reconnect_attempts,
                                    &status,
                                ).await;
                                
                                if connection_state == ConnectionState::Reconnecting {
                                    tokio::time::sleep(config.reconnect_delay).await;
                                }
                            }
                        }
                    }
                    ConnectionState::Connected => {
                        if let Some(ref mut stream) = ws_stream {
                            tokio::select! {
                                // Handle incoming WebSocket messages
                                ws_msg = stream.next() => {
                                    match ws_msg {
                                        Some(Ok(msg)) => {
                                            if let Err(e) = Self::handle_websocket_message(
                                                msg,
                                                &message_tx,
                                                &status,
                                            ).await {
                                                error!("Error handling WebSocket message: {}", e);
                                                Self::increment_error_count(&status).await;
                                            }
                                        }
                                        Some(Err(e)) => {
                                            error!("WebSocket error: {}", e);
                                            connection_state = Self::handle_connection_error(
                                                &mut reconnect_attempts,
                                                config.reconnect_attempts,
                                                &status,
                                            ).await;
                                            ws_stream = None;
                                        }
                                        None => {
                                            warn!("WebSocket stream closed");
                                            connection_state = Self::handle_connection_error(
                                                &mut reconnect_attempts,
                                                config.reconnect_attempts,
                                                &status,
                                            ).await;
                                            ws_stream = None;
                                        }
                                    }
                                }
                                // Handle outgoing messages
                                outgoing_msg = ws_rx.recv() => {
                                    match outgoing_msg {
                                        Some(msg) => {
                                            if let Err(e) = stream.send(msg).await {
                                                error!("Failed to send WebSocket message: {}", e);
                                                connection_state = Self::handle_connection_error(
                                                    &mut reconnect_attempts,
                                                    config.reconnect_attempts,
                                                    &status,
                                                ).await;
                                                ws_stream = None;
                                            } else {
                                                // Update status
                                                let mut status_lock = status.write().await;
                                                status_lock.messages_sent += 1;
                                                status_lock.last_activity = Some(Instant::now());
                                            }
                                        }
                                        None => {
                                            // Sender dropped, should shut down
                                            connection_state = ConnectionState::Closed;
                                        }
                                    }
                                }
                                // Handle shutdown signal
                                _ = shutdown_rx.recv() => {
                                    info!("WebSocket transport shutdown requested");
                                    connection_state = ConnectionState::Closed;
                                }
                                // Periodic ping
                                _ = tokio::time::sleep(config.ping_interval) => {
                                    if let Err(e) = stream.send(Message::Ping(vec![])).await {
                                        error!("Failed to send ping: {}", e);
                                        connection_state = Self::handle_connection_error(
                                            &mut reconnect_attempts,
                                            config.reconnect_attempts,
                                            &status,
                                        ).await;
                                        ws_stream = None;
                                    }
                                }
                            }
                        }
                    }
                    ConnectionState::Connecting => {
                        // Wait for connection attempt to complete
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    ConnectionState::Closed => {
                        info!("WebSocket transport closed");
                        break;
                    }
                }
            }

            // Final cleanup
            if let Some(mut stream) = ws_stream {
                let _ = stream.close(None).await;
            }
            
            // Update final status
            {
                let mut status_lock = status.write().await;
                status_lock.is_connected = false;
            }
        });

        {
            let mut connection_task = self.connection_task.lock().await;
            *connection_task = Some(task);
        }

        Ok(())
    }

    /// Handle WebSocket message
    async fn handle_websocket_message(
        msg: Message,
        message_tx: &mpsc::Sender<Result<ProtocolMessage>>,
        status: &Arc<RwLock<TransportStatus>>,
    ) -> Result<()> {
        match msg {
            Message::Text(text) => {
                debug!("Received WebSocket text message: {}", text);
                
                // Parse as protocol message
                match serde_json::from_str::<ProtocolMessage>(&text) {
                    Ok(protocol_msg) => {
                        // Update status
                        {
                            let mut status_lock = status.write().await;
                            status_lock.messages_received += 1;
                            status_lock.last_activity = Some(Instant::now());
                        }

                        // Send to receiver
                        if message_tx.send(Ok(protocol_msg)).await.is_err() {
                            warn!("Message receiver dropped");
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse WebSocket message as JSON: {}", e);
                        let _ = message_tx.send(Err(Error::mcp(format!(
                            "Invalid JSON in WebSocket message: {}", e
                        )))).await;
                        Self::increment_error_count(status).await;
                    }
                }
            }
            Message::Binary(data) => {
                warn!("Received unexpected binary WebSocket message ({} bytes)", data.len());
                Self::increment_error_count(status).await;
            }
            Message::Ping(data) => {
                debug!("Received WebSocket ping with {} bytes", data.len());
                // Pong is handled automatically by tungstenite
            }
            Message::Pong(data) => {
                debug!("Received WebSocket pong with {} bytes", data.len());
                // Update activity time
                let mut status_lock = status.write().await;
                status_lock.last_activity = Some(Instant::now());
            }
            Message::Close(close_frame) => {
                if let Some(frame) = close_frame {
                    info!("WebSocket close frame: code={}, reason={}", frame.code, frame.reason);
                } else {
                    info!("WebSocket closed without close frame");
                }
            }
            Message::Frame(_) => {
                // Raw frames are not expected in normal operation
                debug!("Received raw WebSocket frame");
            }
        }

        Ok(())
    }

    /// Handle connection error and determine next state
    async fn handle_connection_error(
        reconnect_attempts: &mut u32,
        max_attempts: u32,
        status: &Arc<RwLock<TransportStatus>>,
    ) -> ConnectionState {
        Self::increment_error_count(status).await;

        *reconnect_attempts += 1;
        if *reconnect_attempts <= max_attempts {
            warn!("Connection failed, attempting reconnection {}/{}", 
                  reconnect_attempts, max_attempts);
            ConnectionState::Reconnecting
        } else {
            error!("Max reconnection attempts ({}) exceeded, giving up", max_attempts);
            ConnectionState::Closed
        }
    }

    /// Increment error count in status
    async fn increment_error_count(status: &Arc<RwLock<TransportStatus>>) {
        let mut status_lock = status.write().await;
        status_lock.error_count += 1;
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn send(&self, message: &ProtocolMessage) -> Result<()> {
        debug!("Sending WebSocket message: {:?}", message);

        // Serialize message
        let json = serde_json::to_string(message).map_err(|e| {
            Error::mcp(format!("Failed to serialize message: {}", e))
        })?;

        // Check message size
        if json.len() > self.config.max_message_size {
            return Err(Error::mcp(format!(
                "Message size ({} bytes) exceeds maximum ({} bytes)",
                json.len(),
                self.config.max_message_size
            )));
        }

        // Send through WebSocket
        let sender = self.sender.lock().await;
        if let Some(ref tx) = *sender {
            tx.send(Message::Text(json)).await.map_err(|e| {
                Error::mcp(format!("Failed to send WebSocket message: {}", e))
            })?;
        } else {
            return Err(Error::mcp("WebSocket sender not available".to_string()));
        }

        Ok(())
    }

    async fn receive(&self) -> Result<Option<ProtocolMessage>> {
        let mut receiver = self.receiver.lock().await;
        if let Some(ref mut rx) = *receiver {
            match rx.recv().await {
                Some(result) => result.map(Some),
                None => Ok(None),
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
        info!("Closing WebSocket transport");

        // Send shutdown signal
        {
            let shutdown_tx = self.shutdown_tx.lock().await;
            if let Some(ref tx) = *shutdown_tx {
                let _ = tx.send(()).await;
            }
        }

        // Wait for connection task to finish
        {
            let mut connection_task = self.connection_task.lock().await;
            if let Some(task) = connection_task.take() {
                let _ = task.await;
            }
        }

        // Clear channels
        {
            let mut sender = self.sender.lock().await;
            *sender = None;
        }
        {
            let mut receiver = self.receiver.lock().await;
            *receiver = None;
        }

        info!("WebSocket transport closed");
        Ok(())
    }

    async fn get_status(&self) -> TransportStatus {
        let status = self.status.read().await;
        status.clone()
    }
}

impl Drop for WebSocketTransport {
    fn drop(&mut self) {
        // Note: We can't call async methods in Drop, so we just log
        debug!("WebSocketTransport dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websocket_config_default() {
        let config = WebSocketConfig::default();
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert_eq!(config.ping_interval, Duration::from_secs(30));
        assert_eq!(config.max_message_size, 16 * 1024 * 1024);
        assert!(config.compression);
        assert_eq!(config.reconnect_attempts, 3);
    }

    #[tokio::test]
    async fn test_websocket_transport_creation_invalid_url() {
        let config = WebSocketConfig {
            url: "invalid-url".to_string(),
            ..Default::default()
        };

        let result = WebSocketTransport::new(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_websocket_transport_creation_invalid_scheme() {
        let config = WebSocketConfig {
            url: "http://example.com".to_string(),
            ..Default::default()
        };

        let result = WebSocketTransport::new(config).await;
        assert!(result.is_err());
    }

    // Note: More comprehensive tests would require a test WebSocket server
    // For now, we focus on configuration and basic validation tests
}