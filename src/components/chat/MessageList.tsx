import React, { memo } from 'react';
import { Message, StreamingMessage } from '../../types';
import MessageItem from './MessageItem';
import StreamingMessageItem from './StreamingMessageItem';

interface MessageListProps {
  messages: Message[];
  streamingMessage: StreamingMessage | null;
  isStreaming: boolean;
}

const MessageList: React.FC<MessageListProps> = memo(({
  messages,
  streamingMessage,
  isStreaming
}) => {
  return (
    <div className="message-list">
      {messages.map((message) => (
        <MessageItem key={message.id} message={message} />
      ))}
      
      {isStreaming && streamingMessage && (
        <StreamingMessageItem streamingMessage={streamingMessage} />
      )}
    </div>
  );
});

MessageList.displayName = 'MessageList';

export default MessageList;