import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import { invoke } from '@tauri-apps/api/core';
import { AppConfig, ModelProvider, MCPServer, BillingLimits } from '../types';

interface SettingsState {
  config: AppConfig;
  isLoading: boolean;
  
  // Actions
  updateConfig: (updates: Partial<AppConfig>) => void;
  updateModelProvider: (providerId: string, updates: Partial<ModelProvider>) => void;
  addModelProvider: (provider: ModelProvider) => void;
  removeModelProvider: (providerId: string) => void;
  updateMCPServer: (serverId: string, updates: Partial<MCPServer>) => void;
  addMCPServer: (server: MCPServer) => void;
  removeMCPServer: (serverId: string) => void;
  updateBillingLimits: (limits: Partial<BillingLimits>) => void;
  
  // Data operations
  loadConfig: () => Promise<void>;
  saveConfig: () => Promise<void>;
  resetToDefaults: () => void;
}

const defaultConfig: AppConfig = {
  theme: 'system',
  language: 'en',
  model_providers: [
    {
      id: 'openai',
      name: 'OpenAI',
      type: 'openai',
      enabled: false,
      models: [
        {
          id: 'gpt-4',
          name: 'gpt-4',
          display_name: 'GPT-4',
          provider: 'openai',
          context_length: 8192,
          supports_streaming: true,
          input_price_per_1k: '0.03',
          output_price_per_1k: '0.06',
        },
        {
          id: 'gpt-3.5-turbo',
          name: 'gpt-3.5-turbo',
          display_name: 'GPT-3.5 Turbo',
          provider: 'openai',
          context_length: 4096,
          supports_streaming: true,
          input_price_per_1k: '0.0015',
          output_price_per_1k: '0.002',
        },
      ],
      config: {
        api_key: '',
        base_url: 'https://api.openai.com/v1',
        organization: '',
      },
    },
    {
      id: 'anthropic',
      name: 'Anthropic',
      type: 'anthropic',
      enabled: false,
      models: [
        {
          id: 'claude-3-opus',
          name: 'claude-3-opus-20240229',
          display_name: 'Claude 3 Opus',
          provider: 'anthropic',
          context_length: 200000,
          supports_streaming: true,
          input_price_per_1k: '0.015',
          output_price_per_1k: '0.075',
        },
        {
          id: 'claude-3-sonnet',
          name: 'claude-3-sonnet-20240229',
          display_name: 'Claude 3 Sonnet',
          provider: 'anthropic',
          context_length: 200000,
          supports_streaming: true,
          input_price_per_1k: '0.003',
          output_price_per_1k: '0.015',
        },
      ],
      config: {
        api_key: '',
        base_url: 'https://api.anthropic.com',
      },
    },
  ],
  mcp_servers: [],
  billing_limits: {
    daily_limit: undefined,
    monthly_limit: undefined,
    per_model_limits: {},
    per_conversation_limits: {},
  },
  auto_save: true,
  streaming: true,
};

export const useSettingsStore = create<SettingsState>()(
  immer((set) => ({
    config: defaultConfig,
    isLoading: false,

    updateConfig: (updates) => set((state) => {
      Object.assign(state.config, updates);
    }),

    updateModelProvider: (providerId, updates) => set((state) => {
      const providerIndex = state.config.model_providers.findIndex(p => p.id === providerId);
      if (providerIndex !== -1) {
        Object.assign(state.config.model_providers[providerIndex], updates);
      }
    }),

    addModelProvider: (provider) => set((state) => {
      state.config.model_providers.push(provider);
    }),

    removeModelProvider: (providerId) => set((state) => {
      state.config.model_providers = state.config.model_providers.filter(p => p.id !== providerId);
    }),

    updateMCPServer: async (serverId, updates) => {
      try {
        await invoke('update_mcp_server', {
          id: serverId,
          ...updates,
        });
        
        set((state) => {
          const serverIndex = state.config.mcp_servers.findIndex(s => s.id === serverId);
          if (serverIndex !== -1) {
            Object.assign(state.config.mcp_servers[serverIndex], updates);
          }
        });
      } catch (error) {
        console.error('Failed to update MCP server:', error);
        throw error;
      }
    },

    addMCPServer: async (server) => {
      try {
        const response = await invoke('add_mcp_server', {
          name: server.name,
          command: server.command,
          args: server.args,
        }) as any;
        
        const newServer: MCPServer = {
          id: response.id,
          name: response.name,
          command: response.command,
          args: response.args,
          enabled: response.enabled,
          status: response.status,
          tools: response.tools || [],
          resources: response.resources || [],
        };
        
        set((state) => {
          state.config.mcp_servers.push(newServer);
        });
      } catch (error) {
        console.error('Failed to add MCP server:', error);
        throw error;
      }
    },

    removeMCPServer: async (serverId) => {
      try {
        await invoke('remove_mcp_server', { server_id: serverId });
        
        set((state) => {
          state.config.mcp_servers = state.config.mcp_servers.filter(s => s.id !== serverId);
        });
      } catch (error) {
        console.error('Failed to remove MCP server:', error);
        throw error;
      }
    },

    updateBillingLimits: (limits) => set((state) => {
      Object.assign(state.config.billing_limits, limits);
    }),

    loadConfig: async () => {
      set((state) => { state.isLoading = true; });
      
      try {
        const configResponse = await invoke('get_app_config') as any;
        const config: AppConfig = {
          theme: configResponse.theme,
          language: configResponse.language,
          model_providers: configResponse.model_providers.map((p: any) => ({
            id: p.id,
            name: p.name,
            type: p.provider_type,
            enabled: p.enabled,
            models: p.models.map((m: any) => ({
              id: m.id,
              name: m.name,
              display_name: m.display_name,
              provider: m.provider,
              context_length: m.context_length,
              supports_streaming: m.supports_streaming,
              input_price_per_1k: m.input_price_per_1k,
              output_price_per_1k: m.output_price_per_1k,
            })),
            config: {},
          })),
          mcp_servers: configResponse.mcp_servers.map((s: any) => ({
            id: s.id,
            name: s.name,
            command: s.command,
            args: s.args,
            enabled: s.enabled,
            status: s.status,
            tools: s.tools,
            resources: s.resources,
          })),
          billing_limits: {
            daily_limit: configResponse.billing_limits.daily_limit,
            monthly_limit: configResponse.billing_limits.monthly_limit,
            per_model_limits: configResponse.billing_limits.per_model_limits,
            per_conversation_limits: configResponse.billing_limits.per_conversation_limits,
          },
          auto_save: configResponse.auto_save,
          streaming: configResponse.streaming,
        };
        
        set((state) => { 
          state.config = config;
          state.isLoading = false;
        });
      } catch (error) {
        console.error('Failed to load config:', error);
        set((state) => { state.isLoading = false; });
      }
    },

    saveConfig: async () => {
      try {
        const { config } = useSettingsStore.getState();
        await invoke('update_app_config', {
          theme: config.theme,
          language: config.language,
          auto_save: config.auto_save,
          streaming: config.streaming,
        });
        console.log('Config saved successfully');
      } catch (error) {
        console.error('Failed to save config:', error);
        throw error;
      }
    },

    resetToDefaults: () => set((state) => {
      state.config = { ...defaultConfig };
    }),
  }))
);