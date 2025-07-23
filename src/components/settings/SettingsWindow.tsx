import React, { useState, useEffect } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';
import { useBillingStore } from '../../stores/billingStore';
import ModelProviderSettings from './ModelProviderSettings';
import MCPServerSettings from './MCPServerSettings';
import BillingSettings from './BillingSettings';
import GeneralSettings from './GeneralSettings';
import './SettingsWindow.css';

interface SettingsWindowProps {
  isOpen: boolean;
  onClose: () => void;
}

type SettingsTab = 'general' | 'models' | 'mcp' | 'billing';

const SettingsWindow: React.FC<SettingsWindowProps> = ({ isOpen, onClose }) => {
  const [activeTab, setActiveTab] = useState<SettingsTab>('general');
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);
  const { loadConfig, saveConfig, isLoading } = useSettingsStore();
  const { loadUsageData, loadUsageSummary } = useBillingStore();

  useEffect(() => {
    if (isOpen) {
      loadConfig();
      loadUsageData();
      loadUsageSummary();
    }
  }, [isOpen, loadConfig, loadUsageData, loadUsageSummary]);

  const handleSave = async () => {
    try {
      await saveConfig();
      setHasUnsavedChanges(false);
    } catch (error) {
      console.error('Failed to save settings:', error);
    }
  };

  const handleClose = () => {
    if (hasUnsavedChanges) {
      const confirmed = window.confirm('You have unsaved changes. Are you sure you want to close?');
      if (!confirmed) return;
    }
    onClose();
    setHasUnsavedChanges(false);
  };

  const tabs = [
    { id: 'general' as SettingsTab, label: 'General', icon: '⚙️' },
    { id: 'models' as SettingsTab, label: 'Model Providers', icon: '🤖' },
    { id: 'mcp' as SettingsTab, label: 'MCP Servers', icon: '🔌' },
    { id: 'billing' as SettingsTab, label: 'Billing & Usage', icon: '💰' },
  ];

  if (!isOpen) return null;

  return (
    <div className="settings-overlay">
      <div className="settings-window">
        <div className="settings-header">
          <h2>Settings</h2>
          <div className="settings-header-actions">
            {hasUnsavedChanges && (
              <button 
                className="btn-save" 
                onClick={handleSave}
                disabled={isLoading}
              >
                Save Changes
              </button>
            )}
            <button className="btn-close" onClick={handleClose}>
              ✕
            </button>
          </div>
        </div>

        <div className="settings-content">
          <div className="settings-sidebar">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                className={`settings-tab ${activeTab === tab.id ? 'active' : ''}`}
                onClick={() => setActiveTab(tab.id)}
              >
                <span className="tab-icon">{tab.icon}</span>
                <span className="tab-label">{tab.label}</span>
              </button>
            ))}
          </div>

          <div className="settings-main">
            {isLoading ? (
              <div className="settings-loading">
                <div className="loading-spinner"></div>
                <p>Loading settings...</p>
              </div>
            ) : (
              <>
                {activeTab === 'general' && (
                  <GeneralSettings onSettingsChange={() => setHasUnsavedChanges(true)} />
                )}
                {activeTab === 'models' && (
                  <ModelProviderSettings onSettingsChange={() => setHasUnsavedChanges(true)} />
                )}
                {activeTab === 'mcp' && (
                  <MCPServerSettings onSettingsChange={() => setHasUnsavedChanges(true)} />
                )}
                {activeTab === 'billing' && (
                  <BillingSettings onSettingsChange={() => setHasUnsavedChanges(true)} />
                )}
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettingsWindow;