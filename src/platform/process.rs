use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tracing::{debug, warn};

use crate::error::{Error, Result};

#[async_trait]
pub trait ProcessManager: Send + Sync {
    async fn spawn_sandboxed(&self, config: ProcessConfig) -> Result<SandboxedProcess>;
    async fn terminate(&self, process: &mut SandboxedProcess) -> Result<()>;
    fn get_resource_limits(&self) -> ResourceLimits;
}

#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub env_vars: HashMap<String, String>,
    pub resource_limits: ResourceLimits,
    pub network_access: bool,
    pub file_system_access: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory_mb: u64,
    pub max_cpu_percent: u8,
    pub max_open_files: u32,
    pub timeout_seconds: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: 512,
            max_cpu_percent: 50,
            max_open_files: 100,
            timeout_seconds: 300, // 5 minutes
        }
    }
}

pub struct SandboxedProcess {
    pub id: u32,
    child: Child,
    #[allow(dead_code)]
    config: ProcessConfig,
    start_time: std::time::Instant,
}

impl SandboxedProcess {
    pub fn new(child: Child, config: ProcessConfig) -> Self {
        let id = child.id().unwrap_or(0);
        Self {
            id,
            child,
            config,
            start_time: std::time::Instant::now(),
        }
    }

    pub async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        Ok(self.child.wait().await?)
    }

    pub async fn kill(&mut self) -> Result<()> {
        match self.child.kill().await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Ok(()), // Already dead
            Err(e) => Err(Error::Io(e)),
        }
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn stdin(&mut self) -> Option<&mut tokio::process::ChildStdin> {
        self.child.stdin.as_mut()
    }

    pub fn stdout(&mut self) -> Option<&mut tokio::process::ChildStdout> {
        self.child.stdout.as_mut()
    }

    pub fn stderr(&mut self) -> Option<&mut tokio::process::ChildStderr> {
        self.child.stderr.as_mut()
    }
}

// Platform-specific process manager
pub struct DefaultProcessManager {
    default_limits: ResourceLimits,
}

impl DefaultProcessManager {
    pub fn new() -> Self {
        Self {
            default_limits: ResourceLimits::default(),
        }
    }
}

impl Default for DefaultProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProcessManager for DefaultProcessManager {
    async fn spawn_sandboxed(&self, config: ProcessConfig) -> Result<SandboxedProcess> {
        debug!("Spawning sandboxed process: {} {:?}", config.command, config.args);

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        
        // Set working directory
        if let Some(working_dir) = &config.working_dir {
            cmd.current_dir(working_dir);
        }

        // Set environment variables
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        // Configure stdio for MCP communication
        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        // Platform-specific sandboxing
        #[cfg(unix)]
        {
            // On Unix systems, we can use process groups for better cleanup
            unsafe {
                cmd.pre_exec(|| {
                    // Create new process group
                    libc::setpgid(0, 0);
                    Ok(())
                });
            }
        }

        // TODO: Implement platform-specific resource limits
        // This would involve:
        // - Linux: cgroups, namespaces, seccomp
        // - macOS: sandbox-exec, resource limits
        // - Windows: job objects, restricted tokens

        let child = cmd.spawn().map_err(|e| {
            warn!("Failed to spawn process {}: {}", config.command, e);
            Error::platform(format!("Failed to spawn process: {}", e))
        })?;

        debug!("Successfully spawned process with PID: {:?}", child.id());
        
        Ok(SandboxedProcess::new(child, config))
    }

    async fn terminate(&self, process: &mut SandboxedProcess) -> Result<()> {
        debug!("Terminating process with PID: {}", process.id);

        // Try graceful shutdown first
        #[cfg(unix)]
        {
            // Send SIGTERM
            unsafe {
                libc::kill(process.id as i32, libc::SIGTERM);
            }
            
            // Wait a bit for graceful shutdown
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            // Check if still running
            match process.child.try_wait() {
                Ok(Some(_)) => {
                    debug!("Process {} terminated gracefully", process.id);
                    return Ok(());
                }
                Ok(None) => {
                    // Still running, force kill
                    warn!("Process {} did not terminate gracefully, force killing", process.id);
                }
                Err(e) => {
                    warn!("Error checking process status: {}", e);
                }
            }
        }

        // Force kill
        process.kill().await?;
        debug!("Process {} force terminated", process.id);
        Ok(())
    }

    fn get_resource_limits(&self) -> ResourceLimits {
        self.default_limits.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_spawn_and_terminate() {
        let manager = DefaultProcessManager::new();
        let config = ProcessConfig {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            working_dir: None,
            env_vars: HashMap::new(),
            resource_limits: ResourceLimits::default(),
            network_access: false,
            file_system_access: vec![],
        };

        let mut process = manager.spawn_sandboxed(config).await.unwrap();
        let exit_status = process.wait().await.unwrap();
        assert!(exit_status.success());
    }
}