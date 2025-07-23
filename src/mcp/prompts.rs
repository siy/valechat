use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use serde::{Serialize, Deserialize};
use regex::Regex;

use crate::error::{Error, Result};
use crate::mcp::types::{Prompt, Content};
use crate::mcp::client::MCPClient;

/// Prompt template manager for handling MCP prompts
pub struct PromptTemplateManager {
    templates: Arc<RwLock<HashMap<String, CachedPrompt>>>,
    clients: Arc<RwLock<HashMap<String, Arc<MCPClient>>>>,
    config: PromptTemplateConfig,
}

/// Configuration for prompt template management
#[derive(Debug, Clone)]
pub struct PromptTemplateConfig {
    pub cache_ttl_seconds: u64,
    pub max_cache_size: usize,
    pub max_template_size_chars: usize,
    pub variable_pattern: String,
    pub enable_template_validation: bool,
    pub default_timeout_seconds: u64,
}

impl Default for PromptTemplateConfig {
    fn default() -> Self {
        Self {
            cache_ttl_seconds: 300, // 5 minutes
            max_cache_size: 500,
            max_template_size_chars: 100_000, // 100KB
            variable_pattern: r"\{\{(\w+)\}\}".to_string(), // {{variable}} pattern
            enable_template_validation: true,
            default_timeout_seconds: 30,
        }
    }
}

/// Cached prompt with metadata
#[derive(Debug, Clone)]
struct CachedPrompt {
    prompt: Prompt,
    template_content: Option<Vec<Content>>,
    last_updated: std::time::Instant,
    access_count: u64,
    server_name: String,
    variables: Vec<String>, // Extracted variable names
}

/// Prompt template query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptQuery {
    pub name_pattern: Option<String>,
    pub server_name: Option<String>,
    pub has_variables: Option<bool>,
    pub include_content: bool,
}

/// Prompt search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSearchResult {
    pub prompts: Vec<PromptInfo>,
    pub total_count: usize,
    pub has_more: bool,
}

/// Prompt information with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInfo {
    pub prompt: Prompt,
    pub server_name: String,
    pub last_updated: Option<std::time::SystemTime>,
    pub access_count: u64,
    pub cache_status: CacheStatus,
    pub variables: Vec<String>,
    pub content: Option<Vec<Content>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheStatus {
    Fresh,
    Stale,
    Missing,
    Error,
}

/// Template execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateContext {
    pub variables: HashMap<String, String>,
    pub server_name: String,
    pub prompt_name: String,
}

/// Template execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateResult {
    pub content: Vec<Content>,
    pub variables_used: Vec<String>,
    pub execution_time_ms: u64,
}

/// Template validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateValidationError {
    pub message: String,
    pub variable_name: Option<String>,
    pub line_number: Option<usize>,
}

impl PromptTemplateManager {
    /// Create a new prompt template manager
    pub fn new(config: PromptTemplateConfig) -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register an MCP client for prompt access
    pub async fn register_client(&self, server_name: String, client: Arc<MCPClient>) -> Result<()> {
        info!("Registering MCP client for prompts: {}", server_name);
        
        let mut clients = self.clients.write().await;
        clients.insert(server_name.clone(), client.clone());
        
        // Refresh prompts from this server
        drop(clients);
        self.refresh_server_prompts(&server_name).await?;
        
        Ok(())
    }

    /// Unregister an MCP client
    pub async fn unregister_client(&self, server_name: &str) -> Result<()> {
        info!("Unregistering MCP client: {}", server_name);
        
        let mut clients = self.clients.write().await;
        clients.remove(server_name);
        
        // Remove cached prompts from this server
        let mut templates = self.templates.write().await;
        templates.retain(|_, cached| cached.server_name != server_name);
        
        Ok(())
    }

    /// List all available prompts
    pub async fn list_prompts(&self, query: Option<PromptQuery>) -> Result<PromptSearchResult> {
        debug!("Listing prompts with query: {:?}", query);
        
        let templates = self.templates.read().await;
        let mut filtered_prompts = Vec::new();
        
        for (key, cached) in templates.iter() {
            // Apply filters
            if let Some(ref q) = query {
                if let Some(ref pattern) = q.name_pattern {
                    if !cached.prompt.name.contains(pattern) {
                        continue;
                    }
                }
                
                if let Some(ref server_name) = q.server_name {
                    if &cached.server_name != server_name {
                        continue;
                    }
                }
                
                if let Some(has_vars) = q.has_variables {
                    if has_vars && cached.variables.is_empty() {
                        continue;
                    }
                    if !has_vars && !cached.variables.is_empty() {
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
            
            let content = if query.as_ref().map(|q| q.include_content).unwrap_or(false) {
                cached.template_content.clone()
            } else {
                None
            };
            
            filtered_prompts.push(PromptInfo {
                prompt: cached.prompt.clone(),
                server_name: cached.server_name.clone(),
                last_updated: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(
                    cached.last_updated.elapsed().as_secs()
                )),
                access_count: cached.access_count,
                cache_status,
                variables: cached.variables.clone(),
                content,
            });
        }
        
        // Sort by access count (most used first)
        filtered_prompts.sort_by(|a, b| b.access_count.cmp(&a.access_count));
        
        Ok(PromptSearchResult {
            total_count: filtered_prompts.len(),
            has_more: false, // No pagination for now
            prompts: filtered_prompts,
        })
    }

    /// Get a specific prompt template by name
    pub async fn get_prompt_template(
        &self, 
        server_name: &str, 
        prompt_name: &str, 
        force_refresh: bool
    ) -> Result<Option<Vec<Content>>> {
        debug!("Getting prompt template: {} from {} (force_refresh: {})", prompt_name, server_name, force_refresh);
        
        let cache_key = format!("{}:{}", server_name, prompt_name);
        
        // Check cache first
        if !force_refresh {
            let templates = self.templates.read().await;
            if let Some(cached) = templates.get(&cache_key) {
                if cached.last_updated.elapsed().as_secs() <= self.config.cache_ttl_seconds {
                    // Update access count
                    drop(templates);
                    self.increment_access_count(&cache_key).await;
                    
                    let templates = self.templates.read().await;
                    return Ok(templates.get(&cache_key).and_then(|c| c.template_content.clone()));
                }
            }
        }
        
        // Fetch from server
        self.fetch_prompt_content(server_name, prompt_name).await
    }

    /// Execute a prompt template with given variables
    pub async fn execute_template(
        &self,
        context: TemplateContext,
    ) -> Result<TemplateResult> {
        let start_time = std::time::Instant::now();
        
        info!("Executing template: {} from {} with {} variables", 
              context.prompt_name, context.server_name, context.variables.len());
        
        // Get template content
        let template_content = self.get_prompt_template(
            &context.server_name, 
            &context.prompt_name, 
            false
        ).await?;
        
        let template_content = template_content.ok_or_else(|| {
            Error::mcp(format!("Template not found: {} from {}", context.prompt_name, context.server_name))
        })?;
        
        // Process template variables
        let mut processed_content = Vec::new();
        let mut variables_used = Vec::new();
        
        for content in template_content {
            match content {
                Content::Text { text } => {
                    let (processed_text, used_vars) = self.substitute_variables(&text, &context.variables).await?;
                    variables_used.extend(used_vars);
                    processed_content.push(Content::Text { text: processed_text });
                }
                Content::Image { data, mime_type } => {
                    // Images don't typically have variables, pass through
                    processed_content.push(Content::Image { data, mime_type });
                }
                Content::Resource { resource } => {
                    // Resources might have variables in URI, but for now pass through
                    processed_content.push(Content::Resource { resource });
                }
            }
        }
        
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        // Validate template if enabled
        if self.config.enable_template_validation {
            self.validate_template_result(&processed_content, &context).await?;
        }
        
        info!("Template executed successfully in {}ms with {} variables", 
              execution_time, variables_used.len());
        
        Ok(TemplateResult {
            content: processed_content,
            variables_used,
            execution_time_ms: execution_time,
        })
    }

    /// Validate a template for syntax and variables
    pub async fn validate_template(
        &self,
        server_name: &str,
        prompt_name: &str,
    ) -> Result<Vec<TemplateValidationError>> {
        debug!("Validating template: {} from {}", prompt_name, server_name);
        
        let template_content = self.get_prompt_template(server_name, prompt_name, false).await?;
        let template_content = template_content.ok_or_else(|| {
            Error::mcp(format!("Template not found: {} from {}", prompt_name, server_name))
        })?;
        
        let mut errors = Vec::new();
        let variable_regex = Regex::new(&self.config.variable_pattern).map_err(|e| {
            Error::mcp(format!("Invalid variable pattern: {}", e))
        })?;
        
        for (index, content) in template_content.iter().enumerate() {
            match content {
                Content::Text { text } => {
                    // Check for template syntax errors
                    if let Err(validation_errors) = self.validate_text_template(text, &variable_regex, index) {
                        errors.extend(validation_errors.into_iter());
                    }
                }
                _ => {} // Other content types don't need validation
            }
        }
        
        Ok(errors)
    }

    /// Get template variables from a prompt
    pub async fn extract_template_variables(
        &self,
        server_name: &str,
        prompt_name: &str,
    ) -> Result<Vec<String>> {
        debug!("Extracting variables from template: {} from {}", prompt_name, server_name);
        
        let cache_key = format!("{}:{}", server_name, prompt_name);
        let templates = self.templates.read().await;
        
        if let Some(cached) = templates.get(&cache_key) {
            Ok(cached.variables.clone())
        } else {
            // Template not in cache, try to load it
            drop(templates);
            self.get_prompt_template(server_name, prompt_name, true).await?;
            
            let templates = self.templates.read().await;
            Ok(templates.get(&cache_key).map(|c| c.variables.clone()).unwrap_or_default())
        }
    }

    /// Refresh prompts from all servers
    pub async fn refresh_all_prompts(&self) -> Result<()> {
        info!("Refreshing all prompts");
        
        let server_names: Vec<String> = {
            let clients = self.clients.read().await;
            clients.keys().cloned().collect()
        };
        
        for server_name in server_names {
            if let Err(e) = self.refresh_server_prompts(&server_name).await {
                error!("Failed to refresh prompts from {}: {}", server_name, e);
            }
        }
        
        Ok(())
    }

    /// Refresh prompts from a specific server
    async fn refresh_server_prompts(&self, server_name: &str) -> Result<()> {
        debug!("Refreshing prompts from server: {}", server_name);
        
        let client = {
            let clients = self.clients.read().await;
            clients.get(server_name).cloned()
        };
        
        if let Some(client) = client {
            // List prompts from server
            match client.list_prompts_from_server(server_name).await {
                Ok(prompts) => {
                    let prompt_count = prompts.len();
                    let mut cache = self.templates.write().await;
                    
                    for prompt in prompts {
                        let cache_key = format!("{}:{}", server_name, prompt.name);
                        let cached_prompt = CachedPrompt {
                            prompt: prompt.clone(),
                            template_content: None, // Content loaded on demand
                            last_updated: std::time::Instant::now(),
                            access_count: cache.get(&cache_key).map(|c| c.access_count).unwrap_or(0),
                            server_name: server_name.to_string(),
                            variables: Vec::new(), // Variables extracted when content is loaded
                        };
                        
                        cache.insert(cache_key, cached_prompt);
                    }
                    
                    debug!("Updated {} prompts from server: {}", prompt_count, server_name);
                }
                Err(e) => {
                    error!("Failed to list prompts from {}: {}", server_name, e);
                    return Err(e);
                }
            }
        } else {
            return Err(Error::mcp(format!("Client not found for server: {}", server_name)));
        }
        
        Ok(())
    }

    /// Fetch prompt content from server
    async fn fetch_prompt_content(
        &self,
        server_name: &str,
        prompt_name: &str,
    ) -> Result<Option<Vec<Content>>> {
        debug!("Fetching prompt content: {} from {}", prompt_name, server_name);
        
        let client = {
            let clients = self.clients.read().await;
            clients.get(server_name).cloned()
        };
        
        if let Some(client) = client {
            match client.get_prompt(server_name, prompt_name, None).await {
                Ok(content) => {
                    // Check size limit
                    let content_size = content.iter()
                        .map(|c| match c {
                            Content::Text { text } => text.len(),
                            Content::Image { data, .. } => data.len(),
                            Content::Resource { .. } => 0, // Size not counted for references
                        })
                        .sum::<usize>();
                    
                    if content_size > self.config.max_template_size_chars {
                        warn!("Template {} exceeds size limit: {} chars", prompt_name, content_size);
                        return Err(Error::mcp(format!(
                            "Template size ({} chars) exceeds limit ({} chars)",
                            content_size, self.config.max_template_size_chars
                        )));
                    }
                    
                    // Extract variables from content
                    let variables = self.extract_variables_from_content(&content).await?;
                    
                    // Update cache
                    let cache_key = format!("{}:{}", server_name, prompt_name);
                    let mut templates = self.templates.write().await;
                    if let Some(cached) = templates.get_mut(&cache_key) {
                        cached.template_content = Some(content.clone());
                        cached.last_updated = std::time::Instant::now();
                        cached.access_count += 1;
                        cached.variables = variables;
                    }
                    
                    Ok(Some(content))
                }
                Err(e) => {
                    error!("Failed to get prompt {} from {}: {}", prompt_name, server_name, e);
                    Err(e)
                }
            }
        } else {
            Err(Error::mcp(format!("Client not found for server: {}", server_name)))
        }
    }

    /// Substitute variables in text content
    async fn substitute_variables(
        &self,
        text: &str,
        variables: &HashMap<String, String>,
    ) -> Result<(String, Vec<String>)> {
        let variable_regex = Regex::new(&self.config.variable_pattern).map_err(|e| {
            Error::mcp(format!("Invalid variable pattern: {}", e))
        })?;
        
        let mut result = text.to_string();
        let mut used_variables = Vec::new();
        
        for cap in variable_regex.find_iter(text) {
            let full_match = cap.as_str();
            let var_name = &full_match[2..full_match.len()-2]; // Remove {{ and }}
            
            if let Some(value) = variables.get(var_name) {
                result = result.replace(full_match, value);
                used_variables.push(var_name.to_string());
            } else {
                warn!("Variable '{}' not provided, leaving placeholder", var_name);
            }
        }
        
        Ok((result, used_variables))
    }

    /// Extract variables from content
    async fn extract_variables_from_content(&self, content: &[Content]) -> Result<Vec<String>> {
        let variable_regex = Regex::new(&self.config.variable_pattern).map_err(|e| {
            Error::mcp(format!("Invalid variable pattern: {}", e))
        })?;
        
        let mut variables = Vec::new();
        
        for content_item in content {
            if let Content::Text { text } = content_item {
                for cap in variable_regex.find_iter(text) {
                    let full_match = cap.as_str();
                    let var_name = &full_match[2..full_match.len()-2]; // Remove {{ and }}
                    if !variables.contains(&var_name.to_string()) {
                        variables.push(var_name.to_string());
                    }
                }
            }
        }
        
        variables.sort();
        Ok(variables)
    }

    /// Validate text template syntax
    fn validate_text_template(
        &self,
        text: &str,
        variable_regex: &Regex,
        line_offset: usize,
    ) -> std::result::Result<Vec<TemplateValidationError>, Vec<TemplateValidationError>> {
        let mut errors = Vec::new();
        
        // Check for unmatched braces
        let mut brace_count = 0;
        let mut in_variable = false;
        
        for (line_num, line) in text.lines().enumerate() {
            for (char_pos, ch) in line.chars().enumerate() {
                match ch {
                    '{' => {
                        brace_count += 1;
                        if brace_count == 2 {
                            in_variable = true;
                        } else if brace_count > 2 {
                            errors.push(TemplateValidationError {
                                message: "Too many opening braces".to_string(),
                                variable_name: None,
                                line_number: Some(line_offset + line_num + 1),
                            });
                        }
                    }
                    '}' => {
                        if brace_count > 0 {
                            brace_count -= 1;
                            if brace_count == 0 {
                                in_variable = false;
                            }
                        } else {
                            errors.push(TemplateValidationError {
                                message: "Unexpected closing brace".to_string(),
                                variable_name: None,
                                line_number: Some(line_offset + line_num + 1),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        
        if brace_count > 0 {
            errors.push(TemplateValidationError {
                message: "Unclosed variable braces".to_string(),
                variable_name: None,
                line_number: None,
            });
        }
        
        // Validate variable names
        for cap in variable_regex.find_iter(text) {
            let full_match = cap.as_str();
            let var_name = &full_match[2..full_match.len()-2]; // Remove {{ and }}
            
            if var_name.is_empty() {
                errors.push(TemplateValidationError {
                    message: "Empty variable name".to_string(),
                    variable_name: Some(var_name.to_string()),
                    line_number: None,
                });
            } else if !var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                errors.push(TemplateValidationError {
                    message: "Invalid variable name (use only alphanumeric and underscore)".to_string(),
                    variable_name: Some(var_name.to_string()),
                    line_number: None,
                });
            }
        }
        
        if errors.is_empty() {
            Ok(vec![])
        } else {
            Err(errors)
        }
    }

    /// Validate template execution result
    async fn validate_template_result(
        &self,
        content: &[Content],
        context: &TemplateContext,
    ) -> Result<()> {
        // Check for remaining unsubstituted variables
        let variable_regex = Regex::new(&self.config.variable_pattern).map_err(|e| {
            Error::mcp(format!("Invalid variable pattern: {}", e))
        })?;
        
        for content_item in content {
            if let Content::Text { text } = content_item {
                if variable_regex.is_match(text) {
                    let unsubstituted: Vec<&str> = variable_regex
                        .find_iter(text)
                        .map(|m| m.as_str())
                        .collect();
                    
                    warn!("Template {} has unsubstituted variables: {:?}", 
                          context.prompt_name, unsubstituted);
                    
                    return Err(Error::mcp(format!(
                        "Template contains unsubstituted variables: {:?}",
                        unsubstituted
                    )));
                }
            }
        }
        
        Ok(())
    }

    /// Increment access count for a template
    async fn increment_access_count(&self, cache_key: &str) {
        let mut templates = self.templates.write().await;
        if let Some(cached) = templates.get_mut(cache_key) {
            cached.access_count += 1;
        }
    }

    /// Clean up stale cache entries
    pub async fn cleanup_cache(&self) -> Result<usize> {
        debug!("Cleaning up prompt template cache");
        
        let mut templates = self.templates.write().await;
        let initial_count = templates.len();
        
        // Remove stale entries
        templates.retain(|_, cached| {
            cached.last_updated.elapsed().as_secs() <= self.config.cache_ttl_seconds * 2
        });
        
        // If still over limit, remove least accessed
        if templates.len() > self.config.max_cache_size {
            let mut entries: Vec<_> = templates.iter().map(|(k, v)| (k.clone(), v.access_count)).collect();
            entries.sort_by_key(|(_, access_count)| *access_count);
            
            let to_remove = templates.len() - self.config.max_cache_size;
            for (cache_key, _) in entries.iter().take(to_remove) {
                templates.remove(cache_key);
            }
        }
        
        let removed_count = initial_count - templates.len();
        if removed_count > 0 {
            info!("Cleaned up {} stale prompt template cache entries", removed_count);
        }
        
        Ok(removed_count)
    }

    /// Get template manager statistics
    pub async fn get_statistics(&self) -> PromptTemplateStats {
        let templates = self.templates.read().await;
        let clients = self.clients.read().await;
        
        let total_templates = templates.len();
        let fresh_templates = templates.values()
            .filter(|c| c.last_updated.elapsed().as_secs() <= self.config.cache_ttl_seconds)
            .count();
        
        let mut by_server = HashMap::new();
        let mut total_variables = 0;
        
        for cached in templates.values() {
            *by_server.entry(cached.server_name.clone()).or_insert(0) += 1;
            total_variables += cached.variables.len();
        }
        
        PromptTemplateStats {
            total_templates,
            fresh_templates,
            stale_templates: total_templates - fresh_templates,
            registered_servers: clients.len(),
            templates_by_server: by_server,
            total_variables,
        }
    }
}

/// Prompt template manager statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplateStats {
    pub total_templates: usize,
    pub fresh_templates: usize,
    pub stale_templates: usize,
    pub registered_servers: usize,
    pub templates_by_server: HashMap<String, usize>,
    pub total_variables: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[tokio::test]
    async fn test_prompt_template_manager_creation() {
        let config = PromptTemplateConfig::default();
        let manager = PromptTemplateManager::new(config);
        
        let stats = manager.get_statistics().await;
        assert_eq!(stats.total_templates, 0);
        assert_eq!(stats.registered_servers, 0);
    }

    #[tokio::test]
    async fn test_prompt_query_filtering() {
        let query = PromptQuery {
            name_pattern: Some("test".to_string()),
            server_name: None,
            has_variables: Some(true),
            include_content: false,
        };
        
        assert!(query.name_pattern.is_some());
        assert!(!query.include_content);
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
    async fn test_template_config_defaults() {
        let config = PromptTemplateConfig::default();
        assert_eq!(config.cache_ttl_seconds, 300);
        assert_eq!(config.max_cache_size, 500);
        assert!(config.enable_template_validation);
        assert_eq!(config.variable_pattern, r"\{\{(\w+)\}\}");
    }

    #[tokio::test]
    async fn test_variable_extraction() {
        let config = PromptTemplateConfig::default();
        let manager = PromptTemplateManager::new(config);
        
        let content = vec![
            Content::Text { text: "Hello {{name}}, your age is {{age}}".to_string() },
            Content::Text { text: "Welcome to {{platform}}!".to_string() },
        ];
        
        let variables = manager.extract_variables_from_content(&content).await.unwrap();
        assert_eq!(variables.len(), 3);
        assert!(variables.contains(&"name".to_string()));
        assert!(variables.contains(&"age".to_string()));
        assert!(variables.contains(&"platform".to_string()));
    }

    #[tokio::test]
    async fn test_variable_substitution() {
        let config = PromptTemplateConfig::default();
        let manager = PromptTemplateManager::new(config);
        
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "Alice".to_string());
        variables.insert("age".to_string(), "30".to_string());
        
        let text = "Hello {{name}}, your age is {{age}}";
        let (result, used_vars) = manager.substitute_variables(text, &variables).await.unwrap();
        
        assert_eq!(result, "Hello Alice, your age is 30");
        assert_eq!(used_vars.len(), 2);
        assert!(used_vars.contains(&"name".to_string()));
        assert!(used_vars.contains(&"age".to_string()));
    }

    #[tokio::test]
    async fn test_template_validation() {
        let config = PromptTemplateConfig::default();
        let manager = PromptTemplateManager::new(config);
        
        let variable_regex = Regex::new(r"\{\{(\w+)\}\}").unwrap();
        
        // Valid template
        let valid_text = "Hello {{name}}, welcome to {{platform}}!";
        let result = manager.validate_text_template(valid_text, &variable_regex, 0);
        assert!(result.is_ok());
        
        // Invalid template with unmatched braces
        let invalid_text = "Hello {{name}, welcome to {{platform}}!";
        let result = manager.validate_text_template(invalid_text, &variable_regex, 0);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_template_context() {
        let mut variables = HashMap::new();
        variables.insert("user".to_string(), "test_user".to_string());
        
        let context = TemplateContext {
            variables,
            server_name: "test_server".to_string(),
            prompt_name: "test_prompt".to_string(),
        };
        
        assert_eq!(context.server_name, "test_server");
        assert_eq!(context.prompt_name, "test_prompt");
        assert_eq!(context.variables.len(), 1);
    }
}