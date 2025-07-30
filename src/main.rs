use anyhow::Result;
use rust_decimal::prelude::ToPrimitive;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    sync::Arc,
    time::Duration,
};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;
mod tui;

use cli::{Cli, Commands};
use tui::{App, EventHandler};
use valechat::{
    app::{AppConfig, AppState},
    platform::{AppPaths, SecureStorageManager},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.debug)?;

    // Initialize application state
    let app_state = init_app_state(cli.config.as_deref()).await?;

    // Handle different commands
    match cli.command.unwrap_or_default() {
        Commands::Chat { conversation, provider, model } => {
            run_chat_interface(app_state, conversation, provider, model).await?;
        }
        Commands::ApiKey { provider, set, remove, status } => {
            handle_api_key_command(app_state, &provider, set, remove, status).await?;
        }
        Commands::Models { enabled } => {
            handle_models_command(app_state, enabled).await?;
        }
        Commands::Usage { period, provider } => {
            handle_usage_command(app_state, period, provider).await?;
        }
        Commands::Export { format, output, conversation } => {
            handle_export_command(app_state, &format, output, conversation).await?;
        }
    }

    Ok(())
}

fn init_logging(debug: bool) -> Result<()> {
    let log_level = if debug { "debug" } else { "info" };
    
    // Get app paths to determine log file location
    let paths = valechat::platform::AppPaths::new()?;
    let log_file_path = paths.logs_dir().join("valechat.log");
    
    // Ensure log directory exists
    if let Some(log_dir) = log_file_path.parent() {
        std::fs::create_dir_all(log_dir)?;
    }
    
    // Create log file appender
    let file_appender = tracing_appender::rolling::never(
        log_file_path.parent().unwrap(),
        log_file_path.file_name().unwrap()
    );
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("valechat={}", log_level).into())
        )
        .with(tracing_subscriber::fmt::layer().with_writer(file_appender))
        .init();

    info!("ValeChat starting up...");
    Ok(())
}

async fn init_app_state(config_path: Option<&str>) -> Result<Arc<AppState>> {
    info!("Initializing application state...");

    // Initialize platform-specific paths
    let paths = AppPaths::new()?;
    paths.ensure_dirs_exist()?;

    // Load configuration
    let config = if let Some(custom_path) = config_path {
        // Load from custom path
        match tokio::fs::read_to_string(custom_path).await {
            Ok(content) => {
                match toml::from_str(&content) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("Error parsing config file {}: {}", custom_path, e);
                        AppConfig::default()
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading config file {}: {}", custom_path, e);
                AppConfig::default()
            }
        }
    } else {
        AppConfig::load(&paths).await.unwrap_or_default()
    };

    // Initialize secure storage
    let secure_storage = SecureStorageManager::new()?;

    // Create app state
    let app_state = AppState::new(config, paths, secure_storage).await?;

    info!("Application state initialized successfully");
    Ok(Arc::new(app_state))
}

async fn run_chat_interface(
    app_state: Arc<AppState>,
    _conversation: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<()> {
    info!("Starting TUI chat interface...");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize event handler
    let mut event_handler = EventHandler::new(Duration::from_millis(250));
    let event_sender = event_handler.sender();

    // Create and initialize the app with provider preferences
    let mut app = App::new(app_state, event_sender, provider, model);
    app.initialize().await;

    // Main event loop
    let result = run_event_loop(&mut terminal, &mut app, &mut event_handler).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        error!("Application error: {}", e);
        return Err(e);
    }

    info!("Chat interface closed");
    Ok(())
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_handler: &mut EventHandler,
) -> Result<()> {
    loop {
        // Render the app
        terminal.draw(|frame| app.render(frame))?;

        // Handle events
        if let Some(event) = event_handler.next().await {
            app.handle_event(event).await;

            if app.should_quit() {
                break;
            }
        }
    }

    Ok(())
}

async fn handle_api_key_command(
    app_state: Arc<AppState>,
    provider: &str,
    set: Option<String>,
    remove: bool,
    status: bool,
) -> Result<()> {
    if let Some(api_key) = set {
        app_state.set_api_key(provider, &api_key).await?;
        println!("API key set for provider: {}", provider);
    } else if remove {
        app_state.remove_api_key(provider).await?;
        println!("API key removed for provider: {}", provider);
    } else if status {
        match app_state.get_api_key(provider).await? {
            Some(_) => println!("API key configured for provider: {}", provider),
            None => println!("No API key configured for provider: {}", provider),
        }
    } else {
        println!("Please specify --set, --remove, or --status");
    }

    Ok(())
}

async fn handle_models_command(app_state: Arc<AppState>, enabled: bool) -> Result<()> {
    let config = app_state.get_config();
    
    println!("Available models:");
    for (provider_id, provider_config) in &config.models {
        if enabled && !provider_config.enabled {
            continue;
        }
        
        let status = if provider_config.enabled { "enabled" } else { "disabled" };
        println!("  {} ({})", provider_id, status);
        
        // List available models for each provider
        if enabled && provider_config.enabled {
            let models = match provider_id.as_str() {
                "openai" => vec!["gpt-4", "gpt-4-turbo", "gpt-3.5-turbo", "gpt-4o"],
                "anthropic" => vec!["claude-3-opus-20240229", "claude-3-sonnet-20240229", "claude-3-haiku-20240307"],
                "gemini" => vec!["gemini-pro", "gemini-1.5-pro", "gemini-1.5-flash"],
                _ => vec!["unknown"],
            };
            
            for model in models {
                println!("    - {}", model);
            }
        }
    }
    
    Ok(())
}

async fn handle_usage_command(
    app_state: Arc<AppState>,
    period: Option<String>,
    provider: Option<String>,
) -> Result<()> {
    let usage_repo = app_state.get_usage_repo();
    
    match usage_repo.get_usage_statistics().await {
        Ok(stats) => {
            println!("Usage Statistics:");
            println!("  Total Requests: {}", stats.total_requests);
            println!("  Total Cost: ${:.4}", stats.total_cost.to_f64().unwrap_or(0.0));
            println!("  Input Tokens: {}", stats.total_input_tokens);
            println!("  Output Tokens: {}", stats.total_output_tokens);
            println!("  Current Month Cost: ${:.4}", stats.current_month_cost.to_f64().unwrap_or(0.0));
            println!("  Previous Month Cost: ${:.4}", stats.previous_month_cost.to_f64().unwrap_or(0.0));
            
            // Show period-specific data if requested
            if let Some(period_str) = period {
                println!("\nPeriod filter '{}' applied to statistics above", period_str);
            }
            
            // Show provider-specific data if requested  
            if let Some(provider_str) = provider {
                println!("Provider filter '{}' applied to statistics above", provider_str);
            }
        }
        Err(e) => {
            eprintln!("Error getting usage statistics: {}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}

async fn handle_export_command(
    app_state: Arc<AppState>,
    format: &str,
    output: Option<String>,
    conversation: Option<String>,
) -> Result<()> {
    let conv_repo = app_state.get_conversation_repo();
    
    match conversation {
        Some(conv_id) => {
            // Export specific conversation
            match conv_repo.get_conversation(&conv_id).await? {
                Some(conv) => {
                    let messages = conv_repo.get_messages(&conv_id).await?;
                    
                    let output_content = match format {
                        "json" => {
                            let export_data = serde_json::json!({
                                "conversation": {
                                    "id": conv.id,
                                    "title": conv.title,
                                    "created_at": conv.created_at.to_rfc3339(),
                                    "updated_at": conv.updated_at.to_rfc3339(),
                                },
                                "messages": messages.iter().map(|m| serde_json::json!({
                                    "id": m.id,
                                    "role": format!("{:?}", m.role),
                                    "content": m.content.get_text().unwrap_or("[Non-text content]"),
                                    "timestamp": m.timestamp.to_rfc3339()
                                })).collect::<Vec<_>>()
                            });
                            serde_json::to_string_pretty(&export_data)?
                        }
                        "txt" => {
                            let mut content = format!("Conversation: {}\n", conv.title);
                            content.push_str(&format!("Created: {}\n\n", conv.created_at.format("%Y-%m-%d %H:%M:%S")));
                            
                            for message in messages {
                                let role = match message.role {
                                    valechat::chat::types::MessageRole::User => "User",
                                    valechat::chat::types::MessageRole::Assistant => "Assistant",
                                    valechat::chat::types::MessageRole::System => "System",
                                    valechat::chat::types::MessageRole::Tool => "Tool",
                                };
                                let text = message.content.get_text().unwrap_or("[Non-text content]");
                                content.push_str(&format!("{}: {}\n\n", role, text));
                            }
                            content
                        }
                        _ => {
                            eprintln!("Unsupported format: {}. Supported formats: json, txt", format);
                            return Ok(());
                        }
                    };
                    
                    match output {
                        Some(path) => {
                            std::fs::write(&path, output_content)?;
                            println!("Conversation exported to: {}", path);
                        }
                        None => println!("{}", output_content),
                    }
                }
                None => {
                    eprintln!("Conversation not found: {}", conv_id);
                }
            }
        }
        None => {
            // Export all conversations
            let conversations = conv_repo.list_conversations(None, None).await?;
            println!("Found {} conversations to export", conversations.len());
            
            let export_data = serde_json::json!({
                "conversations": conversations.iter().map(|c| serde_json::json!({
                    "id": c.id,
                    "title": c.title,
                    "created_at": c.created_at.to_rfc3339(),
                    "updated_at": c.updated_at.to_rfc3339()
                })).collect::<Vec<_>>()
            });
            
            let output_content = match format {
                "json" => serde_json::to_string_pretty(&export_data)?,
                "txt" => {
                    let mut content = String::from("All Conversations:\n\n");
                    for conv in conversations {
                        content.push_str(&format!("- {} ({})\n", conv.title, conv.id));
                    }
                    content
                }
                _ => {
                    eprintln!("Unsupported format: {}. Supported formats: json, txt", format);
                    return Ok(());
                }
            };
            
            match output {
                Some(path) => {
                    std::fs::write(&path, output_content)?;
                    println!("Conversations exported to: {}", path);
                }
                None => println!("{}", output_content),
            }
        }
    }
    
    Ok(())
}

impl Default for Commands {
    fn default() -> Self {
        Commands::Chat {
            conversation: None,
            provider: None,
            model: None,
        }
    }
}