// Common types for the application

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  model_used?: string;
  provider?: string;
  input_tokens?: number;
  output_tokens?: number;
  cost?: string;
  processing_time_ms?: number;
}

export interface Conversation {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
  model_provider?: string;
  total_cost?: string;
  message_count: number;
  messages: Message[];
}

export interface ModelProvider {
  id: string;
  name: string;
  type: 'openai' | 'anthropic' | 'gemini' | 'local';
  enabled: boolean;
  models: Model[];
  config: Record<string, any>;
}

export interface Model {
  id: string;
  name: string;
  display_name: string;
  provider: string;
  context_length: number;
  supports_streaming: boolean;
  input_price_per_1k?: string;
  output_price_per_1k?: string;
}

export interface MCPServer {
  id: string;
  name: string;
  command: string;
  args: string[];
  enabled: boolean;
  status: 'running' | 'stopped' | 'error' | 'starting';
  tools: MCPTool[];
  resources: MCPResource[];
  health_status?: {
    status: 'healthy' | 'unhealthy' | 'unknown';
    last_check: number;
    error?: string;
  };
}

export interface MCPTool {
  name: string;
  description: string;
  parameters: Record<string, any>;
}

export interface MCPResource {
  uri: string;
  name: string;
  description?: string;
  mime_type?: string;
}

export interface UsageRecord {
  id?: number;
  timestamp: number;
  provider: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  cost: string;
  conversation_id?: string;
  request_id: string;
}

export interface BillingLimits {
  daily_limit?: string;
  monthly_limit?: string;
  per_model_limits: Record<string, string>;
  per_conversation_limits: Record<string, string>;
}

export interface AppConfig {
  theme: 'light' | 'dark' | 'system';
  language: string;
  model_providers: ModelProvider[];
  mcp_servers: MCPServer[];
  billing_limits: BillingLimits;
  auto_save: boolean;
  streaming: boolean;
}

export interface StreamingMessage {
  id: string;
  content: string;
  is_complete: boolean;
  error?: string;
}

export interface ChatRequest {
  messages: Message[];
  model: string;
  provider: string;
  temperature?: number;
  max_tokens?: number;
  stream: boolean;
  conversation_id?: string;
}

export interface ChatResponse {
  message: Message;
  usage?: {
    input_tokens: number;
    output_tokens: number;
    cost: string;
  };
}

export interface AppError {
  id: string;
  type: 'network' | 'auth' | 'rate_limit' | 'model' | 'mcp' | 'billing' | 'system';
  message: string;
  details?: string;
  timestamp: number;
  recoverable: boolean;
  retry_count?: number;
  severity?: 'info' | 'warning' | 'error';
}