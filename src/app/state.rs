use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use tracing::{info, debug};

use crate::app::config::AppConfig;
use crate::error::Result;
use crate::platform::{AppPaths, SecureStorageManager};
use crate::storage::{Database, ConversationRepository, UsageRepository};
use crate::chat::types::{MessageContent, ChatMessage, MessageRole as ChatMessageRole};
use crate::models::provider::ModelProvider;
use crate::mcp::{MCPClient, MCPClientConfig, MCPServerManager};

pub struct AppState {
    config: Arc<RwLock<AppConfig>>,
    paths: AppPaths,
    secure_storage: SecureStorageManager,
    database: Database,
    conversation_repo: ConversationRepository,
    usage_repo: UsageRepository,
    api_key_cache: Arc<RwLock<HashMap<String, String>>>,
    mcp_client: Option<Arc<tokio::sync::Mutex<MCPClient>>>,
    mcp_server_manager: Arc<tokio::sync::Mutex<MCPServerManager>>,
}

impl AppState {
    pub async fn new(
        config: AppConfig,
        paths: AppPaths,
        secure_storage: SecureStorageManager,
    ) -> Result<Self> {
        info!("Initializing application state");

        // Initialize database
        let database = Database::new(&paths).await?;
        let pool = database.get_pool();
        
        // Initialize repositories
        let conversation_repo = ConversationRepository::new(pool.clone());
        let usage_repo = UsageRepository::new(pool.clone());

        // Initialize MCP server manager
        let mcp_server_manager = Arc::new(tokio::sync::Mutex::new(MCPServerManager::new()));
        
        // Initialize MCP client with server manager
        // Note: We'll initialize the client after creating AppState because MCPClient 
        // needs a cloned MCPServerManager, not a reference
        let mcp_client = None;

        let mut app_state = Self {
            config: Arc::new(RwLock::new(config.clone())),
            paths,
            secure_storage,
            database,
            conversation_repo,
            usage_repo,
            api_key_cache: Arc::new(RwLock::new(HashMap::new())),
            mcp_client,
            mcp_server_manager,
        };
        
        // Now initialize MCP client if there are MCP servers configured
        if !config.mcp_servers.is_empty() {
            let mcp_config = MCPClientConfig::default();
            let server_manager_clone = {
                let guard = app_state.mcp_server_manager.lock().await;
                (*guard).clone()
            };
            
            match MCPClient::new(server_manager_clone, mcp_config) {
                Ok(client) => {
                    app_state.mcp_client = Some(Arc::new(tokio::sync::Mutex::new(client)));
                    info!("MCP client initialized successfully");
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize MCP client: {}", e);
                }
            }
        }


        // Pre-populate API key cache (single keychain prompt)
        let enabled_providers: Vec<&str> = config.models.iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.as_str())
            .collect();
        
        if !enabled_providers.is_empty() {
            info!("Pre-loading API keys for {} enabled providers", enabled_providers.len());
            let _ = app_state.get_api_keys_batch_consolidated(&enabled_providers).await;
        }

        // Initialize MCP servers
        app_state.initialize_mcp_servers().await?;

        Ok(app_state)
    }

    pub fn get_config(&self) -> AppConfig {
        self.config.read().clone()
    }

    pub async fn update_config<F>(&self, updater: F) -> Result<()>
    where
        F: FnOnce(&mut AppConfig),
    {
        debug!("Updating application configuration");
        
        {
            let mut config = self.config.write();
            updater(&mut config);
            config.validate()?;
        }

        // Save the updated configuration
        let config = self.config.read().clone();
        config.save(&self.paths).await?;

        info!("Configuration updated and saved");
        Ok(())
    }

    pub async fn get_available_models(&self) -> Result<Vec<String>> {
        let config = self.config.read();
        let enabled_models = config.get_enabled_models();
        Ok(enabled_models.into_iter().map(|s| s.to_string()).collect())
    }

    pub async fn get_api_key(&self, provider: &str) -> Result<Option<String>> {
        // Check cache first
        {
            let cache = self.api_key_cache.read();
            if let Some(api_key) = cache.get(provider) {
                return Ok(Some(api_key.clone()));
            }
        }

        // If not in cache, retrieve from secure storage
        match self.secure_storage.retrieve_api_key(provider).await? {
            Some(api_key) => {
                // Cache the retrieved key
                {
                    let mut cache = self.api_key_cache.write();
                    cache.insert(provider.to_string(), api_key.clone());
                }
                Ok(Some(api_key))
            }
            None => Ok(None),
        }
    }

    /// Batch retrieve API keys for multiple providers to reduce keychain prompts
    pub async fn get_api_keys_batch(&self, providers: &[&str]) -> Result<std::collections::HashMap<String, String>> {
        let mut result = std::collections::HashMap::new();
        let mut uncached_providers = Vec::new();

        // First, check cache for all providers
        {
            let cache = self.api_key_cache.read();
            for &provider in providers {
                if let Some(api_key) = cache.get(provider) {
                    result.insert(provider.to_string(), api_key.clone());
                } else {
                    uncached_providers.push(provider);
                }
            }
        }

        // Batch retrieve uncached keys to minimize keychain access
        if !uncached_providers.is_empty() {
            let mut cache_updates = std::collections::HashMap::new();
            
            for &provider in &uncached_providers {
                if let Some(api_key) = self.secure_storage.retrieve_api_key(provider).await? {
                    result.insert(provider.to_string(), api_key.clone());
                    cache_updates.insert(provider.to_string(), api_key);
                }
            }

            // Update cache with all retrieved keys at once
            if !cache_updates.is_empty() {
                let mut cache = self.api_key_cache.write();
                for (provider, api_key) in cache_updates {
                    cache.insert(provider, api_key);
                }
            }
        }

        Ok(result)
    }

    /// Batch retrieve API keys (single keychain prompt)
    pub async fn get_api_keys_batch_consolidated(&self, providers: &[&str]) -> Result<std::collections::HashMap<String, String>> {
        let mut result = std::collections::HashMap::new();
        let mut uncached_providers = Vec::new();

        // First, check cache for all providers
        {
            let cache = self.api_key_cache.read();
            for &provider in providers {
                if let Some(api_key) = cache.get(provider) {
                    result.insert(provider.to_string(), api_key.clone());
                } else {
                    uncached_providers.push(provider);
                }
            }
        }

        // If we have uncached providers, retrieve the entire bundle at once (single keychain prompt)
        if !uncached_providers.is_empty() {
            debug!("Retrieving consolidated API key bundle for {} uncached providers", uncached_providers.len());
            
            match self.secure_storage.retrieve_api_key_bundle().await {
                Ok(bundle) => {
                    let mut cache_updates = std::collections::HashMap::new();
                    
                    // Extract keys for requested providers from the bundle
                    for &provider in &uncached_providers {
                        if let Some(api_key) = bundle.get_key(provider) {
                            result.insert(provider.to_string(), api_key.clone());
                            cache_updates.insert(provider.to_string(), api_key.clone());
                        }
                    }

                    // Update cache with all retrieved keys at once
                    if !cache_updates.is_empty() {
                        let mut cache = self.api_key_cache.write();
                        for (provider, api_key) in cache_updates {
                            cache.insert(provider, api_key);
                        }
                        debug!("Updated cache with {} API keys", result.len());
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to retrieve consolidated API key bundle: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(result)
    }

    pub async fn set_api_key(&self, provider: &str, api_key: &str) -> Result<()> {
        self.secure_storage.store_api_key(provider, api_key).await?;
        
        // Update cache
        {
            let mut cache = self.api_key_cache.write();
            cache.insert(provider.to_string(), api_key.to_string());
        }
        
        // Enable the model provider if API key is successfully stored
        self.update_config(|config| {
            if let Some(model_config) = config.models.get_mut(provider) {
                model_config.enabled = true;
                info!("Enabled model provider: {}", provider);
            }
        }).await?;

        Ok(())
    }

    pub async fn remove_api_key(&self, provider: &str) -> Result<()> {
        self.secure_storage.delete_api_key(provider).await?;
        
        // Remove from cache
        {
            let mut cache = self.api_key_cache.write();
            cache.remove(provider);
        }
        
        // Disable the model provider when API key is removed
        self.update_config(|config| {
            if let Some(model_config) = config.models.get_mut(provider) {
                model_config.enabled = false;
                info!("Disabled model provider: {}", provider);
            }
        }).await?;

        Ok(())
    }

    pub fn get_paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn get_conversation_repo(&self) -> &ConversationRepository {
        &self.conversation_repo
    }

    pub fn get_message_repo(&self) -> &ConversationRepository {
        &self.conversation_repo
    }

    pub fn get_usage_repo(&self) -> &UsageRepository {
        &self.usage_repo
    }

    pub fn get_database(&self) -> &Database {
        &self.database
    }

    pub async fn validate_provider_credentials(&self, provider: &str) -> Result<bool> {
        match self.get_api_key(provider).await? {
            Some(api_key) if !api_key.is_empty() => {
                debug!("API key found for provider: {}, performing health check", provider);
                
                // Create a provider instance and test the credentials
                let provider_result = match provider {
                    "openai" => {
                        match crate::models::OpenAIProvider::new(api_key) {
                            Ok(provider) => Some(Box::new(provider) as Box<dyn crate::models::provider::ModelProvider>),
                            Err(e) => {
                                debug!("Failed to create OpenAI provider: {}", e);
                                None
                            }
                        }
                    }
                    "anthropic" => {
                        match crate::models::AnthropicProvider::new(api_key) {
                            Ok(provider) => Some(Box::new(provider) as Box<dyn crate::models::provider::ModelProvider>),
                            Err(e) => {
                                debug!("Failed to create Anthropic provider: {}", e);
                                None
                            }
                        }
                    }
                    "gemini" => {
                        match crate::models::GeminiProvider::new(api_key) {
                            Ok(provider) => Some(Box::new(provider) as Box<dyn crate::models::provider::ModelProvider>),
                            Err(e) => {
                                debug!("Failed to create Gemini provider: {}", e);
                                None
                            }
                        }
                    }
                    _ => {
                        debug!("Unknown provider: {}", provider);
                        None
                    }
                };

                if let Some(provider_instance) = provider_result {
                    // Perform health check to validate credentials
                    match provider_instance.health_check().await {
                        Ok(health_status) => {
                            if health_status.is_healthy {
                                debug!("Credentials valid for provider: {} (response time: {:?}ms)", 
                                       provider, health_status.response_time_ms);
                                Ok(true)
                            } else {
                                debug!("Credentials invalid for provider: {} (error: {:?})", 
                                       provider, health_status.error_message);
                                Ok(false)
                            }
                        }
                        Err(e) => {
                            debug!("Health check failed for provider: {} (error: {})", provider, e);
                            Ok(false)
                        }
                    }
                } else {
                    debug!("Failed to create provider instance for: {}", provider);
                    Ok(false)
                }
            }
            _ => {
                debug!("No API key found for provider: {}", provider);
                Ok(false)
            }
        }
    }

    pub async fn get_provider_status(&self, provider: &str) -> Result<ProviderStatus> {
        let config = self.get_config();
        let model_config = config.models.get(provider);
        
        match model_config {
            Some(config) if config.enabled => {
                let has_credentials = self.validate_provider_credentials(provider).await?;
                if has_credentials {
                    Ok(ProviderStatus::Ready)
                } else {
                    Ok(ProviderStatus::MissingCredentials)
                }
            }
            Some(_) => Ok(ProviderStatus::Disabled),
            None => Ok(ProviderStatus::NotConfigured),
        }
    }

    /// Get the default provider and model based on configuration priority
    pub fn get_default_provider_and_model(&self) -> Result<(String, String)> {
        let config = self.get_config();
        let models_by_priority = config.get_models_by_priority();
        
        if let Some((provider_name, model_config)) = models_by_priority.first() {
            Ok((provider_name.to_string(), model_config.default_model.clone()))
        } else {
            // Fallback if no providers are enabled - this matches the current behavior
            // but should ideally be an error case that prompts user to configure providers
            Ok(("openai".to_string(), "gpt-3.5-turbo".to_string()))
        }
    }

    /// Send a message in a conversation and get the AI response
    pub async fn send_message(&self, conversation_id: &str, content: &str) -> Result<String> {
        self.send_message_with_provider(conversation_id, content, None).await
    }

    /// Send a message in a conversation with a specific provider
    pub async fn send_message_with_provider(
        &self, 
        conversation_id: &str, 
        content: &str, 
        preferred_provider: Option<&str>
    ) -> Result<String> {
        // Get the conversation to find the preferred model
        let conversation = self.conversation_repo.get_conversation(conversation_id).await?
            .ok_or_else(|| crate::error::Error::chat("Conversation not found"))?;

        let config = self.get_config();
        let (provider_name, model_name) = if let Some(preferred) = preferred_provider {
            // Use the preferred provider if specified and enabled
            if let Some(provider_config) = config.models.get(preferred) {
                if provider_config.enabled {
                    (preferred, provider_config.default_model.as_str())
                } else {
                    return Err(crate::error::Error::chat(format!("Provider {} is not enabled", preferred)));
                }
            } else {
                return Err(crate::error::Error::chat(format!("Provider {} is not configured", preferred)));
            }
        } else {
            // Use conversation's preferred provider or find the first enabled provider
            let mut provider_name = conversation.model_provider.as_str();
            let mut model_name = conversation.model_name.as_str();

            // Check if conversation's provider is still enabled
            if let Some(provider_config) = config.models.get(provider_name) {
                if !provider_config.enabled {
                    // Fall back to first enabled provider
                    if let Some((name, provider_config)) = config.models.iter().find(|(_, config)| config.enabled) {
                        provider_name = name;
                        model_name = &provider_config.default_model;
                    } else {
                        return Err(crate::error::Error::chat("No enabled providers found"));
                    }
                }
            } else {
                // Provider no longer exists, fall back to first enabled
                if let Some((name, provider_config)) = config.models.iter().find(|(_, config)| config.enabled) {
                    provider_name = name;
                    model_name = &provider_config.default_model;
                } else {
                    return Err(crate::error::Error::chat("No enabled providers found"));
                }
            }
            
            (provider_name, model_name)
        };

        // Get API key for the provider
        let api_key = self.get_api_key(provider_name).await?
            .ok_or_else(|| crate::error::Error::chat(format!("No API key for provider: {}", provider_name)))?;

        // Create a simple provider instance and send the message
        let response = match provider_name {
            "openai" => {
                use crate::models::OpenAIProvider;
                let provider = OpenAIProvider::new(api_key)?;
                
                // Build conversation history
                let messages = self.conversation_repo.get_messages(conversation_id).await?;
                let mut provider_messages = Vec::new();
                
                // Add system message if exists
                if let Some(ref system_prompt) = conversation.system_prompt {
                    if !system_prompt.is_empty() {
                        provider_messages.push(crate::models::provider::Message::new(
                            crate::models::provider::MessageRole::System,
                            system_prompt.clone(),
                        ));
                    }
                }
                
                // Add conversation history
                for msg in messages {
                    let role = if msg.role == ChatMessageRole::User {
                        crate::models::provider::MessageRole::User
                    } else {
                        crate::models::provider::MessageRole::Assistant
                    };
                    if let Some(text) = msg.content.get_text() {
                        provider_messages.push(crate::models::provider::Message::new(role, text.to_string()));
                    }
                }
                
                // Add current user message
                provider_messages.push(crate::models::provider::Message::new(
                    crate::models::provider::MessageRole::User,
                    content.to_string(),
                ));
                
                let request = crate::models::provider::ChatRequest::new(provider_messages, model_name.to_string());
                let response = provider.send_message(request).await?;
                response.content
            }
            "anthropic" => {
                use crate::models::AnthropicProvider;
                let provider = AnthropicProvider::new(api_key)?;
                
                // Similar message building for Anthropic
                let messages = self.conversation_repo.get_messages(conversation_id).await?;
                let mut provider_messages = Vec::new();
                
                for msg in messages {
                    let role = if msg.role == ChatMessageRole::User {
                        crate::models::provider::MessageRole::User
                    } else {
                        crate::models::provider::MessageRole::Assistant
                    };
                    if let Some(text) = msg.content.get_text() {
                        provider_messages.push(crate::models::provider::Message::new(role, text.to_string()));
                    }
                }
                
                provider_messages.push(crate::models::provider::Message::new(
                    crate::models::provider::MessageRole::User,
                    content.to_string(),
                ));
                
                let request = crate::models::provider::ChatRequest::new(provider_messages, model_name.to_string());
                let response = provider.send_message(request).await?;
                response.content
            }
            "gemini" => {
                use crate::models::GeminiProvider;
                let provider = GeminiProvider::new(api_key)?;
                
                // Build conversation history for Gemini
                let messages = self.conversation_repo.get_messages(conversation_id).await?;
                let mut provider_messages = Vec::new();
                
                // Add system message if exists
                if let Some(ref system_prompt) = conversation.system_prompt {
                    if !system_prompt.is_empty() {
                        provider_messages.push(crate::models::provider::Message::new(
                            crate::models::provider::MessageRole::System,
                            system_prompt.clone(),
                        ));
                    }
                }
                
                // Add conversation history
                for msg in messages {
                    let role = if msg.role == ChatMessageRole::User {
                        crate::models::provider::MessageRole::User
                    } else {
                        crate::models::provider::MessageRole::Assistant
                    };
                    if let Some(text) = msg.content.get_text() {
                        provider_messages.push(crate::models::provider::Message::new(role, text.to_string()));
                    }
                }
                
                // Add current user message
                provider_messages.push(crate::models::provider::Message::new(
                    crate::models::provider::MessageRole::User,
                    content.to_string(),
                ));
                
                let request = crate::models::provider::ChatRequest::new(provider_messages, model_name.to_string());
                let response = provider.send_message(request).await?;
                response.content
            }
            _ => return Err(crate::error::Error::chat(format!("Unsupported provider: {}", provider_name))),
        };

        // Save both user and assistant messages
        let user_msg = ChatMessage::new(
            conversation_id.to_string(),
            ChatMessageRole::User,
            MessageContent::text(content.to_string()),
        );
        
        let assistant_msg = ChatMessage::new(
            conversation_id.to_string(),
            ChatMessageRole::Assistant,
            MessageContent::text(response.clone()),
        );
        
        self.conversation_repo.create_message(&user_msg).await?;
        self.conversation_repo.create_message(&assistant_msg).await?;
        
        Ok(response)
    }

    /// Initialize MCP servers based on configuration
    async fn initialize_mcp_servers(&self) -> Result<()> {
        let config = self.get_config();
        let enabled_servers = config.get_enabled_mcp_servers();
        
        if enabled_servers.is_empty() {
            info!("No MCP servers enabled in configuration");
            return Ok(());
        }

        info!("Initializing {} MCP servers", enabled_servers.len());
        
        let mut server_manager = self.mcp_server_manager.lock().await;
        
        for server_name in enabled_servers {
            if let Some(server_config) = config.mcp_servers.get(server_name) {
                if server_config.auto_start {
                    info!("Adding and starting MCP server: {}", server_name);
                    
                    // Add the server to the manager
                    if let Err(e) = server_manager.add_server(server_name.to_string(), server_config.clone()).await {
                        tracing::warn!("Failed to add MCP server {}: {}", server_name, e);
                        continue;
                    }
                    
                    // Start the server
                    match server_manager.start_server(server_name).await {
                        Ok(_) => info!("Successfully started MCP server: {}", server_name),
                        Err(e) => tracing::warn!("Failed to start MCP server {}: {}", server_name, e),
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Get the MCP client
    pub fn get_mcp_client(&self) -> Option<Arc<tokio::sync::Mutex<MCPClient>>> {
        self.mcp_client.as_ref().map(Arc::clone)
    }

    /// Get the MCP server manager
    pub fn get_mcp_server_manager(&self) -> Arc<tokio::sync::Mutex<MCPServerManager>> {
        Arc::clone(&self.mcp_server_manager)
    }

    /// List available MCP tools
    pub async fn list_mcp_tools(&self) -> Result<HashMap<String, Vec<crate::mcp::Tool>>> {
        if let Some(client) = &self.mcp_client {
            let client = client.lock().await;
            client.list_tools().await
        } else {
            Ok(HashMap::new())
        }
    }

    /// Execute an MCP tool
    pub async fn execute_mcp_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<crate::mcp::ToolResult> {
        if let Some(client) = &self.mcp_client {
            let client = client.lock().await;
            client.call_tool(server_name, tool_name, arguments, None).await
        } else {
            Err(crate::error::Error::mcp("MCP client not initialized".to_string()))
        }
    }

    /// Check if a specific MCP server is running
    pub async fn is_mcp_server_running(&self, server_name: &str) -> bool {
        let server_manager = self.mcp_server_manager.lock().await;
        server_manager.get_server(server_name).await == Some(crate::mcp::ServerState::Ready)
    }

    /// Start an MCP server
    pub async fn start_mcp_server(&self, server_name: &str) -> Result<()> {
        let config = self.get_config();
        
        if let Some(server_config) = config.mcp_servers.get(server_name) {
            let mut server_manager = self.mcp_server_manager.lock().await;
            
            // Add the server if not already added
            if server_manager.get_server(server_name).await.is_none() {
                server_manager.add_server(server_name.to_string(), server_config.clone()).await?;
            }
            
            // Start the server
            server_manager.start_server(server_name).await
        } else {
            Err(crate::error::Error::mcp(format!("MCP server {} not found in configuration", server_name)))
        }
    }

    /// Stop an MCP server
    pub async fn stop_mcp_server(&self, server_name: &str) -> Result<()> {
        let mut server_manager = self.mcp_server_manager.lock().await;
        server_manager.stop_server(server_name).await
    }

    /// Get status of all MCP servers
    pub async fn get_mcp_server_status(&self) -> HashMap<String, (crate::mcp::ServerState, crate::mcp::ServerHealth)> {
        let server_manager = self.mcp_server_manager.lock().await;
        server_manager.get_server_status().await
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderStatus {
    Ready,
    Disabled,
    MissingCredentials,
    NotConfigured,
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_status_enum() {
        let status = ProviderStatus::Ready;
        assert_eq!(status, ProviderStatus::Ready);
        
        let status = ProviderStatus::MissingCredentials;
        assert_eq!(status, ProviderStatus::MissingCredentials);
    }
}