// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, AppHandle, State};
use tracing::info;
use std::sync::Arc;
use uuid::Uuid;
use chrono;
use valechat::{app::AppState, error::Result as AppResult};
use valechat::platform::{AppPaths, SecureStorageManager};
use valechat::app::AppConfig;

// Tauri command result type
type CommandResult<T> = Result<T, String>;

// Convert internal Result to Tauri command result
fn convert_result<T>(result: AppResult<T>) -> CommandResult<T> {
    result.map_err(|e| e.to_string())
}

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
    stream: bool,
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
    // TODO: Implement conversation retrieval from database
    // For now, return empty list
    Ok(vec![])
}

#[tauri::command]
async fn create_conversation(
    request: CreateConversationRequest,
    app_state: State<'_, AppState>
) -> CommandResult<ConversationResponse> {
    info!("Creating conversation: {:?}", request.title);
    
    let conversation_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    
    // TODO: Save to database
    Ok(ConversationResponse {
        id: conversation_id,
        title: request.title.unwrap_or_else(|| "New Conversation".to_string()),
        created_at: now,
        updated_at: now,
        model_provider: None,
        total_cost: None,
        message_count: 0,
    })
}

#[tauri::command]
async fn send_message(
    request: SendMessageRequest,
    app_state: State<'_, AppState>
) -> CommandResult<MessageResponse> {
    info!("Sending message to conversation {}", request.conversation_id);
    
    let message_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    
    // TODO: Implement actual message sending through chat service
    // For now, return a mock response
    Ok(MessageResponse {
        id: message_id,
        role: "assistant".to_string(),
        content: format!("Mock response to: {}", request.content),
        timestamp: now,
        model_used: Some(request.model),
        provider: Some(request.provider),
        input_tokens: Some(50),
        output_tokens: Some(100),
        cost: Some("0.005".to_string()),
        processing_time_ms: Some(1500),
    })
}

#[tauri::command]
async fn get_conversation_messages(
    conversation_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<Vec<MessageResponse>> {
    info!("Getting messages for conversation {}", conversation_id);
    // TODO: Implement message retrieval from database
    Ok(vec![])
}

#[tauri::command]
async fn delete_conversation(
    conversation_id: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Deleting conversation {}", conversation_id);
    // TODO: Implement conversation deletion
    Ok(())
}

#[tauri::command]
async fn update_conversation_title(
    conversation_id: String,
    title: String,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating conversation {} title to {}", conversation_id, title);
    // TODO: Implement conversation title update
    Ok(())
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
    
    // TODO: Load actual configuration from AppState
    // For now, return mock configuration
    Ok(ConfigResponse {
        theme: "system".to_string(),
        language: "en".to_string(),
        model_providers: vec![
            ModelProviderResponse {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                provider_type: "openai".to_string(),
                enabled: false,
                models: vec![
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
                ],
            },
        ],
        mcp_servers: vec![],
        billing_limits: BillingLimitsResponse {
            daily_limit: None,
            monthly_limit: None,
            per_model_limits: std::collections::HashMap::new(),
            per_conversation_limits: std::collections::HashMap::new(),
        },
        auto_save: true,
        streaming: true,
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
    // TODO: Update actual configuration
    Ok(())
}

#[derive(serde::Deserialize)]
struct UpdateModelProviderRequest {
    id: String,
    enabled: Option<bool>,
    config: Option<std::collections::HashMap<String, String>>,
}

#[tauri::command]
async fn update_model_provider(
    request: UpdateModelProviderRequest,
    app_state: State<'_, AppState>
) -> CommandResult<()> {
    info!("Updating model provider {}", request.id);
    // TODO: Update model provider configuration
    Ok(())
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
    // TODO: Implement actual usage record retrieval
    Ok(vec![])
}

#[tauri::command]
async fn get_usage_summary(app_state: State<'_, AppState>) -> CommandResult<UsageSummaryResponse> {
    info!("Getting usage summary");
    // TODO: Implement actual usage summary calculation
    Ok(UsageSummaryResponse {
        daily_cost: "0.00".to_string(),
        monthly_cost: "0.00".to_string(),
        daily_tokens: 0,
        monthly_tokens: 0,
        top_models: vec![],
        cost_trend: vec![],
    })
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
    // TODO: Update billing limits
    Ok(())
}

#[tauri::command]
async fn export_usage_data(
    format: String,
    period: Option<String>,
    app_state: State<'_, AppState>
) -> CommandResult<String> {
    info!("Exporting usage data as {} for period {:?}", format, period);
    // TODO: Implement usage data export
    Ok("Export completed".to_string())
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
            // Initialize app state in background
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match init_app_state().await {
                    Ok(state) => {
                        app_handle.manage(state);
                        info!("App state initialized and managed by Tauri");
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize app state: {}", e);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            // Chat commands
            get_conversations,
            create_conversation,
            send_message,
            get_conversation_messages,
            delete_conversation,
            update_conversation_title,
            // Configuration commands
            get_app_config,
            update_app_config,
            update_model_provider,
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
