use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tracing::{debug, warn, error};

use crate::error::{Error, Result};

/// Validation error types
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Input too large: {size} bytes exceeds limit of {limit} bytes")]
    InputTooLarge { size: usize, limit: usize },
    
    #[error("Invalid input format: {message}")]
    InvalidFormat { message: String },
    
    #[error("Potential security threat detected: {threat}")]
    SecurityThreat { threat: String },
    
    #[error("Potential XSS attack detected")]
    PotentialXSS,
    
    #[error("Potential injection attack detected")]
    PotentialInjection,
    
    #[error("Forbidden content detected: {content}")]
    ForbiddenContent { content: String },
    
    #[error("Rate limit exceeded")]
    RateLimit,
    
    #[error("JSON schema validation failed: {message}")]
    SchemaValidation { message: String },
}

/// Configuration for input validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub max_input_size: usize,
    pub max_nesting_depth: usize,
    pub allow_javascript: bool,
    pub allow_html: bool,
    pub allow_sql: bool,
    pub forbidden_patterns: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub enable_schema_validation: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_input_size: 1024 * 1024, // 1MB
            max_nesting_depth: 10,
            allow_javascript: false,
            allow_html: false,
            allow_sql: false,
            forbidden_patterns: vec![
                r"<script.*?>.*?</script>".to_string(),
                r"javascript:".to_string(),
                r"data:text/html".to_string(),
                r"eval\s*\(".to_string(),
                r"exec\s*\(".to_string(),
            ],
            rate_limit_per_minute: 100,
            enable_schema_validation: true,
        }
    }
}

/// Input validator for MCP tool parameters
pub struct InputValidator {
    config: ValidationConfig,
    compiled_patterns: Vec<Regex>,
    rate_limiter: HashMap<String, Vec<std::time::Instant>>,
}

impl InputValidator {
    pub fn new(config: ValidationConfig) -> Result<Self> {
        let mut compiled_patterns = Vec::new();
        
        for pattern in &config.forbidden_patterns {
            let regex = Regex::new(pattern).map_err(|e| {
                Error::validation(format!("Invalid regex pattern '{}': {}", pattern, e))
            })?;
            compiled_patterns.push(regex);
        }

        Ok(Self {
            config,
            compiled_patterns,
            rate_limiter: HashMap::new(),
        })
    }

    /// Validate tool input parameters
    pub fn validate_tool_input(
        &mut self,
        tool_name: &str,
        parameters: &JsonValue,
        client_id: Option<&str>,
        schema: Option<&JsonValue>,
    ) -> Result<()> {
        debug!("Validating input for tool: {}", tool_name);

        // Rate limiting check
        if let Some(client) = client_id {
            self.check_rate_limit(client)?;
        }

        // Size validation
        let input_str = serde_json::to_string(parameters)?;
        if input_str.len() > self.config.max_input_size {
            return Err(Error::validation(format!(
                "Input too large: {} bytes exceeds limit of {} bytes",
                input_str.len(),
                self.config.max_input_size
            )));
        }

        // Nesting depth validation
        self.validate_nesting_depth(parameters, 0)?;

        // Content validation
        self.validate_content(&input_str)?;

        // Schema validation if enabled and schema provided
        if self.config.enable_schema_validation {
            if let Some(schema_def) = schema {
                self.validate_against_schema(parameters, schema_def)?;
            }
        }

        debug!("Input validation passed for tool: {}", tool_name);
        Ok(())
    }

    /// Check rate limiting for a client
    fn check_rate_limit(&mut self, client_id: &str) -> Result<()> {
        let now = std::time::Instant::now();
        let minute_ago = now - std::time::Duration::from_secs(60);

        // Get or create rate limit entry for client
        let requests = self.rate_limiter.entry(client_id.to_string()).or_insert_with(Vec::new);

        // Remove old entries
        requests.retain(|&timestamp| timestamp > minute_ago);

        // Check if rate limit exceeded
        if requests.len() >= self.config.rate_limit_per_minute as usize {
            warn!("Rate limit exceeded for client: {}", client_id);
            return Err(Error::validation("Rate limit exceeded".to_string()));
        }

        // Add current request
        requests.push(now);

        Ok(())
    }

    /// Validate JSON nesting depth
    fn validate_nesting_depth(&self, value: &JsonValue, current_depth: usize) -> Result<()> {
        if current_depth > self.config.max_nesting_depth {
            return Err(Error::validation(format!(
                "JSON nesting depth {} exceeds maximum of {}",
                current_depth,
                self.config.max_nesting_depth
            )));
        }

        match value {
            JsonValue::Object(obj) => {
                for (_, v) in obj {
                    self.validate_nesting_depth(v, current_depth + 1)?;
                }
            }
            JsonValue::Array(arr) => {
                for v in arr {
                    self.validate_nesting_depth(v, current_depth + 1)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Validate content for security threats
    fn validate_content(&self, content: &str) -> Result<()> {
        // Check for forbidden patterns
        for (i, pattern) in self.compiled_patterns.iter().enumerate() {
            if pattern.is_match(content) {
                let pattern_str = &self.config.forbidden_patterns[i];
                warn!("Forbidden pattern detected: {}", pattern_str);
                return Err(Error::validation(format!(
                    "Forbidden content detected: {}",
                    pattern_str
                )));
            }
        }

        // Check for JavaScript if not allowed
        if !self.config.allow_javascript {
            if self.contains_javascript(content) {
                return Err(Error::validation("JavaScript content not allowed".to_string()));
            }
        }

        // Check for HTML if not allowed
        if !self.config.allow_html {
            if self.contains_html(content) {
                return Err(Error::validation("HTML content not allowed".to_string()));
            }
        }

        // Check for SQL if not allowed
        if !self.config.allow_sql {
            if self.contains_sql_injection_patterns(content) {
                return Err(Error::validation("Potential SQL injection detected".to_string()));
            }
        }

        Ok(())
    }

    /// Check for JavaScript content
    fn contains_javascript(&self, content: &str) -> bool {
        let js_patterns = [
            r"<script",
            r"javascript:",
            r"eval\s*\(",
            r"setTimeout\s*\(",
            r"setInterval\s*\(",
            r"Function\s*\(",
            r"alert\s*\(",
            r"document\.",
            r"window\.",
        ];

        let content_lower = content.to_lowercase();
        for pattern in &js_patterns {
            if Regex::new(pattern).unwrap().is_match(&content_lower) {
                return true;
            }
        }
        false
    }

    /// Check for HTML content
    fn contains_html(&self, content: &str) -> bool {
        let html_patterns = [
            r"<[^>]+>",
            r"&[a-zA-Z]+;",
            r"&#\d+;",
            r"&#x[0-9a-fA-F]+;",
        ];

        for pattern in &html_patterns {
            if Regex::new(pattern).unwrap().is_match(content) {
                return true;
            }
        }
        false
    }

    /// Check for SQL injection patterns
    fn contains_sql_injection_patterns(&self, content: &str) -> bool {
        let sql_patterns = [
            r"(?i)\bunion\s+select\b",
            r"(?i)\bselect\s+.*\bfrom\b",
            r"(?i)\binsert\s+into\b",
            r"(?i)\bdelete\s+from\b",
            r"(?i)\bdrop\s+table\b",
            r"(?i)\b--\s*",
            r"(?i)/\*.*\*/",
            r"(?i)\bor\s+1\s*=\s*1\b",
            r"(?i)\band\s+1\s*=\s*1\b",
            r"';.*--",
        ];

        for pattern in &sql_patterns {
            if Regex::new(pattern).unwrap().is_match(content) {
                return true;
            }
        }
        false
    }

    /// Validate against JSON schema (simplified implementation)
    fn validate_against_schema(&self, value: &JsonValue, schema: &JsonValue) -> Result<()> {
        // This is a simplified schema validation
        // In a production system, you'd use a proper JSON Schema library
        
        if let Some(schema_type) = schema.get("type") {
            match schema_type.as_str().unwrap_or("") {
                "object" => {
                    if !value.is_object() {
                        return Err(Error::validation("Expected object type".to_string()));
                    }
                    
                    // Validate required properties
                    if let Some(required) = schema.get("required") {
                        if let Some(required_array) = required.as_array() {
                            let obj = value.as_object().unwrap();
                            for prop in required_array {
                                let prop_name = prop.as_str().unwrap_or("");
                                if !obj.contains_key(prop_name) {
                                    return Err(Error::validation(format!(
                                        "Required property '{}' is missing",
                                        prop_name
                                    )));
                                }
                            }
                        }
                    }
                }
                "array" => {
                    if !value.is_array() {
                        return Err(Error::validation("Expected array type".to_string()));
                    }
                }
                "string" => {
                    if !value.is_string() {
                        return Err(Error::validation("Expected string type".to_string()));
                    }
                }
                "number" => {
                    if !value.is_number() {
                        return Err(Error::validation("Expected number type".to_string()));
                    }
                }
                "boolean" => {
                    if !value.is_boolean() {
                        return Err(Error::validation("Expected boolean type".to_string()));
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

/// Input sanitizer for cleaning potentially dangerous content
pub struct InputSanitizer {
    config: SanitizerConfig,
}

#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    pub remove_html_tags: bool,
    pub escape_special_chars: bool,
    pub normalize_whitespace: bool,
    pub max_string_length: Option<usize>,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            remove_html_tags: true,
            escape_special_chars: true,
            normalize_whitespace: true,
            max_string_length: Some(10000),
        }
    }
}

impl InputSanitizer {
    pub fn new(config: SanitizerConfig) -> Self {
        Self { config }
    }

    /// Sanitize tool input parameters
    pub fn sanitize_tool_input(&self, parameters: &JsonValue) -> Result<JsonValue> {
        debug!("Sanitizing tool input parameters");
        
        let sanitized = self.sanitize_value(parameters)?;
        
        debug!("Input sanitization completed");
        Ok(sanitized)
    }

    /// Sanitize a JSON value recursively
    fn sanitize_value(&self, value: &JsonValue) -> Result<JsonValue> {
        match value {
            JsonValue::String(s) => {
                let mut sanitized = s.clone();
                
                // Remove HTML tags if configured
                if self.config.remove_html_tags {
                    sanitized = self.remove_html_tags(&sanitized);
                }
                
                // Escape special characters if configured
                if self.config.escape_special_chars {
                    sanitized = self.escape_special_chars(&sanitized);
                }
                
                // Normalize whitespace if configured
                if self.config.normalize_whitespace {
                    sanitized = self.normalize_whitespace(&sanitized);
                }
                
                // Truncate if too long
                if let Some(max_len) = self.config.max_string_length {
                    if sanitized.len() > max_len {
                        sanitized.truncate(max_len);
                        sanitized.push_str("...");
                    }
                }
                
                Ok(JsonValue::String(sanitized))
            }
            JsonValue::Array(arr) => {
                let mut sanitized_arr = Vec::new();
                for item in arr {
                    sanitized_arr.push(self.sanitize_value(item)?);
                }
                Ok(JsonValue::Array(sanitized_arr))
            }
            JsonValue::Object(obj) => {
                let mut sanitized_obj = serde_json::Map::new();
                for (key, val) in obj {
                    let sanitized_key = if self.config.escape_special_chars {
                        self.escape_special_chars(key)
                    } else {
                        key.clone()
                    };
                    sanitized_obj.insert(sanitized_key, self.sanitize_value(val)?);
                }
                Ok(JsonValue::Object(sanitized_obj))
            }
            _ => Ok(value.clone()), // Numbers, booleans, null remain unchanged
        }
    }

    /// Remove HTML tags from string
    fn remove_html_tags(&self, input: &str) -> String {
        // Simple HTML tag removal (in production, use a proper HTML sanitizer)
        let re = Regex::new(r"<[^>]*>").unwrap();
        re.replace_all(input, "").to_string()
    }

    /// Escape special characters
    fn escape_special_chars(&self, input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
            .replace('/', "&#x2F;")
    }

    /// Normalize whitespace
    fn normalize_whitespace(&self, input: &str) -> String {
        // Replace multiple whitespace with single space
        let re = Regex::new(r"\s+").unwrap();
        re.replace_all(input.trim(), " ").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validation_config_default() {
        let config = ValidationConfig::default();
        assert_eq!(config.max_input_size, 1024 * 1024);
        assert_eq!(config.max_nesting_depth, 10);
        assert!(!config.allow_javascript);
    }

    #[test]
    fn test_input_validator_creation() {
        let config = ValidationConfig::default();
        let validator = InputValidator::new(config);
        assert!(validator.is_ok());
    }

    #[tokio::test]
    async fn test_input_size_validation() {
        let config = ValidationConfig {
            max_input_size: 100,
            ..ValidationConfig::default()
        };
        let mut validator = InputValidator::new(config).unwrap();

        let small_input = json!({"message": "Hello"});
        assert!(validator.validate_tool_input("test", &small_input, None, None).is_ok());

        let large_input = json!({"message": "x".repeat(200)});
        assert!(validator.validate_tool_input("test", &large_input, None, None).is_err());
    }

    #[tokio::test]
    async fn test_nesting_depth_validation() {
        let config = ValidationConfig {
            max_nesting_depth: 2,
            ..ValidationConfig::default()
        };
        let mut validator = InputValidator::new(config).unwrap();

        let shallow_input = json!({"level1": {"level2": "value"}});
        assert!(validator.validate_tool_input("test", &shallow_input, None, None).is_ok());

        let deep_input = json!({"l1": {"l2": {"l3": {"l4": "value"}}}});
        assert!(validator.validate_tool_input("test", &deep_input, None, None).is_err());
    }

    #[tokio::test]
    async fn test_javascript_detection() {
        let config = ValidationConfig {
            allow_javascript: false,
            ..ValidationConfig::default()
        };
        let mut validator = InputValidator::new(config).unwrap();

        let safe_input = json!({"message": "Hello world"});
        assert!(validator.validate_tool_input("test", &safe_input, None, None).is_ok());

        let js_input = json!({"message": "<script>alert('xss')</script>"});
        assert!(validator.validate_tool_input("test", &js_input, None, None).is_err());
    }

    #[test]
    fn test_sanitizer_config_default() {
        let config = SanitizerConfig::default();
        assert!(config.remove_html_tags);
        assert!(config.escape_special_chars);
        assert_eq!(config.max_string_length, Some(10000));
    }

    #[test]
    fn test_html_tag_removal() {
        let config = SanitizerConfig::default();
        let sanitizer = InputSanitizer::new(config);

        let input = json!({"message": "<b>Hello</b> <script>alert('xss')</script>world"});
        let result = sanitizer.sanitize_tool_input(&input).unwrap();
        
        let expected_text = "Hello world"; // HTML tags removed and special chars escaped
        assert!(result["message"].as_str().unwrap().contains("Hello"));
        assert!(!result["message"].as_str().unwrap().contains("<script>"));
    }

    #[test]
    fn test_special_char_escaping() {
        let config = SanitizerConfig {
            remove_html_tags: false,
            escape_special_chars: true,
            normalize_whitespace: false,
            max_string_length: None,
        };
        let sanitizer = InputSanitizer::new(config);

        let input = json!({"message": "Hello & <world>"});
        let result = sanitizer.sanitize_tool_input(&input).unwrap();
        
        let sanitized = result["message"].as_str().unwrap();
        assert!(sanitized.contains("&amp;"));
        assert!(sanitized.contains("&lt;"));
        assert!(sanitized.contains("&gt;"));
    }

    #[test]
    fn test_whitespace_normalization() {
        let config = SanitizerConfig {
            normalize_whitespace: true,
            ..SanitizerConfig::default()
        };
        let sanitizer = InputSanitizer::new(config);

        let input = json!({"message": "  Hello    world  \n\t  "});
        let result = sanitizer.sanitize_tool_input(&input).unwrap();
        
        let sanitized = result["message"].as_str().unwrap();
        assert_eq!(sanitized, "Hello world");
    }

    #[test]
    fn test_schema_validation() {
        let config = ValidationConfig::default();
        let mut validator = InputValidator::new(config).unwrap();

        let schema = json!({
            "type": "object",
            "required": ["name", "age"]
        });

        let valid_input = json!({"name": "John", "age": 30});
        assert!(validator.validate_tool_input("test", &valid_input, None, Some(&schema)).is_ok());

        let invalid_input = json!({"name": "John"}); // missing age
        assert!(validator.validate_tool_input("test", &invalid_input, None, Some(&schema)).is_err());
    }

    #[test]
    fn test_rate_limiting() {
        let config = ValidationConfig {
            rate_limit_per_minute: 2,
            ..ValidationConfig::default()
        };
        let mut validator = InputValidator::new(config).unwrap();

        let input = json!({"message": "Hello"});
        
        // First two requests should pass
        assert!(validator.validate_tool_input("test", &input, Some("client1"), None).is_ok());
        assert!(validator.validate_tool_input("test", &input, Some("client1"), None).is_ok());
        
        // Third request should fail
        assert!(validator.validate_tool_input("test", &input, Some("client1"), None).is_err());
        
        // Different client should still work
        assert!(validator.validate_tool_input("test", &input, Some("client2"), None).is_ok());
    }
}