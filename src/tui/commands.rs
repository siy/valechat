use std::sync::Arc;
use tracing::debug;
use rust_decimal::prelude::ToPrimitive;
use tokio::sync::mpsc;

use valechat::app::AppState;
use crate::tui::Event;

#[derive(Debug, Clone)]
pub enum SlashCommand {
    ApiKey {
        provider: String,
        action: ApiKeyAction,
    },
    Usage {
        period: Option<String>,
        provider: Option<String>,
    },
    Export {
        format: String,
        conversation: Option<String>,
    },
    Provider {
        action: ProviderAction,
    },
    Model {
        action: ModelAction,
    },
    Help,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub enum ProviderAction {
    Show,
    Set(String),
    List,
}

#[derive(Debug, Clone)]
pub enum ModelAction {
    Show,
    Set(String),
    List,
}

#[derive(Debug, Clone)]
pub enum ApiKeyAction {
    Status,
    Set(String),
    Remove,
}

pub struct CommandParser;

impl CommandParser {
    pub fn parse(input: &str) -> Option<SlashCommand> {
        if !input.starts_with('/') {
            return None;
        }

        let input = &input[1..]; // Remove the leading '/'
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.is_empty() {
            return Some(SlashCommand::Help);
        }

        let command = parts[0].to_lowercase();
        let args = &parts[1..];

        match command.as_str() {
            "apikey" => parse_apikey_command(args),
            "usage" => parse_usage_command(args),
            "export" => parse_export_command(args),
            "provider" => parse_provider_command(args),
            "model" => parse_model_command(args),
            "help" => Some(SlashCommand::Help),
            _ => Some(SlashCommand::Unknown(parts[0].to_string())), // Use original case for error message
        }
    }
}

fn parse_apikey_command(args: &[&str]) -> Option<SlashCommand> {
    if args.is_empty() {
        return Some(SlashCommand::Unknown("apikey".to_string()));
    }

    let provider = args[0].to_string();
    
    if args.len() == 1 {
        // Just provider name - show status
        return Some(SlashCommand::ApiKey {
            provider,
            action: ApiKeyAction::Status,
        });
    }

    match args[1].to_lowercase().as_str() {
        "--status" | "status" => Some(SlashCommand::ApiKey {
            provider,
            action: ApiKeyAction::Status,
        }),
        "--set" | "set" => {
            if args.len() >= 3 {
                Some(SlashCommand::ApiKey {
                    provider,
                    action: ApiKeyAction::Set(args[2].to_string()),
                })
            } else {
                Some(SlashCommand::Unknown("apikey set requires key".to_string()))
            }
        }
        "--remove" | "remove" => Some(SlashCommand::ApiKey {
            provider,
            action: ApiKeyAction::Remove,
        }),
        _ => Some(SlashCommand::Unknown("apikey".to_string())),
    }
}


fn parse_usage_command(args: &[&str]) -> Option<SlashCommand> {
    let mut period = None;
    let mut provider = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].to_lowercase().as_str() {
            "--period" | "-p" | "period" => {
                if i + 1 < args.len() {
                    period = Some(args[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--provider" | "provider" => {
                if i + 1 < args.len() {
                    provider = Some(args[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    Some(SlashCommand::Usage { period, provider })
}

fn parse_export_command(args: &[&str]) -> Option<SlashCommand> {
    let mut format = "markdown".to_string();
    let mut conversation = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].to_lowercase().as_str() {
            "--format" | "-f" | "format" => {
                if i + 1 < args.len() {
                    format = args[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--conversation" | "-c" | "conversation" => {
                if i + 1 < args.len() {
                    conversation = Some(args[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    Some(SlashCommand::Export { format, conversation })
}

fn parse_provider_command(args: &[&str]) -> Option<SlashCommand> {
    if args.is_empty() {
        // No args - show current provider
        return Some(SlashCommand::Provider {
            action: ProviderAction::Show,
        });
    }

    match args[0].to_lowercase().as_str() {
        "list" | "all" => Some(SlashCommand::Provider {
            action: ProviderAction::List,
        }),
        _ => {
            // First arg is the provider name to set
            Some(SlashCommand::Provider {
                action: ProviderAction::Set(args[0].to_string()),
            })
        }
    }
}

fn parse_model_command(args: &[&str]) -> Option<SlashCommand> {
    if args.is_empty() {
        // No args - show current model
        return Some(SlashCommand::Model {
            action: ModelAction::Show,
        });
    }

    match args[0].to_lowercase().as_str() {
        "list" | "all" => Some(SlashCommand::Model {
            action: ModelAction::List,
        }),
        _ => {
            // First arg is the model name to set
            Some(SlashCommand::Model {
                action: ModelAction::Set(args[0].to_string()),
            })
        }
    }
}

pub struct CommandExecutor {
    app_state: Arc<AppState>,
    event_sender: mpsc::UnboundedSender<Event>,
}

impl CommandExecutor {
    pub fn new(app_state: Arc<AppState>, event_sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { 
            app_state, 
            event_sender,
        }
    }

    pub async fn execute_with_context(
        &self, 
        command: SlashCommand,
        current_provider: Option<&String>,
        current_model: Option<&String>,
    ) -> String {
        debug!("Executing slash command: {:?}", command);

        match command {
            SlashCommand::Provider { action } => {
                self.execute_provider_command(action, current_provider).await
            }
            SlashCommand::Model { action } => {
                self.execute_model_command(action, current_model).await
            }
            _ => self.execute(command).await,
        }
    }

    pub async fn execute(&self, command: SlashCommand) -> String {
        debug!("Executing slash command: {:?}", command);

        match command {
            SlashCommand::ApiKey { provider, action } => {
                self.execute_apikey_command(provider, action).await
            }
            SlashCommand::Usage { period, provider } => {
                self.execute_usage_command(period, provider).await
            }
            SlashCommand::Export { format, conversation } => {
                self.execute_export_command(format, conversation).await
            }
            SlashCommand::Provider { action: _ } => {
                "Use /provider command from chat input for provider switching.".to_string()
            }
            SlashCommand::Model { action: _ } => {
                "Use /model command from chat input for model switching.".to_string()
            }
            SlashCommand::Help => self.show_help(),
            SlashCommand::Unknown(cmd) => {
                format!("Unknown command: /{}\n\nType /help for available commands.", cmd)
            }
        }
    }

    async fn execute_apikey_command(&self, provider: String, action: ApiKeyAction) -> String {
        match action {
            ApiKeyAction::Status => {
                match self.app_state.get_api_key(&provider).await {
                    Ok(Some(key)) => {
                        let preview = if key.len() > 10 {
                            format!("{}...{}", &key[..4], &key[key.len()-4..])
                        } else {
                            "*".repeat(key.len())
                        };
                        format!("✅ API key configured for provider: {} ({})", provider, preview)
                    }
                    Ok(None) => format!("❌ No API key configured for provider: {}", provider),
                    Err(e) => format!("❌ Error checking API key: {}", e),
                }
            }
            ApiKeyAction::Set(key) => {
                match self.app_state.set_api_key(&provider, &key).await {
                    Ok(()) => {
                        format!("✅ API key set for provider: {} (stored in keychain)", provider)
                    }
                    Err(e) => format!("❌ Error setting API key: {}", e),
                }
            }
            ApiKeyAction::Remove => {
                match self.app_state.remove_api_key(&provider).await {
                    Ok(()) => format!("✅ API key removed for provider: {}", provider),
                    Err(e) => format!("❌ Error removing API key: {}", e),
                }
            }
        }
    }


    async fn execute_provider_command(&self, action: ProviderAction, current_provider: Option<&String>) -> String {
        match action {
            ProviderAction::Show => {
                match current_provider {
                    Some(provider) => format!("🔧 **Current Provider**: {}", provider),
                    None => {
                        // Get the default provider from configuration
                        match self.app_state.get_default_provider_and_model() {
                            Ok((default_provider, _)) => {
                                format!("🔧 **Current Provider**: Not set (using default: {})", default_provider)
                            }
                            Err(_) => "🔧 **Current Provider**: Not set (no enabled providers found)".to_string()
                        }
                    }
                }
            }
            ProviderAction::Set(provider) => {
                // Check if provider is valid and enabled
                let config = self.app_state.get_config();
                match config.models.get(&provider) {
                    Some(provider_config) if provider_config.enabled => {
                        // Send event to update the provider
                        let _ = self.event_sender.send(Event::SetProvider(provider.clone()));
                        format!("✅ **Provider switched to**: {}", provider)
                    }
                    Some(_) => {
                        format!("❌ Provider '{}' is not enabled. Enable it in configuration first.", provider)
                    }
                    None => {
                        let available: Vec<String> = config.models.keys().cloned().collect();
                        format!("❌ Provider '{}' not found.\n\n**Available providers**: {}", 
                               provider, available.join(", "))
                    }
                }
            }
            ProviderAction::List => {
                let config = self.app_state.get_config();
                let mut output = String::from("🔧 **Available Providers**\n\n");

                for (provider_id, provider_config) in &config.models {
                    let status = if provider_config.enabled { "✅ enabled" } else { "❌ disabled" };
                    output.push_str(&format!("**{}** ({})\n", provider_id, status));
                }

                output
            }
        }
    }

    async fn execute_model_command(&self, action: ModelAction, current_model: Option<&String>) -> String {
        match action {
            ModelAction::Show => {
                match current_model {
                    Some(model) => format!("🤖 **Current Model**: {}", model),
                    None => {
                        // Get the default model from configuration
                        match self.app_state.get_default_provider_and_model() {
                            Ok((_, default_model)) => {
                                format!("🤖 **Current Model**: Not set (using default: {})", default_model)
                            }
                            Err(_) => "🤖 **Current Model**: Not set (no enabled providers found)".to_string()
                        }
                    }
                }
            }
            ModelAction::Set(model) => {
                // For now, just set the model - in a full implementation you'd validate it
                // against the current provider's available models
                let _ = self.event_sender.send(Event::SetModel(model.clone()));
                format!("✅ **Model switched to**: {}", model)
            }
            ModelAction::List => {
                let config = self.app_state.get_config();
                let mut output = String::from("🤖 **Available Models**\n\n");

                for (provider_id, provider_config) in &config.models {
                    let status = if provider_config.enabled { "✅ enabled" } else { "❌ disabled" };
                    output.push_str(&format!("**{}** ({})\n", provider_id, status));

                    if provider_config.enabled {
                        let models = match provider_id.as_str() {
                            "openai" => vec!["gpt-4", "gpt-4-turbo", "gpt-3.5-turbo", "gpt-4o"],
                            "anthropic" => vec!["claude-3-opus-20240229", "claude-3-sonnet-20240229", "claude-3-haiku-20240307"],
                            "gemini" => vec!["gemini-pro", "gemini-1.5-pro", "gemini-1.5-flash"],
                            _ => vec!["unknown"],
                        };

                        for model in models {
                            output.push_str(&format!("  • {}\n", model));
                        }
                    }
                    output.push('\n');
                }

                output
            }
        }
    }

    async fn execute_usage_command(&self, _period: Option<String>, _provider: Option<String>) -> String {
        match self.app_state.get_usage_repo().get_usage_statistics().await {
            Ok(stats) => {
                format!(
                    "📊 **Usage Statistics**\n\n\
                    Total Requests: {}\n\
                    Total Cost: ${:.4}\n\
                    Input Tokens: {}\n\
                    Output Tokens: {}\n\
                    Current Month: ${:.4}\n\
                    Previous Month: ${:.4}",
                    stats.total_requests,
                    stats.total_cost.to_f64().unwrap_or(0.0),
                    stats.total_input_tokens,
                    stats.total_output_tokens,
                    stats.current_month_cost.to_f64().unwrap_or(0.0),
                    stats.previous_month_cost.to_f64().unwrap_or(0.0)
                )
            }
            Err(e) => format!("❌ Error getting usage statistics: {}", e),
        }
    }

    async fn execute_export_command(&self, format: String, conversation: Option<String>) -> String {
        match conversation {
            Some(conv_id) => {
                match self.app_state.get_conversation_repo().get_conversation(&conv_id).await {
                    Ok(Some(conv)) => {
                        format!("✅ Exporting conversation '{}' in {} format\n\n(Export functionality would generate file here)", conv.title, format)
                    }
                    Ok(None) => format!("❌ Conversation not found: {}", conv_id),
                    Err(e) => format!("❌ Error loading conversation: {}", e),
                }
            }
            None => {
                match self.app_state.get_conversation_repo().list_conversations(None, None).await {
                    Ok(conversations) => {
                        format!("✅ Found {} conversations to export in {} format\n\n(Export functionality would generate file here)", conversations.len(), format)
                    }
                    Err(e) => format!("❌ Error listing conversations: {}", e),
                }
            }
        }
    }

    fn show_help(&self) -> String {
        r#"🔧 **Available Slash Commands** (case-insensitive)

**Provider & Model Control:**
• `/provider` - Show current provider
• `/provider list` - List all available providers
• `/provider <name>` - Switch to provider (openai, anthropic, gemini)
• `/model` - Show current model
• `/model list` - List all available models
• `/model <name>` - Switch to model (gpt-4, claude-3-sonnet, etc.)

**API Key Management:** (matches CLI `apikey` command)
• `/apikey <provider>` - Show API key status
• `/apikey <provider> set <key>` - Set API key
• `/apikey <provider> remove` - Remove API key

**Usage & Billing:** (matches CLI `usage` command)
• `/usage` - Show usage statistics
• `/usage period <today|week|month>` - Usage for period
• `/usage provider <name>` - Usage for specific provider

**Export:** (matches CLI `export` command)
• `/export` - Export all conversations
• `/export format json` - Export in format (json, markdown, txt)
• `/export conversation <id>` - Export specific conversation

**Other:**
• `/help` - Show this help message

**Examples:**
• `/PROVIDER gemini` - Switch to Gemini (case insensitive)
• `/Provider LIST` - List all providers (case insensitive)
• `/Model GPT-4` - Switch to GPT-4 (case insensitive)
• `/MODEL list` - List all models (case insensitive)
• `/APIKEY openai` - Check OpenAI API key status"#.to_string()
    }
}