use std::collections::HashMap;
use sqlx::{SqlitePool, Row};
use tracing::{debug, info, warn};
use rust_decimal::Decimal;
use chrono::{DateTime, Utc, Datelike};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::storage::database::decimal_helpers;

/// Repository for managing usage tracking and billing
pub struct UsageRepository {
    pool: SqlitePool,
}

/// A usage record for tracking API consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost: Decimal,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub request_id: String,
    pub billing_period: String, // YYYY-MM format
    pub verified: bool,
    pub verification_timestamp: Option<DateTime<Utc>>,
}

/// Billing summary for a specific period and model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingSummary {
    pub billing_period: String,
    pub provider: String,
    pub model: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: Decimal,
    pub request_count: u64,
    pub last_updated: DateTime<Utc>,
}

/// Usage statistics for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStatistics {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: Decimal,
    pub by_provider: HashMap<String, ProviderUsage>,
    pub by_model: HashMap<String, ModelUsage>,
    pub current_month_cost: Decimal,
    pub previous_month_cost: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub provider: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: Decimal,
}

impl UsageRepository {
    /// Create a new usage repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Record a usage event for billing tracking
    pub async fn record_usage(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cost: Decimal,
        conversation_id: Option<&str>,
        message_id: Option<&str>,
    ) -> Result<String> {
        let request_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let billing_period = format!("{:04}-{:02}", now.year(), now.month());
        
        debug!(
            "Recording usage: provider={}, model={}, input_tokens={}, output_tokens={}, cost={}",
            provider, model, input_tokens, output_tokens, cost
        );

        // Start transaction for atomic operation
        let mut tx = self.pool.begin().await?;

        // Insert usage record
        sqlx::query(
            r#"
            INSERT INTO usage_records (
                timestamp, provider, model, input_tokens, output_tokens, cost,
                conversation_id, message_id, request_id, billing_period, verified
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(now.timestamp())
        .bind(provider)
        .bind(model)
        .bind(input_tokens as i32)
        .bind(output_tokens as i32)
        .bind(decimal_helpers::decimal_to_string(cost))
        .bind(conversation_id)
        .bind(message_id)
        .bind(&request_id)
        .bind(&billing_period)
        .bind(false) // Not verified initially
        .execute(&mut *tx)
        .await?;

        // Update billing summary
        self.update_billing_summary_tx(
            &mut tx,
            &billing_period,
            provider,
            model,
            input_tokens,
            output_tokens,
            cost,
        ).await?;

        tx.commit().await?;

        info!("Successfully recorded usage with request_id: {}", request_id);
        Ok(request_id)
    }

    /// Update billing summary in a transaction
    async fn update_billing_summary_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        billing_period: &str,
        provider: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cost: Decimal,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO billing_summaries (
                billing_period, provider, model, total_input_tokens, 
                total_output_tokens, total_cost, request_count
            ) VALUES (?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(billing_period, provider, model) DO UPDATE SET
                total_input_tokens = total_input_tokens + ?,
                total_output_tokens = total_output_tokens + ?,
                total_cost = (
                    CAST(total_cost AS REAL) + CAST(? AS REAL)
                ),
                request_count = request_count + 1,
                last_updated = unixepoch()
            "#
        )
        .bind(billing_period)
        .bind(provider)
        .bind(model)
        .bind(input_tokens as i32)
        .bind(output_tokens as i32)
        .bind(decimal_helpers::decimal_to_string(cost))
        .bind(input_tokens as i32)
        .bind(output_tokens as i32)
        .bind(decimal_helpers::decimal_to_string(cost))
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Verify a usage record against provider billing data
    pub async fn verify_usage(&self, request_id: &str, verified_cost: Option<Decimal>) -> Result<()> {
        debug!("Verifying usage record: {}", request_id);

        let now = Utc::now();
        let mut update_cost = false;
        let mut cost_adjustment = Decimal::ZERO;

        // Get current record
        let current_record = sqlx::query(
            "SELECT cost, verified FROM usage_records WHERE request_id = ?"
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(record) = current_record {
            let current_cost_str: String = record.get("cost");
            let current_cost = decimal_helpers::string_to_decimal(&current_cost_str)?;
            let is_verified: bool = record.get("verified");

            if is_verified {
                warn!("Usage record {} is already verified", request_id);
                return Ok(());
            }

            if let Some(verified_cost) = verified_cost {
                if verified_cost != current_cost {
                    update_cost = true;
                    cost_adjustment = verified_cost - current_cost;
                    warn!(
                        "Cost mismatch for request {}: recorded={}, verified={}, adjustment={}",
                        request_id, current_cost, verified_cost, cost_adjustment
                    );
                }
            }
        } else {
            return Err(Error::Database(sqlx::Error::RowNotFound));
        }

        // Start transaction
        let mut tx = self.pool.begin().await?;

        // Update the usage record
        if update_cost {
            sqlx::query(
                r#"
                UPDATE usage_records 
                SET verified = TRUE, verification_timestamp = ?, cost = ?
                WHERE request_id = ?
                "#
            )
            .bind(now.timestamp())
            .bind(decimal_helpers::decimal_to_string(verified_cost.unwrap()))
            .bind(request_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE usage_records 
                SET verified = TRUE, verification_timestamp = ?
                WHERE request_id = ?
                "#
            )
            .bind(now.timestamp())
            .bind(request_id)
            .execute(&mut *tx)
            .await?;
        }

        // Update billing summary if cost changed
        if update_cost && cost_adjustment != Decimal::ZERO {
            // Get record details for billing summary update
            let record_details = sqlx::query(
                "SELECT billing_period, provider, model FROM usage_records WHERE request_id = ?"
            )
            .bind(request_id)
            .fetch_one(&mut *tx)
            .await?;

            let billing_period: String = record_details.get("billing_period");
            let provider: String = record_details.get("provider");
            let model: String = record_details.get("model");

            sqlx::query(
                r#"
                UPDATE billing_summaries 
                SET total_cost = (CAST(total_cost AS REAL) + CAST(? AS REAL)),
                    last_updated = unixepoch()
                WHERE billing_period = ? AND provider = ? AND model = ?
                "#
            )
            .bind(decimal_helpers::decimal_to_string(cost_adjustment))
            .bind(&billing_period)
            .bind(&provider)
            .bind(&model)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        info!("Successfully verified usage record: {}", request_id);
        Ok(())
    }

    /// Get usage records for a specific time period
    pub async fn get_usage_records(
        &self,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        provider: Option<&str>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<UsageRecord>> {
        debug!("Getting usage records with filters");

        let mut query = String::from(
            r#"
            SELECT id, timestamp, provider, model, input_tokens, output_tokens, cost,
                   conversation_id, message_id, request_id, billing_period, verified, verification_timestamp
            FROM usage_records WHERE 1=1
            "#
        );

        if start_time.is_some() {
            query.push_str(" AND timestamp >= ?");
        }
        if end_time.is_some() {
            query.push_str(" AND timestamp <= ?");
        }
        if provider.is_some() {
            query.push_str(" AND provider = ?");
        }

        query.push_str(" ORDER BY timestamp DESC");
        
        if let Some(limit) = limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        let mut query_builder = sqlx::query(&query);

        if let Some(start) = start_time {
            query_builder = query_builder.bind(start.timestamp());
        }
        if let Some(end) = end_time {
            query_builder = query_builder.bind(end.timestamp());
        }
        if let Some(prov) = provider {
            query_builder = query_builder.bind(prov);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut records = Vec::new();
        for row in rows {
            let cost_str: String = row.get("cost");
            let cost = decimal_helpers::string_to_decimal(&cost_str)?;

            let timestamp_unix: i64 = row.get("timestamp");
            let timestamp = DateTime::from_timestamp(timestamp_unix, 0)
                .unwrap_or_else(|| Utc::now());

            let verification_timestamp_unix: Option<i64> = row.get("verification_timestamp");
            let verification_timestamp = verification_timestamp_unix
                .and_then(|ts| DateTime::from_timestamp(ts, 0));

            records.push(UsageRecord {
                id: row.get("id"),
                timestamp,
                provider: row.get("provider"),
                model: row.get("model"),
                input_tokens: row.get::<i32, _>("input_tokens") as u32,
                output_tokens: row.get::<i32, _>("output_tokens") as u32,
                cost,
                conversation_id: row.get("conversation_id"),
                message_id: row.get("message_id"),
                request_id: row.get("request_id"),
                billing_period: row.get("billing_period"),
                verified: row.get("verified"),
                verification_timestamp,
            });
        }

        debug!("Retrieved {} usage records", records.len());
        Ok(records)
    }

    /// Get billing summaries for reporting
    pub async fn get_billing_summaries(
        &self,
        billing_periods: Option<Vec<String>>,
        provider: Option<&str>,
    ) -> Result<Vec<BillingSummary>> {
        debug!("Getting billing summaries");

        let mut query = String::from(
            r#"
            SELECT billing_period, provider, model, total_input_tokens, total_output_tokens,
                   total_cost, request_count, last_updated
            FROM billing_summaries WHERE 1=1
            "#
        );

        if billing_periods.is_some() {
            let periods = billing_periods.as_ref().unwrap();
            let placeholders = vec!["?"; periods.len()].join(",");
            query.push_str(&format!(" AND billing_period IN ({})", placeholders));
        }

        if provider.is_some() {
            query.push_str(" AND provider = ?");
        }

        query.push_str(" ORDER BY billing_period DESC, provider, model");

        let mut query_builder = sqlx::query(&query);

        if let Some(periods) = billing_periods {
            for period in periods {
                query_builder = query_builder.bind(period);
            }
        }

        if let Some(prov) = provider {
            query_builder = query_builder.bind(prov);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let mut summaries = Vec::new();
        for row in rows {
            let cost_str: String = row.get("total_cost");
            let cost = decimal_helpers::string_to_decimal(&cost_str)?;

            let last_updated_unix: i64 = row.get("last_updated");
            let last_updated = DateTime::from_timestamp(last_updated_unix, 0)
                .unwrap_or_else(|| Utc::now());

            summaries.push(BillingSummary {
                billing_period: row.get("billing_period"),
                provider: row.get("provider"),
                model: row.get("model"),
                total_input_tokens: row.get::<i32, _>("total_input_tokens") as u64,
                total_output_tokens: row.get::<i32, _>("total_output_tokens") as u64,
                total_cost: cost,
                request_count: row.get::<i32, _>("request_count") as u64,
                last_updated,
            });
        }

        debug!("Retrieved {} billing summaries", summaries.len());
        Ok(summaries)
    }

    /// Get comprehensive usage statistics
    pub async fn get_usage_statistics(&self) -> Result<UsageStatistics> {
        debug!("Calculating usage statistics");

        // Get total stats
        let total_stats = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as total_requests,
                COALESCE(SUM(input_tokens), 0) as total_input_tokens,
                COALESCE(SUM(output_tokens), 0) as total_output_tokens,
                COALESCE(SUM(CAST(cost AS REAL)), 0.0) as total_cost
            FROM usage_records
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        let total_requests: i64 = total_stats.get("total_requests");
        let total_input_tokens: i64 = total_stats.get("total_input_tokens");
        let total_output_tokens: i64 = total_stats.get("total_output_tokens");
        let total_cost_f64: f64 = total_stats.get("total_cost");

        // Get current and previous month costs
        let now = Utc::now();
        let current_period = format!("{:04}-{:02}", now.year(), now.month());
        
        let previous_month = if now.month() == 1 {
            format!("{:04}-12", now.year() - 1)
        } else {
            format!("{:04}-{:02}", now.year(), now.month() - 1)
        };

        let current_month_cost = self.get_period_cost(&current_period).await?;
        let previous_month_cost = self.get_period_cost(&previous_month).await?;

        // Get stats by provider
        let provider_rows = sqlx::query(
            r#"
            SELECT 
                provider,
                COUNT(*) as requests,
                COALESCE(SUM(input_tokens), 0) as input_tokens,
                COALESCE(SUM(output_tokens), 0) as output_tokens,
                COALESCE(SUM(CAST(cost AS REAL)), 0.0) as cost
            FROM usage_records
            GROUP BY provider
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut by_provider = HashMap::new();
        for row in provider_rows {
            let provider: String = row.get("provider");
            let requests: i64 = row.get("requests");
            let input_tokens: i64 = row.get("input_tokens");
            let output_tokens: i64 = row.get("output_tokens");
            let cost_f64: f64 = row.get("cost");

            by_provider.insert(provider, ProviderUsage {
                requests: requests as u64,
                input_tokens: input_tokens as u64,
                output_tokens: output_tokens as u64,
                cost: Decimal::try_from(cost_f64).unwrap_or(Decimal::ZERO),
            });
        }

        // Get stats by model
        let model_rows = sqlx::query(
            r#"
            SELECT 
                model,
                provider,
                COUNT(*) as requests,
                COALESCE(SUM(input_tokens), 0) as input_tokens,
                COALESCE(SUM(output_tokens), 0) as output_tokens,
                COALESCE(SUM(CAST(cost AS REAL)), 0.0) as cost
            FROM usage_records
            GROUP BY model, provider
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut by_model = HashMap::new();
        for row in model_rows {
            let model: String = row.get("model");
            let provider: String = row.get("provider");
            let requests: i64 = row.get("requests");
            let input_tokens: i64 = row.get("input_tokens");
            let output_tokens: i64 = row.get("output_tokens");
            let cost_f64: f64 = row.get("cost");

            by_model.insert(model, ModelUsage {
                provider,
                requests: requests as u64,
                input_tokens: input_tokens as u64,
                output_tokens: output_tokens as u64,
                cost: Decimal::try_from(cost_f64).unwrap_or(Decimal::ZERO),
            });
        }

        Ok(UsageStatistics {
            total_requests: total_requests as u64,
            total_input_tokens: total_input_tokens as u64,
            total_output_tokens: total_output_tokens as u64,
            total_cost: Decimal::try_from(total_cost_f64).unwrap_or(Decimal::ZERO),
            by_provider,
            by_model,
            current_month_cost,
            previous_month_cost,
        })
    }

    /// Get total cost for a billing period
    async fn get_period_cost(&self, billing_period: &str) -> Result<Decimal> {
        let cost_result: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(CAST(cost AS REAL)), 0.0) FROM usage_records WHERE billing_period = ?"
        )
        .bind(billing_period)
        .fetch_optional(&self.pool)
        .await?;

        Ok(Decimal::try_from(cost_result.unwrap_or(0.0)).unwrap_or(Decimal::ZERO))
    }

    /// Get unverified usage records that need verification
    pub async fn get_unverified_records(&self, limit: Option<i32>) -> Result<Vec<UsageRecord>> {
        debug!("Getting unverified usage records");

        let limit = limit.unwrap_or(100);
        
        let rows = sqlx::query(
            r#"
            SELECT id, timestamp, provider, model, input_tokens, output_tokens, cost,
                   conversation_id, message_id, request_id, billing_period, verified, verification_timestamp
            FROM usage_records 
            WHERE verified = FALSE
            ORDER BY timestamp ASC
            LIMIT ?
            "#
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut records = Vec::new();
        for row in rows {
            let cost_str: String = row.get("cost");
            let cost = decimal_helpers::string_to_decimal(&cost_str)?;

            let timestamp_unix: i64 = row.get("timestamp");
            let timestamp = DateTime::from_timestamp(timestamp_unix, 0)
                .unwrap_or_else(|| Utc::now());

            records.push(UsageRecord {
                id: row.get("id"),
                timestamp,
                provider: row.get("provider"),
                model: row.get("model"),
                input_tokens: row.get::<i32, _>("input_tokens") as u32,
                output_tokens: row.get::<i32, _>("output_tokens") as u32,
                cost,
                conversation_id: row.get("conversation_id"),
                message_id: row.get("message_id"),
                request_id: row.get("request_id"),
                billing_period: row.get("billing_period"),
                verified: row.get("verified"),
                verification_timestamp: None,
            });
        }

        debug!("Retrieved {} unverified records", records.len());
        Ok(records)
    }

    /// Get daily usage statistics for today
    pub async fn get_daily_statistics(&self) -> Result<(Decimal, u64)> {
        debug!("Getting daily usage statistics");

        let today = Utc::now().date_naive();
        let start_of_day = today.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let end_of_day = today.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp();

        let row = sqlx::query(
            r#"
            SELECT 
                COALESCE(SUM(CAST(cost AS REAL)), 0.0) as daily_cost,
                COALESCE(SUM(input_tokens + output_tokens), 0) as daily_tokens
            FROM usage_records 
            WHERE timestamp >= ? AND timestamp <= ?
            "#
        )
        .bind(start_of_day)
        .bind(end_of_day)
        .fetch_one(&self.pool)
        .await?;

        let daily_cost: f64 = row.get("daily_cost");
        let daily_tokens: i64 = row.get("daily_tokens");

        Ok((Decimal::from_f64_retain(daily_cost).unwrap_or(Decimal::ZERO), daily_tokens as u64))
    }

    /// Get cost trend data for the last N days
    pub async fn get_cost_trend(&self, days: u32) -> Result<Vec<(String, Decimal)>> {
        debug!("Getting cost trend for last {} days", days);

        let days_ago = Utc::now() - chrono::Duration::days(days as i64);
        let start_timestamp = days_ago.timestamp();

        let rows = sqlx::query(
            r#"
            SELECT 
                date(timestamp, 'unixepoch') as date,
                SUM(CAST(cost AS REAL)) as daily_cost
            FROM usage_records 
            WHERE timestamp >= ?
            GROUP BY date(timestamp, 'unixepoch')
            ORDER BY date
            "#
        )
        .bind(start_timestamp)
        .fetch_all(&self.pool)
        .await?;

        let mut trend_data = Vec::new();
        for row in rows {
            let date: String = row.get("date");
            let daily_cost: f64 = row.get("daily_cost");
            trend_data.push((date, Decimal::from_f64_retain(daily_cost).unwrap_or(Decimal::ZERO)));
        }

        debug!("Retrieved {} days of cost trend data", trend_data.len());
        Ok(trend_data)
    }

    /// Delete old usage records for cleanup (keeps summaries)
    pub async fn cleanup_old_records(&self, days_to_keep: u32) -> Result<u64> {
        info!("Cleaning up usage records older than {} days", days_to_keep);

        let cutoff_time = Utc::now() - chrono::Duration::days(days_to_keep as i64);
        
        let result = sqlx::query(
            "DELETE FROM usage_records WHERE timestamp < ? AND verified = TRUE"
        )
        .bind(cutoff_time.timestamp())
        .execute(&self.pool)
        .await?;

        let deleted_rows = result.rows_affected();
        info!("Cleaned up {} old usage records", deleted_rows);
        
        Ok(deleted_rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Database;
    use crate::platform::AppPaths;
    use tempfile::TempDir;

    async fn create_test_repository() -> (UsageRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::with_data_dir(temp_dir.path()).unwrap();
        let db = Database::new(&paths).await.unwrap();
        let repo = UsageRepository::new(db.pool().clone());
        (repo, temp_dir)
    }

    #[tokio::test]
    async fn test_record_usage() {
        let (repo, _temp_dir) = create_test_repository().await;

        let request_id = repo.record_usage(
            "openai",
            "gpt-4",
            100,
            50,
            Decimal::new(25, 3), // $0.025
            None, // No conversation reference for this test
            None, // No message reference for this test
        ).await.unwrap();

        assert!(!request_id.is_empty());

        // Verify record exists
        let records = repo.get_usage_records(None, None, None, Some(1), None).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].provider, "openai");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
    }

    #[tokio::test]
    async fn test_billing_summaries() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Record some usage
        repo.record_usage("openai", "gpt-4", 100, 50, Decimal::new(25, 3), None, None).await.unwrap();
        repo.record_usage("openai", "gpt-4", 200, 100, Decimal::new(50, 3), None, None).await.unwrap();

        let summaries = repo.get_billing_summaries(None, None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].request_count, 2);
        assert_eq!(summaries[0].total_input_tokens, 300);
        assert_eq!(summaries[0].total_output_tokens, 150);
    }

    #[tokio::test]
    async fn test_verify_usage() {
        let (repo, _temp_dir) = create_test_repository().await;

        let request_id = repo.record_usage(
            "openai",
            "gpt-4",
            100,
            50,
            Decimal::new(25, 3),
            None,
            None,
        ).await.unwrap();

        // Verify with different cost
        repo.verify_usage(&request_id, Some(Decimal::new(30, 3))).await.unwrap();

        let records = repo.get_usage_records(None, None, None, Some(1), None).await.unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].verified);
        assert_eq!(records[0].cost, Decimal::new(30, 3));
    }

    #[tokio::test]
    async fn test_usage_statistics() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Record usage from different providers
        repo.record_usage("openai", "gpt-4", 100, 50, Decimal::new(25, 3), None, None).await.unwrap();
        repo.record_usage("anthropic", "claude-3", 200, 100, Decimal::new(40, 3), None, None).await.unwrap();

        let stats = repo.get_usage_statistics().await.unwrap();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_input_tokens, 300);
        assert_eq!(stats.total_output_tokens, 150);
        assert_eq!(stats.by_provider.len(), 2);
        assert_eq!(stats.by_model.len(), 2);
    }

    #[tokio::test]
    async fn test_unverified_records() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Record some usage (unverified by default)
        repo.record_usage("openai", "gpt-4", 100, 50, Decimal::new(25, 3), None, None).await.unwrap();
        repo.record_usage("openai", "gpt-3.5", 200, 100, Decimal::new(15, 3), None, None).await.unwrap();

        let unverified = repo.get_unverified_records(Some(10)).await.unwrap();
        assert_eq!(unverified.len(), 2);
        assert!(!unverified[0].verified);
        assert!(!unverified[1].verified);
    }
}