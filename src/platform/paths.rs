use directories::ProjectDirs;
use std::path::PathBuf;

use crate::error::{Error, Result};

pub struct AppPaths {
    project_dirs: ProjectDirs,
}

impl AppPaths {
    pub fn new() -> Result<Self> {
        let project_dirs = ProjectDirs::from("ai", "valechat", "ValeChat")
            .ok_or_else(|| Error::platform("Failed to determine application directories"))?;
        
        Ok(Self { project_dirs })
    }

    pub fn config_dir(&self) -> PathBuf {
        self.project_dirs.config_dir().to_path_buf()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.project_dirs.data_dir().to_path_buf()
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.project_dirs.cache_dir().to_path_buf()
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir().join("config.toml")
    }

    pub fn database_file(&self) -> PathBuf {
        self.data_dir().join("valechat.db")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir().join("logs")
    }

    pub fn mcp_servers_dir(&self) -> PathBuf {
        self.data_dir().join("mcp_servers")
    }

    pub fn ensure_dirs_exist(&self) -> Result<()> {
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::create_dir_all(self.data_dir())?;
        std::fs::create_dir_all(self.cache_dir())?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.mcp_servers_dir())?;
        Ok(())
    }
}

impl Default for AppPaths {
    fn default() -> Self {
        Self::new().expect("Failed to create AppPaths")
    }
}