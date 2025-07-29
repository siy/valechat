import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import { invoke } from '@tauri-apps/api/core';
import { Conversation, Message, StreamingMessage, AppError } from '../types';

interface ChatState {
  // Current conversation
  currentConversation: Conversation | null;
  conversations: Conversation[];
  
  // Streaming state
  streamingMessage: StreamingMessage | null;
  isStreaming: boolean;
  
  // UI state
  isLoading: boolean;
  errors: AppError[];
  
  // Actions
  setCurrentConversation: (conversation: Conversation | null) => void;
  addConversation: (conversation: Conversation) => void;
  updateConversation: (id: string, updates: Partial<Conversation>) => Promise<void>;
  deleteConversation: (id: string) => Promise<void>;
  addMessage: (conversationId: string, message: Message) => void;
  updateMessage: (conversationId: string, messageId: string, updates: Partial<Message>) => void;
  
  // Streaming actions
  startStreaming: (messageId: string) => void;
  updateStreamingMessage: (content: string) => void;
  completeStreamingMessage: (finalMessage: Message) => void;
  stopStreaming: () => void;
  
  // Error handling
  addError: (error: AppError) => void;
  removeError: (errorId: string) => void;
  clearErrors: () => void;
  
  // Loading state
  setLoading: (loading: boolean) => void;
  
  // Data fetching
  loadConversations: () => Promise<void>;
  loadConversationMessages: (conversationId: string) => Promise<void>;
  createNewConversation: (title?: string) => Promise<Conversation>;
  sendMessage: (content: string, model: string, provider: string) => Promise<void>;
}

export const useChatStore = create<ChatState>()(
  immer((set, get) => ({
    // Initial state
    currentConversation: null,
    conversations: [],
    streamingMessage: null,
    isStreaming: false,
    isLoading: false,
    errors: [],

    // Basic setters
    setCurrentConversation: (conversation) => set((state) => {
      state.currentConversation = conversation;
    }),

    addConversation: (conversation) => set((state) => {
      state.conversations.unshift(conversation);
    }),

    updateConversation: async (id, updates) => {
      try {
        // If title is being updated, call the backend
        if (updates.title) {
          await invoke('update_conversation_title', {
            conversation_id: id,
            title: updates.title
          });
        }
        
        // Update local state
        set((state) => {
          const index = state.conversations.findIndex(c => c.id === id);
          if (index !== -1) {
            Object.assign(state.conversations[index], updates);
          }
          if (state.currentConversation?.id === id) {
            Object.assign(state.currentConversation, updates);
          }
        });
      } catch (error) {
        console.error('Failed to update conversation:', error);
        const errorMessage = error instanceof Error ? error.message : String(error);
        get().addError({
          id: Date.now().toString(),
          type: 'system',
          message: 'Failed to update conversation',
          details: errorMessage,
          timestamp: Date.now(),
          recoverable: true,
        });
        throw error;
      }
    },

    deleteConversation: async (id) => {
      try {
        await invoke('delete_conversation', {
          conversation_id: id
        });
        
        // Update local state only if backend call succeeds
        set((state) => {
          state.conversations = state.conversations.filter(c => c.id !== id);
          if (state.currentConversation?.id === id) {
            state.currentConversation = null;
          }
        });
      } catch (error) {
        console.error('Failed to delete conversation:', error);
        const errorMessage = error instanceof Error ? error.message : String(error);
        get().addError({
          id: Date.now().toString(),
          type: 'system',
          message: 'Failed to delete conversation',
          details: errorMessage,
          timestamp: Date.now(),
          recoverable: true,
        });
        throw error;
      }
    },

    addMessage: (conversationId, message) => set((state) => {
      const conversation = state.conversations.find(c => c.id === conversationId);
      if (conversation) {
        conversation.messages.push(message);
        conversation.message_count += 1;
        conversation.updated_at = Date.now();
      }
      if (state.currentConversation?.id === conversationId) {
        state.currentConversation.messages.push(message);
        state.currentConversation.message_count += 1;
        state.currentConversation.updated_at = Date.now();
      }
    }),

    updateMessage: (conversationId, messageId, updates) => set((state) => {
      const conversation = state.conversations.find(c => c.id === conversationId);
      if (conversation) {
        const messageIndex = conversation.messages.findIndex(m => m.id === messageId);
        if (messageIndex !== -1) {
          Object.assign(conversation.messages[messageIndex], updates);
        }
      }
      if (state.currentConversation?.id === conversationId) {
        const messageIndex = state.currentConversation.messages.findIndex(m => m.id === messageId);
        if (messageIndex !== -1) {
          Object.assign(state.currentConversation.messages[messageIndex], updates);
        }
      }
    }),

    // Streaming actions
    startStreaming: (messageId) => set((state) => {
      state.isStreaming = true;
      state.streamingMessage = {
        id: messageId,
        content: '',
        is_complete: false,
      };
    }),

    updateStreamingMessage: (content) => set((state) => {
      if (state.streamingMessage) {
        state.streamingMessage.content = content;
      }
    }),

    completeStreamingMessage: (finalMessage) => set((state) => {
      state.isStreaming = false;
      state.streamingMessage = null;
      
      if (state.currentConversation) {
        get().addMessage(state.currentConversation.id, finalMessage);
      }
    }),

    stopStreaming: () => set((state) => {
      state.isStreaming = false;
      state.streamingMessage = null;
    }),

    // Error handling
    addError: (error) => set((state) => {
      state.errors.push(error);
    }),

    removeError: (errorId) => set((state) => {
      state.errors = state.errors.filter(e => e.id !== errorId);
    }),

    clearErrors: () => set((state) => {
      state.errors = [];
    }),

    setLoading: (loading) => set((state) => {
      state.isLoading = loading;
    }),

    // Data operations
    loadConversations: async () => {
      set((state) => { state.isLoading = true; });
      
      try {
        const conversations = await invoke('get_conversations') as any[];
        const mappedConversations: Conversation[] = conversations.map(c => ({
          id: c.id,
          title: c.title,
          created_at: c.created_at,
          updated_at: c.updated_at,
          model_provider: c.model_provider,
          total_cost: c.total_cost,
          message_count: c.message_count,
          messages: [],
        }));
        
        set((state) => { 
          state.conversations = mappedConversations;
          state.isLoading = false;
        });
      } catch (error) {
        get().addError({
          id: Date.now().toString(),
          type: 'system',
          message: 'Failed to load conversations',
          details: error instanceof Error ? error.message : 'Unknown error',
          timestamp: Date.now(),
          recoverable: true,
        });
        set((state) => { state.isLoading = false; });
      }
    },

    loadConversationMessages: async (conversationId) => {
      try {
        const messages = await invoke('get_conversation_messages', {
          conversation_id: conversationId
        }) as any[];
        
        const mappedMessages: Message[] = messages.map(m => ({
          id: m.id,
          role: m.role,
          content: m.content,
          timestamp: m.timestamp,
          model_used: m.model_used,
          provider: m.provider,
          input_tokens: m.input_tokens,
          output_tokens: m.output_tokens,
          cost: m.cost,
          processing_time_ms: m.processing_time_ms,
        }));
        
        // Update the conversation's messages
        set((state) => {
          const conversation = state.conversations.find(c => c.id === conversationId);
          if (conversation) {
            conversation.messages = mappedMessages;
          }
          if (state.currentConversation?.id === conversationId) {
            state.currentConversation.messages = mappedMessages;
          }
        });
      } catch (error) {
        console.error('Failed to load conversation messages:', error);
        const errorMessage = error instanceof Error ? error.message : String(error);
        get().addError({
          id: Date.now().toString(),
          type: 'system',
          message: 'Failed to load conversation messages',
          details: errorMessage,
          timestamp: Date.now(),
          recoverable: true,
        });
        throw error;
      }
    },

    createNewConversation: async (title) => {
      try {
        const response = await invoke('create_conversation', { 
          request: { title: title || undefined }
        }) as any;
        
        const conversation: Conversation = {
          id: response.id,
          title: response.title,
          created_at: response.created_at,
          updated_at: response.updated_at,
          model_provider: response.model_provider,
          total_cost: response.total_cost,
          message_count: response.message_count,
          messages: [],
        };
        
        get().addConversation(conversation);
        get().setCurrentConversation(conversation);
        return conversation;
      } catch (error) {
        console.error('Failed to create conversation:', error);
        const errorMessage = error instanceof Error ? error.message : String(error);
        get().addError({
          id: Date.now().toString(),
          type: 'system',
          message: 'Failed to create conversation',
          details: errorMessage,
          timestamp: Date.now(),
          recoverable: true,
        });
        throw error;
      }
    },

    sendMessage: async (content, model, provider) => {
      const { currentConversation } = get();
      if (!currentConversation) return;

      const userMessage: Message = {
        id: Date.now().toString(),
        role: 'user',
        content,
        timestamp: Date.now(),
      };

      // Add user message
      get().addMessage(currentConversation.id, userMessage);

      // Start streaming response
      const assistantMessageId = (Date.now() + 1).toString();
      get().startStreaming(assistantMessageId);

      try {
        const response = await invoke('send_message', {
          request: {
            conversation_id: currentConversation.id,
            content,
            model,
            provider,
          }
        }) as any;

        const assistantMessage: Message = {
          id: response.id,
          role: response.role,
          content: response.content,
          timestamp: response.timestamp,
          model_used: response.model_used,
          provider: response.provider,
          input_tokens: response.input_tokens,
          output_tokens: response.output_tokens,
          cost: response.cost,
          processing_time_ms: response.processing_time_ms,
        };
        
        get().completeStreamingMessage(assistantMessage);

      } catch (error) {
        get().stopStreaming();
        get().addError({
          id: Date.now().toString(),
          type: 'model',
          message: 'Failed to send message',
          details: error instanceof Error ? error.message : 'Unknown error',
          timestamp: Date.now(),
          recoverable: true,
        });
      }
    },
  }))
);