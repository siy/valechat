// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use tracing::info;
use valechat::{app::AppState, error::Result};
use valechat::platform::{AppPaths, SecureStorageManager};
use valechat::app::AppConfig;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
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

async fn init_app_state() -> Result<AppState> {
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
        .invoke_handler(tauri::generate_handler![get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
