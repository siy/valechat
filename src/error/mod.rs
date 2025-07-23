use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Model provider error: {0}")]
    ModelProvider(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Chat error: {0}")]
    Chat(String),

    #[error("Secure storage error: {0}")]
    SecureStorage(#[from] keyring::Error),

    // #[error("Tauri error: {0}")]
    // Tauri(#[from] tauri::Error),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl Error {
    pub fn platform(msg: impl Into<String>) -> Self {
        Error::Platform(msg.into())
    }

    pub fn model_provider(msg: impl Into<String>) -> Self {
        Error::ModelProvider(msg.into())
    }

    pub fn mcp(msg: impl Into<String>) -> Self {
        Error::Mcp(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Error::Validation(msg.into())
    }

    pub fn chat(msg: impl Into<String>) -> Self {
        Error::Chat(msg.into())
    }

    pub fn unknown(msg: impl Into<String>) -> Self {
        Error::Unknown(msg.into())
    }
}