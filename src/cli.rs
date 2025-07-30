use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "valechat")]
#[command(about = "Multi-model AI chat application with MCP server support")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Configuration file path
    #[arg(short, long)]
    pub config: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    pub debug: bool,

    /// Disable colors in output
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the interactive chat interface
    Chat {
        /// Start with a specific conversation
        #[arg(short, long)]
        conversation: Option<String>,
        
        /// Use specific model provider
        #[arg(short, long)]
        provider: Option<String>,
        
        /// Use specific model
        #[arg(short, long)]
        model: Option<String>,
    },
    
    /// Manage API keys
    ApiKey {
        /// Provider name (openai, anthropic, etc.)
        provider: String,
        
        /// Set API key for provider
        #[arg(short, long)]
        set: Option<String>,
        
        /// Remove API key for provider
        #[arg(short, long)]
        remove: bool,
        
        /// Show current API key status
        #[arg(long)]
        status: bool,
    },
    
    /// List available models and providers
    Models {
        /// Show only enabled providers
        #[arg(short, long)]
        enabled: bool,
    },
    
    /// Show usage and billing information
    Usage {
        /// Show usage for specific period (today, week, month)
        #[arg(short, long)]
        period: Option<String>,
        
        /// Show usage for specific provider
        #[arg(long)]
        provider: Option<String>,
    },
    
    /// Export conversation data
    Export {
        /// Export format (json, markdown, txt)
        #[arg(short, long, default_value = "markdown")]
        format: String,
        
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
        
        /// Conversation ID to export (all if not specified)
        #[arg(short, long)]
        conversation: Option<String>,
    },
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            command: Some(Commands::Chat {
                conversation: None,
                provider: None,
                model: None,
            }),
            config: None,
            debug: false,
            no_color: false,
        }
    }
}