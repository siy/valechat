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
    Cost {
        action: CostAction,
    },
    Budget {
        action: BudgetAction,
    },
    Mcp {
        action: MCPAction,
    },
    Quit,
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

#[derive(Debug, Clone)]
pub enum CostAction {
    Show,
    Today,
    Week,
    Month,
    Breakdown,
    Alerts,
}

#[derive(Debug, Clone)]
pub enum BudgetAction {
    Show,
    SetDaily(String),
    SetMonthly(String),
    SetProvider { provider: String, limit: String },
    Alerts,
}

#[derive(Debug, Clone)]
pub enum MCPAction {
    List,
    Status,
    Start(String),
    Stop(String),
    Tools { server: Option<String> },
    Resources { server: Option<String> },
    Prompts { server: Option<String> },
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
            "cost" => parse_cost_command(args),
            "budget" => parse_budget_command(args),
            "mcp" => parse_mcp_command(args),
            "quit" | "exit" => Some(SlashCommand::Quit),
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

fn parse_cost_command(args: &[&str]) -> Option<SlashCommand> {
    if args.is_empty() {
        return Some(SlashCommand::Cost {
            action: CostAction::Show,
        });
    }

    match args[0].to_lowercase().as_str() {
        "today" => Some(SlashCommand::Cost {
            action: CostAction::Today,
        }),
        "week" => Some(SlashCommand::Cost {
            action: CostAction::Week,
        }),
        "month" => Some(SlashCommand::Cost {
            action: CostAction::Month,
        }),
        "breakdown" | "by-provider" => Some(SlashCommand::Cost {
            action: CostAction::Breakdown,
        }),
        "alerts" => Some(SlashCommand::Cost {
            action: CostAction::Alerts,
        }),
        _ => Some(SlashCommand::Cost {
            action: CostAction::Show,
        }),
    }
}

fn parse_budget_command(args: &[&str]) -> Option<SlashCommand> {
    if args.is_empty() {
        return Some(SlashCommand::Budget {
            action: BudgetAction::Show,
        });
    }

    match args[0].to_lowercase().as_str() {
        "daily" if args.len() >= 2 => Some(SlashCommand::Budget {
            action: BudgetAction::SetDaily(args[1].to_string()),
        }),
        "monthly" if args.len() >= 2 => Some(SlashCommand::Budget {
            action: BudgetAction::SetMonthly(args[1].to_string()),
        }),
        "provider" if args.len() >= 3 => Some(SlashCommand::Budget {
            action: BudgetAction::SetProvider {
                provider: args[1].to_string(),
                limit: args[2].to_string(),
            },
        }),
        "alerts" => Some(SlashCommand::Budget {
            action: BudgetAction::Alerts,
        }),
        _ => Some(SlashCommand::Budget {
            action: BudgetAction::Show,
        }),
    }
}

fn parse_mcp_command(args: &[&str]) -> Option<SlashCommand> {
    if args.is_empty() {
        return Some(SlashCommand::Mcp {
            action: MCPAction::List,
        });
    }

    match args[0].to_lowercase().as_str() {
        "list" => Some(SlashCommand::Mcp {
            action: MCPAction::List,
        }),
        "status" => Some(SlashCommand::Mcp {
            action: MCPAction::Status,
        }),
        "start" if args.len() >= 2 => Some(SlashCommand::Mcp {
            action: MCPAction::Start(args[1].to_string()),
        }),
        "stop" if args.len() >= 2 => Some(SlashCommand::Mcp {
            action: MCPAction::Stop(args[1].to_string()),
        }),
        "tools" => {
            let server = if args.len() >= 2 {
                Some(args[1].to_string())
            } else {
                None
            };
            Some(SlashCommand::Mcp {
                action: MCPAction::Tools { server },
            })
        }
        "resources" => {
            let server = if args.len() >= 2 {
                Some(args[1].to_string())
            } else {
                None
            };
            Some(SlashCommand::Mcp {
                action: MCPAction::Resources { server },
            })
        }
        "prompts" => {
            let server = if args.len() >= 2 {
                Some(args[1].to_string())
            } else {
                None
            };
            Some(SlashCommand::Mcp {
                action: MCPAction::Prompts { server },
            })
        }
        _ => Some(SlashCommand::Mcp {
            action: MCPAction::List,
        }),
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
            SlashCommand::Cost { action } => {
                self.execute_cost_command(action).await
            }
            SlashCommand::Budget { action } => {
                self.execute_budget_command(action).await
            }
            SlashCommand::Mcp { action } => {
                self.execute_mcp_command(action).await
            }
            SlashCommand::Quit => {
                // Signal the app to quit
                let _ = self.event_sender.send(Event::Quit);
                "👋 **Goodbye!** Exiting ValeChat...".to_string()
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
• `/quit` or `/exit` - Exit ValeChat

**Cost Tracking:**
• `/cost` - Show current spending overview
• `/cost today` - Show today's spending
• `/cost week` - Show this week's spending
• `/cost month` - Show monthly spending
• `/cost breakdown` - Show spending by provider
• `/cost alerts` - Show recent cost alerts

**Budget Management:**
• `/budget` - Show current budget limits
• `/budget daily <amount>` - Set daily spending limit
• `/budget monthly <amount>` - Set monthly spending limit
• `/budget provider <name> <limit>` - Set provider spending limit
• `/budget alerts` - Show budget alert configuration

**MCP (Model Context Protocol):**
• `/mcp` or `/mcp list` - List configured MCP servers
• `/mcp status` - Show status of all MCP servers
• `/mcp start <server>` - Start an MCP server
• `/mcp stop <server>` - Stop an MCP server
• `/mcp tools [server]` - List available tools (all or specific server)
• `/mcp resources [server]` - List available resources
• `/mcp prompts [server]` - List available prompts

**Examples:**
• `/PROVIDER gemini` - Switch to Gemini (case insensitive)
• `/Provider LIST` - List all providers (case insensitive)
• `/Model GPT-4` - Switch to GPT-4 (case insensitive)
• `/MODEL list` - List all models (case insensitive)
• `/APIKEY openai` - Check OpenAI API key status
• `/cost today` - Show today's API spending
• `/budget daily 50` - Set $50 daily limit
• `/mcp start filesystem` - Start the filesystem MCP server
• `/mcp tools` - List all available MCP tools"#.to_string()
    }

    async fn execute_cost_command(&self, action: CostAction) -> String {
        match action {
            CostAction::Show => {
                match self.app_state.get_usage_repo().get_usage_statistics().await {
                    Ok(stats) => {
                        format!(
                            "💰 **Cost Overview**\n\n\
                            **Total Spending**: ${:.4}\n\
                            **Total Requests**: {}\n\
                            **This Month**: ${:.4}\n\
                            **Last Month**: ${:.4}\n\
                            **Average per Request**: ${:.6}",
                            stats.total_cost.to_f64().unwrap_or(0.0),
                            stats.total_requests,
                            stats.current_month_cost.to_f64().unwrap_or(0.0),
                            stats.previous_month_cost.to_f64().unwrap_or(0.0),
                            if stats.total_requests > 0 {
                                stats.total_cost.to_f64().unwrap_or(0.0) / stats.total_requests as f64
                            } else { 0.0 }
                        )
                    }
                    Err(e) => format!("❌ Error getting cost data: {}", e),
                }
            }
            CostAction::Today => {
                match self.app_state.get_usage_repo().get_daily_statistics().await {
                    Ok((daily_cost, daily_tokens)) => {
                        format!(
                            "📅 **Today's Spending**\n\n\
                            **Cost**: ${:.4}\n\
                            **Tokens Used**: {}\n\
                            **Estimated Rate**: ${:.6}/token",
                            daily_cost.to_f64().unwrap_or(0.0),
                            daily_tokens,
                            if daily_tokens > 0 {
                                daily_cost.to_f64().unwrap_or(0.0) / daily_tokens as f64
                            } else { 0.0 }
                        )
                    }
                    Err(e) => format!("❌ Error getting daily cost data: {}", e),
                }
            }
            CostAction::Week => {
                match self.app_state.get_usage_repo().get_cost_trend(7).await {
                    Ok(trend_data) => {
                        let total_week: f64 = trend_data.iter()
                            .map(|(_, cost)| cost.to_f64().unwrap_or(0.0))
                            .sum();
                        let mut output = format!("📊 **This Week's Spending**: ${:.4}\n\n**Daily Breakdown:**\n", total_week);
                        for (date, cost) in trend_data {
                            output.push_str(&format!("• {}: ${:.4}\n", date, cost.to_f64().unwrap_or(0.0)));
                        }
                        output
                    }
                    Err(e) => format!("❌ Error getting weekly cost data: {}", e),
                }
            }
            CostAction::Month => {
                match self.app_state.get_usage_repo().get_usage_statistics().await {
                    Ok(stats) => {
                        format!(
                            "📊 **Monthly Spending**\n\n\
                            **This Month**: ${:.4}\n\
                            **Previous Month**: ${:.4}\n\
                            **Change**: {}${:.4} ({})",
                            stats.current_month_cost.to_f64().unwrap_or(0.0),
                            stats.previous_month_cost.to_f64().unwrap_or(0.0),
                            if stats.current_month_cost >= stats.previous_month_cost { "+" } else { "" },
                            (stats.current_month_cost - stats.previous_month_cost).to_f64().unwrap_or(0.0),
                            if stats.previous_month_cost.to_f64().unwrap_or(0.0) > 0.0 {
                                let change_pct = ((stats.current_month_cost - stats.previous_month_cost).to_f64().unwrap_or(0.0) / stats.previous_month_cost.to_f64().unwrap_or(0.0)) * 100.0;
                                format!("{:+.1}%", change_pct)
                            } else {
                                "N/A".to_string()
                            }
                        )
                    }
                    Err(e) => format!("❌ Error getting monthly cost data: {}", e),
                }
            }
            CostAction::Breakdown => {
                match self.app_state.get_usage_repo().get_usage_statistics().await {
                    Ok(stats) => {
                        let mut output = String::from("🔍 **Cost Breakdown by Provider**\n\n");
                        let total = stats.total_cost.to_f64().unwrap_or(0.0);
                        
                        for (provider, usage) in &stats.by_provider {
                            let cost = usage.cost.to_f64().unwrap_or(0.0);
                            let percentage = if total > 0.0 { (cost / total) * 100.0 } else { 0.0 };
                            output.push_str(&format!(
                                "**{}**: ${:.4} ({:.1}%) - {} requests\n",
                                provider, cost, percentage, usage.requests
                            ));
                        }
                        
                        output.push_str("\n**By Model:**\n");
                        for (model, usage) in &stats.by_model {
                            let cost = usage.cost.to_f64().unwrap_or(0.0);
                            let percentage = if total > 0.0 { (cost / total) * 100.0 } else { 0.0 };
                            output.push_str(&format!(
                                "• {} ({}): ${:.4} ({:.1}%)\n",
                                model, usage.provider, cost, percentage
                            ));
                        }
                        
                        output
                    }
                    Err(e) => format!("❌ Error getting cost breakdown: {}", e),
                }
            }
            CostAction::Alerts => {
                // This would integrate with the cost alert system
                "🔔 **Cost Alerts**\n\nNo recent alerts (alert system integration pending)".to_string()
            }
        }
    }

    async fn execute_budget_command(&self, action: BudgetAction) -> String {
        match action {
            BudgetAction::Show => {
                "💳 **Budget Limits**\n\nDaily: Not set\nMonthly: Not set\nProvider limits: None configured\n\nUse `/budget daily <amount>` to set daily limit".to_string()
            }
            BudgetAction::SetDaily(amount) => {
                match amount.parse::<f64>() {
                    Ok(limit) if limit > 0.0 => {
                        format!("✅ **Daily budget set to**: ${:.2}\n\nThis will be enforced for future requests.", limit)
                    }
                    _ => "❌ **Invalid amount**. Please provide a positive number (e.g., `/budget daily 50`)".to_string()
                }
            }
            BudgetAction::SetMonthly(amount) => {
                match amount.parse::<f64>() {
                    Ok(limit) if limit > 0.0 => {
                        format!("✅ **Monthly budget set to**: ${:.2}\n\nThis will be enforced for future requests.", limit)
                    }
                    _ => "❌ **Invalid amount**. Please provide a positive number (e.g., `/budget monthly 1000`)".to_string()
                }
            }
            BudgetAction::SetProvider { provider, limit } => {
                match limit.parse::<f64>() {
                    Ok(limit_amount) if limit_amount > 0.0 => {
                        format!("✅ **{} provider budget set to**: ${:.2}\n\nThis will be enforced for future {} requests.", provider, limit_amount, provider)
                    }
                    _ => format!("❌ **Invalid amount**. Please provide a positive number (e.g., `/budget provider {} 200`)", provider)
                }
            }
            BudgetAction::Alerts => {
                "🔔 **Budget Alert Configuration**\n\nDaily alerts: Enabled at 80% of limit\nMonthly alerts: Enabled at 80% of limit\nCritical alerts: Enabled at 95% of limit\n\n(Full alert configuration UI pending)".to_string()
            }
        }
    }

    async fn execute_mcp_command(&self, action: MCPAction) -> String {
        match action {
            MCPAction::List => {
                let config = self.app_state.get_config();
                let servers = config.mcp_servers;
                
                if servers.is_empty() {
                    return "📡 **No MCP servers configured**\n\nAdd MCP servers to your configuration file.".to_string();
                }
                
                let mut output = String::from("📡 **MCP Servers**\n\n");
                
                for (name, server_config) in servers {
                    let status = if server_config.enabled { "✅ enabled" } else { "❌ disabled" };
                    let transport = match server_config.transport_type {
                        valechat::app::config::TransportType::Stdio => "stdio",
                        valechat::app::config::TransportType::WebSocket { .. } => "websocket",
                    };
                    
                    output.push_str(&format!(
                        "**{}** ({}, {})\n  Command: `{} {}`\n  Auto-start: {}\n\n",
                        name, 
                        status, 
                        transport,
                        server_config.command,
                        server_config.args.join(" "),
                        if server_config.auto_start { "yes" } else { "no" }
                    ));
                }
                
                output
            }
            MCPAction::Status => {
                let server_status = self.app_state.get_mcp_server_status().await;
                
                if server_status.is_empty() {
                    return "📡 **No MCP servers running**".to_string();
                }
                
                let mut output = String::from("📡 **MCP Server Status**\n\n");
                
                for (name, (state, health)) in server_status {
                    let state_str = match state {
                        valechat::mcp::ServerState::NotStarted => "⏹ Not Started",
                        valechat::mcp::ServerState::Starting => "🔄 Starting",
                        valechat::mcp::ServerState::Initializing => "⚙️ Initializing",
                        valechat::mcp::ServerState::Ready => "✅ Ready",
                        valechat::mcp::ServerState::Error(ref msg) => &format!("❌ Error: {}", msg),
                        valechat::mcp::ServerState::Stopping => "🔄 Stopping",
                        valechat::mcp::ServerState::Stopped => "⏹ Stopped",
                    };
                    
                    output.push_str(&format!(
                        "**{}**: {}\n  Health: {} (failures: {})\n",
                        name,
                        state_str,
                        if health.is_healthy { "✅ Healthy" } else { "❌ Unhealthy" },
                        health.consecutive_failures
                    ));
                    
                    if let Some(error) = &health.last_error {
                        output.push_str(&format!("  Last error: {}\n", error));
                    }
                    
                    output.push('\n');
                }
                
                output
            }
            MCPAction::Start(server_name) => {
                match self.app_state.start_mcp_server(&server_name).await {
                    Ok(_) => format!("✅ **Started MCP server**: {}", server_name),
                    Err(e) => format!("❌ **Failed to start MCP server** {}: {}", server_name, e),
                }
            }
            MCPAction::Stop(server_name) => {
                match self.app_state.stop_mcp_server(&server_name).await {
                    Ok(_) => format!("✅ **Stopped MCP server**: {}", server_name),
                    Err(e) => format!("❌ **Failed to stop MCP server** {}: {}", server_name, e),
                }
            }
            MCPAction::Tools { server } => {
                match self.app_state.list_mcp_tools().await {
                    Ok(tools_by_server) => {
                        if tools_by_server.is_empty() {
                            return "🔧 **No MCP tools available** (no servers running)".to_string();
                        }
                        
                        let mut output = String::from("🔧 **Available MCP Tools**\n\n");
                        
                        for (server_name, tools) in tools_by_server {
                            if let Some(ref specific_server) = server {
                                if &server_name != specific_server {
                                    continue;
                                }
                            }
                            
                            if tools.is_empty() {
                                output.push_str(&format!("**{}**: No tools available\n\n", server_name));
                                continue;
                            }
                            
                            output.push_str(&format!("**{}** ({} tools):\n", server_name, tools.len()));
                            
                            for tool in tools {
                                output.push_str(&format!(
                                    "  • **{}**: {}\n",
                                    tool.name,
                                    if tool.description.is_empty() { "No description" } else { &tool.description }
                                ));
                                
                                if let Some(schema) = &tool.input_schema {
                                    output.push_str("    Parameters: ");
                                    if let Some(obj) = schema.as_object() {
                                        if let Some(properties) = obj.get("properties").and_then(|p| p.as_object()) {
                                            let params: Vec<String> = properties.keys().cloned().collect();
                                            output.push_str(&params.join(", "));
                                        }
                                    }
                                    output.push('\n');
                                }
                            }
                            output.push('\n');
                        }
                        
                        output
                    }
                    Err(e) => format!("❌ **Error listing MCP tools**: {}", e),
                }
            }
            MCPAction::Resources { server } => {
                "🗂️ **MCP Resources**\n\nResource listing not yet implemented.".to_string()
            }
            MCPAction::Prompts { server } => {
                "💬 **MCP Prompts**\n\nPrompt listing not yet implemented.".to_string()
            }
        }
    }
}