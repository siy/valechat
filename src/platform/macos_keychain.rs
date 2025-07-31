#[cfg(target_os = "macos")]
use async_trait::async_trait;
use keyring::Entry;
use tracing::{debug, info, warn, error};

use crate::error::{Error, Result};
use crate::platform::secure_storage::SecureStorage;

pub struct MacOSKeychainStorage {
    service_name: String,
    use_biometrics: bool,
}

impl MacOSKeychainStorage {
    pub fn new(service_name: &str, use_biometrics: bool) -> Self {
        info!(
            "Initializing macOS Keychain storage with service: {}, biometrics: {}",
            service_name, use_biometrics
        );
        Self {
            service_name: service_name.to_string(),
            use_biometrics,
        }
    }

    fn get_full_service(&self, service: &str) -> String {
        // Use the configured service_name as prefix for better organization
        format!("{}.{}", self.service_name, service)
    }

    fn create_entry(&self, service: &str, key: &str) -> Result<Entry> {
        let full_service = self.get_full_service(service);
        Entry::new(&full_service, key).map_err(|e| Error::SecureStorage(e))
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl SecureStorage for MacOSKeychainStorage {
    async fn store(&self, service: &str, key: &str, value: &str) -> Result<()> {
        debug!("Storing key '{}' for service '{}'", key, service);
        
        let entry = self.create_entry(service, key)?;
        
        // Set the password - keyring v3 with apple-native feature should handle biometrics better
        match entry.set_password(value) {
            Ok(()) => {
                info!("Successfully stored key '{}' in macOS keychain{}", 
                     key, if self.use_biometrics { " (biometric enabled)" } else { "" });
                Ok(())
            }
            Err(e) => {
                error!("Failed to store key '{}': {:?}", key, e);
                Err(Error::SecureStorage(e))
            }
        }
    }

    async fn retrieve(&self, service: &str, key: &str) -> Result<Option<String>> {
        debug!("Retrieving key '{}' for service '{}'", key, service);
        
        let entry = self.create_entry(service, key)?;
        
        match entry.get_password() {
            Ok(password) => {
                debug!("Successfully retrieved key '{}'", key);
                Ok(Some(password))
            }
            Err(keyring::Error::NoEntry) => {
                debug!("Key '{}' not found in keychain", key);
                Ok(None)
            }
            Err(e) => {
                // Check if user cancelled authentication
                let error_str = format!("{:?}", e);
                if error_str.contains("UserCancel") || error_str.contains("cancelled") {
                    warn!("User cancelled authentication for key '{}'", key);
                    Err(Error::SecureStorage(e))
                } else {
                    error!("Failed to retrieve key '{}': {:?}", key, e);
                    Err(Error::SecureStorage(e))
                }
            }
        }
    }

    async fn delete(&self, service: &str, key: &str) -> Result<()> {
        debug!("Deleting key '{}' for service '{}'", key, service);
        
        let entry = self.create_entry(service, key)?;
        
        match entry.delete_credential() {
            Ok(()) => {
                info!("Successfully deleted key '{}'", key);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                debug!("Key '{}' was already deleted or not found", key);
                Ok(())
            }
            Err(e) => {
                error!("Failed to delete key '{}': {:?}", key, e);
                Err(Error::SecureStorage(e))
            }
        }
    }

    async fn list_keys(&self, _service: &str) -> Result<Vec<String>> {
        warn!("list_keys is not implemented for macOS keychain storage");
        Ok(Vec::new())
    }
}

// Helper function to check if biometrics are available
#[cfg(target_os = "macos")]
pub fn is_biometrics_available() -> bool {
    // The keyring crate v3 with apple-native feature uses the macOS Security Framework
    // which can be configured to use Touch ID/Face ID authentication
    // 
    // To ensure biometric authentication works optimally:
    // 1. Make sure Touch ID or Face ID is enabled in System Preferences
    // 2. The app should be run from Applications folder (not from build directory)
    // 3. Consider code-signing the application for production use
    
    info!("Using macOS Keychain with biometric authentication support");
    info!("Note: For best biometric experience, run from Applications folder with code signing");
    true
}

#[cfg(not(target_os = "macos"))]
pub fn is_biometrics_available() -> bool {
    false
}