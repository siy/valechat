use std::path::Path;
use sqlx::{SqlitePool, migrate::MigrateDatabase};
use tracing::{info, debug, error, warn};

use crate::error::{Error, Result};
use crate::platform::AppPaths;

/// Database connection manager with migration support
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection and run migrations
    pub async fn new(paths: &AppPaths) -> Result<Self> {
        let db_path = paths.data_dir().join("valechat.db");
        
        info!("Initializing database at: {:?}", db_path);
        
        // Create database if it doesn't exist
        if !db_path.exists() {
            info!("Database doesn't exist, creating new database");
            sqlx::Sqlite::create_database(&format!("sqlite:{}", db_path.display())).await?;
        }

        // Create connection pool
        let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePool::connect(&database_url).await?;

        let db = Self { pool };

        // Run migrations
        db.run_migrations().await?;

        info!("Database initialized successfully");
        Ok(db)
    }

    /// Get a reference to the database connection pool
    pub fn get_pool(&self) -> SqlitePool {
        self.pool.clone()
    }

    /// Run all pending database migrations
    async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations");

        // Check current database version
        let current_version = self.get_database_version().await?;
        debug!("Current database version: {}", current_version);

        // Load and execute migrations
        let migrations_dir = std::env::current_dir()?.join("migrations");
        if !migrations_dir.exists() {
            warn!("Migrations directory not found: {:?}", migrations_dir);
            return Ok(());
        }

        let mut migration_files = std::fs::read_dir(&migrations_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().map_or(false, |ext| ext == "sql"))
            .collect::<Vec<_>>();

        migration_files.sort();

        for migration_file in migration_files {
            let file_name = migration_file.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");

            // Extract migration number from filename (e.g., "001_initial" -> 1)
            let migration_number: i32 = file_name
                .split('_')
                .next()
                .and_then(|num| num.parse().ok())
                .unwrap_or(0);

            if migration_number <= current_version {
                debug!("Skipping migration {} (already applied)", file_name);
                continue;
            }

            info!("Applying migration: {}", file_name);
            let sql_content = std::fs::read_to_string(&migration_file)?;

            // Execute the migration
            sqlx::query(&sql_content).execute(&self.pool).await.map_err(|e| {
                error!("Failed to apply migration {}: {}", file_name, e);
                Error::Database(e)
            })?;

            // Update database version
            self.update_database_version(migration_number).await?;
            info!("Successfully applied migration: {}", file_name);
        }

        info!("All migrations completed successfully");
        Ok(())
    }

    /// Get the current database version
    async fn get_database_version(&self) -> Result<i32> {
        // First check if the app_settings table exists
        let table_exists = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='app_settings'"
        )
        .fetch_optional(&self.pool)
        .await?
        .is_some();

        if !table_exists {
            return Ok(0); // No migrations applied yet
        }

        // Get the database version from settings
        let version: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_settings WHERE key = 'database_version'"
        )
        .fetch_optional(&self.pool)
        .await?;

        match version {
            Some(version_str) => version_str.parse().map_err(|e| {
                Error::Database(sqlx::Error::Decode(format!("Invalid database version: {}", e).into()))
            }),
            None => Ok(0),
        }
    }

    /// Update the database version
    async fn update_database_version(&self, version: i32) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES ('database_version', ?, unixepoch())"
        )
        .bind(version.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get the database connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Close the database connection
    pub async fn close(self) {
        self.pool.close().await;
        info!("Database connection closed");
    }

    /// Get database statistics
    pub async fn get_statistics(&self) -> Result<DatabaseStatistics> {
        let conversations_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations")
            .fetch_one(&self.pool)
            .await?;

        let messages_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&self.pool)
            .await?;

        let usage_records_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usage_records")
            .fetch_one(&self.pool)
            .await?;

        let tool_invocations_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tool_invocations")
            .fetch_one(&self.pool)
            .await?;

        // Get database file size
        let db_size = self.get_database_size().await?;

        Ok(DatabaseStatistics {
            conversations_count: conversations_count as u64,
            messages_count: messages_count as u64,
            usage_records_count: usage_records_count as u64,
            tool_invocations_count: tool_invocations_count as u64,
            database_size_bytes: db_size,
        })
    }

    /// Get the size of the database file in bytes
    async fn get_database_size(&self) -> Result<u64> {
        let size_result: Option<i64> = sqlx::query_scalar("SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()")
            .fetch_optional(&self.pool)
            .await?;

        Ok(size_result.unwrap_or(0) as u64)
    }

    /// Vacuum the database to reclaim space
    pub async fn vacuum(&self) -> Result<()> {
        info!("Starting database vacuum operation");
        
        sqlx::query("VACUUM").execute(&self.pool).await?;
        
        info!("Database vacuum completed successfully");
        Ok(())
    }

    /// Create a backup of the database
    pub async fn backup<P: AsRef<Path>>(&self, backup_path: P) -> Result<()> {
        let backup_path = backup_path.as_ref();
        info!("Creating database backup at: {:?}", backup_path);

        // Use SQLite's backup API through raw SQL
        let backup_sql = format!(
            "VACUUM INTO '{}'",
            backup_path.display().to_string().replace("'", "''")
        );

        sqlx::query(&backup_sql).execute(&self.pool).await?;

        info!("Database backup created successfully");
        Ok(())
    }

    /// Verify database integrity
    pub async fn verify_integrity(&self) -> Result<bool> {
        info!("Verifying database integrity");

        let integrity_result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await?;

        let is_ok = integrity_result == "ok";
        
        if is_ok {
            info!("Database integrity check passed");
        } else {
            error!("Database integrity check failed: {}", integrity_result);
        }

        Ok(is_ok)
    }

    /// Optimize database performance by updating statistics
    pub async fn analyze(&self) -> Result<()> {
        info!("Analyzing database for query optimization");
        
        sqlx::query("ANALYZE").execute(&self.pool).await?;
        
        info!("Database analysis completed");
        Ok(())
    }
}

/// Database statistics for monitoring
#[derive(Debug, Clone)]
pub struct DatabaseStatistics {
    pub conversations_count: u64,
    pub messages_count: u64,
    pub usage_records_count: u64,
    pub tool_invocations_count: u64,
    pub database_size_bytes: u64,
}

impl DatabaseStatistics {
    /// Get database size in a human-readable format
    pub fn size_human_readable(&self) -> String {
        let size = self.database_size_bytes as f64;
        if size < 1024.0 {
            format!("{} B", size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else if size < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} MB", size / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", size / (1024.0 * 1024.0 * 1024.0))
        }
    }
}

/// Helper functions for working with decimal values in the database
pub mod decimal_helpers {
    use rust_decimal::Decimal;
    use crate::error::{Error, Result};

    /// Convert a Decimal to a string for database storage
    pub fn decimal_to_string(decimal: Decimal) -> String {
        decimal.to_string()
    }

    /// Parse a string from the database back to a Decimal
    pub fn string_to_decimal(s: &str) -> Result<Decimal> {
        s.parse().map_err(|e| {
            Error::Database(sqlx::Error::Decode(
                format!("Failed to parse decimal from string '{}': {}", s, e).into()
            ))
        })
    }

    /// Convert an optional string to an optional Decimal
    pub fn option_string_to_decimal(s: Option<String>) -> Result<Option<Decimal>> {
        match s {
            Some(string) => Ok(Some(string_to_decimal(&string)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::platform::AppPaths;
    use rust_decimal::Decimal;

    async fn create_test_database() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::with_data_dir(temp_dir.path()).unwrap();
        let db = Database::new(&paths).await.unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_database_creation() {
        let (db, _temp_dir) = create_test_database().await;
        
        let stats = db.get_statistics().await.unwrap();
        assert_eq!(stats.conversations_count, 0);
        assert_eq!(stats.messages_count, 0);
    }

    #[tokio::test]
    async fn test_database_integrity() {
        let (db, _temp_dir) = create_test_database().await;
        
        let is_ok = db.verify_integrity().await.unwrap();
        assert!(is_ok);
    }

    #[tokio::test]
    async fn test_decimal_helpers() {
        use decimal_helpers::*;

        let decimal = Decimal::new(12345, 2); // 123.45
        let string = decimal_to_string(decimal);
        assert_eq!(string, "123.45");

        let parsed = string_to_decimal(&string).unwrap();
        assert_eq!(parsed, decimal);
    }

    #[tokio::test]
    async fn test_database_vacuum() {
        let (db, _temp_dir) = create_test_database().await;
        
        // Should not fail even on empty database
        db.vacuum().await.unwrap();
    }

    #[tokio::test]
    async fn test_database_analyze() {
        let (db, _temp_dir) = create_test_database().await;
        
        // Should not fail even on empty database
        db.analyze().await.unwrap();
    }
}