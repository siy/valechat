// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, State};
use tracing::{info, debug};
use uuid::Uuid;
use chrono;
use rust_decimal::prelude::ToPrimitive;
use valechat::{app::AppState, error::Result as AppResult};
use valechat::chat::types::{MessageContent, MessageRole, ChatSession, SessionSettings, SessionStatus, SessionMetrics};
use valechat::platform::{AppPaths, SecureStorageManager};
use valechat::app::{AppConfig, config::{MCPServerConfig, TransportType}};

// Tauri command result type
type CommandResult<T> = Result<T, String>;

// App info command
#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "ValeChat".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
    }
}

#[derive(serde::Serialize)]
struct AppInfo {
    name: String,
    version: String,
    description: String,
}

// Chat-related commands
#[derive(serde::Deserialize)]
struct CreateConversationRequest {
    title: Option<String>,
}

#[derive(serde::Serialize)]
struct ConversationResponse {
    id: String,
    title: String,
    created_at: i64,
    updated_at: i64,
    model_provider: Option<String>,
    total_cost: Option<String>,
    message_count: i32,
}

#[derive(serde::Deserialize)]
struct SendMessageRequest {
    conversation_id: String,
    content: String,
    model: String,
    provider: String,
}

#[derive(serde::Serialize)]
struct MessageResponse {
    id: String,
    role: String,
    content: String,
    timestamp: i64,
    model_used: Option<String>,
    provider: Option<String>,
    input_tokens: Option<i32>,
    output_tokens: Option<i32>,
    cost: Option<String>,
    processing_time_ms: Option<i64>,
}

#[tauri::command]
async fn get_conversations(app_state: State<'_, AppState>) -> CommandResult<Vec<ConversationResponse>> {
    info!("Getting conversations");
    
    match app_state.get_conversation_repo().list_conversations(Some(50), Some(0)).await {
        Ok(sessions) => {
            let conversations: Vec<ConversationResponse> = sessions.into_iter().map(|session| {
                ConversationResponse {
                    id: session.id,
                    title: session.title,
                    created_at: session.created_at.timestamp_millis(),
                    updated_at: session.updated_at.timestamp_millis(),
                    model_provider: Some(session.model_provider),
                    total_cost: Some(session.metrics.total_cost.to_string()),
                    message_count: session.metrics.message_count as i32,
                }
            }).collect();
            Ok(conversations)
        }
        Err(e) => {
            eprintln!("Failed to retrieve conversations: {}", e);
            Ok(vec![]) // Return empty list on error for now
        }
    }
}

#[tauri::command]
async fn test_create_conversation() -> CommandResult<String> {
    info!("Test create conversation called");
    Ok("Test successful".to_string())
}

#[tauri::command]
async fn create_conversation_simple(
    request: CreateConversationRequest,
) -> CommandResult<String> {
    info!("Simple create conversation called with title: {:?}", request.title);
    Ok(format!("Request received with title: {:?}", request.title))
}

#[tauri::command]
async fn create_conversation(
    request: CreateConversationRequest,
    app_state: State<'_, AppState>
) -> CommandResult<ConversationResponse> {
    info!("Creating conversation: {:?}", request.title);
    
    // Check if app state is available
    debug!("App state is available, proceeding with conversation creation");
    
    let conversation_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let title = request.title.unwrap_or_else(|| "New Conversation".to_string());
    
    // Create a new ChatSession
    let session = ChatSession {
        id: conversation_id.clone(),
        title: title.clone(),
        created_at: now,
        updated_at: now,
        model_provider: "".to_string(), // Will be set when first message is sent
        model_name: "".to_string(),
        system_prompt: None,
        settings: SessionSettings::default(),
        status: SessionStatus::Active,
        metrics: SessionMetrics::default(),
    };
    
    debug!("Attempting to create conversation with ID: {}", conversation_id);
    
    match app_state.get_conversation_repo().create_conversation(&session).await {
        Ok(_) => {
            info!("Successfully created conversation: {}", conversation_id);
            Ok(ConversationResponse {
                id: conversation_id,
                title,
                created_at: now.timestamp_millis(),
                updated_at: now.timestamp_millis(),
                model_provider: None,
                total_cost: Some("0.00".to_string()),
                message_count: 0,
            })
        }
        Err(e) => {
            eprintln!("Failed to create conversation: {}", e);
            debug!("Error details: {:?}", e);
            Err(format!("Failed to create conversation: {}", e))
        }
    }
}

#[tauri::command]
async fn send_message(
    request: SendMessageRequest,
    app_state: State<'_, AppState>
) -> CommandResult<MessageResponse> {
    info!("Sending message to conversation {}", request.conversation_id);
    debug!("Message request details - model: {}, provider: {}", request.model, request.provider);
    
    let start_time = std::time::Instant::now();
    let _user_message_id = Uuid::new_v4().to_string();
    let assistant_message_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    
    // Create user message and save to database
    let user_message = valechat::chat::types::ChatMessage::new(
        request.conversation_id.clone(),
        MessageRole::User,
        MessageContent::text(request.content.clone()),
    );
    
    // Save user message to database
    if let Err(e) = app_state.get_conversation_repo().create_message(&user_message).await {
        eprintln!("Failed to save user message: {}", e);
        return Err(format!("Failed to save user message: {}", e));
    }
    
    // Get model provider configuration
    let config = app_state.get_config();
    let model_config = config.models.get(&request.provider)
        .ok_or_else(|| format!("Provider {} not configured", request.provider))?;
    
    if !model_config.enabled {
        return Err(format!("Provider {} is not enabled", request.provider));
    }
    
    // Create provider based on type
    let (response_text, actual_input_tokens, actual_output_tokens, actual_cost) = match request.provider.as_str() {
        "openai" => {
            info!("Using OpenAI provider with model: {}", request.model);
            
            // Get API key from secure storage
            let api_key = app_state.get_api_key(&request.provider)
                .await
                .map_err(|e| format!("Failed to get API key: {}", e))?
                .ok_or_else(|| format!("No API key configured for {}", request.provider))?;
            
            debug!("API key retrieved successfully");
            
            // Create OpenAI provider
            let provider = valechat::models::openai::OpenAIProvider::new(api_key)
                .map_err(|e| format!("Failed to create OpenAI provider: {}", e))?;
            
            // Create chat request
            let message = valechat::models::provider::Message::new(
                valechat::models::provider::MessageRole::User,
                request.content.clone()
            );
            
            let chat_request = valechat::models::provider::ChatRequest::new(
                vec![message],
                request.model.clone()
            ).with_temperature(0.7);
            
            // Call OpenAI API using ModelProvider trait
            use valechat::models::provider::ModelProvider;
            info!("Sending request to OpenAI API...");
            match provider.send_message(chat_request).await {
                Ok(response) => {
                    info!("Received response from OpenAI");
                    debug!("Response content length: {}", response.content.len());
                    
                    // Extract token usage and cost from the actual response
                    let (input_tokens, output_tokens, cost) = if let Some(usage) = &response.usage {
                        let cost_value = if let Some(pricing) = provider.get_pricing() {
                            let cost_decimal = pricing.calculate_cost(usage);
                            cost_decimal.to_string()
                        } else {
                            "0.0".to_string()
                        };
                        (usage.input_tokens, usage.output_tokens, cost_value)
                    } else {
                        // Fallback values if usage data is not available
                        (0, 0, "0.0".to_string())
                    };
                    
                    (response.content, input_tokens, output_tokens, cost)
                },
                Err(e) => {
                    eprintln!("OpenAI API error: {}", e);
                    return Err(format!("OpenAI API error: {}", e));
                }
            }
        },
        _ => {
            return Err(format!("Provider {} not yet implemented", request.provider));
        }
    };
    
    let assistant_message = valechat::chat::types::ChatMessage::new(
        request.conversation_id.clone(),
        MessageRole::Assistant,
        MessageContent::text(response_text.clone()),
    );
    
    // Save assistant message to database
    if let Err(e) = app_state.get_conversation_repo().create_message(&assistant_message).await {
        eprintln!("Failed to save assistant message: {}", e);
        return Err(format!("Failed to save assistant message: {}", e));
    }
    
    // Record actual usage data from the response
    let cost_decimal = actual_cost.parse::<f64>().unwrap_or(0.0);
    let _usage_request_id = match app_state.get_usage_repo().record_usage(
        &request.provider,
        &request.model,
        actual_input_tokens,
        actual_output_tokens,  
        rust_decimal::Decimal::from_f64_retain(cost_decimal).unwrap_or_default(),
        Some(&request.conversation_id),
        Some(&assistant_message_id),
    ).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to record usage: {}", e);
            String::new()
        }
    };
    
    let processing_time = start_time.elapsed().as_millis() as u64;
    
    Ok(MessageResponse {
        id: assistant_message_id,
        role: "assistant".to_string(),
        content: response_text,
        timestamp: now.timestamp_millis(),
        model_used: Some(request.model),
        provider: Some(request.provider),
        input_tokens: Some(actual_input_tokens as i32),
        output_tokens: Some(actual_output_tokens as i32),
        cost: Some(actual_cost),
        processing_time_ms: Some(processing_time as i64),
    })
}

#[tauri::command]
async fn get_conversation_messages(
    conversation_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<Vec<MessageResponse>> {
    info!("Getting messages for conversation {}", conversation_id);
    
    match app_state.get_conversation_repo().get_messages(&conversation_id).await {
        Ok(messages) => {
            let message_responses: Vec<MessageResponse> = messages.into_iter().map(|msg| {
                MessageResponse {
                    id: msg.id,
                    role: match msg.role {
                        MessageRole::User => "user".to_string(),
                        MessageRole::Assistant => "assistant".to_string(),
                        MessageRole::System => "system".to_string(),
                        MessageRole::Tool => "tool".to_string(),
                    },
                    content: match msg.content {
                        MessageContent::Text(text) => text,
                        _ => "".to_string(), // Handle other content types if needed
                    },
                    timestamp: msg.timestamp.timestamp_millis(),
                    model_used: msg.metadata.get("model_used").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    provider: msg.metadata.get("provider").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    input_tokens: msg.metadata.get("input_tokens").and_then(|v| v.as_i64()).map(|t| t as i32),
                    output_tokens: msg.metadata.get("output_tokens").and_then(|v| v.as_i64()).map(|t| t as i32),
                    cost: msg.metadata.get("cost").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    processing_time_ms: msg.metadata.get("processing_time_ms").and_then(|v| v.as_i64()),
                }
            }).collect();
            Ok(message_responses)
        }
        Err(e) => {
            eprintln!("Failed to retrieve messages: {}", e);
            Ok(vec![])
        }
    }
}

#[tauri::command]
async fn delete_conversation(
    conversation_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Deleting conversation {}", conversation_id);
    
    match app_state.get_conversation_repo().delete_conversation(&conversation_id).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to delete conversation: {}", e);
            Err(format!("Failed to delete conversation: {}", e))
        }
    }
}

#[tauri::command]
async fn update_conversation_title(
    conversation_id: String,
    title: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating conversation {} title to {}", conversation_id, title);
    
    match app_state.get_conversation_repo().update_conversation_title(&conversation_id, &title).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to update conversation title: {}", e);
            Err(format!("Failed to update conversation title: {}", e))
        }
    }
}

// Configuration commands
#[derive(serde::Serialize)]
struct ConfigResponse {
    theme: String,
    language: String,
    model_providers: Vec<ModelProviderResponse>,
    mcp_servers: Vec<MCPServerResponse>,
    billing_limits: BillingLimitsResponse,
    auto_save: bool,
    streaming: bool,
}

#[derive(serde::Serialize)]
struct ModelProviderResponse {
    id: String,
    name: String,
    provider_type: String,
    enabled: bool,
    models: Vec<ModelResponse>,
}

#[derive(serde::Serialize)]
struct ModelResponse {
    id: String,
    name: String,
    display_name: String,
    provider: String,
    context_length: i32,
    supports_streaming: bool,
    input_price_per_1k: Option<String>,
    output_price_per_1k: Option<String>,
}

#[derive(serde::Serialize)]
struct MCPServerResponse {
    id: String,
    name: String,
    command: String,
    args: Vec<String>,
    enabled: bool,
    status: String,
    tools: Vec<MCPToolResponse>,
    resources: Vec<MCPResourceResponse>,
}

#[derive(serde::Serialize)]
struct MCPToolResponse {
    name: String,
    description: String,
}

#[derive(serde::Serialize)]
struct MCPResourceResponse {
    uri: String,
    name: String,
    description: Option<String>,
    mime_type: Option<String>,
}

#[derive(serde::Serialize)]
struct BillingLimitsResponse {
    daily_limit: Option<String>,
    monthly_limit: Option<String>,
    per_model_limits: std::collections::HashMap<String, String>,
    per_conversation_limits: std::collections::HashMap<String, String>,
}

#[tauri::command]
async fn get_app_config(app_state: State<'_, AppState>) -> CommandResult<ConfigResponse> {
    info!("Getting app configuration");
    
    let config = app_state.get_config();
    
    // Convert model providers
    let mut model_providers = Vec::new();
    for (provider_id, provider_config) in &config.models {
        let models = match provider_id.as_str() {
            "openai" => vec![
                ModelResponse {
                    id: "gpt-4".to_string(),
                    name: "gpt-4".to_string(),
                    display_name: "GPT-4".to_string(),
                    provider: "openai".to_string(),
                    context_length: 8192,
                    supports_streaming: true,
                    input_price_per_1k: Some("0.03".to_string()),
                    output_price_per_1k: Some("0.06".to_string()),
                },
                ModelResponse {
                    id: "gpt-3.5-turbo".to_string(),
                    name: "gpt-3.5-turbo".to_string(),
                    display_name: "GPT-3.5 Turbo".to_string(),
                    provider: "openai".to_string(),
                    context_length: 4096,
                    supports_streaming: true,
                    input_price_per_1k: Some("0.0015".to_string()),
                    output_price_per_1k: Some("0.002".to_string()),
                },
            ],
            "anthropic" => vec![
                ModelResponse {
                    id: "claude-3-opus".to_string(),
                    name: "claude-3-opus-20240229".to_string(),
                    display_name: "Claude 3 Opus".to_string(),
                    provider: "anthropic".to_string(),
                    context_length: 200000,
                    supports_streaming: true,
                    input_price_per_1k: Some("0.015".to_string()),
                    output_price_per_1k: Some("0.075".to_string()),
                },
                ModelResponse {
                    id: "claude-3-sonnet".to_string(),
                    name: "claude-3-sonnet-20240229".to_string(),
                    display_name: "Claude 3 Sonnet".to_string(),
                    provider: "anthropic".to_string(),
                    context_length: 200000,
                    supports_streaming: true,
                    input_price_per_1k: Some("0.003".to_string()),
                    output_price_per_1k: Some("0.015".to_string()),
                },
            ],
            _ => vec![],
        };

        model_providers.push(ModelProviderResponse {
            id: provider_id.clone(),
            name: provider_config.provider.clone(),
            provider_type: provider_id.clone(),
            enabled: provider_config.enabled,
            models,
        });
    }

    Ok(ConfigResponse {
        theme: config.ui.theme.clone(),
        language: config.ui.language.clone(),
        model_providers,
        mcp_servers: {
            let mut servers = Vec::new();
            for (server_id, server_config) in &config.mcp_servers {
                servers.push(MCPServerResponse {
                    id: server_id.clone(),
                    name: server_config.name.clone(),
                    command: server_config.command.clone(),
                    args: server_config.args.clone(),
                    enabled: server_config.enabled,
                    status: if server_config.enabled { "ready".to_string() } else { "disabled".to_string() },
                    tools: vec![], // Tools would be populated after connecting to the server
                    resources: vec![], // Resources would be populated after connecting to the server
                });
            }
            servers
        },
        billing_limits: BillingLimitsResponse {
            daily_limit: config.billing.daily_limit_usd.map(|d| d.to_string()),
            monthly_limit: config.billing.monthly_limit_usd.map(|m| m.to_string()),
            per_model_limits: std::collections::HashMap::new(),
            per_conversation_limits: std::collections::HashMap::new(),
        },
        auto_save: config.ui.auto_save,
        streaming: config.ui.streaming,
    })
}

#[derive(serde::Deserialize)]
struct UpdateConfigRequest {
    theme: Option<String>,
    language: Option<String>,
    auto_save: Option<bool>,
    streaming: Option<bool>,
}

#[tauri::command]
async fn update_app_config(
    request: UpdateConfigRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating app configuration");
    
    match app_state.update_config(|config| {
        if let Some(theme) = request.theme {
            config.ui.theme = theme;
        }
        if let Some(language) = request.language {
            config.ui.language = language;
        }
        if let Some(auto_save) = request.auto_save {
            config.ui.auto_save = auto_save;
        }
        if let Some(streaming) = request.streaming {
            config.ui.streaming = streaming;
        }
    }).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to update configuration: {}", e);
            Err(format!("Failed to update configuration: {}", e))
        }
    }
}

#[derive(serde::Deserialize)]
struct UpdateModelProviderRequest {
    id: String,
    enabled: Option<bool>,
}

#[derive(serde::Deserialize)]
struct SetApiKeyRequest {
    provider: String,
    api_key: String,
}

#[derive(serde::Deserialize)]
struct GetApiKeyRequest {
    provider: String,
}

#[tauri::command]
async fn update_model_provider(
    request: UpdateModelProviderRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating model provider {}", request.id);
    
    match app_state.update_config(|config| {
        if let Some(model_config) = config.models.get_mut(&request.id) {
            if let Some(enabled) = request.enabled {
                model_config.enabled = enabled;
            }
            // Additional configuration updates could be added here based on the config HashMap
        }
    }).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to update model provider: {}", e);
            Err(format!("Failed to update model provider: {}", e))
        }
    }
}

#[tauri::command]
async fn set_api_key(
    request: SetApiKeyRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Setting API key for provider: {}", request.provider);
    
    match app_state.set_api_key(&request.provider, &request.api_key).await {
        Ok(_) => {
            info!("API key set successfully for provider: {}", request.provider);
            Ok(())
        },
        Err(e) => {
            eprintln!("Failed to set API key for provider {}: {}", request.provider, e);
            Err(format!("Failed to set API key: {}", e))
        }
    }
}

#[tauri::command]
async fn get_api_key(
    request: GetApiKeyRequest,
    app_state: State<'_, AppState>
) -> CommandResult<Option<String>> {
    info!("Getting API key for provider: {}", request.provider);
    
    match app_state.get_api_key(&request.provider).await {
        Ok(api_key) => {
            if api_key.is_some() {
                info!("API key found for provider: {}", request.provider);
            } else {
                info!("No API key found for provider: {}", request.provider);
            }
            Ok(api_key)
        },
        Err(e) => {
            eprintln!("Failed to get API key for provider {}: {}", request.provider, e);
            Err(format!("Failed to get API key: {}", e))
        }
    }
}

#[tauri::command]
async fn remove_api_key(
    request: GetApiKeyRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Removing API key for provider: {}", request.provider);
    
    match app_state.remove_api_key(&request.provider).await {
        Ok(_) => {
            info!("API key removed successfully for provider: {}", request.provider);
            Ok(())
        },
        Err(e) => {
            eprintln!("Failed to remove API key for provider {}: {}", request.provider, e);
            Err(format!("Failed to remove API key: {}", e))
        }
    }
}

// MCP server commands
#[derive(serde::Deserialize)]
struct AddMCPServerRequest {
    name: String,
    command: String,
    args: Vec<String>,
}

#[derive(serde::Deserialize)]
struct UpdateMCPServerRequest {
    id: String,
    enabled: Option<bool>,
    name: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
}

#[tauri::command]
async fn get_mcp_servers(app_state: State<'_, AppState>) -> CommandResult<Vec<MCPServerResponse>> {
    info!("Getting MCP servers");
    
    let config = app_state.get_config();
    let mut servers = Vec::new();
    
    for (server_id, server_config) in &config.mcp_servers {
        servers.push(MCPServerResponse {
            id: server_id.clone(),
            name: server_config.name.clone(),
            command: server_config.command.clone(),
            args: server_config.args.clone(),
            enabled: server_config.enabled,
            status: if server_config.enabled { "ready".to_string() } else { "disabled".to_string() },
            tools: vec![], // Tools would be populated after connecting to the server
            resources: vec![], // Resources would be populated after connecting to the server
        });
    }
    
    Ok(servers)
}

#[tauri::command]
async fn add_mcp_server(
    request: AddMCPServerRequest,
    app_state: State<'_, AppState>
) -> CommandResult<MCPServerResponse> {
    info!("Adding MCP server: {}", request.name);
    
    let server_id = Uuid::new_v4().to_string();
    
    // Create new MCP server configuration
    let mcp_config = MCPServerConfig {
        name: request.name.clone(),
        command: request.command.clone(),
        args: request.args.clone(),
        transport_type: TransportType::Stdio,
        env_vars: std::collections::HashMap::new(),
        enabled: true,
        auto_start: false,
        timeout_seconds: 30,
    };
    
    // Save to configuration
    match app_state.update_config(|config| {
        config.mcp_servers.insert(server_id.clone(), mcp_config);
    }).await {
        Ok(_) => {
            Ok(MCPServerResponse {
                id: server_id,
                name: request.name,
                command: request.command,
                args: request.args,
                enabled: true,
                status: "stopped".to_string(),
                tools: vec![],
                resources: vec![],
            })
        }
        Err(e) => {
            eprintln!("Failed to add MCP server: {}", e);
            Err(format!("Failed to add MCP server: {}", e))
        }
    }
}

#[tauri::command]
async fn update_mcp_server(
    request: UpdateMCPServerRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating MCP server {}", request.id);
    
    match app_state.update_config(|config| {
        if let Some(server_config) = config.mcp_servers.get_mut(&request.id) {
            if let Some(enabled) = request.enabled {
                server_config.enabled = enabled;
            }
            if let Some(name) = request.name {
                server_config.name = name;
            }
            if let Some(command) = request.command {
                server_config.command = command;
            }
            if let Some(args) = request.args {
                server_config.args = args;
            }
        }
    }).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to update MCP server: {}", e);
            Err(format!("Failed to update MCP server: {}", e))
        }
    }
}

#[tauri::command]
async fn remove_mcp_server(
    server_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Removing MCP server {}", server_id);
    
    match app_state.update_config(|config| {
        config.mcp_servers.remove(&server_id);
    }).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to remove MCP server: {}", e);
            Err(format!("Failed to remove MCP server: {}", e))
        }
    }
}

#[tauri::command]
async fn start_mcp_server(
    server_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Starting MCP server {}", server_id);
    
    let config = app_state.get_config();
    if let Some(server_config) = config.mcp_servers.get(&server_id) {
        if !server_config.enabled {
            return Err("Server is disabled".to_string());
        }
        
        // For now, just enable the server in config (actual process management would be more complex)
        info!("MCP server {} is configured to start with command: {} {:?}", 
              server_id, server_config.command, server_config.args);
        
        // In a full implementation, this would:
        // 1. Spawn the process with the command and args
        // 2. Set up stdio/websocket transport based on transport_type
        // 3. Initialize the MCP protocol connection
        // 4. Store the running process handle for management
        
        Ok(())
    } else {
        Err(format!("MCP server {} not found", server_id))
    }
}

#[tauri::command]
async fn stop_mcp_server(
    server_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Stopping MCP server {}", server_id);
    
    let config = app_state.get_config();
    if let Some(_server_config) = config.mcp_servers.get(&server_id) {
        // In a full implementation, this would:
        // 1. Send graceful shutdown signal to the process
        // 2. Wait for process to terminate with timeout
        // 3. Force kill if graceful shutdown fails
        // 4. Clean up transport connections
        // 5. Remove process handle from management
        
        info!("MCP server {} stop requested", server_id);
        Ok(())
    } else {
        Err(format!("MCP server {} not found", server_id))
    }
}

// Billing and usage commands
#[derive(serde::Serialize)]
struct UsageRecordResponse {
    id: Option<i64>,
    timestamp: i64,
    provider: String,
    model: String,
    input_tokens: i32,
    output_tokens: i32,
    cost: String,
    conversation_id: Option<String>,
    request_id: String,
}

#[derive(serde::Serialize)]
struct UsageSummaryResponse {
    daily_cost: String,
    monthly_cost: String,
    daily_tokens: i32,
    monthly_tokens: i32,
    top_models: Vec<TopModelUsage>,
    cost_trend: Vec<CostTrendData>,
}

#[derive(serde::Serialize)]
struct TopModelUsage {
    model: String,
    provider: String,
    cost: String,
    tokens: i32,
    percentage: f64,
}

#[derive(serde::Serialize)]
struct CostTrendData {
    date: String,
    cost: String,
}

#[tauri::command]
async fn get_usage_records(
    limit: Option<i32>,
    offset: Option<i32>,
    app_state: State<'_, AppState>
) -> CommandResult<Vec<UsageRecordResponse>> {
    info!("Getting usage records (limit: {:?}, offset: {:?})", limit, offset);
    
    let limit = limit.unwrap_or(100) as usize;
    let offset = offset.unwrap_or(0) as usize;
    
    match app_state.get_usage_repo().get_usage_records(None, None, None, Some(limit as i32), Some(offset as i32)).await {
        Ok(records) => {
            let usage_responses: Vec<UsageRecordResponse> = records.into_iter().map(|record| {
                UsageRecordResponse {
                    id: Some(record.id),
                    timestamp: record.timestamp.timestamp_millis(),
                    provider: record.provider,
                    model: record.model,
                    input_tokens: record.input_tokens as i32,
                    output_tokens: record.output_tokens as i32,
                    cost: record.cost.to_string(),
                    conversation_id: record.conversation_id,
                    request_id: record.request_id,
                }
            }).collect();
            Ok(usage_responses)
        }
        Err(e) => {
            eprintln!("Failed to retrieve usage records: {}", e);
            Ok(vec![])
        }
    }
}

#[tauri::command]
async fn get_usage_summary(app_state: State<'_, AppState>) -> CommandResult<UsageSummaryResponse> {
    info!("Getting usage summary");
    
    match app_state.get_usage_repo().get_usage_statistics().await {
        Ok(stats) => {
            // Convert model usage to top models
            let mut top_models: Vec<TopModelUsage> = stats.by_model.into_iter().map(|(model, usage)| {
                let total_cost = usage.cost;
                let total_tokens = usage.input_tokens + usage.output_tokens;
                let percentage = if stats.total_cost > rust_decimal::Decimal::ZERO {
                    (total_cost / stats.total_cost * rust_decimal::Decimal::from(100)).to_f64().unwrap_or(0.0)
                } else {
                    0.0
                };
                
                TopModelUsage {
                    model: model.clone(),
                    provider: {
                        // Extract provider from model name (e.g., "openai:gpt-4" -> "openai")
                        if model.contains(':') {
                            model.split(':').next().unwrap_or("unknown").to_string()
                        } else {
                            // Fallback: try to infer from model name patterns
                            if model.starts_with("gpt-") || model.starts_with("text-") {
                                "openai".to_string()
                            } else if model.starts_with("claude-") {
                                "anthropic".to_string()
                            } else if model.starts_with("gemini-") {
                                "gemini".to_string()
                            } else {
                                "unknown".to_string()
                            }
                        }
                    },
                    cost: total_cost.to_string(),
                    tokens: total_tokens as i32,
                    percentage,
                }
            }).collect();
            
            // Sort by cost and take top models
            top_models.sort_by(|a, b| b.cost.parse::<f64>().unwrap_or(0.0).partial_cmp(&a.cost.parse::<f64>().unwrap_or(0.0)).unwrap());
            top_models.truncate(5);

            // Get daily statistics
            let (daily_cost, daily_tokens) = match app_state.get_usage_repo().get_daily_statistics().await {
                Ok((cost, tokens)) => (cost.to_string(), tokens as i32),
                Err(e) => {
                    eprintln!("Failed to get daily statistics: {}", e);
                    ("0.00".to_string(), 0)
                }
            };

            // Get cost trend for the last 30 days
            let cost_trend = match app_state.get_usage_repo().get_cost_trend(30).await {
                Ok(trend_data) => {
                    trend_data.into_iter().map(|(date, cost)| {
                        CostTrendData {
                            date,
                            cost: cost.to_string(),
                        }
                    }).collect()
                }
                Err(e) => {
                    eprintln!("Failed to get cost trend: {}", e);
                    vec![]
                }
            };

            Ok(UsageSummaryResponse {
                daily_cost,
                monthly_cost: stats.current_month_cost.to_string(),
                daily_tokens,
                monthly_tokens: (stats.total_input_tokens + stats.total_output_tokens) as i32,
                top_models,
                cost_trend,
            })
        }
        Err(e) => {
            eprintln!("Failed to retrieve usage summary: {}", e);
            Ok(UsageSummaryResponse {
                daily_cost: "0.00".to_string(),
                monthly_cost: "0.00".to_string(),
                daily_tokens: 0,
                monthly_tokens: 0,
                top_models: vec![],
                cost_trend: vec![],
            })
        }
    }
}

#[derive(serde::Deserialize)]
struct UpdateBillingLimitsRequest {
    daily_limit: Option<String>,
    monthly_limit: Option<String>,
    per_model_limits: Option<std::collections::HashMap<String, String>>,
}

#[tauri::command]
async fn update_billing_limits(
    request: UpdateBillingLimitsRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating billing limits");
    
    match app_state.update_config(|config| {
        if let Some(daily_limit) = request.daily_limit {
            config.billing.daily_limit_usd = daily_limit.parse().ok();
        }
        if let Some(monthly_limit) = request.monthly_limit {
            config.billing.monthly_limit_usd = monthly_limit.parse().ok();
        }
        if let Some(per_model_limits) = request.per_model_limits {
            for (model, limit_str) in per_model_limits {
                if let Ok(limit) = limit_str.parse::<f64>() {
                    config.billing.per_model_limits.insert(model, limit);
                }
            }
        }
    }).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Failed to update billing limits: {}", e);
            Err(format!("Failed to update billing limits: {}", e))
        }
    }
}

#[tauri::command]
async fn export_usage_data(
    format: String,
    period: Option<String>,
    app_state: State<'_, AppState>
) -> CommandResult<String> {
    info!("Exporting usage data as {} for period {:?}", format, period);
    
    // Get usage records for export
    match app_state.get_usage_repo().get_usage_records(None, None, None, Some(10000), Some(0)).await {
        Ok(records) => {
            match format.to_lowercase().as_str() {
                "csv" => {
                    let mut csv_content = String::from("timestamp,provider,model,input_tokens,output_tokens,cost,conversation_id,request_id\n");
                    for record in records {
                        csv_content.push_str(&format!(
                            "{},{},{},{},{},{},{},{}\n",
                            record.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            record.provider,
                            record.model,
                            record.input_tokens,
                            record.output_tokens,
                            record.cost,
                            record.conversation_id.unwrap_or_else(|| "".to_string()),
                            record.request_id
                        ));
                    }
                    Ok(csv_content)
                }
                "json" => {
                    match serde_json::to_string_pretty(&records) {
                        Ok(json_content) => Ok(json_content),
                        Err(e) => {
                            eprintln!("Failed to serialize records to JSON: {}", e);
                            Err(format!("Failed to export as JSON: {}", e))
                        }
                    }
                }
                _ => {
                    Err(format!("Unsupported export format: {}. Supported formats: csv, json", format))
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to retrieve usage records for export: {}", e);
            Err(format!("Failed to retrieve usage data: {}", e))
        }
    }
}

async fn init_app_state() -> AppResult<AppState> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("valechat=debug".parse().unwrap()),
        )
        .init();

    info!("Starting ValeChat application");

    // Initialize paths and create directories
    let paths = AppPaths::new()?;
    paths.ensure_dirs_exist()?;
    info!("Application directories created successfully");

    // Initialize secure storage
    let secure_storage = SecureStorageManager::new()?;
    info!("Secure storage initialized successfully");

    // Load configuration
    let config = AppConfig::load(&paths).await?;
    info!("Configuration loaded successfully");

    // Initialize application state
    let state = AppState::new(config, paths, secure_storage).await?;
    info!("Application state initialized successfully");

    Ok(state)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Initialize app state synchronously to avoid race conditions
            let app_handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                match init_app_state().await {
                    Ok(state) => {
                        app_handle.manage(state);
                        info!("App state initialized and managed by Tauri");
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize app state: {}", e);
                        return Err(format!("Failed to initialize app state: {}", e).into());
                    }
                }
                Ok(())
            })
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            // Chat commands
            get_conversations,
            test_create_conversation,
            create_conversation_simple,
            create_conversation,
            send_message,
            get_conversation_messages,
            delete_conversation,
            update_conversation_title,
            // Configuration commands
            get_app_config,
            update_app_config,
            update_model_provider,
            set_api_key,
            get_api_key,
            remove_api_key,
            // MCP server commands
            get_mcp_servers,
            add_mcp_server,
            update_mcp_server,
            remove_mcp_server,
            start_mcp_server,
            stop_mcp_server,
            // Billing commands
            get_usage_records,
            get_usage_summary,
            update_billing_limits,
            export_usage_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
