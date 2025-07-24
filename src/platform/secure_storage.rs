use async_trait::async_trait;
use keyring::Entry;
use tracing::{debug, warn};

use crate::error::{Error, Result};

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
        debug!("Storing API key for provider: {}", provider);
        
        let result = self.backend.store("ai.valechat.api_keys", provider, key).await;
        
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
        debug!("Retrieving API key for provider: {}", provider);
        
        let result = self.backend.retrieve("ai.valechat.api_keys", provider).await;
        
        match &result {
            Ok(Some(_)) => {
                self.log_key_access("retrieve", provider, true).await?;
                debug!("Successfully retrieved API key for provider: {}", provider);
            }
            Ok(None) => {
                self.log_key_access("retrieve", provider, true).await?;
                debug!("No API key found for provider: {}", provider);
            }
            Err(_) => {
                self.log_key_access("retrieve", provider, false).await?;
                warn!("Failed to retrieve API key for provider: {}", provider);
            }
        }
        
        result
    }

    pub async fn delete_api_key(&self, provider: &str) -> Result<()> {
        debug!("Deleting API key for provider: {}", provider);
        
        let result = self.backend.delete("ai.valechat.api_keys", provider).await;
        
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
        match entry.delete_password() {
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
    // Use keyring for all platforms - it handles platform-specific backends internally
    Ok(Box::new(KeyringStorage))
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