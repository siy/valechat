import React, { useState, useEffect } from 'react';
import { useBillingStore } from '../../stores/billingStore';

interface BillingSettingsProps {
  onSettingsChange: () => void;
}

const BillingSettings: React.FC<BillingSettingsProps> = ({ onSettingsChange }) => {
  const {
    usage_summary,
    billing_limits,
    limit_alerts,
    updateBillingLimits,
    clearAlert,
    exportUsageData,
    getCurrentSpending,
  } = useBillingStore();
  
  const [exportFormat, setExportFormat] = useState<'csv' | 'json'>('csv');
  const [exportPeriod, setExportPeriod] = useState('month');

  const [limits, setLimits] = useState({
    daily_limit: billing_limits.daily_limit || '',
    monthly_limit: billing_limits.monthly_limit || '',
  });

  useEffect(() => {
    setLimits({
      daily_limit: billing_limits.daily_limit || '',
      monthly_limit: billing_limits.monthly_limit || '',
    });
  }, [billing_limits]);

  const handleLimitChange = (type: 'daily_limit' | 'monthly_limit', value: string) => {
    setLimits(prev => ({ ...prev, [type]: value }));
  };

  const handleSaveLimits = () => {
    const updates: any = {};
    if (limits.daily_limit) updates.daily_limit = limits.daily_limit;
    if (limits.monthly_limit) updates.monthly_limit = limits.monthly_limit;
    
    updateBillingLimits(updates);
    onSettingsChange();
  };

  const handleExport = async () => {
    try {
      await exportUsageData(exportFormat, exportPeriod);
    } catch (error) {
      console.error('Export failed:', error);
    }
  };

  const formatCurrency = (amount: string) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 4,
    }).format(parseFloat(amount));
  };

  const dailySpending = getCurrentSpending('daily');
  const monthlySpending = getCurrentSpending('monthly');
  const dailyLimit = parseFloat(limits.daily_limit || '0');
  const monthlyLimit = parseFloat(limits.monthly_limit || '0');

  return (
    <div className="settings-section">
      <h3 className="settings-section-title">Billing & Usage</h3>

      {/* Alerts */}
      {limit_alerts.length > 0 && (
        <div className="settings-group">
          <h4 style={{ margin: '0 0 12px 0', color: 'var(--text-primary)' }}>Alerts</h4>
          {limit_alerts.map((alert) => (
            <div
              key={alert.id}
              style={{
                padding: '12px',
                background: alert.severity === 'error' ? '#fee2e2' : '#fff3cd',
                color: alert.severity === 'error' ? '#991b1b' : '#856404',
                border: `1px solid ${alert.severity === 'error' ? '#fecaca' : '#ffeeba'}`,
                borderRadius: '6px',
                marginBottom: '8px',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <span>{alert.message}</span>
              <button
                onClick={() => clearAlert(alert.id)}
                style={{
                  background: 'transparent',
                  border: 'none',
                  color: 'inherit',
                  cursor: 'pointer',
                  fontSize: '16px',
                }}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Current Usage */}
      <div className="settings-group">
        <h4 style={{ margin: '0 0 16px 0', color: 'var(--text-primary)' }}>Current Usage</h4>
        
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '16px' }}>
          <div style={{
            padding: '16px',
            background: 'var(--bg-primary)',
            border: '1px solid var(--border-color)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '24px', fontWeight: 600, color: 'var(--text-primary)' }}>
              {usage_summary ? formatCurrency(usage_summary.daily_cost) : '$0.0000'}
            </div>
            <div style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>Today</div>
            {dailyLimit > 0 && (
              <div style={{ marginTop: '8px' }}>
                <div style={{
                  width: '100%',
                  height: '4px',
                  background: 'var(--border-color)',
                  borderRadius: '2px',
                  overflow: 'hidden',
                }}>
                  <div
                    style={{
                      width: `${Math.min((parseFloat(dailySpending) / dailyLimit) * 100, 100)}%`,
                      height: '100%',
                      background: parseFloat(dailySpending) / dailyLimit > 0.8 ? '#ef4444' : '#3b82f6',
                      transition: 'width 0.3s ease',
                    }}
                  />
                </div>
                <div style={{ fontSize: '12px', color: 'var(--text-secondary)', marginTop: '4px' }}>
                  {((parseFloat(dailySpending) / dailyLimit) * 100).toFixed(1)}% of ${dailyLimit.toFixed(2)} limit
                </div>
              </div>
            )}
          </div>

          <div style={{
            padding: '16px',
            background: 'var(--bg-primary)',
            border: '1px solid var(--border-color)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '24px', fontWeight: 600, color: 'var(--text-primary)' }}>
              {usage_summary ? formatCurrency(usage_summary.monthly_cost) : '$0.0000'}
            </div>
            <div style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>This Month</div>
            {monthlyLimit > 0 && (
              <div style={{ marginTop: '8px' }}>
                <div style={{
                  width: '100%',
                  height: '4px',
                  background: 'var(--border-color)',
                  borderRadius: '2px',
                  overflow: 'hidden',
                }}>
                  <div
                    style={{
                      width: `${Math.min((parseFloat(monthlySpending) / monthlyLimit) * 100, 100)}%`,
                      height: '100%',
                      background: parseFloat(monthlySpending) / monthlyLimit > 0.8 ? '#ef4444' : '#3b82f6',
                      transition: 'width 0.3s ease',
                    }}
                  />
                </div>
                <div style={{ fontSize: '12px', color: 'var(--text-secondary)', marginTop: '4px' }}>
                  {((parseFloat(monthlySpending) / monthlyLimit) * 100).toFixed(1)}% of ${monthlyLimit.toFixed(2)} limit
                </div>
              </div>
            )}
          </div>

          <div style={{
            padding: '16px',
            background: 'var(--bg-primary)',
            border: '1px solid var(--border-color)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '24px', fontWeight: 600, color: 'var(--text-primary)' }}>
              {usage_summary ? usage_summary.daily_tokens.toLocaleString() : '0'}
            </div>
            <div style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>Tokens Today</div>
          </div>

          <div style={{
            padding: '16px',
            background: 'var(--bg-primary)',
            border: '1px solid var(--border-color)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '24px', fontWeight: 600, color: 'var(--text-primary)' }}>
              {usage_summary ? usage_summary.monthly_tokens.toLocaleString() : '0'}
            </div>
            <div style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>Tokens This Month</div>
          </div>
        </div>
      </div>

      {/* Spending Limits */}
      <div className="settings-group">
        <h4 style={{ margin: '0 0 16px 0', color: 'var(--text-primary)' }}>Spending Limits</h4>
        
        <div className="settings-row">
          <div className="settings-label">
            <h4>Daily Limit</h4>
            <p>Maximum spending per day (USD)</p>
          </div>
          <div className="settings-control">
            <input
              type="number"
              className="form-input"
              placeholder="0.00"
              step="0.01"
              min="0"
              value={limits.daily_limit}
              onChange={(e) => handleLimitChange('daily_limit', e.target.value)}
            />
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-label">
            <h4>Monthly Limit</h4>
            <p>Maximum spending per month (USD)</p>
          </div>
          <div className="settings-control">
            <input
              type="number"
              className="form-input"
              placeholder="0.00"
              step="0.01"
              min="0"
              value={limits.monthly_limit}
              onChange={(e) => handleLimitChange('monthly_limit', e.target.value)}
            />
          </div>
        </div>

        <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: '16px' }}>
          <button className="btn btn-primary" onClick={handleSaveLimits}>
            Save Limits
          </button>
        </div>
      </div>

      {/* Top Models */}
      {usage_summary && usage_summary.top_models.length > 0 && (
        <div className="settings-group">
          <h4 style={{ margin: '0 0 16px 0', color: 'var(--text-primary)' }}>Top Models This Month</h4>
          <div style={{ display: 'grid', gap: '8px' }}>
            {usage_summary.top_models.map((model, index) => (
              <div
                key={index}
                style={{
                  padding: '12px',
                  background: 'var(--bg-primary)',
                  border: '1px solid var(--border-color)',
                  borderRadius: '6px',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                }}
              >
                <div>
                  <div style={{ fontWeight: 500, color: 'var(--text-primary)' }}>
                    {model.model}
                  </div>
                  <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>
                    {model.provider} • {model.tokens.toLocaleString()} tokens
                  </div>
                </div>
                <div style={{ textAlign: 'right' }}>
                  <div style={{ fontWeight: 500, color: 'var(--text-primary)' }}>
                    {formatCurrency(model.cost)}
                  </div>
                  <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>
                    {model.percentage.toFixed(1)}%
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Export Data */}
      <div className="settings-group">
        <h4 style={{ margin: '0 0 16px 0', color: 'var(--text-primary)' }}>Export Usage Data</h4>
        
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr auto', gap: '12px', alignItems: 'end' }}>
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '14px', fontWeight: 500 }}>
              Format
            </label>
            <select
              className="form-select"
              value={exportFormat}
              onChange={(e) => setExportFormat(e.target.value as 'csv' | 'json')}
            >
              <option value="csv">CSV</option>
              <option value="json">JSON</option>
            </select>
          </div>
          
          <div>
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '14px', fontWeight: 500 }}>
              Period
            </label>
            <select
              className="form-select"
              value={exportPeriod}
              onChange={(e) => setExportPeriod(e.target.value)}
            >
              <option value="week">Last Week</option>
              <option value="month">Last Month</option>
              <option value="quarter">Last Quarter</option>
              <option value="year">Last Year</option>
              <option value="all">All Time</option>
            </select>
          </div>
          
          <button className="btn btn-primary" onClick={handleExport}>
            Export
          </button>
        </div>
      </div>
    </div>
  );
};

export default BillingSettings;