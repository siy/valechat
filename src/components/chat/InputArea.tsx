import React, { useState, useRef, useCallback, KeyboardEvent } from 'react';
import { AppConfig } from '../../types';

interface InputAreaProps {
  onSendMessage: (content: string, model: string, provider: string) => Promise<void>;
  disabled: boolean;
  config: AppConfig;
}

const InputArea: React.FC<InputAreaProps> = ({ onSendMessage, disabled, config }) => {
  const [input, setInput] = useState('');
  const [selectedModel, setSelectedModel] = useState('');
  const [selectedProvider, setSelectedProvider] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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
      const defaultModel = availableModels[0];
      setSelectedModel(defaultModel.id);
      setSelectedProvider(defaultModel.provider);
    }
  }, [availableModels, selectedModel]);

  const handleSubmit = useCallback(async (e?: React.FormEvent) => {
    e?.preventDefault();
    
    if (!input.trim() || disabled || !selectedModel || !selectedProvider) {
      return;
    }

    const content = input.trim();
    setInput('');
    
    try {
      await onSendMessage(content, selectedModel, selectedProvider);
    } catch (error) {
      console.error('Failed to send message:', error);
      // Restore input on error
      setInput(content);
    }
  }, [input, disabled, selectedModel, selectedProvider, onSendMessage]);

  const handleKeyDown = useCallback((e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter') {
      if (e.shiftKey) {
        // Allow new line with Shift+Enter
        return;
      } else {
        // Send message with Enter
        e.preventDefault();
        handleSubmit();
      }
    }
  }, [handleSubmit]);

  const handleModelChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const modelId = e.target.value;
    const model = availableModels.find(m => m.id === modelId);
    if (model) {
      setSelectedModel(modelId);
      setSelectedProvider(model.provider);
    }
  };

  // Auto-resize textarea
  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  }, []);

  React.useEffect(() => {
    adjustTextareaHeight();
  }, [input, adjustTextareaHeight]);

  if (availableModels.length === 0) {
    return (
      <div className="input-area disabled">
        <div className="no-models-warning">
          No AI models configured. Please configure at least one model provider in settings.
        </div>
      </div>
    );
  }

  return (
    <div className="input-area">
      <form onSubmit={handleSubmit} className="input-form">
        <div className="input-controls">
          <select 
            value={selectedModel}
            onChange={handleModelChange}
            className="model-selector"
            disabled={disabled}
          >
            {availableModels.map((model) => (
              <option key={`${model.provider}-${model.id}`} value={model.id}>
                {model.display_name} ({model.providerName})
              </option>
            ))}
          </select>
        </div>
        
        <div className="input-row">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={disabled ? "Sending..." : "Type your message... (Enter to send, Shift+Enter for new line)"}
            disabled={disabled}
            className="message-input"
            rows={1}
          />
          
          <button
            type="submit"
            disabled={disabled || !input.trim()}
            className="send-button"
          >
            {disabled ? (
              <span className="sending-indicator">⏳</span>
            ) : (
              <span className="send-icon">➤</span>
            )}
          </button>
        </div>
      </form>
      
      <div className="input-hints">
        <span>Enter to send • Shift+Enter for new line</span>
        {selectedModel && (
          <span className="current-model">
            Using: {availableModels.find(m => m.id === selectedModel)?.display_name}
          </span>
        )}
      </div>
    </div>
  );
};

export default InputArea;