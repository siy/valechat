-- migrations/001_initial.sql
-- Initial database schema for ValeChat
-- Uses proper decimal handling and optimized indexes

PRAGMA foreign_keys = ON;

-- Conversations table for chat sessions
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()), -- Unix timestamp
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    model_provider TEXT,
    model_name TEXT,
    system_prompt TEXT,
    total_cost TEXT, -- Store as string representation of rust_decimal::Decimal
    message_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'active' CHECK (status IN ('active', 'archived', 'deleted')),
    settings TEXT -- JSON string for session settings
);

CREATE INDEX idx_conversations_updated_at ON conversations(updated_at);
CREATE INDEX idx_conversations_provider ON conversations(model_provider);
CREATE INDEX idx_conversations_status ON conversations(status);

-- Messages table for individual chat messages
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    content_type TEXT DEFAULT 'text' CHECK (content_type IN ('text', 'multimodal', 'tool_call', 'tool_result')),
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    model_used TEXT,
    provider TEXT,
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    cost TEXT, -- Store as string representation of rust_decimal::Decimal
    processing_time_ms INTEGER,
    metadata TEXT, -- JSON string for additional metadata
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX idx_messages_conversation_id ON messages(conversation_id);
CREATE INDEX idx_messages_timestamp ON messages(timestamp);
CREATE INDEX idx_messages_provider ON messages(provider);
CREATE INDEX idx_messages_role ON messages(role);

-- Usage records table for billing tracking
CREATE TABLE usage_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost TEXT NOT NULL, -- Store as string representation of rust_decimal::Decimal
    conversation_id TEXT,
    message_id TEXT,
    request_id TEXT UNIQUE, -- For deduplication
    billing_period TEXT, -- YYYY-MM format for monthly aggregation
    verified BOOLEAN DEFAULT FALSE, -- For cost verification
    verification_timestamp INTEGER,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id),
    FOREIGN KEY (message_id) REFERENCES messages(id)
);

CREATE INDEX idx_usage_records_timestamp ON usage_records(timestamp);
CREATE INDEX idx_usage_records_provider ON usage_records(provider);
CREATE INDEX idx_usage_records_billing_period ON usage_records(billing_period);
CREATE INDEX idx_usage_records_request_id ON usage_records(request_id);
CREATE INDEX idx_usage_records_verified ON usage_records(verified);

-- Tool invocations table for MCP tool tracking
CREATE TABLE tool_invocations (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    server_name TEXT NOT NULL,
    arguments TEXT NOT NULL, -- JSON string
    result TEXT, -- JSON string, NULL if failed
    error TEXT, -- Error message if failed
    duration_ms INTEGER,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    cost TEXT, -- Some tools may have associated costs
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX idx_tool_invocations_message_id ON tool_invocations(message_id);
CREATE INDEX idx_tool_invocations_tool_name ON tool_invocations(tool_name);
CREATE INDEX idx_tool_invocations_server_name ON tool_invocations(server_name);
CREATE INDEX idx_tool_invocations_timestamp ON tool_invocations(timestamp);

-- Billing summary table for quick reporting
CREATE TABLE billing_summaries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    billing_period TEXT NOT NULL, -- YYYY-MM format
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    total_input_tokens INTEGER DEFAULT 0,
    total_output_tokens INTEGER DEFAULT 0,
    total_cost TEXT NOT NULL, -- Store as string representation of rust_decimal::Decimal
    request_count INTEGER DEFAULT 0,
    last_updated INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(billing_period, provider, model)
);

CREATE INDEX idx_billing_summaries_period ON billing_summaries(billing_period);
CREATE INDEX idx_billing_summaries_provider ON billing_summaries(provider);

-- API keys audit table for security tracking
CREATE TABLE api_key_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('created', 'updated', 'deleted', 'accessed')),
    timestamp INTEGER NOT NULL DEFAULT (unixepoch()),
    success BOOLEAN NOT NULL,
    error_message TEXT
);

CREATE INDEX idx_api_key_audit_timestamp ON api_key_audit(timestamp);
CREATE INDEX idx_api_key_audit_provider ON api_key_audit(provider);

-- Settings table for application configuration
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Insert default settings
INSERT INTO app_settings (key, value) VALUES 
    ('database_version', '1'),
    ('created_at', unixepoch()),
    ('default_spending_limit', '100.00'), -- $100 default monthly limit
    ('billing_alerts_enabled', 'true'),
    ('backup_enabled', 'true');

-- Create triggers for automatic timestamp updates
CREATE TRIGGER update_conversations_timestamp 
    AFTER UPDATE ON conversations
    BEGIN
        UPDATE conversations SET updated_at = unixepoch() WHERE id = NEW.id;
    END;

CREATE TRIGGER update_billing_summaries_timestamp
    AFTER UPDATE ON billing_summaries
    BEGIN
        UPDATE billing_summaries SET last_updated = unixepoch() WHERE id = NEW.id;
    END;

-- Create trigger to update conversation message count
CREATE TRIGGER increment_message_count
    AFTER INSERT ON messages
    BEGIN
        UPDATE conversations 
        SET message_count = message_count + 1,
            updated_at = unixepoch()
        WHERE id = NEW.conversation_id;
    END;

CREATE TRIGGER decrement_message_count
    AFTER DELETE ON messages
    BEGIN
        UPDATE conversations 
        SET message_count = message_count - 1,
            updated_at = unixepoch()
        WHERE id = OLD.conversation_id;
    END;