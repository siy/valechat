import React, { memo } from 'react';
import { Message } from '../../types';

interface MessageItemProps {
  message: Message;
}

const MessageItem: React.FC<MessageItemProps> = memo(({ message }) => {
  const formatTimestamp = (timestamp: number) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const formatCost = (cost?: string) => {
    if (!cost) return null;
    const costValue = parseFloat(cost);
    return costValue > 0 ? `$${costValue.toFixed(4)}` : null;
  };

  return (
    <div className={`message-item message-${message.role}`}>
      <div className="message-header">
        <div className="message-role">
          {message.role === 'user' ? 'You' : 'Assistant'}
        </div>
        <div className="message-meta">
          {message.model_used && (
            <span className="message-model">{message.model_used}</span>
          )}
          {message.provider && (
            <span className="message-provider">({message.provider})</span>
          )}
          {formatCost(message.cost) && (
            <span className="message-cost">{formatCost(message.cost)}</span>
          )}
          <span className="message-time">{formatTimestamp(message.timestamp)}</span>
        </div>
      </div>
      
      <div className="message-content">
        {message.content}
      </div>
      
      {(message.input_tokens || message.output_tokens) && (
        <div className="message-tokens">
          {message.input_tokens && (
            <span className="token-count">
              Input: {message.input_tokens.toLocaleString()}
            </span>
          )}
          {message.output_tokens && (
            <span className="token-count">
              Output: {message.output_tokens.toLocaleString()}
            </span>
          )}
          {message.processing_time_ms && (
            <span className="processing-time">
              {message.processing_time_ms}ms
            </span>
          )}
        </div>
      )}
    </div>
  );
});

MessageItem.displayName = 'MessageItem';

export default MessageItem;