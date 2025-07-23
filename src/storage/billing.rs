use std::collections::HashMap;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use chrono::{DateTime, Utc, Datelike};
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};
use crate::storage::database::decimal_helpers;
use crate::storage::usage::{UsageRepository, UsageRecord, BillingSummary};

/// Billing management system with cost verification and spending limits
pub struct BillingSystem {
    pool: SqlitePool,
    usage_repo: UsageRepository,
}

/// Spending limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingLimit {
    pub limit_type: SpendingLimitType,
    pub amount: Decimal,
    pub period: BillingPeriod,
    pub enabled: bool,
    pub alert_threshold: Option<f32>, // Percentage (0.0-1.0) to trigger alerts
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpendingLimitType {
    Global,
    PerProvider(String),
    PerModel(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BillingPeriod {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Billing alert configuration and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingAlert {
    pub id: String,
    pub alert_type: AlertType,
    pub threshold: f32, // Percentage of limit
    pub triggered: bool,
    pub last_triggered: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    SpendingThreshold,
    DailyLimit,
    MonthlyLimit,
    UnverifiedCosts,
    CostDiscrepancy,
}

/// Billing verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub request_id: String,
    pub original_cost: Decimal,
    pub verified_cost: Decimal,
    pub discrepancy: Decimal,
    pub verified_at: DateTime<Utc>,
    pub verification_source: String,
}

/// Comprehensive billing report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingReport {
    pub period: String,
    pub total_cost: Decimal,
    pub verified_cost: Decimal,
    pub unverified_cost: Decimal,
    pub verification_rate: f32, // Percentage
    pub cost_discrepancies: Vec<VerificationResult>,
    pub by_provider: HashMap<String, ProviderBilling>,
    pub by_model: HashMap<String, ModelBilling>,
    pub spending_limits: Vec<SpendingLimitStatus>,
    pub alerts: Vec<BillingAlert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBilling {
    pub total_cost: Decimal,
    pub verified_cost: Decimal,
    pub request_count: u64,
    pub verification_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBilling {
    pub provider: String,
    pub total_cost: Decimal,
    pub verified_cost: Decimal,
    pub request_count: u64,
    pub verification_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingLimitStatus {
    pub limit: SpendingLimit,
    pub current_spending: Decimal,
    pub percentage_used: f32,
    pub is_exceeded: bool,
    pub days_remaining: Option<i32>,
}

/// Result of checking spending limits
#[derive(Debug, Clone)]
pub struct SpendingCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub current_spending: Decimal,
    pub limit: Option<Decimal>,
    pub percentage_used: Option<f32>,
}

impl BillingSystem {
    /// Create a new billing system
    pub fn new(pool: SqlitePool) -> Self {
        let usage_repo = UsageRepository::new(pool.clone());
        Self { pool, usage_repo }
    }

    /// Set a spending limit
    pub async fn set_spending_limit(
        &self,
        limit_type: SpendingLimitType,
        amount: Decimal,
        period: BillingPeriod,
        alert_threshold: Option<f32>,
    ) -> Result<()> {
        debug!("Setting spending limit: {:?} = ${}", limit_type, amount);

        let limit_key = match &limit_type {
            SpendingLimitType::Global => "global".to_string(),
            SpendingLimitType::PerProvider(provider) => format!("provider:{}", provider),
            SpendingLimitType::PerModel(model) => format!("model:{}", model),
        };

        let period_str = match period {
            BillingPeriod::Daily => "daily",
            BillingPeriod::Weekly => "weekly",
            BillingPeriod::Monthly => "monthly",
            BillingPeriod::Yearly => "yearly",
        };

        let limit_json = serde_json::to_string(&SpendingLimit {
            limit_type,
            amount,
            period,
            enabled: true,
            alert_threshold,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }).map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to serialize limit: {}", e))))?;

        sqlx::query(
            "INSERT OR REPLACE INTO app_settings (key, value, updated_at) VALUES (?, ?, unixepoch())"
        )
        .bind(format!("spending_limit_{}_{}", period_str, limit_key))
        .bind(limit_json)
        .execute(&self.pool)
        .await?;

        info!("Set spending limit: {} = ${}", limit_key, amount);
        Ok(())
    }

    /// Check if a proposed cost would exceed spending limits
    pub async fn check_spending_limits(
        &self,
        provider: &str,
        model: &str,
        proposed_cost: Decimal,
    ) -> Result<SpendingCheckResult> {
        debug!("Checking spending limits for cost: ${}", proposed_cost);

        // Get current spending for different scopes
        let now = Utc::now();
        
        // Check global monthly limit first
        if let Some(global_limit) = self.get_spending_limit(&SpendingLimitType::Global, &BillingPeriod::Monthly).await? {
            let current_period = format!("{:04}-{:02}", now.year(), now.month());
            let current_spending = self.get_period_spending(None, None, &current_period).await?;
            let projected_spending = current_spending + proposed_cost;

            if projected_spending > global_limit.amount {
                return Ok(SpendingCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Would exceed global monthly limit of ${} (current: ${}, proposed: ${})",
                        global_limit.amount, current_spending, proposed_cost
                    )),
                    current_spending,
                    limit: Some(global_limit.amount),
                    percentage_used: Some((projected_spending / global_limit.amount * Decimal::from(100)).to_f32().unwrap_or(0.0)),
                });
            }
        }

        // Check provider-specific limits
        if let Some(provider_limit) = self.get_spending_limit(
            &SpendingLimitType::PerProvider(provider.to_string()), 
            &BillingPeriod::Monthly
        ).await? {
            let current_period = format!("{:04}-{:02}", now.year(), now.month());
            let current_spending = self.get_period_spending(Some(provider), None, &current_period).await?;
            let projected_spending = current_spending + proposed_cost;

            if projected_spending > provider_limit.amount {
                return Ok(SpendingCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Would exceed {} monthly limit of ${} (current: ${}, proposed: ${})",
                        provider, provider_limit.amount, current_spending, proposed_cost
                    )),
                    current_spending,
                    limit: Some(provider_limit.amount),
                    percentage_used: Some((projected_spending / provider_limit.amount * Decimal::from(100)).to_f32().unwrap_or(0.0)),
                });
            }
        }

        // Check model-specific limits
        if let Some(model_limit) = self.get_spending_limit(
            &SpendingLimitType::PerModel(model.to_string()), 
            &BillingPeriod::Monthly
        ).await? {
            let current_period = format!("{:04}-{:02}", now.year(), now.month());
            let current_spending = self.get_model_spending(model, &current_period).await?;
            let projected_spending = current_spending + proposed_cost;

            if projected_spending > model_limit.amount {
                return Ok(SpendingCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Would exceed {} monthly limit of ${} (current: ${}, proposed: ${})",
                        model, model_limit.amount, current_spending, proposed_cost
                    )),
                    current_spending,
                    limit: Some(model_limit.amount),
                    percentage_used: Some((projected_spending / model_limit.amount * Decimal::from(100)).to_f32().unwrap_or(0.0)),
                });
            }
        }

        Ok(SpendingCheckResult {
            allowed: true,
            reason: None,
            current_spending: Decimal::ZERO, // Would need to calculate if needed
            limit: None,
            percentage_used: None,
        })
    }

    /// Get a spending limit from settings
    async fn get_spending_limit(
        &self,
        limit_type: &SpendingLimitType,
        period: &BillingPeriod,
    ) -> Result<Option<SpendingLimit>> {
        let limit_key = match limit_type {
            SpendingLimitType::Global => "global".to_string(),
            SpendingLimitType::PerProvider(provider) => format!("provider:{}", provider),
            SpendingLimitType::PerModel(model) => format!("model:{}", model),
        };

        let period_str = match period {
            BillingPeriod::Daily => "daily",
            BillingPeriod::Weekly => "weekly",
            BillingPeriod::Monthly => "monthly",
            BillingPeriod::Yearly => "yearly",
        };

        let limit_json: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_settings WHERE key = ?"
        )
        .bind(format!("spending_limit_{}_{}", period_str, limit_key))
        .fetch_optional(&self.pool)
        .await?;

        match limit_json {
            Some(json) => {
                let limit: SpendingLimit = serde_json::from_str(&json)
                    .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to deserialize limit: {}", e))))?;
                Ok(Some(limit))
            }
            None => Ok(None),
        }
    }

    /// Get spending for a period and optional provider filter
    async fn get_period_spending(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
        billing_period: &str,
    ) -> Result<Decimal> {
        let mut query = "SELECT COALESCE(SUM(CAST(cost AS REAL)), 0.0) FROM usage_records WHERE billing_period = ?".to_string();
        let mut _bind_count = 1;

        if provider.is_some() {
            query.push_str(" AND provider = ?");
            _bind_count += 1;
        }
        if model.is_some() {
            query.push_str(" AND model = ?");
            _bind_count += 1;
        }

        let mut query_builder = sqlx::query_scalar::<_, f64>(&query).bind(billing_period);

        if let Some(prov) = provider {
            query_builder = query_builder.bind(prov);
        }
        if let Some(mod_name) = model {
            query_builder = query_builder.bind(mod_name);
        }

        let total_cost: f64 = query_builder.fetch_one(&self.pool).await?;
        Ok(Decimal::try_from(total_cost).unwrap_or(Decimal::ZERO))
    }

    /// Get spending for a specific model
    async fn get_model_spending(&self, model: &str, billing_period: &str) -> Result<Decimal> {
        let total_cost: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(CAST(cost AS REAL)), 0.0) FROM usage_records WHERE model = ? AND billing_period = ?"
        )
        .bind(model)
        .bind(billing_period)
        .fetch_one(&self.pool)
        .await?;

        Ok(Decimal::try_from(total_cost).unwrap_or(Decimal::ZERO))
    }

    /// Generate a comprehensive billing report
    pub async fn generate_billing_report(&self, billing_period: &str) -> Result<BillingReport> {
        debug!("Generating billing report for period: {}", billing_period);

        // Get usage records for the period
        let usage_records = self.usage_repo.get_usage_records(None, None, None, None, None).await?
            .into_iter()
            .filter(|r| r.billing_period == billing_period)
            .collect::<Vec<_>>();

        let total_cost = usage_records.iter()
            .map(|r| r.cost)
            .sum::<Decimal>();

        let verified_cost = usage_records.iter()
            .filter(|r| r.verified)
            .map(|r| r.cost)
            .sum::<Decimal>();

        let unverified_cost = total_cost - verified_cost;

        let verification_rate = if total_cost > Decimal::ZERO {
            (verified_cost / total_cost * Decimal::from(100)).to_f32().unwrap_or(0.0)
        } else {
            100.0
        };

        // Calculate by provider
        let mut by_provider = HashMap::new();
        for record in &usage_records {
            let entry = by_provider.entry(record.provider.clone()).or_insert(ProviderBilling {
                total_cost: Decimal::ZERO,
                verified_cost: Decimal::ZERO,
                request_count: 0,
                verification_rate: 0.0,
            });

            entry.total_cost += record.cost;
            entry.request_count += 1;
            if record.verified {
                entry.verified_cost += record.cost;
            }
        }

        // Calculate verification rates for providers
        for (_, billing) in by_provider.iter_mut() {
            billing.verification_rate = if billing.total_cost > Decimal::ZERO {
                (billing.verified_cost / billing.total_cost * Decimal::from(100)).to_f32().unwrap_or(0.0)
            } else {
                100.0
            };
        }

        // Calculate by model
        let mut by_model = HashMap::new();
        for record in &usage_records {
            let entry = by_model.entry(record.model.clone()).or_insert(ModelBilling {
                provider: record.provider.clone(),
                total_cost: Decimal::ZERO,
                verified_cost: Decimal::ZERO,
                request_count: 0,
                verification_rate: 0.0,
            });

            entry.total_cost += record.cost;
            entry.request_count += 1;
            if record.verified {
                entry.verified_cost += record.cost;
            }
        }

        // Calculate verification rates for models
        for (_, billing) in by_model.iter_mut() {
            billing.verification_rate = if billing.total_cost > Decimal::ZERO {
                (billing.verified_cost / billing.total_cost * Decimal::from(100)).to_f32().unwrap_or(0.0)
            } else {
                100.0
            };
        }

        // Get spending limit statuses
        let spending_limits = self.get_spending_limit_statuses(billing_period).await?;

        // Get cost discrepancies (placeholder - would need verification history)
        let cost_discrepancies = Vec::new();

        // Get alerts (placeholder - would need alert system)
        let alerts = Vec::new();

        Ok(BillingReport {
            period: billing_period.to_string(),
            total_cost,
            verified_cost,
            unverified_cost,
            verification_rate,
            cost_discrepancies,
            by_provider,
            by_model,
            spending_limits,
            alerts,
        })
    }

    /// Get status of all spending limits for a period
    async fn get_spending_limit_statuses(&self, billing_period: &str) -> Result<Vec<SpendingLimitStatus>> {
        let mut statuses = Vec::new();

        // Get all spending limit settings
        let settings: Vec<(String, String)> = sqlx::query_as(
            "SELECT key, value FROM app_settings WHERE key LIKE 'spending_limit_%'"
        )
        .fetch_all(&self.pool)
        .await?;

        for (key, value) in settings {
            if let Ok(limit) = serde_json::from_str::<SpendingLimit>(&value) {
                // Calculate current spending based on limit type
                let current_spending = match &limit.limit_type {
                    SpendingLimitType::Global => {
                        self.get_period_spending(None, None, billing_period).await?
                    }
                    SpendingLimitType::PerProvider(provider) => {
                        self.get_period_spending(Some(provider), None, billing_period).await?
                    }
                    SpendingLimitType::PerModel(model) => {
                        self.get_model_spending(model, billing_period).await?
                    }
                };

                let percentage_used = if limit.amount > Decimal::ZERO {
                    (current_spending / limit.amount * Decimal::from(100)).to_f32().unwrap_or(0.0)
                } else {
                    0.0
                };

                let is_exceeded = current_spending > limit.amount;

                statuses.push(SpendingLimitStatus {
                    limit,
                    current_spending,
                    percentage_used,
                    is_exceeded,
                    days_remaining: None, // Could calculate based on period
                });
            }
        }

        Ok(statuses)
    }

    /// Verify costs against provider billing data
    pub async fn verify_costs_batch(&self, verifications: Vec<(String, Decimal)>) -> Result<Vec<VerificationResult>> {
        debug!("Verifying {} cost records", verifications.len());

        let mut results = Vec::new();
        
        for (request_id, verified_cost) in verifications {
            // Get original record
            let original_record: Option<(String, bool)> = sqlx::query_as(
                "SELECT cost, verified FROM usage_records WHERE request_id = ?"
            )
            .bind(&request_id)
            .fetch_optional(&self.pool)
            .await?;

            if let Some((original_cost_str, is_verified)) = original_record {
                if is_verified {
                    warn!("Record {} already verified, skipping", request_id);
                    continue;
                }

                let original_cost = decimal_helpers::string_to_decimal(&original_cost_str)?;
                let discrepancy = verified_cost - original_cost;

                // Verify through usage repository
                self.usage_repo.verify_usage(&request_id, Some(verified_cost)).await?;

                results.push(VerificationResult {
                    request_id: request_id.clone(),
                    original_cost,
                    verified_cost,
                    discrepancy,
                    verified_at: Utc::now(),
                    verification_source: "provider_api".to_string(),
                });

                if discrepancy.abs() > Decimal::new(1, 2) { // More than $0.01 difference
                    warn!(
                        "Cost discrepancy detected: request_id={}, original=${}, verified=${}, diff=${}",
                        request_id, original_cost, verified_cost, discrepancy
                    );
                }
            }
        }

        info!("Verified {} cost records", results.len());
        Ok(results)
    }

    /// Get unverified costs that need verification
    pub async fn get_unverified_costs(&self, limit: Option<i32>) -> Result<Vec<UsageRecord>> {
        self.usage_repo.get_unverified_records(limit).await
    }

    /// Remove a spending limit
    pub async fn remove_spending_limit(
        &self,
        limit_type: &SpendingLimitType,
        period: &BillingPeriod,
    ) -> Result<()> {
        let limit_key = match limit_type {
            SpendingLimitType::Global => "global".to_string(),
            SpendingLimitType::PerProvider(provider) => format!("provider:{}", provider),
            SpendingLimitType::PerModel(model) => format!("model:{}", model),
        };

        let period_str = match period {
            BillingPeriod::Daily => "daily",
            BillingPeriod::Weekly => "weekly",
            BillingPeriod::Monthly => "monthly",
            BillingPeriod::Yearly => "yearly",
        };

        sqlx::query("DELETE FROM app_settings WHERE key = ?")
            .bind(format!("spending_limit_{}_{}", period_str, limit_key))
            .execute(&self.pool)
            .await?;

        info!("Removed spending limit: {}", limit_key);
        Ok(())
    }

    /// Get current month's spending summary
    pub async fn get_current_month_summary(&self) -> Result<BillingReport> {
        let now = Utc::now();
        let current_period = format!("{:04}-{:02}", now.year(), now.month());
        self.generate_billing_report(&current_period).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Database;
    use crate::platform::AppPaths;
    use tempfile::TempDir;

    async fn create_test_billing_system() -> (BillingSystem, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::with_data_dir(temp_dir.path()).unwrap();
        let db = Database::new(&paths).await.unwrap();
        let billing = BillingSystem::new(db.pool().clone());
        (billing, temp_dir)
    }

    #[tokio::test]
    async fn test_set_and_check_spending_limit() {
        let (billing, _temp_dir) = create_test_billing_system().await;

        // Set a global monthly limit
        billing.set_spending_limit(
            SpendingLimitType::Global,
            Decimal::new(10000, 2), // $100.00
            BillingPeriod::Monthly,
            Some(0.8), // 80% alert threshold
        ).await.unwrap();

        // Check a small cost (should be allowed)
        let result = billing.check_spending_limits(
            "openai", 
            "gpt-4", 
            Decimal::new(500, 2) // $5.00
        ).await.unwrap();
        assert!(result.allowed);

        // Check a cost that would exceed limit (should be denied)
        let result = billing.check_spending_limits(
            "openai", 
            "gpt-4", 
            Decimal::new(15000, 2) // $150.00
        ).await.unwrap();
        assert!(!result.allowed);
        assert!(result.reason.is_some());
    }

    #[tokio::test]
    async fn test_verify_costs_batch() {
        let (billing, _temp_dir) = create_test_billing_system().await;

        // First record some usage
        let request_id = billing.usage_repo.record_usage(
            "openai",
            "gpt-4",
            100,
            50,
            Decimal::new(25, 3), // $0.025
            None,
            None,
        ).await.unwrap();

        // Verify with a different cost
        let verifications = vec![(request_id.clone(), Decimal::new(30, 3))]; // $0.030
        let results = billing.verify_costs_batch(verifications).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, request_id);
        assert_eq!(results[0].verified_cost, Decimal::new(30, 3));
        assert_eq!(results[0].discrepancy, Decimal::new(5, 3)); // $0.005 difference
    }

    #[tokio::test]
    async fn test_billing_report() {
        let (billing, _temp_dir) = create_test_billing_system().await;

        // Record some usage
        billing.usage_repo.record_usage("openai", "gpt-4", 100, 50, Decimal::new(25, 3), None, None).await.unwrap();
        billing.usage_repo.record_usage("anthropic", "claude-3", 200, 100, Decimal::new(40, 3), None, None).await.unwrap();

        let now = Utc::now();
        let current_period = format!("{:04}-{:02}", now.year(), now.month());
        
        let report = billing.generate_billing_report(&current_period).await.unwrap();
        
        assert_eq!(report.period, current_period);
        assert_eq!(report.by_provider.len(), 2);
        assert_eq!(report.by_model.len(), 2);
        assert!(report.total_cost > Decimal::ZERO);
    }

    #[tokio::test]
    async fn test_provider_specific_limit() {
        let (billing, _temp_dir) = create_test_billing_system().await;

        // Set a provider-specific limit
        billing.set_spending_limit(
            SpendingLimitType::PerProvider("openai".to_string()),
            Decimal::new(5000, 2), // $50.00
            BillingPeriod::Monthly,
            None,
        ).await.unwrap();

        // Check within limit
        let result = billing.check_spending_limits("openai", "gpt-4", Decimal::new(1000, 2)).await.unwrap();
        assert!(result.allowed);

        // Check exceeding limit
        let result = billing.check_spending_limits("openai", "gpt-4", Decimal::new(6000, 2)).await.unwrap();
        assert!(!result.allowed);

        // Check different provider (should be allowed)
        let result = billing.check_spending_limits("anthropic", "claude-3", Decimal::new(6000, 2)).await.unwrap();
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_unverified_costs() {
        let (billing, _temp_dir) = create_test_billing_system().await;

        // Record unverified usage
        billing.usage_repo.record_usage("openai", "gpt-4", 100, 50, Decimal::new(25, 3), None, None).await.unwrap();
        billing.usage_repo.record_usage("openai", "gpt-3.5", 200, 100, Decimal::new(15, 3), None, None).await.unwrap();

        let unverified = billing.get_unverified_costs(Some(10)).await.unwrap();
        assert_eq!(unverified.len(), 2);
        assert!(!unverified[0].verified);
        assert!(!unverified[1].verified);
    }
}