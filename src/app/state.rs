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

pub struct AppState {
    config: Arc<RwLock<AppConfig>>,
    paths: AppPaths,
    secure_storage: SecureStorageManager,
    database: Database,
    conversation_repo: ConversationRepository,
    usage_repo: UsageRepository,
    api_key_cache: Arc<RwLock<HashMap<String, String>>>,
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

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            paths,
            secure_storage,
            database,
            conversation_repo,
            usage_repo,
            api_key_cache: Arc::new(RwLock::new(HashMap::new())),
        })
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
                    let model = match preferred {
                        "openai" => &provider_config.default_model,
                        "anthropic" => &provider_config.default_model,
                        "gemini" => &provider_config.default_model,
                        _ => "gpt-3.5-turbo",
                    };
                    (preferred, model)
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