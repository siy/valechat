use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;
use tracing::info;

use crate::error::{Error, Result};
use crate::platform::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub models: HashMap<String, ModelConfig>,
    pub mcp_servers: HashMap<String, MCPServerConfig>,
    pub billing: BillingConfig,
    pub ui: UIConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub default_model: String,
    pub enabled: bool,
    pub api_endpoint: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub transport_type: TransportType,
    pub env_vars: HashMap<String, String>,
    pub enabled: bool,
    pub auto_start: bool,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    Stdio,
    WebSocket { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingConfig {
    pub daily_limit_usd: Option<f64>,
    pub monthly_limit_usd: Option<f64>,
    pub per_model_limits: HashMap<String, f64>,
    pub alert_threshold_percent: f64,
    pub track_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    pub theme: String,
    pub font_size: u8,
    pub window_width: u32,
    pub window_height: u32,
    pub show_token_counts: bool,
    pub show_cost_estimates: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut default_models = HashMap::new();
        
        // Default OpenAI configuration
        default_models.insert("openai".to_string(), ModelConfig {
            provider: "openai".to_string(),
            default_model: "gpt-4".to_string(),
            enabled: false, // Disabled by default until API key is configured
            api_endpoint: None, // Uses default OpenAI endpoint
            timeout_seconds: Some(60),
            max_retries: Some(3),
        });

        // Default Anthropic configuration
        default_models.insert("anthropic".to_string(), ModelConfig {
            provider: "anthropic".to_string(),
            default_model: "claude-3-sonnet-20240229".to_string(),
            enabled: false, // Disabled by default until API key is configured
            api_endpoint: None, // Uses default Anthropic endpoint
            timeout_seconds: Some(60),
            max_retries: Some(3),
        });

        Self {
            models: default_models,
            mcp_servers: HashMap::new(),
            billing: BillingConfig {
                daily_limit_usd: Some(10.0),
                monthly_limit_usd: Some(100.0),
                per_model_limits: HashMap::new(),
                alert_threshold_percent: 80.0,
                track_usage: true,
            },
            ui: UIConfig {
                theme: "system".to_string(), // system, light, dark
                font_size: 14,
                window_width: 1200,
                window_height: 800,
                show_token_counts: true,
                show_cost_estimates: true,
            },
        }
    }
}

impl AppConfig {
    pub async fn load(paths: &AppPaths) -> Result<Self> {
        let config_file = paths.config_file();
        
        if !config_file.exists() {
            info!("Config file not found, creating default configuration");
            let default_config = Self::default();
            default_config.save(paths).await?;
            return Ok(default_config);
        }

        info!("Loading configuration from: {:?}", config_file);
        
        let config_content = fs::read_to_string(&config_file).await?;
        let config: AppConfig = toml::from_str(&config_content)
            .map_err(|e| Error::Config(config::ConfigError::Message(e.to_string())))?;

        // Validate configuration
        config.validate()?;

        info!("Configuration loaded successfully");
        Ok(config)
    }

    pub async fn save(&self, paths: &AppPaths) -> Result<()> {
        let config_file = paths.config_file();
        
        info!("Saving configuration to: {:?}", config_file);
        
        let config_content = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(config::ConfigError::Message(e.to_string())))?;
        
        fs::write(&config_file, config_content).await?;
        
        info!("Configuration saved successfully");
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // Validate model configurations
        for (name, model_config) in &self.models {
            if model_config.provider.is_empty() {
                return Err(Error::validation(format!("Model {} has empty provider", name)));
            }
            if model_config.default_model.is_empty() {
                return Err(Error::validation(format!("Model {} has empty default_model", name)));
            }
        }

        // Validate MCP server configurations
        for (name, mcp_config) in &self.mcp_servers {
            if mcp_config.command.is_empty() {
                return Err(Error::validation(format!("MCP server {} has empty command", name)));
            }
        }

        // Validate billing configuration
        if self.billing.alert_threshold_percent < 0.0 || self.billing.alert_threshold_percent > 100.0 {
            return Err(Error::validation("Alert threshold must be between 0 and 100"));
        }

        // Validate UI configuration
        if self.ui.font_size < 8 || self.ui.font_size > 32 {
            return Err(Error::validation("Font size must be between 8 and 32"));
        }

        Ok(())
    }

    pub fn get_enabled_models(&self) -> Vec<&str> {
        self.models
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    pub fn get_enabled_mcp_servers(&self) -> Vec<&str> {
        self.mcp_servers
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    pub fn update_model_config(&mut self, name: &str, config: ModelConfig) {
        self.models.insert(name.to_string(), config);
    }

    pub fn update_mcp_server_config(&mut self, name: &str, config: MCPServerConfig) {
        self.mcp_servers.insert(name.to_string(), config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let mut config = AppConfig::default();
        assert!(config.validate().is_ok());

        // Test invalid alert threshold
        config.billing.alert_threshold_percent = 150.0;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.models.len(), 2);
        assert!(config.models.contains_key("openai"));
        assert!(config.models.contains_key("anthropic"));
        assert_eq!(config.billing.daily_limit_usd, Some(10.0));
        assert_eq!(config.ui.theme, "system");
    }
}