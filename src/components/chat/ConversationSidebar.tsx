import React, { useState } from 'react';
import { useChatStore } from '../../stores/chatStore';
import { formatDistanceToNow } from 'date-fns';

interface ConversationSidebarProps {
  onNewConversation: () => void;
}

const ConversationSidebar: React.FC<ConversationSidebarProps> = ({ 
  onNewConversation 
}) => {
  const { 
    conversations, 
    currentConversation, 
    setCurrentConversation,
    deleteConversation,
    updateConversation 
  } = useChatStore();
  
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState('');

  const handleConversationClick = (conversation: any) => {
    setCurrentConversation(conversation);
  };

  const handleDeleteConversation = (e: React.MouseEvent, conversationId: string) => {
    e.stopPropagation();
    if (confirm('Delete this conversation? This action cannot be undone.')) {
      deleteConversation(conversationId);
    }
  };

  const handleEditTitle = (e: React.MouseEvent, conversation: any) => {
    e.stopPropagation();
    setEditingId(conversation.id);
    setEditTitle(conversation.title);
  };

  const handleSaveTitle = (conversationId: string) => {
    if (editTitle.trim()) {
      updateConversation(conversationId, { 
        title: editTitle.trim(),
        updated_at: Date.now()
      });
    }
    setEditingId(null);
    setEditTitle('');
  };

  const handleKeyPress = (e: React.KeyboardEvent, conversationId: string) => {
    if (e.key === 'Enter') {
      handleSaveTitle(conversationId);
    } else if (e.key === 'Escape') {
      setEditingId(null);
      setEditTitle('');
    }
  };

  const formatRelativeTime = (timestamp: number) => {
    try {
      return formatDistanceToNow(new Date(timestamp), { addSuffix: true });
    } catch {
      return 'Unknown';
    }
  };

  const formatCost = (cost?: string) => {
    if (!cost) return null;
    const costValue = parseFloat(cost);
    return costValue > 0 ? `$${costValue.toFixed(4)}` : null;
  };

  return (
    <div className="conversation-sidebar">
      <div className="sidebar-header">
        <h2>Conversations</h2>
        <button 
          className="new-conversation-btn"
          onClick={onNewConversation}
          title="New Conversation"
        >
          +
        </button>
      </div>
      
      <div className="conversation-list">
        {conversations.length === 0 ? (
          <div className="empty-conversations">
            <p>No conversations yet</p>
            <p>Start a new one to begin chatting</p>
          </div>
        ) : (
          conversations.map((conversation) => (
            <div
              key={conversation.id}
              className={`conversation-item ${
                currentConversation?.id === conversation.id ? 'active' : ''
              }`}
              onClick={() => handleConversationClick(conversation)}
            >
              <div className="conversation-content">
                {editingId === conversation.id ? (
                  <input
                    type="text"
                    value={editTitle}
                    onChange={(e) => setEditTitle(e.target.value)}
                    onBlur={() => handleSaveTitle(conversation.id)}
                    onKeyDown={(e) => handleKeyPress(e, conversation.id)}
                    className="title-edit-input"
                    autoFocus
                  />
                ) : (
                  <div 
                    className="conversation-title"
                    title={conversation.title}
                  >
                    {conversation.title}
                  </div>
                )}
                
                <div className="conversation-meta">
                  <span className="message-count">
                    {conversation.message_count} messages
                  </span>
                  <span className="last-updated">
                    {formatRelativeTime(conversation.updated_at)}
                  </span>
                  {formatCost(conversation.total_cost) && (
                    <span className="total-cost">
                      {formatCost(conversation.total_cost)}
                    </span>
                  )}
                </div>
              </div>
              
              <div className="conversation-actions">
                <button
                  className="action-btn edit-btn"
                  onClick={(e) => handleEditTitle(e, conversation)}
                  title="Rename conversation"
                >
                  ✏️
                </button>
                <button
                  className="action-btn delete-btn"
                  onClick={(e) => handleDeleteConversation(e, conversation.id)}
                  title="Delete conversation"
                >
                  🗑️
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

export default ConversationSidebar;