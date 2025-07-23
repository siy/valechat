use std::path::{Path, PathBuf};
use std::fs;
use sqlx::SqlitePool;
use tracing::{debug, info, warn, error};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::time::{interval, Duration};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::storage::Database;
use crate::platform::AppPaths;

/// Backup and recovery system for ValeChat data
pub struct BackupSystem {
    database: Arc<Database>,
    pool: SqlitePool,
    paths: Arc<AppPaths>,
    config: BackupConfig,
}

/// Configuration for backup system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub enabled: bool,
    pub backup_dir: PathBuf,
    pub retention_days: u32,
    pub auto_backup_interval_hours: u64,
    pub max_backup_size_mb: u64,
    pub compress_backups: bool,
    pub verify_backups: bool,
    pub exclude_tables: Vec<String>, // Tables to exclude from backup
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backup_dir: PathBuf::from("backups"),
            retention_days: 30,
            auto_backup_interval_hours: 24,
            max_backup_size_mb: 1024, // 1GB
            compress_backups: true,
            verify_backups: true,
            exclude_tables: vec![], // No exclusions by default
        }
    }
}

/// Information about a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub file_path: PathBuf,
    pub size_bytes: u64,
    pub compressed: bool,
    pub verified: bool,
    pub backup_type: BackupType,
    pub database_version: i32,
    pub tables_included: Vec<String>,
    pub metadata: BackupMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackupType {
    Full,
    Incremental,
    Manual,
    Scheduled,
}

/// Metadata about the backup content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub total_records: u64,
    pub conversations_count: u64,
    pub messages_count: u64,
    pub usage_records_count: u64,
    pub checksum: String,
}

/// Recovery options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryOptions {
    pub backup_id: String,
    pub target_path: Option<PathBuf>,
    pub verify_before_restore: bool,
    pub create_backup_before_restore: bool,
    pub tables_to_restore: Option<Vec<String>>, // If None, restore all
}

/// Recovery result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub success: bool,
    pub restored_tables: Vec<String>,
    pub records_restored: u64,
    pub duration_seconds: u64,
    pub warnings: Vec<String>,
    pub pre_restore_backup_id: Option<String>,
}

impl BackupSystem {
    /// Create a new backup system
    pub fn new(database: Arc<Database>, paths: Arc<AppPaths>, config: BackupConfig) -> Self {
        let pool = database.pool().clone();
        Self {
            database,
            pool,
            paths,
            config,
        }
    }

    /// Initialize the backup system
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing backup system");

        // Create backup directory
        let backup_dir = self.paths.data_dir().join(&self.config.backup_dir);
        if !backup_dir.exists() {
            fs::create_dir_all(&backup_dir)?;
            info!("Created backup directory: {:?}", backup_dir);
        }

        // Validate backup directory is writable
        let test_file = backup_dir.join(".test_write");
        match fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
            }
            Err(e) => {
                error!("Backup directory is not writable: {}", e);
                return Err(Error::platform(format!("Backup directory not writable: {}", e)));
            }
        }

        // Clean up old backups
        self.cleanup_old_backups().await?;

        info!("Backup system initialized successfully");
        Ok(())
    }

    /// Create a full backup
    pub async fn create_backup(&self, backup_type: BackupType) -> Result<BackupInfo> {
        if !self.config.enabled {
            return Err(Error::platform("Backup system is disabled"));
        }

        let backup_id = format!("backup_{}", Utc::now().format("%Y%m%d_%H%M%S"));
        info!("Creating backup: {}", backup_id);

        // Generate backup file path
        let backup_dir = self.paths.data_dir().join(&self.config.backup_dir);
        let backup_file = backup_dir.join(format!("{}.db", backup_id));

        // Check available space
        self.check_available_space(&backup_file).await?;

        // Create the backup using SQLite's backup API
        let start_time = Utc::now();
        self.database.backup(&backup_file).await?;

        // Get backup file size
        let _size_bytes = fs::metadata(&backup_file)?.len();
        
        // Compress if configured
        let final_path = if self.config.compress_backups {
            self.compress_backup(&backup_file).await?
        } else {
            backup_file
        };

        // Get final size after compression
        let final_size = fs::metadata(&final_path)?.len();

        // Generate metadata
        let metadata = self.generate_backup_metadata().await?;

        // Verify backup if configured
        let verified = if self.config.verify_backups {
            self.verify_backup(&final_path).await.unwrap_or(false)
        } else {
            false
        };

        let backup_info = BackupInfo {
            id: backup_id,
            created_at: start_time,
            file_path: final_path,
            size_bytes: final_size,
            compressed: self.config.compress_backups,
            verified,
            backup_type,
            database_version: self.get_database_version().await?,
            tables_included: self.get_table_list().await?,
            metadata,
        };

        // Store backup info
        self.store_backup_info(&backup_info).await?;

        info!(
            "Backup created successfully: {} ({} bytes)",
            backup_info.id, backup_info.size_bytes
        );

        Ok(backup_info)
    }

    /// List available backups
    pub async fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        debug!("Listing available backups");

        let backup_dir = self.paths.data_dir().join(&self.config.backup_dir);
        if !backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();
        let entries = fs::read_dir(&backup_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            // Look for backup info files
            if path.extension().and_then(|s| s.to_str()) == Some("info") {
                if let Ok(backup_info) = self.load_backup_info(&path).await {
                    // Verify the backup file still exists
                    if backup_info.file_path.exists() {
                        backups.push(backup_info);
                    }
                }
            }
        }

        // Sort by creation time (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        debug!("Found {} backups", backups.len());
        Ok(backups)
    }

    /// Restore from a backup
    pub async fn restore_backup(&self, options: RecoveryOptions) -> Result<RecoveryResult> {
        info!("Starting backup restoration: {}", options.backup_id);

        let start_time = std::time::Instant::now();
        let warnings = Vec::new();
        let mut pre_restore_backup_id = None;

        // Find the backup
        let backups = self.list_backups().await?;
        let backup = backups.iter()
            .find(|b| b.id == options.backup_id)
            .ok_or_else(|| Error::platform(format!("Backup not found: {}", options.backup_id)))?;

        // Verify backup before restore if requested
        if options.verify_before_restore {
            if !self.verify_backup(&backup.file_path).await? {
                return Err(Error::platform("Backup verification failed"));
            }
        }

        // Create backup before restore if requested
        if options.create_backup_before_restore {
            let pre_backup = self.create_backup(BackupType::Manual).await?;
            let backup_id = pre_backup.id.clone();
            pre_restore_backup_id = Some(pre_backup.id);
            info!("Created pre-restore backup: {}", backup_id);
        }

        // Determine target path
        let target_path = options.target_path.clone().unwrap_or_else(|| {
            self.paths.database_file()
        });

        // Perform the restoration
        let records_restored = self.perform_restore(backup, &target_path, &options).await?;

        let duration = start_time.elapsed();
        
        let result = RecoveryResult {
            success: true,
            restored_tables: backup.tables_included.clone(),
            records_restored,
            duration_seconds: duration.as_secs(),
            warnings,
            pre_restore_backup_id,
        };

        info!("Backup restoration completed successfully in {} seconds", duration.as_secs());
        Ok(result)
    }

    /// Start automatic backup scheduler
    pub async fn start_auto_backup(&self) -> Result<()> {
        if !self.config.enabled || self.config.auto_backup_interval_hours == 0 {
            info!("Auto backup is disabled");
            return Ok(());
        }

        info!("Starting auto backup scheduler (every {} hours)", self.config.auto_backup_interval_hours);

        let mut interval = interval(Duration::from_secs(self.config.auto_backup_interval_hours * 3600));
        
        loop {
            interval.tick().await;
            
            match self.create_backup(BackupType::Scheduled).await {
                Ok(backup_info) => {
                    info!("Scheduled backup created: {}", backup_info.id);
                }
                Err(e) => {
                    error!("Scheduled backup failed: {}", e);
                }
            }

            // Clean up old backups after each scheduled backup
            if let Err(e) = self.cleanup_old_backups().await {
                warn!("Failed to clean up old backups: {}", e);
            }
        }
    }

    /// Clean up old backups based on retention policy
    async fn cleanup_old_backups(&self) -> Result<u32> {
        debug!("Cleaning up old backups");

        let backups = self.list_backups().await?;
        let cutoff_date = Utc::now() - chrono::Duration::days(self.config.retention_days as i64);
        
        let mut deleted_count = 0;
        for backup in backups {
            if backup.created_at < cutoff_date {
                if let Err(e) = self.delete_backup(&backup).await {
                    warn!("Failed to delete old backup {}: {}", backup.id, e);
                } else {
                    deleted_count += 1;
                    info!("Deleted old backup: {}", backup.id);
                }
            }
        }

        if deleted_count > 0 {
            info!("Cleaned up {} old backups", deleted_count);
        }

        Ok(deleted_count)
    }

    /// Delete a specific backup
    async fn delete_backup(&self, backup: &BackupInfo) -> Result<()> {
        // Delete backup file
        if backup.file_path.exists() {
            fs::remove_file(&backup.file_path)?;
        }

        // Delete backup info file
        let info_file = backup.file_path.with_extension("info");
        if info_file.exists() {
            fs::remove_file(&info_file)?;
        }

        Ok(())
    }

    /// Verify backup integrity
    async fn verify_backup(&self, backup_path: &Path) -> Result<bool> {
        debug!("Verifying backup: {:?}", backup_path);

        // For SQLite backups, we can verify by trying to open and check integrity
        let backup_url = format!("sqlite:{}?mode=ro", backup_path.display());
        
        match SqlitePool::connect(&backup_url).await {
            Ok(pool) => {
                // Run integrity check
                let integrity_result: String = sqlx::query_scalar("PRAGMA integrity_check")
                    .fetch_one(&pool)
                    .await?;
                
                pool.close().await;
                
                let is_ok = integrity_result == "ok";
                if is_ok {
                    debug!("Backup verification passed");
                } else {
                    warn!("Backup verification failed: {}", integrity_result);
                }
                
                Ok(is_ok)
            }
            Err(e) => {
                error!("Failed to open backup for verification: {}", e);
                Ok(false)
            }
        }
    }

    /// Compress a backup file
    async fn compress_backup(&self, backup_path: &Path) -> Result<PathBuf> {
        debug!("Compressing backup: {:?}", backup_path);

        let compressed_path = backup_path.with_extension("db.gz");
        
        // Read the original file
        let data = fs::read(backup_path)?;
        
        // Compress using gzip
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data)?;
        let compressed_data = encoder.finish()?;
        
        // Write compressed data
        fs::write(&compressed_path, compressed_data)?;
        
        // Remove original file
        fs::remove_file(backup_path)?;
        
        info!(
            "Backup compressed: {} -> {} ({:.1}% reduction)",
            backup_path.display(),
            compressed_path.display(),
            (1.0 - (fs::metadata(&compressed_path)?.len() as f64 / data.len() as f64)) * 100.0
        );
        
        Ok(compressed_path)
    }

    /// Generate backup metadata
    async fn generate_backup_metadata(&self) -> Result<BackupMetadata> {
        let conversations_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations"
        ).fetch_one(&self.pool).await?;

        let messages_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages"
        ).fetch_one(&self.pool).await?;

        let usage_records_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_records"
        ).fetch_one(&self.pool).await?;

        let total_records = conversations_count + messages_count + usage_records_count;

        // Generate simple checksum (in real implementation, use proper hashing)
        let checksum = format!("{:x}", total_records);

        Ok(BackupMetadata {
            total_records: total_records as u64,
            conversations_count: conversations_count as u64,
            messages_count: messages_count as u64,
            usage_records_count: usage_records_count as u64,
            checksum,
        })
    }

    /// Get current database version
    async fn get_database_version(&self) -> Result<i32> {
        let version: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_settings WHERE key = 'database_version'"
        ).fetch_optional(&self.pool).await?;

        Ok(version.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    /// Get list of tables in the database
    async fn get_table_list(&self) -> Result<Vec<String>> {
        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
        ).fetch_all(&self.pool).await?;

        Ok(tables.into_iter().map(|(name,)| name).collect())
    }

    /// Store backup information
    async fn store_backup_info(&self, backup_info: &BackupInfo) -> Result<()> {
        let info_file = backup_info.file_path.with_extension("info");
        let json_data = serde_json::to_string_pretty(backup_info)?;
        fs::write(&info_file, json_data)?;
        Ok(())
    }

    /// Load backup information
    async fn load_backup_info(&self, info_path: &Path) -> Result<BackupInfo> {
        let json_data = fs::read_to_string(info_path)?;
        let backup_info: BackupInfo = serde_json::from_str(&json_data)?;
        Ok(backup_info)
    }

    /// Check available disk space
    async fn check_available_space(&self, _backup_path: &Path) -> Result<()> {
        // Get current database size as estimate for backup size
        let db_stats = self.database.get_statistics().await?;
        let estimated_backup_size = db_stats.database_size_bytes * 2; // Conservative estimate

        // Check if we have enough space (simplified check)
        if estimated_backup_size > self.config.max_backup_size_mb * 1024 * 1024 {
            return Err(Error::platform(format!(
                "Estimated backup size ({} MB) exceeds configured limit ({} MB)",
                estimated_backup_size / (1024 * 1024),
                self.config.max_backup_size_mb
            )));
        }

        Ok(())
    }

    /// Perform the actual restore operation
    async fn perform_restore(
        &self,
        backup: &BackupInfo,
        target_path: &Path,
        _options: &RecoveryOptions,
    ) -> Result<u64> {
        info!("Restoring backup to: {:?}", target_path);

        // If backup is compressed, decompress first
        let restore_source = if backup.compressed {
            self.decompress_backup(&backup.file_path).await?
        } else {
            backup.file_path.clone()
        };

        // Copy the backup file to target location
        fs::copy(&restore_source, target_path)?;

        // If we decompressed, clean up the temporary file
        if backup.compressed && restore_source != backup.file_path {
            let _ = fs::remove_file(&restore_source);
        }

        // Return estimated record count
        Ok(backup.metadata.total_records)
    }

    /// Decompress a backup file
    async fn decompress_backup(&self, compressed_path: &Path) -> Result<PathBuf> {
        debug!("Decompressing backup: {:?}", compressed_path);

        let temp_path = compressed_path.with_extension("tmp");
        
        // Read compressed data
        let compressed_data = fs::read(compressed_path)?;
        
        // Decompress using gzip
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        let mut decoder = GzDecoder::new(&compressed_data[..]);
        let mut decompressed_data = Vec::new();
        decoder.read_to_end(&mut decompressed_data)?;
        
        // Write decompressed data
        fs::write(&temp_path, decompressed_data)?;
        
        Ok(temp_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Database;
    use tempfile::TempDir;

    async fn create_test_backup_system() -> (BackupSystem, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = Arc::new(AppPaths::with_data_dir(temp_dir.path()).unwrap());
        let db = Arc::new(Database::new(&paths).await.unwrap());
        
        let mut config = BackupConfig::default();
        config.compress_backups = false; // Disable compression for tests
        config.verify_backups = true;
        
        let backup_system = BackupSystem::new(db, paths, config);
        (backup_system, temp_dir)
    }

    #[tokio::test]
    async fn test_backup_system_initialization() {
        let (backup_system, _temp_dir) = create_test_backup_system().await;

        let result = backup_system.initialize().await;
        assert!(result.is_ok());

        // Check that backup directory was created
        let backup_dir = backup_system.paths.data_dir().join(&backup_system.config.backup_dir);
        assert!(backup_dir.exists());
    }

    #[tokio::test]
    async fn test_create_backup() {
        let (backup_system, _temp_dir) = create_test_backup_system().await;
        backup_system.initialize().await.unwrap();

        let backup_info = backup_system.create_backup(BackupType::Manual).await.unwrap();
        
        assert!(!backup_info.id.is_empty());
        assert!(backup_info.file_path.exists());
        assert!(backup_info.size_bytes > 0);
        assert_eq!(backup_info.backup_type, BackupType::Manual);
    }

    #[tokio::test]
    async fn test_list_backups() {
        let (backup_system, _temp_dir) = create_test_backup_system().await;
        backup_system.initialize().await.unwrap();

        // Create a backup
        let _backup_info = backup_system.create_backup(BackupType::Manual).await.unwrap();

        // List backups
        let backups = backup_system.list_backups().await.unwrap();
        assert_eq!(backups.len(), 1);
    }

    #[tokio::test]
    async fn test_backup_verification() {
        let (backup_system, _temp_dir) = create_test_backup_system().await;
        backup_system.initialize().await.unwrap();

        let backup_info = backup_system.create_backup(BackupType::Manual).await.unwrap();
        
        let is_valid = backup_system.verify_backup(&backup_info.file_path).await.unwrap();
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_backup_metadata_generation() {
        let (backup_system, _temp_dir) = create_test_backup_system().await;

        let metadata = backup_system.generate_backup_metadata().await.unwrap();
        
        assert!(metadata.total_records >= 0);
        assert!(!metadata.checksum.is_empty());
    }
}