import React, { useState, useEffect } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';
import { useApiKeyStore } from '../../stores/apiKeyStore';
import { ModelProvider } from '../../types';

interface ModelProviderSettingsProps {
  onSettingsChange: () => void;
}

const ModelProviderSettings: React.FC<ModelProviderSettingsProps> = ({ onSettingsChange }) => {
  const { config, updateModelProvider } = useSettingsStore();
  const { apiKeys, setApiKey, loadApiKeys } = useApiKeyStore();
  const [expandedProvider, setExpandedProvider] = useState<string | null>(null);
  const [localKeys, setLocalKeys] = useState<Record<string, string>>({});

  useEffect(() => {
    // Load API keys when component mounts
    const providers = config.model_providers.map(p => p.id);
    loadApiKeys(providers);
  }, [config.model_providers, loadApiKeys]);

  useEffect(() => {
    // Update local state when API keys change
    setLocalKeys({ ...apiKeys });
  }, [apiKeys]);

  const handleProviderToggle = (providerId: string, enabled: boolean) => {
    updateModelProvider(providerId, { enabled });
    onSettingsChange();
  };

  const handleConfigUpdate = (providerId: string, key: string, value: string) => {
    const provider = config.model_providers.find(p => p.id === providerId);
    if (provider) {
      const newConfig = { ...provider.config, [key]: value };
      updateModelProvider(providerId, { config: newConfig });
      onSettingsChange();
    }
  };

  const toggleProviderExpansion = (providerId: string) => {
    setExpandedProvider(expandedProvider === providerId ? null : providerId);
  };

  const getProviderStatusColor = (provider: ModelProvider) => {
    if (!provider.enabled) return 'var(--text-secondary)';
    
    // Check if API key exists in secure storage
    const hasApiKey = apiKeys[provider.id] && apiKeys[provider.id].length > 0;
    return hasApiKey ? '#16a34a' : '#ea580c';
  };

  const getProviderStatusText = (provider: ModelProvider) => {
    if (!provider.enabled) return 'Disabled';
    
    const hasApiKey = apiKeys[provider.id] && apiKeys[provider.id].length > 0;
    return hasApiKey ? 'Configured' : 'Missing API Key';
  };

  const handleApiKeyChange = (providerId: string, value: string) => {
    setLocalKeys(prev => ({
      ...prev,
      [providerId]: value
    }));
  };

  const handleApiKeySave = async (providerId: string) => {
    const key = localKeys[providerId]?.trim();
    if (!key) return;

    try {
      await setApiKey(providerId, key);
      onSettingsChange();
    } catch (error) {
      console.error('Failed to save API key:', error);
    }
  };

  return (
    <div className="settings-section">
      <h3 className="settings-section-title">Model Providers</h3>
      <p style={{ color: 'var(--text-secondary)', marginBottom: '24px', fontSize: '14px' }}>
        Configure AI model providers to enable chat functionality. Each provider requires an API key.
      </p>

      {config.model_providers.map((provider) => (
        <div key={provider.id} className="settings-group">
          <div className="settings-row">
            <div className="settings-label">
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <h4>{provider.name}</h4>
                <span
                  className="status-indicator"
                  style={{
                    background: provider.enabled ? '#dcfce7' : '#f3f4f6',
                    color: getProviderStatusColor(provider),
                  }}
                >
                  ●
                  {getProviderStatusText(provider)}
                </span>
              </div>
              <p>{provider.models.length} models available</p>
            </div>
            <div className="settings-control" style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
              <label className="form-switch">
                <input
                  type="checkbox"
                  checked={provider.enabled}
                  onChange={(e) => handleProviderToggle(provider.id, e.target.checked)}
                />
                <span className="form-switch-slider"></span>
              </label>
              <button
                className="btn btn-small"
                onClick={() => toggleProviderExpansion(provider.id)}
              >
                {expandedProvider === provider.id ? '▼' : '▶'}
              </button>
            </div>
          </div>

          {expandedProvider === provider.id && (
            <div style={{ marginTop: '16px', paddingTop: '16px', borderTop: '1px solid var(--border-color)' }}>
              <div className="settings-row">
                <div className="settings-label">
                  <h4>API Key</h4>
                  <p>Your API key for {provider.name} (stored securely)</p>
                </div>
                <div className="settings-control" style={{ display: 'flex', gap: '8px' }}>
                  <input
                    type="password"
                    className="form-input"
                    placeholder={apiKeys[provider.id] ? "••••••••••••" : "Enter API key..."}
                    value={localKeys[provider.id] || ''}
                    onChange={(e) => handleApiKeyChange(provider.id, e.target.value)}
                    style={{ flex: 1 }}
                  />
                  <button
                    className="btn btn-small"
                    onClick={() => handleApiKeySave(provider.id)}
                    disabled={!localKeys[provider.id] || localKeys[provider.id] === apiKeys[provider.id]}
                  >
                    {apiKeys[provider.id] ? 'Update' : 'Save'}
                  </button>
                </div>
              </div>

              {provider.type === 'openai' && (
                <>
                  <div className="settings-row">
                    <div className="settings-label">
                      <h4>Base URL</h4>
                      <p>API endpoint URL (optional, for custom deployments)</p>
                    </div>
                    <div className="settings-control">
                      <input
                        type="url"
                        className="form-input"
                        placeholder="https://api.openai.com/v1"
                        value={provider.config.base_url || ''}
                        onChange={(e) => handleConfigUpdate(provider.id, 'base_url', e.target.value)}
                      />
                    </div>
                  </div>

                  <div className="settings-row">
                    <div className="settings-label">
                      <h4>Organization</h4>
                      <p>Organization ID (optional)</p>
                    </div>
                    <div className="settings-control">
                      <input
                        type="text"
                        className="form-input"
                        placeholder="org-..."
                        value={provider.config.organization || ''}
                        onChange={(e) => handleConfigUpdate(provider.id, 'organization', e.target.value)}
                      />
                    </div>
                  </div>
                </>
              )}

              {provider.type === 'anthropic' && (
                <div className="settings-row">
                  <div className="settings-label">
                    <h4>Base URL</h4>
                    <p>API endpoint URL (optional, for custom deployments)</p>
                  </div>
                  <div className="settings-control">
                    <input
                      type="url"
                      className="form-input"
                      placeholder="https://api.anthropic.com"
                      value={provider.config.base_url || ''}
                      onChange={(e) => handleConfigUpdate(provider.id, 'base_url', e.target.value)}
                    />
                  </div>
                </div>
              )}

              <div style={{ marginTop: '16px' }}>
                <h4 style={{ marginBottom: '12px', color: 'var(--text-primary)' }}>Available Models</h4>
                <div style={{ display: 'grid', gap: '8px' }}>
                  {provider.models.map((model) => (
                    <div
                      key={model.id}
                      style={{
                        padding: '12px',
                        background: 'var(--bg-primary)',
                        border: '1px solid var(--border-color)',
                        borderRadius: '6px',
                        fontSize: '14px',
                      }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                        <div>
                          <div style={{ fontWeight: 500, color: 'var(--text-primary)' }}>
                            {model.display_name}
                          </div>
                          <div style={{ color: 'var(--text-secondary)', fontSize: '12px' }}>
                            Context: {model.context_length.toLocaleString()} tokens
                            {model.supports_streaming && ' • Streaming'}
                          </div>
                        </div>
                        {model.input_price_per_1k && (
                          <div style={{ color: 'var(--text-secondary)', fontSize: '12px', textAlign: 'right' }}>
                            <div>${model.input_price_per_1k}/1K in</div>
                            <div>${model.output_price_per_1k}/1K out</div>
                          </div>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
};

export default ModelProviderSettings;