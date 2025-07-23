use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::app::AppState;
use crate::chat::types::{
    ChatMessage, ChatSession, ChatResponse, ChatError, MessageRole, MessageContent,
    ToolInvocation, ResponseUsage, SessionStatus
};
use crate::error::{Error, Result};
use crate::mcp::{MCPClient, MCPClientConfig, MCPServerManager};
use crate::models::provider::{ModelProvider, Message};
use crate::models::{OpenAIProvider, AnthropicProvider, GeminiProvider};

/// Main chat service that coordinates AI models and MCP tools
pub struct ChatService {
    app_state: Arc<AppState>,
    mcp_client: Arc<MCPClient>,
    sessions: Arc<RwLock<HashMap<String, ChatSession>>>,
    messages: Arc<RwLock<HashMap<String, Vec<ChatMessage>>>>, // session_id -> messages
    model_providers: Arc<RwLock<HashMap<String, Box<dyn ModelProvider>>>>,
    config: ChatServiceConfig,
}

/// Configuration for the chat service
#[derive(Debug, Clone)]
pub struct ChatServiceConfig {
    pub max_sessions: usize,
    pub session_timeout: Duration,
    pub max_messages_per_session: usize,
    pub enable_mcp_tools: bool,
    pub tool_timeout: Duration,
    pub max_concurrent_tool_calls: usize,
}

impl Default for ChatServiceConfig {
    fn default() -> Self {
        Self {
            max_sessions: 1000,
            session_timeout: Duration::from_secs(24 * 60 * 60), // 24 hours
            max_messages_per_session: 10000,
            enable_mcp_tools: true,
            tool_timeout: Duration::from_secs(30),
            max_concurrent_tool_calls: 10,
        }
    }
}

impl ChatService {
    /// Create a new chat service
    pub async fn new(app_state: Arc<AppState>, config: ChatServiceConfig) -> Result<Self> {
        info!("Initializing chat service");

        // Initialize MCP server manager and client
        let server_manager = MCPServerManager::new();
        let mcp_config = MCPClientConfig {
            request_timeout: config.tool_timeout,
            max_concurrent_requests: config.max_concurrent_tool_calls,
            ..MCPClientConfig::default()
        };
        let mcp_client = Arc::new(MCPClient::new(server_manager, mcp_config)?);

        // Initialize model providers
        let model_providers = Arc::new(RwLock::new(HashMap::new()));

        let service = Self {
            app_state,
            mcp_client,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(HashMap::new())),
            model_providers,
            config,
        };

        // Initialize available model providers
        service.initialize_model_providers().await?;

        info!("Chat service initialized successfully");
        Ok(service)
    }

    /// Initialize available model providers based on configuration
    async fn initialize_model_providers(&self) -> Result<()> {
        let app_config = self.app_state.get_config();
        let mut providers = self.model_providers.write().await;

        // Initialize OpenAI provider if configured
        if let Some(openai_config) = app_config.models.get("openai") {
            if openai_config.enabled {
                if let Ok(Some(api_key)) = self.app_state.get_api_key("openai").await {
                    let provider = OpenAIProvider::new(api_key)?;
                    providers.insert("openai".to_string(), Box::new(provider));
                    debug!("OpenAI provider initialized");
                }
            }
        }

        // Initialize Anthropic provider if configured
        if let Some(anthropic_config) = app_config.models.get("anthropic") {
            if anthropic_config.enabled {
                if let Ok(Some(api_key)) = self.app_state.get_api_key("anthropic").await {
                    let provider = AnthropicProvider::new(api_key)?;
                    providers.insert("anthropic".to_string(), Box::new(provider));
                    debug!("Anthropic provider initialized");
                }
            }
        }

        // Initialize Gemini provider if configured
        if let Some(gemini_config) = app_config.models.get("google") {
            if gemini_config.enabled {
                if let Ok(Some(api_key)) = self.app_state.get_api_key("google").await {
                    let provider = GeminiProvider::new(api_key)?;
                    providers.insert("google".to_string(), Box::new(provider));
                    debug!("Gemini provider initialized");
                }
            }
        }

        info!("Initialized {} model providers", providers.len());
        Ok(())
    }

    /// Create a new chat session
    pub async fn create_session(
        &self,
        title: impl Into<String>,
        model_provider: impl Into<String>,
        model_name: impl Into<String>,
        system_prompt: Option<String>,
    ) -> Result<ChatSession> {
        let provider_name = model_provider.into();
        let model_name = model_name.into();

        // Verify the model provider is available
        {
            let providers = self.model_providers.read().await;
            if !providers.contains_key(&provider_name) {
                return Err(ChatError::ModelProviderUnavailable {
                    provider: provider_name,
                }.into());
            }
        }

        let mut session = ChatSession::new(title, provider_name, model_name);
        if let Some(prompt) = system_prompt {
            session = session.with_system_prompt(prompt);
        }

        // Store the session
        {
            let mut sessions = self.sessions.write().await;
            
            // Check session limit
            if sessions.len() >= self.config.max_sessions {
                warn!("Maximum session limit reached, cleaning up old sessions");
                self.cleanup_old_sessions().await;
            }
            
            sessions.insert(session.id.clone(), session.clone());
        }

        // Initialize empty message list for the session
        {
            let mut messages = self.messages.write().await;
            messages.insert(session.id.clone(), Vec::new());
        }

        info!("Created new chat session: {}", session.id);
        Ok(session)
    }

    /// Send a message in a chat session
    pub async fn send_message(
        &self,
        session_id: &str,
        content: MessageContent,
        enable_tools: Option<bool>,
    ) -> Result<ChatResponse> {
        let start_time = Instant::now();
        
        // Get the session
        let mut session = {
            let mut sessions = self.sessions.write().await;
            sessions.get_mut(session_id)
                .ok_or_else(|| ChatError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?
                .clone()
        };

        // Create user message
        let user_message = ChatMessage::new(session_id.to_string(), MessageRole::User, content);
        
        // Add user message to session
        {
            let mut messages = self.messages.write().await;
            let session_messages = messages.get_mut(session_id)
                .ok_or_else(|| ChatError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;
            
            // Check message limit
            if session_messages.len() >= self.config.max_messages_per_session {
                return Err(Error::Chat("Message limit exceeded for session".to_string()));
            }
            
            session_messages.push(user_message.clone());
        }

        // Check if model provider exists and get conversation history
        {
            let providers = self.model_providers.read().await;
            if !providers.contains_key(&session.model_provider) {
                return Err(ChatError::ModelProviderUnavailable {
                    provider: session.model_provider.clone(),
                }.into());
            }
        }

        // Get conversation history
        let conversation_history = self.get_conversation_history(session_id).await?;

        // Convert to provider format
        let provider_messages = self.convert_to_provider_messages(&conversation_history)?;

        // Check if tools should be enabled for this request
        let tools_enabled = enable_tools.unwrap_or(session.settings.enable_tools) && self.config.enable_mcp_tools;
        
        // Get available tools if enabled
        let _available_tools = if tools_enabled {
            match self.get_available_tools(&session).await {
                Ok(tools) => tools,
                Err(e) => {
                    warn!("Failed to get available tools: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Create chat request
        let mut chat_request = crate::models::provider::ChatRequest::new(
            provider_messages,
            session.model_name.clone(),
        );

        if let Some(temp) = session.settings.temperature {
            chat_request.temperature = Some(temp);
        }
        if let Some(max_tokens) = session.settings.max_tokens {
            chat_request.max_tokens = Some(max_tokens);
        }

        // Generate response from AI model
        let model_response = {
            let providers = self.model_providers.read().await;
            let provider = providers.get(&session.model_provider).unwrap(); // We checked earlier
            provider.send_message(chat_request).await?
        };

        // Handle tool calls if present (TODO: implement when ModelProvider supports tools)
        let tool_invocations = Vec::new();
        let final_content = model_response.content.clone();

        // Create assistant response message
        let assistant_message = ChatMessage::new(
            session_id.to_string(),
            MessageRole::Assistant,
            MessageContent::text(final_content),
        ).with_tool_invocations(tool_invocations.clone());

        // Add assistant message to session
        {
            let mut messages = self.messages.write().await;
            let session_messages = messages.get_mut(session_id)
                .ok_or_else(|| ChatError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;
            session_messages.push(assistant_message.clone());
        }

        // Update session metrics
        session.increment_message_count();
        for invocation in &tool_invocations {
            if let Some(duration) = invocation.duration_ms {
                session.add_tool_usage(&invocation.tool_name, duration);
            }
        }

        // Update session in storage
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.to_string(), session);
        }

        let processing_time = start_time.elapsed().as_millis() as u64;
        
        // Create response
        let usage = if let Some(token_usage) = model_response.usage {
            ResponseUsage::new(
                token_usage.input_tokens,
                token_usage.output_tokens,
                0.0, // TODO: calculate cost based on pricing
            )
        } else {
            ResponseUsage::default()
        };

        let response = ChatResponse::new(assistant_message, usage, processing_time)
            .with_tool_calls(tool_invocations);

        info!("Generated response for session {} in {}ms", session_id, processing_time);
        Ok(response)
    }

    /// Get conversation history for a session
    pub async fn get_conversation_history(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let messages = self.messages.read().await;
        let session_messages = messages.get(session_id)
            .ok_or_else(|| ChatError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
        
        Ok(session_messages.clone())
    }

    /// Get a specific session
    pub async fn get_session(&self, session_id: &str) -> Result<ChatSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id)
            .cloned()
            .ok_or_else(|| ChatError::SessionNotFound {
                session_id: session_id.to_string(),
            }.into())
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<ChatSession> {
        let sessions = self.sessions.read().await;
        sessions.values()
            .filter(|session| session.status == SessionStatus::Active)
            .cloned()
            .collect()
    }

    /// Delete a session
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id);
        }
        
        {
            let mut messages = self.messages.write().await;
            messages.remove(session_id);
        }

        info!("Deleted session: {}", session_id);
        Ok(())
    }

    /// Get available MCP tools for a session
    async fn get_available_tools(&self, session: &ChatSession) -> Result<Vec<serde_json::Value>> {
        let all_tools = self.mcp_client.list_tools().await?;
        let mut available_tools = Vec::new();

        for (server_name, tools) in all_tools {
            // Filter by allowed servers if specified
            if !session.settings.allowed_servers.is_empty() && 
               !session.settings.allowed_servers.contains(&server_name) {
                continue;
            }

            for tool in tools {
                let tool_spec = serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": format!("{}:{}", server_name, tool.name),
                        "description": tool.description,
                        "parameters": tool.input_schema.unwrap_or_else(|| serde_json::json!({}))
                    }
                });
                available_tools.push(tool_spec);
            }
        }

        debug!("Found {} available tools for session {}", available_tools.len(), session.id);
        Ok(available_tools)
    }

    /// Execute tool calls from the AI model
    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<serde_json::Value>,
        _session: &ChatSession,
    ) -> Result<Vec<ToolInvocation>> {
        let mut invocations = Vec::new();

        for tool_call in tool_calls {
            let tool_name = tool_call.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .ok_or_else(|| Error::Chat("Invalid tool call format".to_string()))?;

            let arguments = tool_call.get("function")
                .and_then(|f| f.get("arguments"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            // Parse server and tool name
            let parts: Vec<&str> = tool_name.split(':').collect();
            if parts.len() != 2 {
                warn!("Invalid tool name format: {}", tool_name);
                continue;
            }

            let server_name = parts[0];
            let actual_tool_name = parts[1];

            let start_time = Instant::now();
            let mut invocation = ToolInvocation::new(
                actual_tool_name.to_string(),
                server_name.to_string(),
                arguments.clone(),
            );

            // Execute the tool call
            match self.mcp_client.call_tool(server_name, actual_tool_name, arguments, None).await {
                Ok(result) => {
                    let duration = start_time.elapsed().as_millis() as u64;
                    let result_json = serde_json::to_value(&result)
                        .unwrap_or_else(|_| serde_json::json!({"error": "Failed to serialize result"}));
                    invocation = invocation.with_result(result_json, duration);
                    
                    debug!("Tool call succeeded: {} on {} in {}ms", actual_tool_name, server_name, duration);
                }
                Err(e) => {
                    let duration = start_time.elapsed().as_millis() as u64;
                    invocation = invocation.with_error(e.to_string(), duration);
                    
                    warn!("Tool call failed: {} on {}: {}", actual_tool_name, server_name, e);
                }
            }

            invocations.push(invocation);
        }

        Ok(invocations)
    }

    /// Convert chat messages to provider format
    fn convert_to_provider_messages(&self, messages: &[ChatMessage]) -> Result<Vec<crate::models::provider::Message>> {
        let mut provider_messages = Vec::new();

        for message in messages {
            let role = match message.role {
                MessageRole::User => crate::models::provider::MessageRole::User,
                MessageRole::Assistant => crate::models::provider::MessageRole::Assistant,
                MessageRole::System => crate::models::provider::MessageRole::System,
                MessageRole::Tool => continue, // Skip tool messages for now
            };

            if let Some(text) = message.content.get_text() {
                provider_messages.push(crate::models::provider::Message::new(
                    role,
                    text.to_string(),
                ));
            }
        }

        Ok(provider_messages)
    }

    /// Format response content with tool results
    fn format_response_with_tools(&self, content: &str, tool_invocations: &[ToolInvocation]) -> String {
        let mut formatted = content.to_string();
        
        if !tool_invocations.is_empty() {
            formatted.push_str("\n\n**Tool Results:**\n");
            
            for invocation in tool_invocations {
                if let Some(result) = &invocation.result {
                    formatted.push_str(&format!(
                        "- **{}** ({}): {}\n",
                        invocation.tool_name,
                        invocation.server_name,
                        serde_json::to_string_pretty(result).unwrap_or_else(|_| "Invalid result".to_string())
                    ));
                } else if let Some(error) = &invocation.error {
                    formatted.push_str(&format!(
                        "- **{}** ({}): Error - {}\n",
                        invocation.tool_name,
                        invocation.server_name,
                        error
                    ));
                }
            }
        }

        formatted
    }

    /// Clean up old sessions
    async fn cleanup_old_sessions(&self) {
        let cutoff = std::time::SystemTime::now() - self.config.session_timeout;
        let cutoff_dt = chrono::DateTime::<chrono::Utc>::from(cutoff);

        let mut sessions_to_remove = Vec::new();
        
        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if session.updated_at < cutoff_dt {
                    sessions_to_remove.push(id.clone());
                }
            }
        }

        for session_id in sessions_to_remove {
            if let Err(e) = self.delete_session(&session_id).await {
                error!("Failed to cleanup old session {}: {}", session_id, e);
            }
        }
    }

    /// Get service statistics
    pub async fn get_statistics(&self) -> ChatServiceStatistics {
        let sessions = self.sessions.read().await;
        let messages = self.messages.read().await;
        let mcp_stats = self.mcp_client.get_statistics().await;

        let total_messages: usize = messages.values().map(|msgs| msgs.len()).sum();
        let active_sessions = sessions.values()
            .filter(|s| s.status == SessionStatus::Active)
            .count();

        ChatServiceStatistics {
            total_sessions: sessions.len(),
            active_sessions,
            total_messages,
            mcp_servers_ready: mcp_stats.ready_servers,
            mcp_servers_total: mcp_stats.total_servers,
        }
    }
}

/// Statistics about the chat service
#[derive(Debug, Clone)]
pub struct ChatServiceStatistics {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub total_messages: usize,
    pub mcp_servers_ready: usize,
    pub mcp_servers_total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppConfig;
    use crate::platform::AppPaths;
    use crate::platform::SecureStorageManager;
    use tempfile::TempDir;

    async fn create_test_app_state() -> Arc<AppState> {
        let _temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::new().unwrap();
        paths.ensure_dirs_exist().unwrap();
        
        let config = AppConfig::default();
        let secure_storage = SecureStorageManager::new().unwrap();
        
        Arc::new(AppState::new(config, paths, secure_storage).await.unwrap())
    }

    #[tokio::test]
    async fn test_chat_service_creation() {
        let app_state = create_test_app_state().await;
        let config = ChatServiceConfig::default();
        
        let service = ChatService::new(app_state, config).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_session_creation() {
        let app_state = create_test_app_state().await;
        let config = ChatServiceConfig::default();
        let service = ChatService::new(app_state, config).await.unwrap();

        // Note: This test will fail because no model providers are configured
        // In a real scenario, we would need to set up API keys and configure providers
        let result = service.create_session(
            "Test Chat",
            "openai",
            "gpt-3.5-turbo",
            Some("You are a helpful assistant.".to_string()),
        ).await;

        // Expect this to fail due to missing provider
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_service_statistics() {
        let app_state = create_test_app_state().await;
        let config = ChatServiceConfig::default();
        let service = ChatService::new(app_state, config).await.unwrap();

        let stats = service.get_statistics().await;
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.active_sessions, 0);
        assert_eq!(stats.total_messages, 0);
    }
}