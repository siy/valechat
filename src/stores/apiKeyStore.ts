import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import { invoke } from '@tauri-apps/api/core';

interface ApiKeyState {
  apiKeys: Record<string, string>; // provider -> api key
  isLoading: boolean;
  errors: string[];
  
  // Actions
  setApiKey: (provider: string, apiKey: string) => Promise<void>;
  getApiKey: (provider: string) => Promise<string | null>;
  removeApiKey: (provider: string) => Promise<void>;
  loadApiKeys: (providers: string[]) => Promise<void>;
  clearErrors: () => void;
}

export const useApiKeyStore = create<ApiKeyState>()(
  immer((set) => ({
    apiKeys: {},
    isLoading: false,
    errors: [],

    setApiKey: async (provider: string, apiKey: string) => {
      set((state) => { state.isLoading = true; });
      
      try {
        await invoke('set_api_key', { provider, api_key: apiKey });
        
        set((state) => {
          state.apiKeys[provider] = apiKey;
          state.isLoading = false;
          state.errors = [];
        });
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        set((state) => {
          state.errors.push(`Failed to save API key for ${provider}: ${errorMessage}`);
          state.isLoading = false;
        });
        throw error;
      }
    },

    getApiKey: async (provider: string) => {
      try {
        const result = await invoke('get_api_key', { provider }) as string | null;
        
        set((state) => {
          if (result) {
            state.apiKeys[provider] = result;
          } else {
            delete state.apiKeys[provider];
          }
        });
        
        return result;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        set((state) => {
          state.errors.push(`Failed to get API key for ${provider}: ${errorMessage}`);
        });
        throw error;
      }
    },

    removeApiKey: async (provider: string) => {
      set((state) => { state.isLoading = true; });
      
      try {
        await invoke('remove_api_key', { provider });
        
        set((state) => {
          delete state.apiKeys[provider];
          state.isLoading = false;
          state.errors = [];
        });
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        set((state) => {
          state.errors.push(`Failed to remove API key for ${provider}: ${errorMessage}`);
          state.isLoading = false;
        });
        throw error;
      }
    },

    loadApiKeys: async (providers: string[]) => {
      set((state) => { state.isLoading = true; });
      
      try {
        const keys: Record<string, string> = {};
        
        for (const provider of providers) {
          try {
            const apiKey = await invoke('get_api_key', { provider }) as string | null;
            if (apiKey) {
              keys[provider] = apiKey;
            }
          } catch (error) {
            console.warn(`Failed to load API key for ${provider}:`, error);
          }
        }
        
        set((state) => {
          state.apiKeys = keys;
          state.isLoading = false;
          state.errors = [];
        });
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        set((state) => {
          state.errors.push(`Failed to load API keys: ${errorMessage}`);
          state.isLoading = false;
        });
        throw error;
      }
    },

    clearErrors: () => set((state) => {
      state.errors = [];
    }),
  }))
);