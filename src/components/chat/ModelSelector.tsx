import React, { useState } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';

const ModelSelector: React.FC = () => {
  const { config } = useSettingsStore();
  const [isOpen, setIsOpen] = useState(false);
  const [selectedModel, setSelectedModel] = useState('');

  // Get available models
  const availableModels = config.model_providers
    .filter(provider => provider.enabled)
    .flatMap(provider => 
      provider.models.map(model => ({
        ...model,
        provider: provider.id,
        providerName: provider.name,
      }))
    );

  // Set default model if none selected
  React.useEffect(() => {
    if (!selectedModel && availableModels.length > 0) {
      setSelectedModel(availableModels[0].id);
    }
  }, [availableModels, selectedModel]);

  const currentModel = availableModels.find(m => m.id === selectedModel);

  if (availableModels.length === 0) {
    return (
      <div className="model-selector error">
        <span className="no-models">No models configured</span>
      </div>
    );
  }

  return (
    <div className="model-selector">
      <button
        className="model-selector-button"
        onClick={() => setIsOpen(!isOpen)}
      >
        <div className="selected-model">
          <span className="model-name">
            {currentModel?.display_name || 'Select Model'}
          </span>
          <span className="provider-name">
            {currentModel?.providerName}
          </span>
        </div>
        <span className={`dropdown-arrow ${isOpen ? 'open' : ''}`}>
          ▼
        </span>
      </button>

      {isOpen && (
        <div className="model-dropdown">
          {availableModels.map((model) => (
            <div
              key={`${model.provider}-${model.id}`}
              className={`model-option ${selectedModel === model.id ? 'selected' : ''}`}
              onClick={() => {
                setSelectedModel(model.id);
                setIsOpen(false);
              }}
            >
              <div className="model-info">
                <div className="model-name">{model.display_name}</div>
                <div className="model-details">
                  <span className="provider">{model.providerName}</span>
                  <span className="context-length">
                    {model.context_length.toLocaleString()} tokens
                  </span>
                  {model.input_price_per_1k && (
                    <span className="pricing">
                      ${parseFloat(model.input_price_per_1k).toFixed(4)}/1K input
                    </span>
                  )}
                </div>
              </div>
              {model.supports_streaming && (
                <span className="streaming-badge">Streaming</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

export default ModelSelector;