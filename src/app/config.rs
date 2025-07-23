use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;
use tracing::info;
use rust_decimal::Decimal;

use crate::error::{Error, Result};
use crate::platform::AppPaths;
use crate::models::{QualityPriority, TaskType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub models: HashMap<String, ModelConfig>,
    pub mcp_servers: HashMap<String, MCPServerConfig>,
    pub billing: BillingConfig,
    pub ui: UIConfig,
    pub fallback: FallbackConfig,
    pub rate_limiting: RateLimitingConfig,
    pub capability_detection: CapabilityDetectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub default_model: String,
    pub enabled: bool,
    pub api_endpoint: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub max_retries: Option<u32>,
    pub rate_limits: Option<ProviderRateLimits>,
    pub cost_limits: Option<CostLimits>,
    pub priority: i32, // Higher number = higher priority for fallback
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRateLimits {
    pub requests_per_minute: Option<u32>,
    pub tokens_per_minute: Option<u32>,
    pub requests_per_day: Option<u32>,
    pub concurrent_requests: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostLimits {
    pub max_cost_per_request: Option<String>, // Decimal as string
    pub daily_cost_limit: Option<String>,     // Decimal as string
    pub monthly_cost_limit: Option<String>,   // Decimal as string
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
    pub language: String,
    pub font_size: u8,
    pub window_width: u32,
    pub window_height: u32,
    pub show_token_counts: bool,
    pub show_cost_estimates: bool,
    pub auto_save: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    pub enabled: bool,
    pub max_retries: usize,
    pub retry_delay_ms: u64,
    pub timeout_ms: u64,
    pub fallback_on_rate_limit: bool,
    pub fallback_on_error: bool,
    pub fallback_on_timeout: bool,
    pub quality_degradation_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    pub enabled: bool,
    pub token_bucket_refill_rate: f64,
    pub burst_allowance_multiplier: f64,
    pub backoff_base_delay_ms: u64,
    pub backoff_max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDetectionConfig {
    pub enabled: bool,
    pub default_quality_priority: QualityPriority,
    pub task_type_inference: bool,
    pub cost_optimization: bool,
    pub performance_tracking: bool,
    pub model_preferences: HashMap<TaskType, Vec<String>>, // Preferred models for each task type
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
            rate_limits: Some(ProviderRateLimits {
                requests_per_minute: Some(60),
                tokens_per_minute: Some(10000),
                requests_per_day: Some(1000),
                concurrent_requests: Some(5),
            }),
            cost_limits: None,
            priority: 100, // High priority
        });

        // Default Anthropic configuration
        default_models.insert("anthropic".to_string(), ModelConfig {
            provider: "anthropic".to_string(),
            default_model: "claude-3-sonnet-20240229".to_string(),
            enabled: false, // Disabled by default until API key is configured
            api_endpoint: None, // Uses default Anthropic endpoint
            timeout_seconds: Some(60),
            max_retries: Some(3),
            rate_limits: Some(ProviderRateLimits {
                requests_per_minute: Some(50),
                tokens_per_minute: Some(40000),
                requests_per_day: Some(1000),
                concurrent_requests: Some(5),
            }),
            cost_limits: None,
            priority: 90, // High priority
        });

        // Default Gemini configuration
        default_models.insert("gemini".to_string(), ModelConfig {
            provider: "gemini".to_string(),
            default_model: "gemini-1.5-flash".to_string(),
            enabled: false, // Disabled by default until API key is configured
            api_endpoint: None, // Uses default Gemini endpoint
            timeout_seconds: Some(60),
            max_retries: Some(3),
            rate_limits: Some(ProviderRateLimits {
                requests_per_minute: Some(60),
                tokens_per_minute: Some(32000),
                requests_per_day: Some(1500),
                concurrent_requests: Some(5),
            }),
            cost_limits: None,
            priority: 80, // Medium priority
        });

        // Default model preferences for different task types
        let mut model_preferences = HashMap::new();
        model_preferences.insert(TaskType::CodeGeneration, vec![
            "anthropic:claude-3-5-sonnet-20241022".to_string(),
            "openai:gpt-4".to_string(),
            "gemini:gemini-1.5-pro".to_string(),
        ]);
        model_preferences.insert(TaskType::ConversationalChat, vec![
            "anthropic:claude-3-haiku-20240307".to_string(),
            "openai:gpt-3.5-turbo".to_string(),
            "gemini:gemini-1.5-flash".to_string(),
        ]);
        model_preferences.insert(TaskType::ReasoningAndAnalysis, vec![
            "anthropic:claude-3-opus-20240229".to_string(),
            "openai:gpt-4".to_string(),
            "gemini:gemini-1.5-pro".to_string(),
        ]);

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
                language: "en".to_string(),
                font_size: 14,
                window_width: 1200,
                window_height: 800,
                show_token_counts: true,
                show_cost_estimates: true,
                auto_save: true,
                streaming: true,
            },
            fallback: FallbackConfig {
                enabled: true,
                max_retries: 3,
                retry_delay_ms: 1000,
                timeout_ms: 30000,
                fallback_on_rate_limit: true,
                fallback_on_error: true,
                fallback_on_timeout: true,
                quality_degradation_allowed: true,
            },
            rate_limiting: RateLimitingConfig {
                enabled: true,
                token_bucket_refill_rate: 1.0,
                burst_allowance_multiplier: 2.0,
                backoff_base_delay_ms: 1000,
                backoff_max_delay_ms: 60000,
                backoff_multiplier: 2.0,
            },
            capability_detection: CapabilityDetectionConfig {
                enabled: true,
                default_quality_priority: QualityPriority::Balanced,
                task_type_inference: true,
                cost_optimization: true,
                performance_tracking: true,
                model_preferences,
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

    pub fn get_models_by_priority(&self) -> Vec<(&str, &ModelConfig)> {
        let mut models: Vec<_> = self.models
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, config)| (name.as_str(), config))
            .collect();
        
        // Sort by priority (descending)
        models.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
        models
    }

    pub fn get_cost_limit_as_decimal(&self, provider: &str, limit_type: &str) -> Option<Decimal> {
        self.models.get(provider)
            .and_then(|config| config.cost_limits.as_ref())
            .and_then(|limits| {
                let limit_str = match limit_type {
                    "request" => limits.max_cost_per_request.as_ref(),
                    "daily" => limits.daily_cost_limit.as_ref(),
                    "monthly" => limits.monthly_cost_limit.as_ref(),
                    _ => None,
                }?;
                limit_str.parse().ok()
            })
    }

    pub fn to_rate_limits(&self, provider: &str) -> crate::models::provider::RateLimits {
        let provider_limits = self.models.get(provider)
            .and_then(|config| config.rate_limits.as_ref());

        crate::models::provider::RateLimits {
            requests_per_minute: provider_limits.and_then(|l| l.requests_per_minute),
            tokens_per_minute: provider_limits.and_then(|l| l.tokens_per_minute),
            requests_per_day: provider_limits.and_then(|l| l.requests_per_day),
            concurrent_requests: provider_limits.and_then(|l| l.concurrent_requests),
        }
    }

    pub fn to_fallback_config(&self) -> crate::models::fallback::FallbackConfig {
        crate::models::fallback::FallbackConfig {
            max_retries: self.fallback.max_retries,
            retry_delay_ms: self.fallback.retry_delay_ms,
            timeout_ms: self.fallback.timeout_ms,
            fallback_on_rate_limit: self.fallback.fallback_on_rate_limit,
            fallback_on_error: self.fallback.fallback_on_error,
            fallback_on_timeout: self.fallback.fallback_on_timeout,
            quality_degradation_allowed: self.fallback.quality_degradation_allowed,
        }
    }

    pub fn to_rate_limiter_config(&self) -> crate::models::rate_limiter::RateLimiterConfig {
        crate::models::rate_limiter::RateLimiterConfig {
            enable_rate_limiting: self.rate_limiting.enabled,
            token_bucket_refill_rate: self.rate_limiting.token_bucket_refill_rate,
            burst_allowance_multiplier: self.rate_limiting.burst_allowance_multiplier,
            backoff_base_delay_ms: self.rate_limiting.backoff_base_delay_ms,
            backoff_max_delay_ms: self.rate_limiting.backoff_max_delay_ms,
            backoff_multiplier: self.rate_limiting.backoff_multiplier,
        }
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
        assert_eq!(config.models.len(), 3);
        assert!(config.models.contains_key("openai"));
        assert!(config.models.contains_key("anthropic"));
        assert!(config.models.contains_key("gemini"));
        assert_eq!(config.billing.daily_limit_usd, Some(10.0));
        assert_eq!(config.ui.theme, "system");
        assert!(config.fallback.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.capability_detection.enabled);
    }

    #[test]
    fn test_priority_sorting() {
        let config = AppConfig::default();
        let mut config = config;
        
        // Enable all providers
        for (_, model_config) in config.models.iter_mut() {
            model_config.enabled = true;
        }
        
        let models_by_priority = config.get_models_by_priority();
        assert_eq!(models_by_priority.len(), 3);
        
        // Should be sorted by priority (descending)
        assert_eq!(models_by_priority[0].0, "openai"); // priority 100
        assert_eq!(models_by_priority[1].0, "anthropic"); // priority 90
        assert_eq!(models_by_priority[2].0, "gemini"); // priority 80
    }

    #[test]
    fn test_config_conversion() {
        let config = AppConfig::default();
        
        let rate_limits = config.to_rate_limits("openai");
        assert_eq!(rate_limits.requests_per_minute, Some(60));
        
        let fallback_config = config.to_fallback_config();
        assert!(fallback_config.fallback_on_error);
        
        let rate_limiter_config = config.to_rate_limiter_config();
        assert!(rate_limiter_config.enable_rate_limiting);
    }
}