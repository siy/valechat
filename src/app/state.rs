use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug};

use crate::app::config::AppConfig;
use crate::error::Result;
use crate::platform::{AppPaths, SecureStorageManager};
use crate::storage::{Database, ConversationRepository, UsageRepository};

pub struct AppState {
    config: Arc<RwLock<AppConfig>>,
    paths: AppPaths,
    secure_storage: SecureStorageManager,
    database: Database,
    conversation_repo: ConversationRepository,
    usage_repo: UsageRepository,
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
        self.secure_storage.retrieve_api_key(provider).await
    }

    pub async fn set_api_key(&self, provider: &str, api_key: &str) -> Result<()> {
        self.secure_storage.store_api_key(provider, api_key).await?;
        
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