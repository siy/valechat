import React, { useEffect, useState } from 'react';
import { useBillingStore } from '../../stores/billingStore';
import { useChatStore } from '../../stores/chatStore';
import './UsageDashboard.css';

interface UsageDashboardProps {
  isOpen: boolean;
  onClose: () => void;
}

const UsageDashboard: React.FC<UsageDashboardProps> = ({ isOpen, onClose }) => {
  const {
    usage_summary,
    usage_records,
    billing_limits,
    limit_alerts,
    loadUsageData,
    loadUsageSummary,
    getCurrentSpending,
    clearAlert,
  } = useBillingStore();
  
  const { conversations } = useChatStore();
  const [refreshInterval, setRefreshInterval] = useState<number | null>(null);
  const [activeTab, setActiveTab] = useState<'overview' | 'details' | 'trends'>('overview');

  useEffect(() => {
    if (isOpen) {
      // Load initial data
      loadUsageData();
      loadUsageSummary();
      
      // Set up auto-refresh every 30 seconds
      const interval = setInterval(() => {
        loadUsageData();
        loadUsageSummary();
      }, 30000);
      
      setRefreshInterval(interval);
      
      return () => {
        if (interval) clearInterval(interval);
      };
    } else if (refreshInterval) {
      clearInterval(refreshInterval);
      setRefreshInterval(null);
    }
  }, [isOpen, loadUsageData, loadUsageSummary]);

  const formatCurrency = (amount: string | number) => {
    const value = typeof amount === 'string' ? parseFloat(amount) : amount;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 4,
    }).format(value);
  };

  const formatTokens = (tokens: number) => {
    if (tokens >= 1000000) {
      return `${(tokens / 1000000).toFixed(1)}M`;
    } else if (tokens >= 1000) {
      return `${(tokens / 1000).toFixed(1)}K`;
    }
    return tokens.toString();
  };

  const getUsagePercentage = (current: string, limit: string) => {
    if (!limit || parseFloat(limit) === 0) return 0;
    return (parseFloat(current) / parseFloat(limit)) * 100;
  };

  const dailySpending = getCurrentSpending('daily');
  const monthlySpending = getCurrentSpending('monthly');
  const dailyPercentage = billing_limits.daily_limit 
    ? getUsagePercentage(dailySpending, billing_limits.daily_limit)
    : 0;
  const monthlyPercentage = billing_limits.monthly_limit 
    ? getUsagePercentage(monthlySpending, billing_limits.monthly_limit)
    : 0;

  if (!isOpen) return null;

  return (
    <div className="dashboard-overlay">
      <div className="dashboard-window">
        <div className="dashboard-header">
          <h2>💰 Usage Dashboard</h2>
          <div className="dashboard-header-actions">
            <div className="live-indicator">
              <span className="live-dot"></span>
              Live
            </div>
            <button className="btn-close" onClick={onClose}>✕</button>
          </div>
        </div>

        {/* Alerts */}
        {limit_alerts.length > 0 && (
          <div className="dashboard-alerts">
            {limit_alerts.map((alert) => (
              <div
                key={alert.id}
                className={`alert alert-${alert.severity}`}
              >
                <span className="alert-message">{alert.message}</span>
                <button
                  className="alert-dismiss"
                  onClick={() => clearAlert(alert.id)}
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="dashboard-content">
          {/* Tabs */}
          <div className="dashboard-tabs">
            <button
              className={`tab ${activeTab === 'overview' ? 'active' : ''}`}
              onClick={() => setActiveTab('overview')}
            >
              Overview
            </button>
            <button
              className={`tab ${activeTab === 'details' ? 'active' : ''}`}
              onClick={() => setActiveTab('details')}
            >
              Details
            </button>
            <button
              className={`tab ${activeTab === 'trends' ? 'active' : ''}`}
              onClick={() => setActiveTab('trends')}
            >
              Trends
            </button>
          </div>

          {/* Overview Tab */}
          {activeTab === 'overview' && (
            <div className="tab-content">
              {/* Key Metrics */}
              <div className="metrics-grid">
                <div className="metric-card">
                  <div className="metric-header">
                    <h3>Today's Spending</h3>
                    <span className="metric-change positive">Live</span>
                  </div>
                  <div className="metric-value">
                    {usage_summary ? formatCurrency(usage_summary.daily_cost) : '$0.0000'}
                  </div>
                  {billing_limits.daily_limit && (
                    <div className="metric-progress">
                      <div className="progress-bar">
                        <div 
                          className={`progress-fill ${dailyPercentage > 80 ? 'danger' : 'normal'}`}
                          style={{ width: `${Math.min(dailyPercentage, 100)}%` }}
                        />
                      </div>
                      <span className="progress-text">
                        {dailyPercentage.toFixed(1)}% of {formatCurrency(billing_limits.daily_limit)} limit
                      </span>
                    </div>
                  )}
                </div>

                <div className="metric-card">
                  <div className="metric-header">
                    <h3>Monthly Spending</h3>
                    <span className="metric-change">MTD</span>
                  </div>
                  <div className="metric-value">
                    {usage_summary ? formatCurrency(usage_summary.monthly_cost) : '$0.0000'}
                  </div>
                  {billing_limits.monthly_limit && (
                    <div className="metric-progress">
                      <div className="progress-bar">
                        <div 
                          className={`progress-fill ${monthlyPercentage > 80 ? 'danger' : 'normal'}`}
                          style={{ width: `${Math.min(monthlyPercentage, 100)}%` }}
                        />
                      </div>
                      <span className="progress-text">
                        {monthlyPercentage.toFixed(1)}% of {formatCurrency(billing_limits.monthly_limit)} limit
                      </span>
                    </div>
                  )}
                </div>

                <div className="metric-card">
                  <div className="metric-header">
                    <h3>Tokens Used</h3>
                    <span className="metric-change">Today</span>
                  </div>
                  <div className="metric-value">
                    {usage_summary ? formatTokens(usage_summary.daily_tokens) : '0'}
                  </div>
                  <div className="metric-subtitle">
                    {usage_summary ? formatTokens(usage_summary.monthly_tokens) : '0'} this month
                  </div>
                </div>

                <div className="metric-card">
                  <div className="metric-header">
                    <h3>Conversations</h3>
                    <span className="metric-change">Total</span>
                  </div>
                  <div className="metric-value">
                    {conversations.length}
                  </div>
                  <div className="metric-subtitle">
                    {conversations.filter(c => c.updated_at > Date.now() - 24 * 60 * 60 * 1000).length} active today
                  </div>
                </div>
              </div>

              {/* Top Models */}
              {usage_summary && usage_summary.top_models.length > 0 && (
                <div className="section">
                  <h3>Top Models This Month</h3>
                  <div className="model-usage-list">
                    {usage_summary.top_models.map((model, index) => (
                      <div key={index} className="model-usage-item">
                        <div className="model-info">
                          <div className="model-name">{model.model}</div>
                          <div className="model-provider">{model.provider}</div>
                        </div>
                        <div className="model-stats">
                          <div className="model-cost">{formatCurrency(model.cost)}</div>
                          <div className="model-tokens">{formatTokens(model.tokens)} tokens</div>
                        </div>
                        <div className="model-percentage">
                          <div className="percentage-bar">
                            <div 
                              className="percentage-fill"
                              style={{ width: `${model.percentage}%` }}
                            />
                          </div>
                          <span>{model.percentage.toFixed(1)}%</span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Details Tab */}
          {activeTab === 'details' && (
            <div className="tab-content">
              <div className="section">
                <h3>Recent Usage</h3>
                <div className="usage-table">
                  <div className="table-header">
                    <div>Time</div>
                    <div>Model</div>
                    <div>Provider</div>
                    <div>Tokens</div>
                    <div>Cost</div>
                  </div>
                  {usage_records.slice(0, 20).map((record) => (
                    <div key={record.id || record.request_id} className="table-row">
                      <div className="timestamp">
                        {new Date(record.timestamp).toLocaleTimeString()}
                      </div>
                      <div className="model">{record.model}</div>
                      <div className="provider">{record.provider}</div>
                      <div className="tokens">
                        {(record.input_tokens + record.output_tokens).toLocaleString()}
                      </div>
                      <div className="cost">{formatCurrency(record.cost)}</div>
                    </div>
                  ))}
                </div>
                {usage_records.length === 0 && (
                  <div className="empty-state">
                    <p>No usage data available</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Trends Tab */}
          {activeTab === 'trends' && (
            <div className="tab-content">
              {usage_summary && usage_summary.cost_trend.length > 0 && (
                <div className="section">
                  <h3>Cost Trend</h3>
                  <div className="trend-chart">
                    {usage_summary.cost_trend.map((point, index) => (
                      <div key={index} className="trend-point">
                        <div className="trend-date">
                          {new Date(point.date).toLocaleDateString('en', { month: 'short', day: 'numeric' })}
                        </div>
                        <div className="trend-bar">
                          <div 
                            className="trend-fill"
                            style={{ 
                              height: `${(parseFloat(point.cost) / Math.max(...usage_summary.cost_trend.map(p => parseFloat(p.cost)))) * 100}%` 
                            }}
                          />
                        </div>
                        <div className="trend-value">{formatCurrency(point.cost)}</div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              
              {/* Provider Breakdown */}
              {usage_records.length > 0 && (
                <div className="section">
                  <h3>Provider Usage</h3>
                  <div className="provider-breakdown">
                    {Array.from(new Set(usage_records.map(r => r.provider))).map(provider => {
                      const providerRecords = usage_records.filter(r => r.provider === provider);
                      const totalCost = providerRecords.reduce((sum, r) => sum + parseFloat(r.cost), 0);
                      const totalTokens = providerRecords.reduce((sum, r) => sum + r.input_tokens + r.output_tokens, 0);
                      
                      return (
                        <div key={provider} className="provider-item">
                          <div className="provider-name">{provider}</div>
                          <div className="provider-stats">
                            <div>{formatCurrency(totalCost)}</div>
                            <div>{formatTokens(totalTokens)} tokens</div>
                            <div>{providerRecords.length} requests</div>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default UsageDashboard;