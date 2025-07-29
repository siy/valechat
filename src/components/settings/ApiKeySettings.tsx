import React, { useState, useEffect } from 'react';
import { useApiKeyStore } from '../../stores/apiKeyStore';
import { useSettingsStore } from '../../stores/settingsStore';
import './ApiKeySettings.css';

const ApiKeySettings: React.FC = () => {
  const { 
    apiKeys, 
    isLoading, 
    errors, 
    setApiKey, 
    removeApiKey, 
    loadApiKeys, 
    clearErrors 
  } = useApiKeyStore();
  
  const { config } = useSettingsStore();
  
  const [localKeys, setLocalKeys] = useState<Record<string, string>>({});
  const [showKeys, setShowKeys] = useState<Record<string, boolean>>({});

  useEffect(() => {
    // Load existing API keys on mount
    const providers = config.model_providers.map((p: any) => p.id);
    loadApiKeys(providers);
  }, [config.model_providers, loadApiKeys]);

  useEffect(() => {
    // Update local state when store changes
    setLocalKeys({ ...apiKeys });
  }, [apiKeys]);

  const handleKeyChange = (provider: string, value: string) => {
    setLocalKeys(prev => ({
      ...prev,
      [provider]: value
    }));
  };

  const handleSaveKey = async (provider: string) => {
    const key = localKeys[provider]?.trim();
    if (!key) return;

    try {
      await setApiKey(provider, key);
    } catch (error) {
      console.error('Failed to save API key:', error);
    }
  };

  const handleRemoveKey = async (provider: string) => {
    if (confirm(`Remove API key for ${provider}?`)) {
      try {
        await removeApiKey(provider);
        setLocalKeys(prev => {
          const updated = { ...prev };
          delete updated[provider];
          return updated;
        });
      } catch (error) {
        console.error('Failed to remove API key:', error);
      }
    }
  };

  const toggleShowKey = (provider: string) => {
    setShowKeys(prev => ({
      ...prev,
      [provider]: !prev[provider]
    }));
  };

  const maskKey = (key: string) => {
    if (!key) return '';
    if (key.length <= 8) return '*'.repeat(key.length);
    return key.substring(0, 4) + '*'.repeat(key.length - 8) + key.substring(key.length - 4);
  };

  return (
    <div className="api-key-settings">
      <h3>API Key Management</h3>
      
      {errors.length > 0 && (
        <div className="error-messages">
          {errors.map((error, index) => (
            <div key={index} className="error-message">
              {error}
            </div>
          ))}
          <button onClick={clearErrors} className="clear-errors-btn">
            Clear Errors
          </button>
        </div>
      )}

      <div className="api-key-list">
        {config.model_providers.map((provider: any) => {
          const hasKey = apiKeys[provider.id];
          const localValue = localKeys[provider.id] || '';
          const isModified = localValue !== (apiKeys[provider.id] || '');
          
          return (
            <div key={provider.id} className="api-key-item">
              <div className="provider-info">
                <h4>{provider.name}</h4>
                <span className={`status ${hasKey ? 'configured' : 'missing'}`}>
                  {hasKey ? 'Configured' : 'Not configured'}
                </span>
              </div>
              
              <div className="key-input-group">
                <div className="input-container">
                  <input
                    type={showKeys[provider.id] ? 'text' : 'password'}
                    value={localValue}
                    onChange={(e) => handleKeyChange(provider.id, e.target.value)}
                    placeholder={hasKey ? maskKey(apiKeys[provider.id]) : `Enter ${provider.name} API key`}
                    className="api-key-input"
                    disabled={isLoading}
                  />
                  <button
                    type="button"
                    onClick={() => toggleShowKey(provider.id)}
                    className="toggle-visibility-btn"
                    title={showKeys[provider.id] ? 'Hide key' : 'Show key'}
                  >
                    {showKeys[provider.id] ? '👁️' : '👁️‍🗨️'}
                  </button>
                </div>
                
                <div className="key-actions">
                  <button
                    onClick={() => handleSaveKey(provider.id)}
                    disabled={isLoading || !localValue.trim() || !isModified}
                    className="save-key-btn"
                  >
                    {hasKey ? 'Update' : 'Save'}
                  </button>
                  
                  {hasKey && (
                    <button
                      onClick={() => handleRemoveKey(provider.id)}
                      disabled={isLoading}
                      className="remove-key-btn"
                    >
                      Remove
                    </button>
                  )}
                </div>
              </div>
              
              <div className="key-help">
                <p>
                  Get your API key from{' '}
                  <a 
                    href={getProviderUrl(provider.id)} 
                    target="_blank" 
                    rel="noopener noreferrer"
                  >
                    {provider.name}
                  </a>
                </p>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

function getProviderUrl(providerId: string): string {
  switch (providerId) {
    case 'openai':
      return 'https://platform.openai.com/api-keys';
    case 'anthropic':
      return 'https://console.anthropic.com/';
    case 'google':
      return 'https://aistudio.google.com/app/apikey';
    default:
      return '#';
  }
}

export default ApiKeySettings;