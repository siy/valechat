use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug};

use crate::app::config::AppConfig;
use crate::error::Result;
use crate::platform::{AppPaths, SecureStorageManager};

pub struct AppState {
    config: Arc<RwLock<AppConfig>>,
    paths: AppPaths,
    secure_storage: SecureStorageManager,
}

impl AppState {
    pub async fn new(
        config: AppConfig,
        paths: AppPaths,
        secure_storage: SecureStorageManager,
    ) -> Result<Self> {
        info!("Initializing application state");

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            paths,
            secure_storage,
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

    pub async fn validate_provider_credentials(&self, provider: &str) -> Result<bool> {
        match self.get_api_key(provider).await? {
            Some(api_key) if !api_key.is_empty() => {
                // TODO: In Phase 2, we'll add actual credential validation
                // by making a test request to the provider
                debug!("API key found for provider: {}", provider);
                Ok(true)
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