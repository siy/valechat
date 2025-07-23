use std::collections::HashMap;
use std::sync::Arc;
use sqlx::SqlitePool;
use tracing::debug;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use chrono::{DateTime, Utc, Datelike, NaiveDate};
use serde::{Serialize, Deserialize};

use crate::error::Result;
use crate::storage::billing::BillingSystem;
use crate::storage::usage::UsageRepository;

/// Comprehensive billing dashboard service
pub struct BillingDashboard {
    pool: SqlitePool,
    billing: Arc<BillingSystem>,
    usage_repo: Arc<UsageRepository>,
}

/// Dashboard data for the main overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub overview: BillingOverview,
    pub current_month: MonthlyReport,
    pub recent_activity: Vec<RecentActivity>,
    pub spending_trends: SpendingTrends,
    pub cost_breakdown: CostBreakdown,
    pub alerts: Vec<DashboardAlert>,
    pub efficiency_metrics: EfficiencyMetrics,
}

/// High-level billing overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingOverview {
    pub current_month_spend: Decimal,
    pub current_month_budget: Option<Decimal>,
    pub budget_utilization: Option<f32>, // Percentage
    pub previous_month_spend: Decimal,
    pub month_over_month_change: f32, // Percentage change
    pub total_requests_this_month: u64,
    pub average_cost_per_request: Decimal,
    pub most_expensive_day: Option<DailySpend>,
    pub projected_monthly_spend: Decimal,
}

/// Monthly billing report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyReport {
    pub period: String,
    pub total_spend: Decimal,
    pub total_requests: u64,
    pub verified_percentage: f32,
    pub top_providers: Vec<ProviderSpend>,
    pub top_models: Vec<ModelSpend>,
    pub daily_breakdown: Vec<DailySpend>,
    pub cost_categories: HashMap<String, Decimal>,
}

/// Recent activity in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentActivity {
    pub timestamp: DateTime<Utc>,
    pub activity_type: ActivityType,
    pub description: String,
    pub amount: Option<Decimal>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityType {
    ApiCall,
    CostVerification,
    LimitExceeded,
    LimitWarning,
    EmergencyStop,
    BackupCreated,
    SettingsChanged,
}

/// Spending trends analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingTrends {
    pub daily_trend: Vec<DailySpend>,
    pub weekly_average: Decimal,
    pub trend_direction: TrendDirection,
    pub trend_percentage: f32,
    pub seasonal_patterns: Vec<SeasonalData>,
    pub usage_patterns: UsagePatterns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonalData {
    pub period: String, // "Monday", "Week 1", etc.
    pub average_spend: Decimal,
    pub request_count: u64,
}

/// Daily spending data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySpend {
    pub date: NaiveDate,
    pub amount: Decimal,
    pub requests: u64,
    pub providers: HashMap<String, Decimal>,
}

/// Provider spending data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSpend {
    pub provider: String,
    pub amount: Decimal,
    pub requests: u64,
    pub percentage_of_total: f32,
    pub average_cost_per_request: Decimal,
}

/// Model spending data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpend {
    pub model: String,
    pub provider: String,
    pub amount: Decimal,
    pub requests: u64,
    pub percentage_of_total: f32,
    pub tokens_used: u64, // Total input + output tokens
}

/// Cost breakdown by categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub by_provider: HashMap<String, Decimal>,
    pub by_model: HashMap<String, Decimal>,
    pub by_token_type: HashMap<String, Decimal>, // input, output
    pub by_request_type: HashMap<String, Decimal>, // chat, completion, embedding, etc.
    pub verified_vs_unverified: HashMap<String, Decimal>,
}

/// Dashboard alerts and notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardAlert {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub action_required: bool,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    BudgetExceeded,
    BudgetWarning,
    UnusualSpending,
    VerificationIssues,
    SystemMaintenance,
    CostAnomaly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Usage patterns analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagePatterns {
    pub peak_hours: Vec<HourlyUsage>,
    pub peak_days: Vec<String>, // ["Monday", "Friday"]
    pub average_session_cost: Decimal,
    pub most_active_models: Vec<String>,
    pub efficiency_score: f32, // 0-100 score based on cost per output
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyUsage {
    pub hour: u8, // 0-23
    pub requests: u64,
    pub cost: Decimal,
}

/// Efficiency metrics for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfficiencyMetrics {
    pub cost_per_token: Decimal,
    pub cost_per_successful_request: Decimal,
    pub verification_rate: f32,
    pub provider_efficiency: HashMap<String, f32>, // Cost effectiveness score
    pub model_efficiency: HashMap<String, f32>,
    pub optimization_suggestions: Vec<OptimizationSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    pub suggestion_type: SuggestionType,
    pub description: String,
    pub potential_savings: Option<Decimal>,
    pub confidence: f32, // 0-1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionType {
    ModelSwitch,
    ProviderSwitch,
    TokenOptimization,
    UsagePattern,
    BudgetAdjustment,
}

impl BillingDashboard {
    /// Create a new billing dashboard
    pub fn new(pool: SqlitePool, billing: Arc<BillingSystem>, usage_repo: Arc<UsageRepository>) -> Self {
        Self {
            pool,
            billing,
            usage_repo,
        }
    }

    /// Get comprehensive dashboard data
    pub async fn get_dashboard_data(&self) -> Result<DashboardData> {
        debug!("Generating comprehensive dashboard data");

        let overview = self.get_billing_overview().await?;
        let current_month = self.get_current_month_report().await?;
        let recent_activity = self.get_recent_activity(20).await?;
        let spending_trends = self.get_spending_trends(30).await?;
        let cost_breakdown = self.get_cost_breakdown().await?;
        let alerts = self.get_dashboard_alerts().await?;
        let efficiency_metrics = self.get_efficiency_metrics().await?;

        Ok(DashboardData {
            overview,
            current_month,
            recent_activity,
            spending_trends,
            cost_breakdown,
            alerts,
            efficiency_metrics,
        })
    }

    /// Get high-level billing overview
    async fn get_billing_overview(&self) -> Result<BillingOverview> {
        let now = Utc::now();
        let current_period = format!("{:04}-{:02}", now.year(), now.month());
        
        let previous_month = if now.month() == 1 {
            format!("{:04}-12", now.year() - 1)
        } else {
            format!("{:04}-{:02}", now.year(), now.month() - 1)
        };

        // Get current month spending
        let current_month_spend = self.get_period_spending(&current_period).await?;
        let previous_month_spend = self.get_period_spending(&previous_month).await?;

        // Calculate month-over-month change
        let month_over_month_change = if previous_month_spend > Decimal::ZERO {
            ((current_month_spend - previous_month_spend) / previous_month_spend * Decimal::from(100))
                .to_f32().unwrap_or(0.0)
        } else {
            0.0
        };

        // Get request count
        let total_requests_this_month = self.get_period_request_count(&current_period).await?;

        // Calculate average cost per request
        let average_cost_per_request = if total_requests_this_month > 0 {
            current_month_spend / Decimal::from(total_requests_this_month)
        } else {
            Decimal::ZERO
        };

        // Get most expensive day
        let most_expensive_day = self.get_most_expensive_day(&current_period).await?;

        // Project monthly spend based on current trend
        let days_in_month = self.get_days_in_current_month();
        let days_elapsed = now.day();
        let projected_monthly_spend = if days_elapsed > 0 {
            current_month_spend * Decimal::from(days_in_month) / Decimal::from(days_elapsed)
        } else {
            current_month_spend
        };

        Ok(BillingOverview {
            current_month_spend,
            current_month_budget: None, // TODO: Get from settings
            budget_utilization: None,
            previous_month_spend,
            month_over_month_change,
            total_requests_this_month,
            average_cost_per_request,
            most_expensive_day,
            projected_monthly_spend,
        })
    }

    /// Get current month detailed report
    async fn get_current_month_report(&self) -> Result<MonthlyReport> {
        let now = Utc::now();
        let current_period = format!("{:04}-{:02}", now.year(), now.month());
        
        let billing_report = self.billing.generate_billing_report(&current_period).await?;
        
        // Get top providers
        let mut top_providers: Vec<ProviderSpend> = billing_report.by_provider.iter()
            .map(|(provider, billing)| {
                let percentage = if billing_report.total_cost > Decimal::ZERO {
                    (billing.total_cost / billing_report.total_cost * Decimal::from(100))
                        .to_f32().unwrap_or(0.0)
                } else {
                    0.0
                };

                let avg_cost = if billing.request_count > 0 {
                    billing.total_cost / Decimal::from(billing.request_count)
                } else {
                    Decimal::ZERO
                };

                ProviderSpend {
                    provider: provider.clone(),
                    amount: billing.total_cost,
                    requests: billing.request_count,
                    percentage_of_total: percentage,
                    average_cost_per_request: avg_cost,
                }
            })
            .collect();
        
        top_providers.sort_by(|a, b| b.amount.cmp(&a.amount));
        top_providers.truncate(5);

        // Get top models
        let mut top_models: Vec<ModelSpend> = billing_report.by_model.iter()
            .map(|(model, billing)| {
                let percentage = if billing_report.total_cost > Decimal::ZERO {
                    (billing.total_cost / billing_report.total_cost * Decimal::from(100))
                        .to_f32().unwrap_or(0.0)
                } else {
                    0.0
                };

                ModelSpend {
                    model: model.clone(),
                    provider: billing.provider.clone(),
                    amount: billing.total_cost,
                    requests: billing.request_count,
                    percentage_of_total: percentage,
                    tokens_used: 0, // TODO: Calculate from usage records
                }
            })
            .collect();
            
        top_models.sort_by(|a, b| b.amount.cmp(&a.amount));
        top_models.truncate(5);

        // Get daily breakdown
        let daily_breakdown = self.get_daily_breakdown(&current_period).await?;

        // Create cost categories
        let mut cost_categories = HashMap::new();
        cost_categories.insert("Verified".to_string(), billing_report.verified_cost);
        cost_categories.insert("Unverified".to_string(), billing_report.unverified_cost);

        Ok(MonthlyReport {
            period: current_period,
            total_spend: billing_report.total_cost,
            total_requests: top_providers.iter().map(|p| p.requests).sum(),
            verified_percentage: billing_report.verification_rate,
            top_providers,
            top_models,
            daily_breakdown,
            cost_categories,
        })
    }

    /// Get recent activity
    async fn get_recent_activity(&self, limit: u32) -> Result<Vec<RecentActivity>> {
        // This would typically query an activity log table
        // For now, return mock data based on recent usage records
        let usage_records = self.usage_repo.get_usage_records(None, None, None, Some(limit as i32), None).await?;
        
        let mut activities = Vec::new();
        for record in usage_records {
            activities.push(RecentActivity {
                timestamp: record.timestamp,
                activity_type: ActivityType::ApiCall,
                description: format!("API call to {} using {}", record.provider, record.model),
                amount: Some(record.cost),
                provider: Some(record.provider),
                model: Some(record.model),
            });
        }

        Ok(activities)
    }

    /// Get spending trends
    async fn get_spending_trends(&self, days: u32) -> Result<SpendingTrends> {
        let daily_trend = self.get_daily_trend(days).await?;
        
        // Calculate weekly average
        let total_spend: Decimal = daily_trend.iter().map(|d| d.amount).sum();
        let weekly_average = if days >= 7 {
            total_spend * Decimal::from(7) / Decimal::from(days)
        } else {
            total_spend / Decimal::from(days.max(1))
        };

        // Determine trend direction
        let (trend_direction, trend_percentage) = if daily_trend.len() >= 7 {
            let recent_avg = daily_trend[..7].iter().map(|d| d.amount).sum::<Decimal>() / Decimal::from(7);
            let older_avg = daily_trend[7..].iter().map(|d| d.amount).sum::<Decimal>() / Decimal::from((daily_trend.len() - 7).max(1));
            
            if recent_avg > older_avg * Decimal::new(105, 2) { // 5% increase
                (TrendDirection::Increasing, ((recent_avg - older_avg) / older_avg * Decimal::from(100)).to_f32().unwrap_or(0.0))
            } else if recent_avg < older_avg * Decimal::new(95, 2) { // 5% decrease
                (TrendDirection::Decreasing, ((older_avg - recent_avg) / older_avg * Decimal::from(100)).to_f32().unwrap_or(0.0))
            } else {
                (TrendDirection::Stable, 0.0)
            }
        } else {
            (TrendDirection::Stable, 0.0)
        };

        // Generate seasonal patterns (simplified)
        let seasonal_patterns = vec![]; // TODO: Implement seasonal analysis

        // Generate usage patterns
        let usage_patterns = self.get_usage_patterns().await?;

        Ok(SpendingTrends {
            daily_trend,
            weekly_average,
            trend_direction,
            trend_percentage,
            seasonal_patterns,
            usage_patterns,
        })
    }

    /// Get cost breakdown
    async fn get_cost_breakdown(&self) -> Result<CostBreakdown> {
        let now = Utc::now();
        let current_period = format!("{:04}-{:02}", now.year(), now.month());
        let billing_report = self.billing.generate_billing_report(&current_period).await?;

        let by_provider = billing_report.by_provider.iter()
            .map(|(k, v)| (k.clone(), v.total_cost))
            .collect();

        let by_model = billing_report.by_model.iter()
            .map(|(k, v)| (k.clone(), v.total_cost))
            .collect();

        // TODO: Implement more detailed breakdowns
        let by_token_type = HashMap::new();
        let by_request_type = HashMap::new();
        
        let mut verified_vs_unverified = HashMap::new();
        verified_vs_unverified.insert("Verified".to_string(), billing_report.verified_cost);
        verified_vs_unverified.insert("Unverified".to_string(), billing_report.unverified_cost);

        Ok(CostBreakdown {
            by_provider,
            by_model,
            by_token_type,
            by_request_type,
            verified_vs_unverified,
        })
    }

    /// Get dashboard alerts
    async fn get_dashboard_alerts(&self) -> Result<Vec<DashboardAlert>> {
        let mut alerts = Vec::new();

        // Check for budget warnings
        let overview = self.get_billing_overview().await?;
        if let Some(budget) = overview.current_month_budget {
            let utilization = overview.current_month_spend / budget * Decimal::from(100);
            let utilization_f32 = utilization.to_f32().unwrap_or(0.0);
            
            if utilization_f32 > 100.0 {
                alerts.push(DashboardAlert {
                    alert_type: AlertType::BudgetExceeded,
                    severity: AlertSeverity::Critical,
                    message: format!("Monthly budget exceeded by {:.1}%", utilization_f32 - 100.0),
                    timestamp: Utc::now(),
                    action_required: true,
                    details: Some("Consider reviewing spending limits or increasing budget".to_string()),
                });
            } else if utilization_f32 > 80.0 {
                alerts.push(DashboardAlert {
                    alert_type: AlertType::BudgetWarning,
                    severity: AlertSeverity::High,
                    message: format!("Monthly budget {:.1}% utilized", utilization_f32),
                    timestamp: Utc::now(),
                    action_required: false,
                    details: Some("Monitor spending closely for remainder of month".to_string()),
                });
            }
        }

        // Check for unverified costs
        let usage_stats = self.usage_repo.get_usage_statistics().await?;
        if usage_stats.total_cost > Decimal::ZERO {
            // TODO: Calculate unverified percentage
            // This would require tracking verification status in usage stats
        }

        Ok(alerts)
    }

    /// Get efficiency metrics
    async fn get_efficiency_metrics(&self) -> Result<EfficiencyMetrics> {
        let usage_stats = self.usage_repo.get_usage_statistics().await?;
        
        // Calculate cost per token (simplified)
        let total_tokens = usage_stats.total_input_tokens + usage_stats.total_output_tokens;
        let cost_per_token = if total_tokens > 0 {
            usage_stats.total_cost / Decimal::from(total_tokens)
        } else {
            Decimal::ZERO
        };

        // Calculate cost per successful request
        let cost_per_successful_request = if usage_stats.total_requests > 0 {
            usage_stats.total_cost / Decimal::from(usage_stats.total_requests)
        } else {
            Decimal::ZERO
        };

        // TODO: Implement more sophisticated efficiency calculations
        let verification_rate = 100.0; // Placeholder
        let provider_efficiency = HashMap::new();
        let model_efficiency = HashMap::new();
        let optimization_suggestions = Vec::new();

        Ok(EfficiencyMetrics {
            cost_per_token,
            cost_per_successful_request,
            verification_rate,
            provider_efficiency,
            model_efficiency,
            optimization_suggestions,
        })
    }

    /// Helper methods
    async fn get_period_spending(&self, period: &str) -> Result<Decimal> {
        let total_cost: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(CAST(cost AS REAL)), 0.0) FROM usage_records WHERE billing_period = ?"
        )
        .bind(period)
        .fetch_one(&self.pool)
        .await?;

        Ok(Decimal::try_from(total_cost).unwrap_or(Decimal::ZERO))
    }

    async fn get_period_request_count(&self, period: &str) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM usage_records WHERE billing_period = ?"
        )
        .bind(period)
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u64)
    }

    async fn get_most_expensive_day(&self, _period: &str) -> Result<Option<DailySpend>> {
        // This would require more complex date handling in SQL
        // For now, return None as placeholder
        Ok(None)
    }

    fn get_days_in_current_month(&self) -> u32 {
        let now = Utc::now();
        let next_month = if now.month() == 12 {
            NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
        };

        match next_month {
            Some(next) => {
                let current_month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
                (next - current_month_start).num_days() as u32
            }
            None => 31, // Fallback
        }
    }

    async fn get_daily_breakdown(&self, _period: &str) -> Result<Vec<DailySpend>> {
        // Placeholder implementation
        Ok(Vec::new())
    }

    async fn get_daily_trend(&self, _days: u32) -> Result<Vec<DailySpend>> {
        // Placeholder implementation
        Ok(Vec::new())
    }

    async fn get_usage_patterns(&self) -> Result<UsagePatterns> {
        // Placeholder implementation
        Ok(UsagePatterns {
            peak_hours: Vec::new(),
            peak_days: Vec::new(),
            average_session_cost: Decimal::ZERO,
            most_active_models: Vec::new(),
            efficiency_score: 75.0,
        })
    }

    /// Export dashboard data to various formats
    pub async fn export_report(&self, format: ExportFormat, _period: Option<String>) -> Result<String> {
        let dashboard_data = self.get_dashboard_data().await?;
        
        match format {
            ExportFormat::Json => Ok(serde_json::to_string_pretty(&dashboard_data)?),
            ExportFormat::Csv => {
                // Create CSV export (simplified)
                let mut csv = String::new();
                csv.push_str("Provider,Amount,Requests,Percentage\n");
                
                for provider in &dashboard_data.current_month.top_providers {
                    csv.push_str(&format!(
                        "{},{},{},{:.2}%\n",
                        provider.provider,
                        provider.amount,
                        provider.requests,
                        provider.percentage_of_total
                    ));
                }
                
                Ok(csv)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExportFormat {
    Json,
    Csv,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Database;
    use crate::platform::AppPaths;
    use tempfile::TempDir;

    async fn create_test_dashboard() -> (BillingDashboard, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::with_data_dir(temp_dir.path()).unwrap();
        let db = Database::new(&paths).await.unwrap();
        let pool = db.pool().clone();
        let billing = Arc::new(BillingSystem::new(pool.clone()));
        let usage_repo = Arc::new(UsageRepository::new(pool.clone()));
        let dashboard = BillingDashboard::new(pool, billing, usage_repo);
        (dashboard, temp_dir)
    }

    #[tokio::test]
    async fn test_dashboard_data_generation() {
        let (dashboard, _temp_dir) = create_test_dashboard().await;

        let data = dashboard.get_dashboard_data().await.unwrap();
        
        // Basic structure checks
        assert!(data.overview.current_month_spend >= Decimal::ZERO);
        assert!(data.current_month.total_spend >= Decimal::ZERO);
        assert!(data.efficiency_metrics.cost_per_token >= Decimal::ZERO);
    }

    #[tokio::test]
    async fn test_billing_overview() {
        let (dashboard, _temp_dir) = create_test_dashboard().await;

        let overview = dashboard.get_billing_overview().await.unwrap();
        
        assert!(overview.current_month_spend >= Decimal::ZERO);
        assert!(overview.previous_month_spend >= Decimal::ZERO);
        assert!(overview.total_requests_this_month >= 0);
        assert!(overview.projected_monthly_spend >= Decimal::ZERO);
    }

    #[tokio::test]
    async fn test_export_functionality() {
        let (dashboard, _temp_dir) = create_test_dashboard().await;

        let json_export = dashboard.export_report(ExportFormat::Json, None).await.unwrap();
        assert!(!json_export.is_empty());
        
        let csv_export = dashboard.export_report(ExportFormat::Csv, None).await.unwrap();
        assert!(csv_export.contains("Provider,Amount,Requests,Percentage"));
    }

    #[tokio::test]
    async fn test_efficiency_metrics() {
        let (dashboard, _temp_dir) = create_test_dashboard().await;

        let metrics = dashboard.get_efficiency_metrics().await.unwrap();
        
        assert!(metrics.cost_per_token >= Decimal::ZERO);
        assert!(metrics.cost_per_successful_request >= Decimal::ZERO);
        assert!(metrics.verification_rate >= 0.0 && metrics.verification_rate <= 100.0);
    }
}