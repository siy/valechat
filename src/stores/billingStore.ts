import { create } from 'zustand';
import { immer } from 'zustand/middleware/immer';
import { invoke } from '@tauri-apps/api/core';
import { UsageRecord, BillingLimits } from '../types';

interface UsageSummary {
  daily_cost: string;
  monthly_cost: string;
  daily_tokens: number;
  monthly_tokens: number;
  top_models: Array<{
    model: string;
    provider: string;
    cost: string;
    tokens: number;
    percentage: number;
  }>;
  cost_trend: Array<{
    date: string;
    cost: string;
  }>;
}

interface BillingState {
  usage_records: UsageRecord[];
  usage_summary: UsageSummary | null;
  billing_limits: BillingLimits;
  isLoading: boolean;
  
  // Alerts
  limit_alerts: Array<{
    id: string;
    type: 'daily' | 'monthly' | 'model' | 'conversation';
    message: string;
    severity: 'info' | 'warning' | 'error';
    timestamp: number;
  }>;
  
  // Actions
  addUsageRecord: (record: UsageRecord) => void;
  updateBillingLimits: (limits: Partial<BillingLimits>) => void;
  clearAlert: (alertId: string) => void;
  
  // Data operations
  loadUsageData: () => Promise<void>;
  loadUsageSummary: () => Promise<void>;
  exportUsageData: (format: 'csv' | 'json', period?: string) => Promise<void>;
  
  // Utility functions
  checkLimits: () => void;
  getCurrentSpending: (period: 'daily' | 'monthly') => string;
  getModelUsage: (modelId: string, period?: string) => UsageRecord[];
}

export const useBillingStore = create<BillingState>()(
  immer((set, get) => ({
    usage_records: [],
    usage_summary: null,
    billing_limits: {
      daily_limit: undefined,
      monthly_limit: undefined,
      per_model_limits: {},
      per_conversation_limits: {},
    },
    isLoading: false,
    limit_alerts: [],

    addUsageRecord: (record) => set((state) => {
      state.usage_records.push(record);
      // Sort by timestamp descending
      state.usage_records.sort((a, b) => b.timestamp - a.timestamp);
    }),

    updateBillingLimits: (limits) => set((state) => {
      Object.assign(state.billing_limits, limits);
    }),

    clearAlert: (alertId) => set((state) => {
      state.limit_alerts = state.limit_alerts.filter(a => a.id !== alertId);
    }),

    loadUsageData: async () => {
      set((state) => { state.isLoading = true; });
      
      try {
        const records = await invoke('get_usage_records', { 
          limit: 1000,
          offset: 0 
        }) as any[];
        
        const usageRecords: UsageRecord[] = records.map(r => ({
          id: r.id,
          timestamp: r.timestamp,
          provider: r.provider,
          model: r.model,
          input_tokens: r.input_tokens,
          output_tokens: r.output_tokens,
          cost: r.cost,
          conversation_id: r.conversation_id,
          request_id: r.request_id,
        }));
        
        set((state) => { 
          state.usage_records = usageRecords;
          state.isLoading = false;
        });
      } catch (error) {
        console.error('Failed to load usage data:', error);
        set((state) => { state.isLoading = false; });
      }
    },

    loadUsageSummary: async () => {
      try {
        const summary = await invoke('get_usage_summary') as any;
        
        const usageSummary: UsageSummary = {
          daily_cost: summary.daily_cost,
          monthly_cost: summary.monthly_cost,
          daily_tokens: summary.daily_tokens,
          monthly_tokens: summary.monthly_tokens,
          top_models: summary.top_models.map((m: any) => ({
            model: m.model,
            provider: m.provider,
            cost: m.cost,
            tokens: m.tokens,
            percentage: m.percentage,
          })),
          cost_trend: summary.cost_trend.map((t: any) => ({
            date: t.date,
            cost: t.cost,
          })),
        };
        
        set((state) => { state.usage_summary = usageSummary; });
      } catch (error) {
        console.error('Failed to load usage summary:', error);
      }
    },

    exportUsageData: async (format, period) => {
      try {
        const result = await invoke('export_usage_data', { format, period }) as string;
        console.log(`Usage data exported: ${result}`);
      } catch (error) {
        console.error('Failed to export usage data:', error);
        throw error;
      }
    },

    checkLimits: () => {
      const { usage_records, billing_limits } = get();
      const now = Date.now();
      const dayStart = now - (24 * 60 * 60 * 1000);
      const monthStart = new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime();
      
      // Calculate daily spending
      const dailyCost = usage_records
        .filter(r => r.timestamp >= dayStart)
        .reduce((sum, r) => sum + parseFloat(r.cost), 0);
      
      // Calculate monthly spending
      const monthlyCost = usage_records
        .filter(r => r.timestamp >= monthStart)
        .reduce((sum, r) => sum + parseFloat(r.cost), 0);
      
      const alerts: Array<{
        id: string;
        type: 'daily' | 'monthly' | 'model' | 'conversation';
        message: string;
        severity: 'info' | 'warning' | 'error';
        timestamp: number;
      }> = [];
      
      // Check daily limit
      if (billing_limits.daily_limit) {
        const dailyLimit = parseFloat(billing_limits.daily_limit);
        const percentage = (dailyCost / dailyLimit) * 100;
        
        if (percentage >= 100) {
          alerts.push({
            id: `daily_limit_${Date.now()}`,
            type: 'daily' as const,
            message: `Daily spending limit of $${dailyLimit.toFixed(2)} exceeded`,
            severity: 'error' as const,
            timestamp: now,
          });
        } else if (percentage >= 80) {
          alerts.push({
            id: `daily_warning_${Date.now()}`,
            type: 'daily' as const,
            message: `Daily spending at ${percentage.toFixed(1)}% of limit`,
            severity: 'warning' as const,
            timestamp: now,
          });
        }
      }
      
      // Check monthly limit
      if (billing_limits.monthly_limit) {
        const monthlyLimit = parseFloat(billing_limits.monthly_limit);
        const percentage = (monthlyCost / monthlyLimit) * 100;
        
        if (percentage >= 100) {
          alerts.push({
            id: `monthly_limit_${Date.now()}`,
            type: 'monthly' as const,
            message: `Monthly spending limit of $${monthlyLimit.toFixed(2)} exceeded`,
            severity: 'error' as const,
            timestamp: now,
          });
        } else if (percentage >= 80) {
          alerts.push({
            id: `monthly_warning_${Date.now()}`,
            type: 'monthly' as const,
            message: `Monthly spending at ${percentage.toFixed(1)}% of limit`,
            severity: 'warning' as const,
            timestamp: now,
          });
        }
      }
      
      if (alerts.length > 0) {
        set((state) => {
          state.limit_alerts.push(...alerts);
        });
      }
    },

    getCurrentSpending: (period) => {
      const { usage_records } = get();
      const now = Date.now();
      const periodStart = period === 'daily' 
        ? now - (24 * 60 * 60 * 1000)
        : new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime();
      
      const cost = usage_records
        .filter(r => r.timestamp >= periodStart)
        .reduce((sum, r) => sum + parseFloat(r.cost), 0);
      
      return cost.toFixed(4);
    },

    getModelUsage: (modelId, period) => {
      const { usage_records } = get();
      let filteredRecords = usage_records.filter(r => r.model === modelId);
      
      if (period) {
        const now = Date.now();
        const periodStart = period === 'daily' 
          ? now - (24 * 60 * 60 * 1000)
          : new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime();
        
        filteredRecords = filteredRecords.filter(r => r.timestamp >= periodStart);
      }
      
      return filteredRecords;
    },
  }))
);