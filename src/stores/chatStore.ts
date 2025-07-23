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
  updateConversation: (id: string, updates: Partial<Conversation>) => void;
  deleteConversation: (id: string) => void;
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

    updateConversation: (id, updates) => set((state) => {
      const index = state.conversations.findIndex(c => c.id === id);
      if (index !== -1) {
        Object.assign(state.conversations[index], updates);
      }
      if (state.currentConversation?.id === id) {
        Object.assign(state.currentConversation, updates);
      }
    }),

    deleteConversation: (id) => set((state) => {
      state.conversations = state.conversations.filter(c => c.id !== id);
      if (state.currentConversation?.id === id) {
        state.currentConversation = null;
      }
    }),

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

    createNewConversation: async (title) => {
      try {
        const response = await invoke('create_conversation', { 
          title: title || undefined 
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
        get().addError({
          id: Date.now().toString(),
          type: 'system',
          message: 'Failed to create conversation',
          details: error instanceof Error ? error.message : 'Unknown error',
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
          conversation_id: currentConversation.id,
          content,
          model,
          provider,
          stream: true,
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