import React, { memo } from 'react';
import { StreamingMessage } from '../../types';

interface StreamingMessageItemProps {
  streamingMessage: StreamingMessage;
}

const StreamingMessageItem: React.FC<StreamingMessageItemProps> = memo(({
  streamingMessage
}) => {
  return (
    <div className="message-item message-assistant streaming">
      <div className="message-header">
        <div className="message-role">Assistant</div>
        <div className="message-meta">
          <div className="streaming-indicator">
            <span className="dot"></span>
            <span className="dot"></span>
            <span className="dot"></span>
          </div>
        </div>
      </div>
      
      <div className="message-content">
        {streamingMessage.content}
        <span className="cursor">|</span>
      </div>
      
      {streamingMessage.error && (
        <div className="message-error">
          Error: {streamingMessage.error}
        </div>
      )}
    </div>
  );
});

StreamingMessageItem.displayName = 'StreamingMessageItem';

export default StreamingMessageItem;