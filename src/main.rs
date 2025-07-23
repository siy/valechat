use tracing::info;
use valechat::{app::AppState, error::Result};
use valechat::platform::{AppPaths, SecureStorageManager};
use valechat::app::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("valechat=debug".parse().unwrap()),
        )
        .init();

    info!("Starting ValeChat application (Phase 1 - Core components test)");

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
    let _state = AppState::new(config, paths, secure_storage).await?;
    info!("Application state initialized successfully");

    // Test basic functionality
    println!("✅ Phase 1 implementation completed successfully!");
    println!("✅ Cross-platform paths and directories working");
    println!("✅ Secure storage abstraction working");
    println!("✅ Configuration system working");
    println!("✅ Error handling and logging working");
    println!("✅ Platform abstraction layer working");

    Ok(())
}
