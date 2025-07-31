use async_trait::async_trait;
use keyring::Entry;
use tracing::{debug, warn, info};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApiKeyBundle {
    pub keys: HashMap<String, String>,
    pub version: u32,
}

impl ApiKeyBundle {
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
            version: 1,
        }
    }

    pub fn get_key(&self, provider: &str) -> Option<&String> {
        self.keys.get(provider)
    }

    pub fn set_key(&mut self, provider: &str, key: &str) {
        self.keys.insert(provider.to_string(), key.to_string());
    }

    pub fn remove_key(&mut self, provider: &str) -> bool {
        self.keys.remove(provider).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| Error::platform(format!("Failed to serialize API keys: {}", e)))
    }

    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| Error::platform(format!("Failed to deserialize API keys: {}", e)))
    }
}

#[async_trait]
pub trait SecureStorage: Send + Sync {
    async fn store(&self, service: &str, key: &str, value: &str) -> Result<()>;
    async fn retrieve(&self, service: &str, key: &str) -> Result<Option<String>>;
    async fn delete(&self, service: &str, key: &str) -> Result<()>;
    async fn list_keys(&self, service: &str) -> Result<Vec<String>>;
}

pub struct SecureStorageManager {
    backend: Box<dyn SecureStorage>,
}

impl SecureStorageManager {
    pub fn new() -> Result<Self> {
        let backend = create_platform_storage()?;
        Ok(Self {
            backend,
        })
    }

    pub async fn store_api_key(&self, provider: &str, key: &str) -> Result<()> {
        debug!("Storing API key for provider: {} using consolidated storage", provider);
        
        // Get current bundle or create new one
        let mut bundle = self.retrieve_api_key_bundle().await?;
        bundle.set_key(provider, key);
        
        // Store the updated bundle
        let result = self.store_api_key_bundle(&bundle).await;
        
        if result.is_ok() {
            self.log_key_access("store", provider, true).await?;
            debug!("Successfully stored API key for provider: {}", provider);
        } else {
            self.log_key_access("store", provider, false).await?;
            warn!("Failed to store API key for provider: {}", provider);
        }
        
        result
    }

    pub async fn retrieve_api_key(&self, provider: &str) -> Result<Option<String>> {
        debug!("Retrieving API key for provider: {} using consolidated storage", provider);
        
        let bundle = self.retrieve_api_key_bundle().await?;
        let result = bundle.get_key(provider).cloned();
        
        match &result {
            Some(_) => {
                self.log_key_access("retrieve", provider, true).await?;
                debug!("Successfully retrieved API key for provider: {}", provider);
            }
            None => {
                self.log_key_access("retrieve", provider, true).await?;
                debug!("No API key found for provider: {}", provider);
            }
        }
        
        Ok(result)
    }

    pub async fn delete_api_key(&self, provider: &str) -> Result<()> {
        debug!("Deleting API key for provider: {} using consolidated storage", provider);
        
        // Get current bundle
        let mut bundle = self.retrieve_api_key_bundle().await?;
        let was_removed = bundle.remove_key(provider);
        
        let result = if was_removed {
            if bundle.is_empty() {
                // If bundle is empty, delete the entire entry
                self.backend.delete("ai.valechat.consolidated", "api_keys").await
            } else {
                // Otherwise, store the updated bundle
                self.store_api_key_bundle(&bundle).await
            }
        } else {
            // Key wasn't there to begin with, so it's effectively deleted
            Ok(())
        };
        
        if result.is_ok() {
            self.log_key_access("delete", provider, true).await?;
            debug!("Successfully deleted API key for provider: {}", provider);
        } else {
            self.log_key_access("delete", provider, false).await?;
            warn!("Failed to delete API key for provider: {}", provider);
        }
        
        result
    }

    pub async fn list_providers(&self) -> Result<Vec<String>> {
        self.backend.list_keys("ai.valechat.api_keys").await
    }

    /// Store all API keys in a single consolidated keychain entry
    pub async fn store_api_key_bundle(&self, bundle: &ApiKeyBundle) -> Result<()> {
        debug!("Storing consolidated API key bundle with {} keys", bundle.keys.len());
        
        let json = bundle.to_json()?;
        let result = self.backend.store("ai.valechat.consolidated", "api_keys", &json).await;
        
        if result.is_ok() {
            info!("Successfully stored consolidated API key bundle");
        } else {
            warn!("Failed to store consolidated API key bundle");
        }
        
        result
    }

    /// Retrieve all API keys from the consolidated keychain entry
    pub async fn retrieve_api_key_bundle(&self) -> Result<ApiKeyBundle> {
        debug!("Retrieving consolidated API key bundle");
        
        match self.backend.retrieve("ai.valechat.consolidated", "api_keys").await? {
            Some(json) => {
                let bundle = ApiKeyBundle::from_json(&json)?;
                debug!("Successfully retrieved consolidated API key bundle with {} keys", bundle.keys.len());
                Ok(bundle)
            }
            None => {
                debug!("No consolidated API key bundle found, returning empty bundle");
                Ok(ApiKeyBundle::new())
            }
        }
    }

    /// Migrate existing individual API keys to consolidated storage
    pub async fn migrate_to_consolidated_storage(&self) -> Result<()> {
        info!("Starting migration to consolidated API key storage");
        
        // Try to retrieve individual keys first
        let providers = vec!["openai", "anthropic", "gemini"];
        let mut bundle = self.retrieve_api_key_bundle().await?;
        let mut migrated_count = 0;
        
        for provider in &providers {
            if let Ok(Some(api_key)) = self.backend.retrieve("ai.valechat.api_keys", provider).await {
                if bundle.get_key(provider).is_none() {
                    bundle.set_key(provider, &api_key);
                    migrated_count += 1;
                    info!("Migrated API key for provider: {}", provider);
                    
                    // Clean up old individual entry
                    if let Err(e) = self.backend.delete("ai.valechat.api_keys", provider).await {
                        warn!("Failed to cleanup old API key for {}: {}", provider, e);
                    }
                }
            }
        }
        
        if migrated_count > 0 {
            self.store_api_key_bundle(&bundle).await?;
            info!("Successfully migrated {} API keys to consolidated storage", migrated_count);
        } else {
            debug!("No API keys to migrate");
        }
        
        Ok(())
    }

    async fn log_key_access(&self, operation: &str, provider: &str, success: bool) -> Result<()> {
        // In a real implementation, this would write to a secure audit log
        // For now, we just use tracing
        if success {
            debug!("Audit: {} operation successful for provider: {}", operation, provider);
        } else {
            warn!("Audit: {} operation failed for provider: {}", operation, provider);
        }
        Ok(())
    }
}

// Cross-platform storage implementation using keyring
pub struct KeyringStorage;

#[async_trait]
impl SecureStorage for KeyringStorage {
    async fn store(&self, service: &str, key: &str, value: &str) -> Result<()> {
        let entry = Entry::new(service, key)?;
        entry.set_password(value)?;
        Ok(())
    }

    async fn retrieve(&self, service: &str, key: &str) -> Result<Option<String>> {
        let entry = Entry::new(service, key)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::SecureStorage(e)),
        }
    }

    async fn delete(&self, service: &str, key: &str) -> Result<()> {
        let entry = Entry::new(service, key)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(Error::SecureStorage(e)),
        }
    }

    async fn list_keys(&self, _service: &str) -> Result<Vec<String>> {
        // Note: keyring crate doesn't support listing keys directly
        // In a real implementation, we might maintain a separate index
        // For now, return empty list
        warn!("list_keys not fully implemented for keyring backend");
        Ok(Vec::new())
    }
}

fn create_platform_storage() -> Result<Box<dyn SecureStorage>> {
    #[cfg(target_os = "macos")]
    {
        use crate::platform::macos_keychain::{MacOSKeychainStorage, is_biometrics_available};
        
        if is_biometrics_available() {
            tracing::info!("Using macOS Keychain with biometric authentication");
            Ok(Box::new(MacOSKeychainStorage::new("ai.valechat", true)))
        } else {
            tracing::info!("Using standard keyring (biometrics not available)");
            Ok(Box::new(KeyringStorage))
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        tracing::info!("Using standard keyring for non-macOS platform");
        Ok(Box::new(KeyringStorage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_secure_storage_roundtrip() {
        let storage = SecureStorageManager::new().unwrap();
        let _test_key = "test_api_key";
        let test_value = "sk-test123456789";

        // Store
        storage.store_api_key("test_provider", test_value).await.unwrap();

        // Retrieve
        let retrieved = storage.retrieve_api_key("test_provider").await.unwrap();
        assert_eq!(retrieved, Some(test_value.to_string()));

        // Delete
        storage.delete_api_key("test_provider").await.unwrap();

        // Verify deletion
        let retrieved_after_delete = storage.retrieve_api_key("test_provider").await.unwrap();
        assert_eq!(retrieved_after_delete, None);
    }
}