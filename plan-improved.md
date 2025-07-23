# Multi-Model Chat Application Implementation Plan (Improved)
*Cross-Platform Desktop Application with MCP Server Support*

## Project Overview

This document outlines the implementation plan for a cross-platform desktop chat application built in Rust that provides:
- Access to multiple AI models (configurable by user)
- Integration with local MCP (Model Context Protocol) servers
- Billing/expense tracking with spending limits
- Native experience on Linux, macOS, and Windows

**Platform Priority:**
- **Primary**: Linux, macOS
- **Secondary**: Windows

## Phase 1: Core Architecture & Foundation

### 1.1 Cross-Platform Project Structure

```
src/
├── main.rs
├── app/
│   ├── mod.rs
│   ├── state.rs          // Global app state with Arc<RwLock>
│   └── config.rs         // Cross-platform configuration management
├── models/
│   ├── mod.rs
│   ├── provider.rs       // Model provider trait with error recovery
│   ├── openai.rs         // OpenAI implementation
│   ├── anthropic.rs      // Anthropic implementation
│   ├── local.rs          // Local model support (Ollama/LM Studio)
│   └── circuit_breaker.rs // Circuit breaker pattern for reliability
├── mcp/
│   ├── mod.rs
│   ├── client.rs         // MCP client implementation
│   ├── protocol.rs       // MCP protocol definitions with versioning
│   ├── server_manager.rs // MCP server lifecycle management
│   ├── sandbox.rs        // Process sandboxing (platform-specific)
│   └── transport.rs      // Stdio/WebSocket transport abstraction
├── platform/
│   ├── mod.rs
│   ├── secure_storage.rs // Platform-specific secure storage abstraction
│   ├── process.rs        // Platform-specific process management
│   └── paths.rs          // Platform-specific directory paths
├── ui/
│   ├── mod.rs
│   ├── chat.rs           // Chat interface with streaming support
│   ├── settings.rs       // Settings panels
│   └── components/       // Reusable UI components
├── storage/
│   ├── mod.rs
│   ├── database.rs       // SQLite with proper decimal handling
│   ├── migrations/       // DB schema migrations
│   └── backup.rs         // Data backup and recovery
├── billing/
│   ├── mod.rs
│   ├── tracker.rs        // Usage tracking with independent verification
│   ├── limits.rs         // Spending limits enforcement
│   └── pricing.rs        // Dynamic pricing updates
└── error/
    ├── mod.rs
    ├── recovery.rs       // Error recovery strategies
    └── reporting.rs      // Error reporting and diagnostics
```

### 1.2 Core Dependencies (Updated)

```toml
[dependencies]
# UI Framework - REMOVED macos-private-api for cross-platform compatibility
tauri = { version = "2.0", features = ["system-tray", "updater"] }
tauri-plugin-store = "2.0"
tauri-plugin-dialog = "2.0"
tauri-plugin-notification = "2.0"

# Async Runtime
tokio = { version = "1.0", features = ["full"] }
futures = "0.3"
async-stream = "0.3"

# HTTP & WebSockets
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio-tungstenite = "0.21"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"

# Database - Fixed decimal handling
sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio-rustls", "migrate", "decimal"] }
rust_decimal = { version = "1.33", features = ["serde"] }

# Configuration & Platform Support
config = "0.14"
directories = "5.0"

# Cross-platform secure storage
keyring = "2.0"

# Process management and sandboxing
tokio-process = "0.2"
nix = { version = "0.27", features = ["process", "signal"] } # Unix platforms
winapi = { version = "0.3", features = ["processthreadsapi"] } # Windows

# Error Handling & Circuit Breaker
anyhow = "1.0"
thiserror = "1.0"
tokio-retry = "0.3"

# Logging & Monitoring
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

# State Management
arc-swap = "1.6"
parking_lot = "0.12"

# Input Validation
validator = { version = "0.18", features = ["derive"] }
sanitize-html = "0.4"

[target.'cfg(unix)'.dependencies]
# Unix-specific dependencies for process management
libc = "0.2"

[target.'cfg(windows)'.dependencies]
# Windows-specific dependencies
windows = { version = "0.52", features = ["Win32_System_Threading"] }
```

## Phase 2: Cross-Platform Architecture

### 2.1 Platform Abstraction Layer

```rust
// platform/secure_storage.rs
#[async_trait]
pub trait SecureStorage: Send + Sync {
    async fn store(&self, service: &str, key: &str, value: &str) -> Result<()>;
    async fn retrieve(&self, service: &str, key: &str) -> Result<Option<String>>;
    async fn delete(&self, service: &str, key: &str) -> Result<()>;
}

pub fn create_secure_storage() -> Box<dyn SecureStorage> {
    #[cfg(target_os = "macos")]
    return Box::new(KeychainStorage::new());
    
    #[cfg(target_os = "linux")]
    return Box::new(SecretServiceStorage::new());
    
    #[cfg(target_os = "windows")]
    return Box::new(CredentialManagerStorage::new());
}

// platform/process.rs
#[async_trait]
pub trait ProcessManager: Send + Sync {
    async fn spawn_sandboxed(&self, config: ProcessConfig) -> Result<SandboxedProcess>;
    async fn terminate(&self, process: &mut SandboxedProcess) -> Result<()>;
    fn get_resource_limits(&self) -> ResourceLimits;
}

pub struct ProcessConfig {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub env_vars: HashMap<String, String>,
    pub resource_limits: ResourceLimits,
    pub network_access: bool,
    pub file_system_access: Vec<PathBuf>,
}

pub struct ResourceLimits {
    pub max_memory_mb: u64,
    pub max_cpu_percent: u8,
    pub max_open_files: u32,
    pub timeout_seconds: u64,
}
```

### 2.2 Enhanced Model Provider with Error Recovery

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn send_message(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn stream_message(&self, request: ChatRequest) -> Result<ChatStream>;
    fn get_pricing(&self) -> Option<PricingInfo>;
    fn get_capabilities(&self) -> ModelCapabilities;
    fn get_config_schema(&self) -> ConfigSchema;
    
    // New methods for reliability
    async fn health_check(&self) -> Result<HealthStatus>;
    fn get_rate_limits(&self) -> RateLimits;
    fn supports_streaming(&self) -> bool;
}

pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub model: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<Tool>>,
    pub stream: bool,
    // New fields for better control
    pub timeout: Option<Duration>,
    pub retry_config: Option<RetryConfig>,
}

// Circuit breaker implementation
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
    failure_count: Arc<AtomicU32>,
}

#[derive(Debug, Clone)]
pub enum CircuitState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen,
}
```

## Phase 3: Enhanced MCP Integration with Sandboxing

### 3.1 Secure MCP Server Management

```rust
pub struct MCPServerManager {
    servers: HashMap<String, MCPServerInstance>,
    process_manager: Box<dyn ProcessManager>,
    config: Arc<RwLock<AppConfig>>,
    health_checker: HealthChecker,
}

pub struct MCPServerInstance {
    client: MCPClient,
    process: Option<SandboxedProcess>,
    tools: Vec<Tool>,
    resources: Vec<Resource>,
    health_status: Arc<RwLock<HealthStatus>>,
    circuit_breaker: CircuitBreaker,
}

// Enhanced MCP client with versioning support
pub struct MCPClient {
    transport: Box<dyn MCPTransport>,
    capabilities: ClientCapabilities,
    server_info: Option<ServerInfo>,
    protocol_version: ProtocolVersion,
    request_timeout: Duration,
    retry_config: RetryConfig,
}

// Platform-specific sandboxing implementations
#[cfg(target_os = "linux")]
pub struct LinuxSandbox {
    // Uses namespaces, cgroups, and seccomp
    namespace_config: NamespaceConfig,
    cgroup_limits: CgroupLimits,
    seccomp_policy: SeccompPolicy,
}

#[cfg(target_os = "macos")]
pub struct MacOSSandbox {
    // Uses sandbox-exec and resource limits
    sandbox_profile: String,
    resource_limits: RusageResourceLimits,
}

#[cfg(target_os = "windows")]
pub struct WindowsSandbox {
    // Uses job objects and restricted tokens
    job_object: JobObject,
    security_attributes: SecurityAttributes,
}
```

### 3.2 MCP Protocol Enhancements

```rust
// Versioned protocol support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolVersion {
    V1_0,
    V1_1,
    V2_0, // Future versions
}

// Enhanced error handling for MCP operations
#[derive(Debug, thiserror::Error)]
pub enum MCPError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("Protocol version mismatch: client {client}, server {server}")]
    VersionMismatch { client: String, server: String },
    #[error("Server unresponsive after {timeout:?}")]
    Timeout { timeout: Duration },
    #[error("Resource limit exceeded: {resource}")]
    ResourceLimit { resource: String },
    #[error("Sandbox violation: {violation}")]
    SandboxViolation { violation: String },
}

// Tool execution with input validation
pub struct ToolExecutor {
    validator: InputValidator,
    sanitizer: InputSanitizer,
    executor: Box<dyn ToolBackend>,
}

impl ToolExecutor {
    pub async fn execute_tool(&self, request: ToolRequest) -> Result<ToolResponse> {
        // Validate and sanitize inputs
        let validated_request = self.validator.validate(request)?;
        let sanitized_request = self.sanitizer.sanitize(validated_request)?;
        
        // Execute with timeout and resource monitoring
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            self.executor.execute(sanitized_request)
        ).await??;
        
        Ok(result)
    }
}
```

## Phase 4: Fixed Database & Billing System

### 4.1 Proper Database Schema with Decimal Support

```sql
-- migrations/001_initial.sql
PRAGMA foreign_keys = ON;

CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()), -- Unix timestamp
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    model_provider TEXT,
    total_cost TEXT, -- Store as string representation of decimal
    message_count INTEGER DEFAULT 0
);

CREATE INDEX idx_conversations_updated_at ON conversations(updated_at);
CREATE INDEX idx_conversations_provider ON conversations(model_provider);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    model_used TEXT,
    provider TEXT,
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    cost TEXT, -- Store as string representation of decimal
    processing_time_ms INTEGER,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX idx_messages_conversation_id ON messages(conversation_id);
CREATE INDEX idx_messages_timestamp ON messages(timestamp);
CREATE INDEX idx_messages_provider ON messages(provider);

CREATE TABLE usage_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost TEXT NOT NULL, -- Store as string representation of decimal
    conversation_id TEXT,
    request_id TEXT, -- For deduplication
    billing_period TEXT, -- YYYY-MM format for monthly aggregation
    FOREIGN KEY (conversation_id) REFERENCES conversations(id),
    UNIQUE(request_id) -- Prevent duplicate billing
);

CREATE INDEX idx_usage_records_timestamp ON usage_records(timestamp);
CREATE INDEX idx_usage_records_provider_model ON usage_records(provider, model);
CREATE INDEX idx_usage_records_billing_period ON usage_records(billing_period);

CREATE TABLE pricing_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_price_per_1k TEXT NOT NULL, -- Decimal as string
    output_price_per_1k TEXT NOT NULL, -- Decimal as string
    effective_date INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(provider, model, effective_date)
);

CREATE INDEX idx_pricing_data_provider_model ON pricing_data(provider, model);
```

### 4.2 Enhanced Usage Tracking with Independent Verification

```rust
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct UsageTracker {
    db: SqlitePool,
    limits: Arc<RwLock<BillingLimits>>,
    pricing_service: Arc<PricingService>,
    // Independent token counting for verification
    token_counter: Arc<dyn TokenCounter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Option<i64>,
    pub timestamp: i64,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost: Decimal,
    pub conversation_id: Option<String>,
    pub request_id: String, // For deduplication
    pub billing_period: String,
    // Verification fields
    pub provider_reported_tokens: Option<TokenCounts>,
    pub independent_token_count: Option<TokenCounts>,
    pub cost_verification_status: CostVerificationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: u32,
    pub output: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CostVerificationStatus {
    Verified,
    Discrepancy { difference_percent: f64 },
    UnableToVerify { reason: String },
}

impl UsageTracker {
    pub async fn record_usage(&self, mut usage: UsageRecord) -> Result<()> {
        // Independent token counting for verification
        if let Some(content) = self.get_message_content(&usage.request_id).await? {
            let independent_count = self.token_counter.count_tokens(&usage.model, &content).await?;
            usage.independent_token_count = Some(independent_count);
            
            // Verify cost calculation
            usage.cost_verification_status = self.verify_cost(&usage).await?;
        }
        
        // Check spending limits before recording
        self.check_spending_limits(&usage).await?;
        
        // Store with proper decimal handling
        let cost_str = usage.cost.to_string();
        sqlx::query!(
            r#"
            INSERT INTO usage_records 
            (timestamp, provider, model, input_tokens, output_tokens, cost, 
             conversation_id, request_id, billing_period)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            usage.timestamp,
            usage.provider,
            usage.model,
            usage.input_tokens as i64,
            usage.output_tokens as i64,
            cost_str,
            usage.conversation_id,
            usage.request_id,
            usage.billing_period
        )
        .execute(&self.db)
        .await?;
        
        Ok(())
    }
    
    async fn verify_cost(&self, usage: &UsageRecord) -> Result<CostVerificationStatus> {
        let pricing = self.pricing_service.get_current_pricing(&usage.provider, &usage.model).await?;
        
        let calculated_cost = 
            (Decimal::from(usage.input_tokens) * pricing.input_price_per_1k / Decimal::from(1000)) +
            (Decimal::from(usage.output_tokens) * pricing.output_price_per_1k / Decimal::from(1000));
        
        let difference = ((usage.cost - calculated_cost) / calculated_cost * Decimal::from(100)).abs();
        
        if difference > Decimal::from_str("5.0")? { // 5% threshold
            Ok(CostVerificationStatus::Discrepancy { 
                difference_percent: difference.to_f64().unwrap_or(0.0) 
            })
        } else {
            Ok(CostVerificationStatus::Verified)
        }
    }
}

// Enhanced spending limits with multiple dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingLimits {
    pub daily_limit: Option<Decimal>,
    pub monthly_limit: Option<Decimal>,
    pub per_model_limits: HashMap<String, Decimal>,
    pub per_conversation_limits: HashMap<String, Decimal>,
    pub rate_limits: HashMap<String, RateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests_per_minute: u32,
    pub tokens_per_minute: u32,
    pub cost_per_minute: Decimal,
}
```

## Phase 5: Cross-Platform UI & Frontend

### 5.1 Enhanced Frontend Architecture

```typescript
// Frontend structure optimized for cross-platform
src/
├── components/
│   ├── chat/
│   │   ├── ChatWindow.tsx         // Main chat interface
│   │   ├── MessageList.tsx        // Virtual scrolling for performance
│   │   ├── StreamingMessage.tsx   // Real-time streaming display
│   │   └── InputArea.tsx          // Multi-line input with shortcuts
│   ├── settings/
│   │   ├── ModelSettings.tsx      // Model configuration
│   │   ├── MCPSettings.tsx        // MCP server management
│   │   ├── BillingSettings.tsx    // Usage limits and monitoring
│   │   └── PlatformSettings.tsx   // Platform-specific settings
│   ├── platform/
│   │   ├── TitleBar.tsx           // Custom title bar for consistency
│   │   ├── MenuBar.tsx            // Cross-platform menu
│   │   └── NotificationManager.tsx // Native notifications
│   └── common/
│       ├── ErrorBoundary.tsx      // React error boundaries
│       ├── LoadingSpinner.tsx     // Loading states
│       └── Modal.tsx              // Reusable modal component
├── hooks/
│   ├── useChat.ts                 // Chat state management
│   ├── useModels.ts               // Model provider management
│   ├── useBilling.ts              // Usage tracking
│   ├── usePlatform.ts             // Platform-specific functionality
│   └── useStreaming.ts            // Streaming response handling
├── stores/
│   ├── chatStore.ts               // Zustand store for chat state
│   ├── settingsStore.ts           // Application settings
│   └── billingStore.ts            // Billing data
└── utils/
    ├── streaming.ts               // Streaming utilities with backpressure
    ├── validation.ts              // Input validation
    └── platform.ts                // Platform detection utilities
```

### 5.2 Streaming Architecture with Backpressure

```typescript
// Enhanced streaming with backpressure handling
export class StreamingManager {
    private buffer: MessageChunk[] = [];
    private maxBufferSize = 100;
    private processingRate = 50; // chunks per second
    private isProcessing = false;

    async handleStream(stream: ReadableStream<MessageChunk>): Promise<void> {
        const reader = stream.getReader();
        
        try {
            while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                
                // Implement backpressure
                if (this.buffer.length >= this.maxBufferSize) {
                    await this.waitForBufferSpace();
                }
                
                this.buffer.push(value);
                
                if (!this.isProcessing) {
                    this.processBuffer();
                }
            }
        } finally {
            reader.releaseLock();
        }
    }
    
    private async processBuffer(): Promise<void> {
        this.isProcessing = true;
        
        while (this.buffer.length > 0) {
            const chunk = this.buffer.shift()!;
            await this.displayChunk(chunk);
            
            // Rate limiting to prevent UI freezing
            await new Promise(resolve => 
                setTimeout(resolve, 1000 / this.processingRate)
            );
        }
        
        this.isProcessing = false;
    }
}
```

## Phase 6: Enhanced Security Architecture

### 6.1 Unified Secure Storage

```rust
// Cross-platform secure storage implementation
pub struct SecureStorageManager {
    backend: Box<dyn SecureStorage>,
    encryption_key: Option<Vec<u8>>,
}

impl SecureStorageManager {
    pub fn new() -> Result<Self> {
        let backend = create_platform_storage()?;
        Ok(Self {
            backend,
            encryption_key: None,
        })
    }
    
    pub async fn store_api_key(&self, provider: &str, key: &str) -> Result<()> {
        // Additional encryption layer for sensitive data
        let encrypted_key = self.encrypt_if_needed(key)?;
        
        self.backend.store(
            "valechat_api_keys",
            provider,
            &encrypted_key
        ).await?;
        
        // Audit logging
        self.log_key_access("store", provider).await?;
        
        Ok(())
    }
    
    pub async fn retrieve_api_key(&self, provider: &str) -> Result<Option<String>> {
        let encrypted_key = self.backend.retrieve("valechat_api_keys", provider).await?;
        
        match encrypted_key {
            Some(key) => {
                let decrypted = self.decrypt_if_needed(&key)?;
                self.log_key_access("retrieve", provider).await?;
                Ok(Some(decrypted))
            }
            None => Ok(None),
        }
    }
    
    fn encrypt_if_needed(&self, data: &str) -> Result<String> {
        if let Some(key) = &self.encryption_key {
            // Use AES-256-GCM for additional encryption
            let encrypted = encrypt_aes_gcm(data.as_bytes(), key)?;
            Ok(base64::encode(encrypted))
        } else {
            Ok(data.to_string())
        }
    }
}

#[cfg(target_os = "linux")]
fn create_platform_storage() -> Result<Box<dyn SecureStorage>> {
    Ok(Box::new(SecretServiceStorage::new()?))
}

#[cfg(target_os = "macos")]
fn create_platform_storage() -> Result<Box<dyn SecureStorage>> {
    Ok(Box::new(KeychainStorage::new()?))
}

#[cfg(target_os = "windows")]
fn create_platform_storage() -> Result<Box<dyn SecureStorage>> {
    Ok(Box::new(CredentialManagerStorage::new()?))
}
```

### 6.2 Input Validation & Sanitization

```rust
pub struct InputValidator {
    html_sanitizer: Html5Sanitizer,
    json_validator: JsonValidator,
    size_limits: SizeLimits,
}

#[derive(Debug, Clone)]
pub struct SizeLimits {
    pub max_message_length: usize,      // 50KB
    pub max_conversation_length: usize,  // 1MB
    pub max_tool_input_size: usize,     // 10KB
    pub max_file_upload_size: usize,    // 10MB
}

impl InputValidator {
    pub fn validate_chat_message(&self, message: &str) -> Result<String> {
        // Size validation
        if message.len() > self.size_limits.max_message_length {
            return Err(ValidationError::MessageTooLarge);
        }
        
        // HTML sanitization
        let sanitized = self.html_sanitizer.sanitize(message)?;
        
        // Content validation (no malicious patterns)
        self.validate_content_safety(&sanitized)?;
        
        Ok(sanitized)
    }
    
    pub fn validate_tool_input(&self, input: &serde_json::Value) -> Result<serde_json::Value> {
        // Size validation
        let input_str = serde_json::to_string(input)?;
        if input_str.len() > self.size_limits.max_tool_input_size {
            return Err(ValidationError::ToolInputTooLarge);
        }
        
        // JSON schema validation
        self.json_validator.validate(input)?;
        
        // Sanitize string values
        let sanitized = self.sanitize_json_strings(input)?;
        
        Ok(sanitized)
    }
    
    fn validate_content_safety(&self, content: &str) -> Result<()> {
        // Check for potential security issues
        if content.contains("<script") || content.contains("javascript:") {
            return Err(ValidationError::PotentialXSS);
        }
        
        // Check for potential injection attacks
        if self.contains_sql_injection_patterns(content) {
            return Err(ValidationError::PotentialInjection);
        }
        
        Ok(())
    }
}
```

## Revised Implementation Timeline (16 Weeks)

### Phase 1: Foundation (Weeks 1-4)
**Deliverables:**
- Cross-platform Tauri project setup
- Platform abstraction layer implementation
- Configuration system with secure storage
- Basic OpenAI provider with circuit breaker
- Foundation UI components

**Key Tasks:**
- Platform-specific secure storage backends
- Configuration loading/saving across platforms
- Error recovery and circuit breaker implementation
- Basic React components with TypeScript
- Cross-platform build system setup

### Phase 2: Multi-Model Support (Weeks 5-8)
**Deliverables:**
- Anthropic and local model providers
- Enhanced model selection interface
- Streaming architecture with backpressure
- Model configuration management
- Basic chat functionality

**Key Tasks:**
- Multiple provider implementations
- Streaming response handling
- Model switching and configuration UI
- Performance optimization for streaming
- Error handling and fallback mechanisms

### Phase 3: MCP Integration - Basic (Weeks 9-12)
**Deliverables:**
- MCP protocol implementation
- Basic server management (stdio transport)
- Tool integration in chat interface
- Process sandboxing (platform-specific)
- MCP configuration UI

**Key Tasks:**
- JSON-RPC protocol implementation
- Stdio transport with process management
- Basic sandboxing for security
- Tool execution with validation
- Server lifecycle management UI

### Phase 4: Database & Billing (Weeks 13-14)
**Deliverables:**
- SQLite database with proper decimal handling
- Usage tracking with verification
- Billing dashboard and limits
- Data migration system
- Backup and recovery

**Key Tasks:**
- Database schema with indexes
- Independent token counting
- Cost verification and discrepancy detection
- Spending limits enforcement
- Historical data analysis

### Phase 5: Advanced MCP & Polish (Weeks 15-16)
**Deliverables:**
- WebSocket MCP transport
- Advanced MCP features (resources, prompts)
- Comprehensive error recovery
- UI/UX improvements
- Performance optimizations

**Key Tasks:**
- WebSocket transport implementation
- Resource and prompt template support
- Advanced error handling and recovery
- UI polish and accessibility
- Performance testing and optimization

## Cross-Platform Considerations

### Platform-Specific Features

**Linux:**
- GTK integration for native dialogs
- Secret Service for credential storage
- Desktop file and icon installation
- Package manager distribution (deb/rpm)

**macOS:**
- Keychain integration
- Apple Silicon + Intel universal binaries
- Code signing and notarization
- App Store distribution preparation

**Windows:**
- Credential Manager integration
- Windows Installer (MSI) packages
- Code signing for trust
- Auto-update with proper permissions

### Build System

```toml
# Cross-platform build configuration
[build-dependencies]
tauri-build = { version = "2.0", features = ["isolation"] }

[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=lld"]

[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-undefined", "-C", "link-arg=dynamic_lookup"]

[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]
```

## Risk Mitigation

### Technical Risks
- **Cross-platform compatibility**: Comprehensive testing on all platforms
- **MCP server stability**: Robust sandboxing and error recovery
- **Performance with streaming**: Backpressure handling and buffering
- **Security vulnerabilities**: Regular security audits and penetration testing

### Implementation Risks
- **Timeline slippage**: Built-in buffer time and phased approach
- **Dependency issues**: Fallback plans and alternative implementations
- **Platform-specific bugs**: Platform-specific testing and debugging
- **User adoption**: Beta testing program and feedback integration

The improved plan addresses the original issues while adding comprehensive cross-platform support with realistic timelines and proper error handling throughout the system.

<function_calls>
<invoke name="TodoWrite">
<parameter name="todos">[{"id": "1", "content": "Analyze cross-platform requirements and constraints", "status": "completed", "priority": "high"}, {"id": "2", "content": "Revise architecture for multi-platform support", "status": "completed", "priority": "high"}, {"id": "3", "content": "Update MCP integration approach with sandboxing", "status": "in_progress", "priority": "high"}, {"id": "4", "content": "Fix database and billing system issues", "status": "pending", "priority": "high"}, {"id": "5", "content": "Revise timeline with realistic estimates", "status": "pending", "priority": "medium"}, {"id": "6", "content": "Update security architecture", "status": "pending", "priority": "medium"}, {"id": "7", "content": "Improve error handling and resilience", "status": "pending", "priority": "medium"}]