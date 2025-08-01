use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline},
    Frame,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::HashMap;

use valechat::storage::{CostAlert, AlertSeverity, UsageStatistics};

/// Cost tracking display component
pub struct CostTracker {
    /// Current usage statistics
    stats: Option<UsageStatistics>,
    /// Recent cost alerts
    alerts: Vec<CostAlert>,
    /// Daily spending trend (last 7 days)
    daily_trend: Vec<u64>,
    /// Current day's spending
    today_spending: Decimal,
    /// Daily limit (if set)
    daily_limit: Option<Decimal>,
    /// Monthly spending
    monthly_spending: Decimal,
    /// Monthly limit (if set)
    monthly_limit: Option<Decimal>,
    /// Provider breakdown
    provider_breakdown: HashMap<String, Decimal>,
    /// Show detailed view
    pub show_details: bool,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            stats: None,
            alerts: Vec::new(),
            daily_trend: Vec::new(),
            today_spending: Decimal::ZERO,
            daily_limit: None,
            monthly_spending: Decimal::ZERO,
            monthly_limit: None,
            provider_breakdown: HashMap::new(),
            show_details: false,
        }
    }

    /// Update with new statistics
    pub fn update_stats(&mut self, stats: UsageStatistics) {
        self.monthly_spending = stats.current_month_cost;
        self.stats = Some(stats);
    }

    /// Update daily spending and trend
    pub fn update_daily_data(&mut self, today: Decimal, trend: Vec<u64>) {
        self.today_spending = today;
        self.daily_trend = trend;
    }

    /// Update spending limits
    pub fn update_limits(&mut self, daily: Option<Decimal>, monthly: Option<Decimal>) {
        self.daily_limit = daily;
        self.monthly_limit = monthly;
    }

    /// Update provider breakdown
    pub fn update_provider_breakdown(&mut self, breakdown: HashMap<String, Decimal>) {
        self.provider_breakdown = breakdown;
    }

    /// Add new alert
    pub fn add_alert(&mut self, alert: CostAlert) {
        // Keep only recent alerts
        self.alerts.push(alert);
        if self.alerts.len() > 10 {
            self.alerts.remove(0);
        }
    }

    /// Toggle detailed view
    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    /// Render the cost tracker
    pub fn render(&self, f: &mut Frame, area: Rect) {
        if self.show_details {
            self.render_detailed_view(f, area);
        } else {
            self.render_compact_view(f, area);
        }
    }

    /// Render compact view for status bar
    fn render_compact_view(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(25), // Daily spending
                Constraint::Length(25), // Monthly spending
                Constraint::Min(20),    // Alerts
            ])
            .split(area);

        // Daily spending with gauge
        self.render_daily_spending(f, chunks[0]);

        // Monthly spending with gauge
        self.render_monthly_spending(f, chunks[1]);

        // Active alerts
        self.render_alerts_compact(f, chunks[2]);
    }

    /// Render detailed view
    fn render_detailed_view(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Spending overview
                Constraint::Length(8),  // Provider breakdown
                Constraint::Length(6),  // Trend chart
                Constraint::Min(6),     // Alerts
            ])
            .split(area);

        // Spending overview
        self.render_spending_overview(f, chunks[0]);

        // Provider breakdown
        self.render_provider_breakdown(f, chunks[1]);

        // Cost trend
        self.render_cost_trend(f, chunks[2]);

        // Detailed alerts
        self.render_alerts_detailed(f, chunks[3]);
    }

    fn render_daily_spending(&self, f: &mut Frame, area: Rect) {
        let ratio = if let Some(limit) = self.daily_limit {
            if limit > Decimal::ZERO {
                (self.today_spending / limit).to_f64().unwrap_or(0.0).min(1.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        let color = if ratio > 0.9 {
            Color::Red
        } else if ratio > 0.7 {
            Color::Yellow
        } else {
            Color::Green
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Daily"))
            .gauge_style(Style::default().fg(color))
            .ratio(ratio)
            .label(format!("${:.2}", self.today_spending.to_f64().unwrap_or(0.0)));

        f.render_widget(gauge, area);
    }

    fn render_monthly_spending(&self, f: &mut Frame, area: Rect) {
        let ratio = if let Some(limit) = self.monthly_limit {
            if limit > Decimal::ZERO {
                (self.monthly_spending / limit).to_f64().unwrap_or(0.0).min(1.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        let color = if ratio > 0.9 {
            Color::Red
        } else if ratio > 0.7 {
            Color::Yellow
        } else {
            Color::Green
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Monthly"))
            .gauge_style(Style::default().fg(color))
            .ratio(ratio)
            .label(format!("${:.2}", self.monthly_spending.to_f64().unwrap_or(0.0)));

        f.render_widget(gauge, area);
    }

    fn render_alerts_compact(&self, f: &mut Frame, area: Rect) {
        let alert_count = self.alerts.len();
        let critical_count = self.alerts.iter()
            .filter(|a| matches!(a.severity, AlertSeverity::Critical | AlertSeverity::Emergency))
            .count();

        let color = if critical_count > 0 {
            Color::Red
        } else if alert_count > 0 {
            Color::Yellow
        } else {
            Color::Green
        };

        let text = if alert_count > 0 {
            format!("🚨 {} alerts", alert_count)
        } else {
            "✅ No alerts".to_string()
        };

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(color))
            .block(Block::default().borders(Borders::ALL).title("Alerts"));

        f.render_widget(paragraph, area);
    }

    fn render_spending_overview(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Daily spending details
        let daily_text = vec![
            Line::from(vec![
                Span::styled("Today: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::from(format!("${:.2}", self.today_spending.to_f64().unwrap_or(0.0))),
            ]),
            Line::from(vec![
                Span::styled("Limit: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::from(match self.daily_limit {
                    Some(limit) => format!("${:.2}", limit.to_f64().unwrap_or(0.0)),
                    None => "Not set".to_string(),
                }),
            ]),
            Line::from(vec![
                Span::styled("Remaining: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::from(match self.daily_limit {
                    Some(limit) => {
                        let remaining = limit - self.today_spending;
                        if remaining >= Decimal::ZERO {
                            format!("${:.2}", remaining.to_f64().unwrap_or(0.0))
                        } else {
                            format!("-${:.2}", (-remaining).to_f64().unwrap_or(0.0))
                        }
                    }
                    None => "∞".to_string(),
                }),
            ]),
        ];

        let daily_paragraph = Paragraph::new(daily_text)
            .block(Block::default().borders(Borders::ALL).title("Daily Spending"));
        f.render_widget(daily_paragraph, chunks[0]);

        // Monthly spending details
        let monthly_text = vec![
            Line::from(vec![
                Span::styled("This month: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::from(format!("${:.2}", self.monthly_spending.to_f64().unwrap_or(0.0))),
            ]),
            Line::from(vec![
                Span::styled("Limit: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::from(match self.monthly_limit {
                    Some(limit) => format!("${:.2}", limit.to_f64().unwrap_or(0.0)),
                    None => "Not set".to_string(),
                }),
            ]),
            Line::from(vec![
                Span::styled("Remaining: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::from(match self.monthly_limit {
                    Some(limit) => {
                        let remaining = limit - self.monthly_spending;
                        if remaining >= Decimal::ZERO {
                            format!("${:.2}", remaining.to_f64().unwrap_or(0.0))
                        } else {
                            format!("-${:.2}", (-remaining).to_f64().unwrap_or(0.0))
                        }
                    }
                    None => "∞".to_string(),
                }),
            ]),
        ];

        let monthly_paragraph = Paragraph::new(monthly_text)
            .block(Block::default().borders(Borders::ALL).title("Monthly Spending"));
        f.render_widget(monthly_paragraph, chunks[1]);
    }

    fn render_provider_breakdown(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.provider_breakdown
            .iter()
            .map(|(provider, cost)| {
                let total = self.monthly_spending.to_f64().unwrap_or(1.0);
                let cost_f64 = cost.to_f64().unwrap_or(0.0);
                let percentage = if total > 0.0 { (cost_f64 / total) * 100.0 } else { 0.0 };
                
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:10}", provider),
                        Style::default().add_modifier(Modifier::BOLD)
                    ),
                    Span::from(format!(" ${:8.2} ({:5.1}%)", cost_f64, percentage)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Provider Breakdown"));

        f.render_widget(list, area);
    }

    fn render_cost_trend(&self, f: &mut Frame, area: Rect) {
        let sparkline = Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title("7-Day Trend"))
            .data(&self.daily_trend)
            .style(Style::default().fg(Color::Cyan));

        f.render_widget(sparkline, area);
    }

    fn render_alerts_detailed(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.alerts
            .iter()
            .rev() // Show most recent first
            .take(10)
            .map(|alert| {
                let severity_symbol = match alert.severity {
                    AlertSeverity::Emergency => "🚨",
                    AlertSeverity::Critical => "❌",
                    AlertSeverity::Warning => "⚠️",
                    AlertSeverity::Info => "ℹ️",
                };

                let color = match alert.severity {
                    AlertSeverity::Emergency => Color::Red,
                    AlertSeverity::Critical => Color::Red,
                    AlertSeverity::Warning => Color::Yellow,
                    AlertSeverity::Info => Color::Blue,
                };

                ListItem::new(Line::from(vec![
                    Span::styled(severity_symbol, Style::default().fg(color)),
                    Span::from(" "),
                    Span::from(alert.message.clone()),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Recent Alerts"));

        f.render_widget(list, area);
    }

    /// Get summary for status bar
    pub fn get_status_summary(&self) -> String {
        let daily = self.today_spending.to_f64().unwrap_or(0.0);
        let monthly = self.monthly_spending.to_f64().unwrap_or(0.0);
        let alert_count = self.alerts.len();

        if alert_count > 0 {
            format!("💰 ${:.2}/day ${:.2}/mo 🚨{}", daily, monthly, alert_count)
        } else {
            format!("💰 ${:.2}/day ${:.2}/mo", daily, monthly)
        }
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}