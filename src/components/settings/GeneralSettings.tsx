import React from 'react';
import { useSettingsStore } from '../../stores/settingsStore';

interface GeneralSettingsProps {
  onSettingsChange: () => void;
}

const GeneralSettings: React.FC<GeneralSettingsProps> = ({ onSettingsChange }) => {
  const { config, updateConfig } = useSettingsStore();

  const handleThemeChange = (theme: 'light' | 'dark' | 'system') => {
    updateConfig({ theme });
    onSettingsChange();
  };

  const handleLanguageChange = (language: string) => {
    updateConfig({ language });
    onSettingsChange();
  };

  const handleAutoSaveChange = (auto_save: boolean) => {
    updateConfig({ auto_save });
    onSettingsChange();
  };

  const handleStreamingChange = (streaming: boolean) => {
    updateConfig({ streaming });
    onSettingsChange();
  };

  return (
    <div className="settings-section">
      <h3 className="settings-section-title">General Settings</h3>

      <div className="settings-group">
        <div className="settings-row">
          <div className="settings-label">
            <h4>Theme</h4>
            <p>Choose your preferred interface theme</p>
          </div>
          <div className="settings-control">
            <select
              className="form-select"
              value={config.theme}
              onChange={(e) => handleThemeChange(e.target.value as 'light' | 'dark' | 'system')}
            >
              <option value="system">System</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-label">
            <h4>Language</h4>
            <p>Select your preferred language</p>
          </div>
          <div className="settings-control">
            <select
              className="form-select"
              value={config.language}
              onChange={(e) => handleLanguageChange(e.target.value)}
            >
              <option value="en">English</option>
              <option value="es">Español</option>
              <option value="fr">Français</option>
              <option value="de">Deutsch</option>
              <option value="ja">日本語</option>
              <option value="zh">中文</option>
            </select>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-row">
          <div className="settings-label">
            <h4>Auto-save Conversations</h4>
            <p>Automatically save conversations as you chat</p>
          </div>
          <div className="settings-control">
            <label className="form-switch">
              <input
                type="checkbox"
                checked={config.auto_save}
                onChange={(e) => handleAutoSaveChange(e.target.checked)}
              />
              <span className="form-switch-slider"></span>
            </label>
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-label">
            <h4>Streaming Responses</h4>
            <p>Show responses as they're being generated</p>
          </div>
          <div className="settings-control">
            <label className="form-switch">
              <input
                type="checkbox"
                checked={config.streaming}
                onChange={(e) => handleStreamingChange(e.target.checked)}
              />
              <span className="form-switch-slider"></span>
            </label>
          </div>
        </div>
      </div>

      <div className="settings-group">
        <div className="settings-row">
          <div className="settings-label">
            <h4>Application Information</h4>
            <p>Version and build information</p>
          </div>
          <div className="settings-control">
            <div style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
              <div>ValeChat v0.1.0</div>
              <div style={{ marginTop: '4px', fontSize: '12px' }}>
                Multi-model AI chat application
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default GeneralSettings;