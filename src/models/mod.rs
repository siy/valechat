pub mod anthropic;
pub mod capability_detection;
pub mod circuit_breaker;
pub mod fallback;
pub mod gemini;
pub mod openai;
pub mod provider;
pub mod rate_limiter;

pub use provider::{
    ModelProvider, ChatRequest, ChatResponse, ChatStream, StreamChunk,
    Message, MessageRole, TokenUsage, PricingInfo, ModelCapabilities, 
    HealthStatus, RateLimits
};
pub use anthropic::AnthropicProvider;
pub use capability_detection::{CapabilityDetector, TaskRequirements, TaskType, QualityPriority, ModelRecommendation};
pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use fallback::{ModelFallbackManager, FallbackConfig, FallbackAttempt, ProviderStats};
pub use gemini::GeminiProvider;
pub use openai::OpenAIProvider;
pub use rate_limiter::{MultiProviderRateLimiter, RateLimiterConfig, RateLimitPermit, RateLimiterStatus, TokenBucket};