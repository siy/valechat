import React, { useEffect, useRef, useState } from 'react';
import { useChatStore } from '../../stores/chatStore';
import { useSettingsStore } from '../../stores/settingsStore';
import MessageList from './MessageList';
import InputArea from './InputArea';
import ConversationSidebar from './ConversationSidebar';
import ModelSelector from './ModelSelector';
import SettingsWindow from '../settings/SettingsWindow';
import UsageDashboard from '../dashboard/UsageDashboard';
import './ChatWindow.css';

const ChatWindow: React.FC = () => {
  const { 
    currentConversation, 
    isStreaming, 
    streamingMessage, 
    errors,
    loadConversations,
    createNewConversation,
    sendMessage,
    removeError 
  } = useChatStore();
  
  const { config, loadConfig } = useSettingsStore();
  const chatContainerRef = useRef<HTMLDivElement>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [showDashboard, setShowDashboard] = useState(false);

  useEffect(() => {
    // Load initial data
    loadConfig();
    loadConversations();
  }, [loadConfig, loadConversations]);

  useEffect(() => {
    // Auto-scroll to bottom when new messages arrive
    if (chatContainerRef.current) {
      chatContainerRef.current.scrollTop = chatContainerRef.current.scrollHeight;
    }
  }, [currentConversation?.messages, streamingMessage]);

  const handleSendMessage = async (content: string, model: string, provider: string) => {
    if (!currentConversation) {
      // Create new conversation if none exists
      await createNewConversation();
    }
    await sendMessage(content, model, provider);
  };

  const handleNewConversation = async () => {
    await createNewConversation();
  };

  return (
    <div className="chat-window">
      {/* Sidebar */}
      <ConversationSidebar onNewConversation={handleNewConversation} />
      
      {/* Main chat area */}
      <div className="chat-main">
        {/* Header */}
        <div className="chat-header">
          <div className="conversation-title">
            {currentConversation?.title || 'Select or create a conversation'}
          </div>
          <div className="chat-header-actions">
            <ModelSelector />
            <button 
              className="dashboard-button"
              onClick={() => setShowDashboard(true)}
              title="Usage Dashboard"
            >
              📊
            </button>
            <button 
              className="settings-button"
              onClick={() => setShowSettings(true)}
              title="Settings"
            >
              ⚙️
            </button>
          </div>
        </div>
        
        {/* Error alerts */}
        {errors.length > 0 && (
          <div className="error-alerts">
            {errors.map((error) => (
              <div key={error.id} className={`error-alert error-${error.severity || 'error'}`}>
                <div className="error-message">{error.message}</div>
                {error.details && (
                  <div className="error-details">{error.details}</div>
                )}
                <button 
                  className="error-dismiss"
                  onClick={() => removeError(error.id)}
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        )}
        
        {/* Messages */}
        <div ref={chatContainerRef} className="chat-container">
          {currentConversation ? (
            <>
              <MessageList 
                messages={currentConversation.messages}
                streamingMessage={streamingMessage}
                isStreaming={isStreaming}
              />
            </>
          ) : (
            <div className="chat-empty">
              <div className="empty-state">
                <h2>Welcome to ValeChat</h2>
                <p>Create a new conversation to get started</p>
                <button className="primary-button" onClick={handleNewConversation}>
                  New Conversation
                </button>
              </div>
            </div>
          )}
        </div>
        
        {/* Input area */}
        {currentConversation && (
          <InputArea 
            onSendMessage={handleSendMessage}
            disabled={isStreaming}
            config={config}
          />
        )}
      </div>

      {/* Settings Window */}
      <SettingsWindow 
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
      />

      {/* Usage Dashboard */}
      <UsageDashboard 
        isOpen={showDashboard}
        onClose={() => setShowDashboard(false)}
      />
    </div>
  );
};

export default ChatWindow;