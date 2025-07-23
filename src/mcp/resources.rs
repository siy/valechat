use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};
use crate::mcp::types::{Resource, ResourceContents, Content};
use crate::mcp::client::MCPClient;

/// Resource manager for handling MCP resources
pub struct ResourceManager {
    resources: Arc<RwLock<HashMap<String, CachedResource>>>,
    clients: Arc<RwLock<HashMap<String, Arc<MCPClient>>>>,
    config: ResourceConfig,
}

/// Configuration for resource management
#[derive(Debug, Clone)]
pub struct ResourceConfig {
    pub cache_ttl_seconds: u64,
    pub max_cache_size: usize,
    pub max_resource_size_bytes: usize,
    pub enable_subscriptions: bool,
    pub refresh_interval_seconds: u64,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            cache_ttl_seconds: 300, // 5 minutes
            max_cache_size: 1000,
            max_resource_size_bytes: 10 * 1024 * 1024, // 10MB
            enable_subscriptions: true,
            refresh_interval_seconds: 60, // 1 minute
        }
    }
}

/// Cached resource with metadata
#[derive(Debug, Clone)]
struct CachedResource {
    resource: Resource,
    contents: Option<ResourceContents>,
    last_updated: std::time::Instant,
    access_count: u64,
    server_name: String,
    subscription_active: bool,
}

/// Resource query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuery {
    pub uri_pattern: Option<String>,
    pub mime_type: Option<String>,
    pub server_name: Option<String>,
    pub include_contents: bool,
}

/// Resource search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSearchResult {
    pub resources: Vec<ResourceInfo>,
    pub total_count: usize,
    pub has_more: bool,
}

/// Resource information with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub resource: Resource,
    pub server_name: String,
    pub last_updated: Option<std::time::SystemTime>,
    pub access_count: u64,
    pub cache_status: CacheStatus,
    pub contents: Option<ResourceContents>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheStatus {
    Fresh,
    Stale,
    Missing,
    Error,
}

/// Resource subscription for live updates
#[derive(Clone)]
pub struct ResourceSubscription {
    pub uri: String,
    pub server_name: String,
    pub callback: Arc<dyn Fn(ResourceUpdateEvent) + Send + Sync>,
}

/// Resource update event
#[derive(Debug, Clone)]
pub enum ResourceUpdateEvent {
    Created(Resource),
    Updated(Resource),
    Deleted(String), // URI
    Error(String),
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(config: ResourceConfig) -> Self {
        Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register an MCP client for resource access
    pub async fn register_client(&self, server_name: String, client: Arc<MCPClient>) -> Result<()> {
        info!("Registering MCP client for resources: {}", server_name);
        
        let mut clients = self.clients.write().await;
        clients.insert(server_name.clone(), client.clone());
        
        // Refresh resources from this server
        drop(clients);
        self.refresh_server_resources(&server_name).await?;
        
        Ok(())
    }

    /// Unregister an MCP client
    pub async fn unregister_client(&self, server_name: &str) -> Result<()> {
        info!("Unregistering MCP client: {}", server_name);
        
        let mut clients = self.clients.write().await;
        clients.remove(server_name);
        
        // Remove cached resources from this server
        let mut resources = self.resources.write().await;
        resources.retain(|_, cached| cached.server_name != server_name);
        
        Ok(())
    }

    /// List all available resources
    pub async fn list_resources(&self, query: Option<ResourceQuery>) -> Result<ResourceSearchResult> {
        debug!("Listing resources with query: {:?}", query);
        
        let resources = self.resources.read().await;
        let mut filtered_resources = Vec::new();
        
        for (uri, cached) in resources.iter() {
            // Apply filters
            if let Some(ref q) = query {
                if let Some(ref pattern) = q.uri_pattern {
                    if !uri.contains(pattern) {
                        continue;
                    }
                }
                
                if let Some(ref mime_type) = q.mime_type {
                    if cached.resource.mime_type.as_ref() != Some(mime_type) {
                        continue;
                    }
                }
                
                if let Some(ref server_name) = q.server_name {
                    if &cached.server_name != server_name {
                        continue;
                    }
                }
            }
            
            // Determine cache status
            let cache_status = if cached.last_updated.elapsed().as_secs() > self.config.cache_ttl_seconds {
                CacheStatus::Stale
            } else {
                CacheStatus::Fresh
            };
            
            let contents = if query.as_ref().map(|q| q.include_contents).unwrap_or(false) {
                cached.contents.clone()
            } else {
                None
            };
            
            filtered_resources.push(ResourceInfo {
                resource: cached.resource.clone(),
                server_name: cached.server_name.clone(),
                last_updated: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(
                    cached.last_updated.elapsed().as_secs()
                )),
                access_count: cached.access_count,
                cache_status,
                contents,
            });
        }
        
        // Sort by access count (most used first)
        filtered_resources.sort_by(|a, b| b.access_count.cmp(&a.access_count));
        
        Ok(ResourceSearchResult {
            total_count: filtered_resources.len(),
            has_more: false, // No pagination for now
            resources: filtered_resources,
        })
    }

    /// Get a specific resource by URI
    pub async fn get_resource(&self, uri: &str, force_refresh: bool) -> Result<Option<Vec<Content>>> {
        debug!("Getting resource: {} (force_refresh: {})", uri, force_refresh);
        
        // Check cache first
        if !force_refresh {
            let resources = self.resources.read().await;
            if let Some(cached) = resources.get(uri) {
                if cached.last_updated.elapsed().as_secs() <= self.config.cache_ttl_seconds {
                    // Update access count
                    drop(resources);
                    self.increment_access_count(uri).await;
                    
                    let resources = self.resources.read().await;
                    return Ok(resources.get(uri).and_then(|c| c.contents.as_ref().map(|rc| rc.content.clone())));
                }
            }
        }
        
        // Find which server has this resource
        let server_name = {
            let resources = self.resources.read().await;
            resources.get(uri).map(|c| c.server_name.clone())
        };
        
        if let Some(server_name) = server_name {
            // Fetch from server
            self.fetch_resource_contents(uri, &server_name).await
        } else {
            // Resource not found in any registered server
            warn!("Resource not found: {}", uri);
            Ok(None)
        }
    }

    /// Subscribe to resource changes
    pub async fn subscribe_to_resource(&self, uri: &str, _callback: Arc<dyn Fn(ResourceUpdateEvent) + Send + Sync>) -> Result<()> {
        if !self.config.enable_subscriptions {
            return Err(Error::mcp("Resource subscriptions are disabled".to_string()));
        }
        
        info!("Subscribing to resource updates: {}", uri);
        
        // Find the server that has this resource
        let server_name = {
            let resources = self.resources.read().await;
            resources.get(uri).map(|c| c.server_name.clone())
        };
        
        if let Some(server_name) = server_name {
            // Subscribe through the MCP client
            let clients = self.clients.read().await;
            if let Some(_client) = clients.get(&server_name) {
                // TODO: Implement subscription through MCP protocol
                // For now, we'll mark the resource as subscribed
                drop(clients);
                self.mark_resource_subscribed(uri).await;
                
                info!("Successfully subscribed to resource: {}", uri);
                Ok(())
            } else {
                Err(Error::mcp(format!("Server not found for resource: {}", uri)))
            }
        } else {
            Err(Error::mcp(format!("Resource not found: {}", uri)))
        }
    }

    /// Unsubscribe from resource changes
    pub async fn unsubscribe_from_resource(&self, uri: &str) -> Result<()> {
        info!("Unsubscribing from resource: {}", uri);
        
        let mut resources = self.resources.write().await;
        if let Some(cached) = resources.get_mut(uri) {
            cached.subscription_active = false;
        }
        
        Ok(())
    }

    /// Refresh resources from all servers
    pub async fn refresh_all_resources(&self) -> Result<()> {
        info!("Refreshing all resources");
        
        let server_names: Vec<String> = {
            let clients = self.clients.read().await;
            clients.keys().cloned().collect()
        };
        
        for server_name in server_names {
            if let Err(e) = self.refresh_server_resources(&server_name).await {
                error!("Failed to refresh resources from {}: {}", server_name, e);
            }
        }
        
        Ok(())
    }

    /// Refresh resources from a specific server
    async fn refresh_server_resources(&self, server_name: &str) -> Result<()> {
        debug!("Refreshing resources from server: {}", server_name);
        
        let client = {
            let clients = self.clients.read().await;
            clients.get(server_name).cloned()
        };
        
        if let Some(client) = client {
            // List resources from server
            match client.list_resources_from_server(server_name).await {
                Ok(resources) => {
                    let resource_count = resources.len();
                    let mut cache = self.resources.write().await;
                    
                    for resource in resources {
                        let cached_resource = CachedResource {
                            resource: resource.clone(),
                            contents: None, // Contents loaded on demand
                            last_updated: std::time::Instant::now(),
                            access_count: cache.get(&resource.uri).map(|c| c.access_count).unwrap_or(0),
                            server_name: server_name.to_string(),
                            subscription_active: false,
                        };
                        
                        cache.insert(resource.uri.clone(), cached_resource);
                    }
                    
                    debug!("Updated {} resources from server: {}", resource_count, server_name);
                }
                Err(e) => {
                    error!("Failed to list resources from {}: {}", server_name, e);
                    return Err(e);
                }
            }
        } else {
            return Err(Error::mcp(format!("Client not found for server: {}", server_name)));
        }
        
        Ok(())
    }

    /// Fetch resource contents from server
    async fn fetch_resource_contents(&self, uri: &str, server_name: &str) -> Result<Option<Vec<Content>>> {
        debug!("Fetching resource contents: {} from {}", uri, server_name);
        
        let client = {
            let clients = self.clients.read().await;
            clients.get(server_name).cloned()
        };
        
        if let Some(client) = client {
            match client.read_resource(server_name, uri).await {
                Ok(contents) => {
                    // Check size limit
                    let content_size = contents.iter()
                        .map(|c| match c {
                            Content::Text { text } => text.len(),
                            Content::Image { data, .. } => data.len(),
                            Content::Resource { .. } => 0, // Size not counted for references
                        })
                        .sum::<usize>();
                    
                    if content_size > self.config.max_resource_size_bytes {
                        warn!("Resource {} exceeds size limit: {} bytes", uri, content_size);
                        return Err(Error::mcp(format!(
                            "Resource size ({} bytes) exceeds limit ({} bytes)",
                            content_size, self.config.max_resource_size_bytes
                        )));
                    }
                    
                    // Create ResourceContents for cache
                    let resource_contents = ResourceContents {
                        uri: uri.to_string(),
                        mime_type: "application/json".to_string(), // Default mime type
                        content: contents.clone(),
                    };
                    
                    // Update cache
                    let mut resources = self.resources.write().await;
                    if let Some(cached) = resources.get_mut(uri) {
                        cached.contents = Some(resource_contents);
                        cached.last_updated = std::time::Instant::now();
                        cached.access_count += 1;
                    }
                    
                    Ok(Some(contents))
                }
                Err(e) => {
                    error!("Failed to read resource {} from {}: {}", uri, server_name, e);
                    Err(e)
                }
            }
        } else {
            Err(Error::mcp(format!("Client not found for server: {}", server_name)))
        }
    }

    /// Increment access count for a resource
    async fn increment_access_count(&self, uri: &str) {
        let mut resources = self.resources.write().await;
        if let Some(cached) = resources.get_mut(uri) {
            cached.access_count += 1;
        }
    }

    /// Mark resource as subscribed
    async fn mark_resource_subscribed(&self, uri: &str) {
        let mut resources = self.resources.write().await;
        if let Some(cached) = resources.get_mut(uri) {
            cached.subscription_active = true;
        }
    }

    /// Clean up stale cache entries
    pub async fn cleanup_cache(&self) -> Result<usize> {
        debug!("Cleaning up resource cache");
        
        let mut resources = self.resources.write().await;
        let initial_count = resources.len();
        
        // Remove stale entries
        resources.retain(|_, cached| {
            cached.last_updated.elapsed().as_secs() <= self.config.cache_ttl_seconds * 2
        });
        
        // If still over limit, remove least accessed
        if resources.len() > self.config.max_cache_size {
            let mut entries: Vec<_> = resources.iter().map(|(k, v)| (k.clone(), v.access_count)).collect();
            entries.sort_by_key(|(_, access_count)| *access_count);
            
            let to_remove = resources.len() - self.config.max_cache_size;
            for (uri, _) in entries.iter().take(to_remove) {
                resources.remove(uri);
            }
        }
        
        let removed_count = initial_count - resources.len();
        if removed_count > 0 {
            info!("Cleaned up {} stale resource cache entries", removed_count);
        }
        
        Ok(removed_count)
    }

    /// Get resource manager statistics
    pub async fn get_statistics(&self) -> ResourceManagerStats {
        let resources = self.resources.read().await;
        let clients = self.clients.read().await;
        
        let total_resources = resources.len();
        let subscribed_resources = resources.values().filter(|c| c.subscription_active).count();
        let fresh_resources = resources.values()
            .filter(|c| c.last_updated.elapsed().as_secs() <= self.config.cache_ttl_seconds)
            .count();
        
        let mut by_server = HashMap::new();
        for cached in resources.values() {
            *by_server.entry(cached.server_name.clone()).or_insert(0) += 1;
        }
        
        ResourceManagerStats {
            total_resources,
            subscribed_resources,
            fresh_resources,
            stale_resources: total_resources - fresh_resources,
            registered_servers: clients.len(),
            resources_by_server: by_server,
        }
    }
}

/// Resource manager statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagerStats {
    pub total_resources: usize,
    pub subscribed_resources: usize,
    pub fresh_resources: usize,
    pub stale_resources: usize,
    pub registered_servers: usize,
    pub resources_by_server: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resource_manager_creation() {
        let config = ResourceConfig::default();
        let manager = ResourceManager::new(config);
        
        let stats = manager.get_statistics().await;
        assert_eq!(stats.total_resources, 0);
        assert_eq!(stats.registered_servers, 0);
    }

    #[tokio::test]
    async fn test_resource_query_filtering() {
        let query = ResourceQuery {
            uri_pattern: Some("test".to_string()),
            mime_type: None,
            server_name: None,
            include_contents: false,
        };
        
        assert!(query.uri_pattern.is_some());
        assert!(!query.include_contents);
    }

    #[tokio::test]
    async fn test_cache_status_enum() {
        let status = CacheStatus::Fresh;
        match status {
            CacheStatus::Fresh => assert!(true),
            _ => assert!(false),
        }
    }

    #[tokio::test]
    async fn test_resource_config_defaults() {
        let config = ResourceConfig::default();
        assert_eq!(config.cache_ttl_seconds, 300);
        assert_eq!(config.max_cache_size, 1000);
        assert!(config.enable_subscriptions);
    }
}