use std::collections::HashMap;
use sqlx::{SqlitePool, Row};
use tracing::{debug, info};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use chrono::{DateTime, Utc};
use serde_json;

use crate::error::{Error, Result};
use crate::chat::types::{ChatSession, ChatMessage, MessageRole, MessageContent, ToolInvocation, SessionSettings, SessionStatus};
use crate::storage::database::decimal_helpers;

/// Repository for managing conversations and messages in the database
pub struct ConversationRepository {
    pool: SqlitePool,
}

impl ConversationRepository {
    /// Create a new conversation repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Save a new conversation to the database
    pub async fn create_conversation(&self, session: &ChatSession) -> Result<()> {
        debug!("Creating conversation in database: {}", session.id);

        let settings_json = serde_json::to_string(&session.settings)
            .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to serialize settings: {}", e))))?;

        let total_cost = session.metrics.total_cost.to_string();
        let status_str = match session.status {
            SessionStatus::Active => "active",
            SessionStatus::Paused => "paused", 
            SessionStatus::Archived => "archived",
            SessionStatus::Error(_) => "error",
        };

        sqlx::query(
            r#"
            INSERT INTO conversations (
                id, title, created_at, updated_at, model_provider, model_name, 
                system_prompt, total_cost, message_count, status, settings
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&session.id)
        .bind(&session.title)
        .bind(session.created_at.timestamp())
        .bind(session.updated_at.timestamp())
        .bind(&session.model_provider)
        .bind(&session.model_name)
        .bind(&session.system_prompt)
        .bind(&total_cost)
        .bind(session.metrics.message_count as i64)
        .bind(status_str)
        .bind(&settings_json)
        .execute(&self.pool)
        .await?;

        info!("Successfully created conversation: {}", session.id);
        Ok(())
    }

    /// Update an existing conversation
    pub async fn update_conversation(&self, session: &ChatSession) -> Result<()> {
        debug!("Updating conversation in database: {}", session.id);

        let settings_json = serde_json::to_string(&session.settings)
            .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to serialize settings: {}", e))))?;

        let total_cost = session.metrics.total_cost.to_string();
        let status_str = match session.status {
            SessionStatus::Active => "active",
            SessionStatus::Paused => "paused", 
            SessionStatus::Archived => "archived",
            SessionStatus::Error(_) => "error",
        };

        let rows_affected = sqlx::query(
            r#"
            UPDATE conversations SET 
                title = ?, updated_at = ?, model_provider = ?, model_name = ?,
                system_prompt = ?, total_cost = ?, message_count = ?, status = ?, settings = ?
            WHERE id = ?
            "#
        )
        .bind(&session.title)
        .bind(session.updated_at.timestamp())
        .bind(&session.model_provider)
        .bind(&session.model_name)
        .bind(&session.system_prompt)
        .bind(&total_cost)
        .bind(session.metrics.message_count as i64)
        .bind(status_str)
        .bind(&settings_json)
        .bind(&session.id)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            return Err(Error::Database(sqlx::Error::RowNotFound));
        }

        debug!("Successfully updated conversation: {}", session.id);
        Ok(())
    }

    /// Retrieve a conversation by ID
    pub async fn get_conversation(&self, conversation_id: &str) -> Result<Option<ChatSession>> {
        debug!("Retrieving conversation from database: {}", conversation_id);

        let row = sqlx::query(
            r#"
            SELECT id, title, created_at, updated_at, model_provider, model_name,
                   system_prompt, total_cost, message_count, status, settings
            FROM conversations 
            WHERE id = ?
            "#
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let settings_json: String = row.get("settings");
                let settings: SessionSettings = serde_json::from_str(&settings_json)
                    .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to deserialize settings: {}", e))))?;

                let total_cost_str: String = row.get("total_cost");
                let total_cost = decimal_helpers::string_to_decimal(&total_cost_str)?;

                let created_at_timestamp: i64 = row.get("created_at");
                let updated_at_timestamp: i64 = row.get("updated_at");

                let status_str: String = row.get("status");
                let status = match status_str.as_str() {
                    "active" => SessionStatus::Active,
                    "paused" => SessionStatus::Paused,
                    "archived" => SessionStatus::Archived,
                    "error" => SessionStatus::Error("Unknown error".to_string()),
                    _ => SessionStatus::Active,
                };

                let mut session = ChatSession::new(
                    row.get::<String, _>("title"),
                    row.get::<String, _>("model_provider"),
                    row.get::<String, _>("model_name"),
                );

                session.id = row.get("id");
                session.created_at = DateTime::from_timestamp(created_at_timestamp, 0)
                    .unwrap_or_else(|| Utc::now());
                session.updated_at = DateTime::from_timestamp(updated_at_timestamp, 0)
                    .unwrap_or_else(|| Utc::now());
                session.system_prompt = row.get("system_prompt");
                session.status = status;
                session.settings = settings;
                session.metrics.message_count = row.get::<i64, _>("message_count") as u64;
                session.metrics.total_cost = total_cost.to_f64().unwrap_or(0.0);

                debug!("Successfully retrieved conversation: {}", conversation_id);
                Ok(Some(session))
            }
            None => {
                debug!("Conversation not found: {}", conversation_id);
                Ok(None)
            }
        }
    }

    /// List conversations with pagination
    pub async fn list_conversations(
        &self, 
        limit: Option<i32>, 
        offset: Option<i32>
    ) -> Result<Vec<ChatSession>> {
        debug!("Listing conversations with limit: {:?}, offset: {:?}", limit, offset);

        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let rows = sqlx::query(
            r#"
            SELECT id, title, created_at, updated_at, model_provider, model_name,
                   system_prompt, total_cost, message_count, status, settings
            FROM conversations 
            WHERE status != 'deleted'
            ORDER BY updated_at DESC
            LIMIT ? OFFSET ?
            "#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let mut sessions = Vec::new();
        for row in rows {
            let settings_json: String = row.get("settings");
            let settings: SessionSettings = serde_json::from_str(&settings_json)
                .unwrap_or_default();

            let total_cost_str: String = row.get("total_cost");
            let total_cost = decimal_helpers::string_to_decimal(&total_cost_str)
                .unwrap_or(Decimal::ZERO);

            let created_at_timestamp: i64 = row.get("created_at");
            let updated_at_timestamp: i64 = row.get("updated_at");

            let status_str: String = row.get("status");
            let status = match status_str.as_str() {
                "active" => SessionStatus::Active,
                "paused" => SessionStatus::Paused,
                "archived" => SessionStatus::Archived,
                "error" => SessionStatus::Error("Unknown error".to_string()),
                _ => SessionStatus::Active,
            };

            let mut session = ChatSession::new(
                row.get::<String, _>("title"),
                row.get::<String, _>("model_provider"),
                row.get::<String, _>("model_name"),
            );

            session.id = row.get("id");
            session.created_at = DateTime::from_timestamp(created_at_timestamp, 0)
                .unwrap_or_else(|| Utc::now());
            session.updated_at = DateTime::from_timestamp(updated_at_timestamp, 0)
                .unwrap_or_else(|| Utc::now());
            session.system_prompt = row.get("system_prompt");
            session.status = status;
            session.settings = settings;
            session.metrics.message_count = row.get::<i64, _>("message_count") as u64;
            session.metrics.total_cost = total_cost.to_f64().unwrap_or(0.0);

            sessions.push(session);
        }

        debug!("Retrieved {} conversations", sessions.len());
        Ok(sessions)
    }

    /// Delete a conversation and all its messages
    pub async fn delete_conversation(&self, conversation_id: &str) -> Result<()> {
        debug!("Deleting conversation: {}", conversation_id);

        // Start a transaction
        let mut tx = self.pool.begin().await?;

        // Delete all messages first (foreign key constraint)
        sqlx::query("DELETE FROM messages WHERE conversation_id = ?")
            .bind(conversation_id)
            .execute(&mut *tx)
            .await?;

        // Delete the conversation
        let rows_affected = sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();

        if rows_affected == 0 {
            tx.rollback().await?;
            return Err(Error::Database(sqlx::Error::RowNotFound));
        }

        tx.commit().await?;

        info!("Successfully deleted conversation: {}", conversation_id);
        Ok(())
    }

    /// Save a message to the database
    pub async fn create_message(&self, message: &ChatMessage) -> Result<()> {
        debug!("Creating message in database: {}", message.id);

        let role_str = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool => "tool",
        };

        let (content_str, content_type) = match &message.content {
            MessageContent::Text(text) => (text.clone(), "text"),
            MessageContent::MultiModal { text, .. } => {
                // For now, just store the text part
                (text.clone().unwrap_or_default(), "multimodal")
            }
            MessageContent::ToolCall { tool_name, arguments, call_id } => {
                let content = serde_json::json!({
                    "tool_name": tool_name,
                    "arguments": arguments,
                    "call_id": call_id
                });
                (content.to_string(), "tool_call")
            }
            MessageContent::ToolResult { call_id, result, is_error } => {
                let content = serde_json::json!({
                    "call_id": call_id,
                    "result": result,
                    "is_error": is_error
                });
                (content.to_string(), "tool_result")
            }
        };

        let metadata_json = serde_json::to_string(&message.metadata)
            .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to serialize metadata: {}", e))))?;

        sqlx::query(
            r#"
            INSERT INTO messages (
                id, conversation_id, role, content, content_type, timestamp,
                model_used, provider, input_tokens, output_tokens, cost,
                processing_time_ms, metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&message.id)
        .bind(&message.session_id)
        .bind(role_str)
        .bind(&content_str)
        .bind(content_type)
        .bind(message.timestamp.timestamp())
        .bind::<Option<String>>(None) // model_used - will be set later
        .bind::<Option<String>>(None) // provider - will be set later
        .bind(0i32) // input_tokens - will be set later
        .bind(0i32) // output_tokens - will be set later
        .bind("0.00") // cost - will be set later
        .bind::<Option<i32>>(None) // processing_time_ms - will be set later
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

        // Save tool invocations if any
        for invocation in &message.tool_invocations {
            self.create_tool_invocation(invocation).await?;
        }

        debug!("Successfully created message: {}", message.id);
        Ok(())
    }

    /// Save a tool invocation to the database
    async fn create_tool_invocation(&self, invocation: &ToolInvocation) -> Result<()> {
        debug!("Creating tool invocation: {}", invocation.id);

        let arguments_json = serde_json::to_string(&invocation.arguments)
            .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to serialize arguments: {}", e))))?;

        let result_json = match &invocation.result {
            Some(result) => Some(serde_json::to_string(result)
                .map_err(|e| Error::Database(sqlx::Error::decode(format!("Failed to serialize result: {}", e))))?),
            None => None,
        };

        sqlx::query(
            r#"
            INSERT INTO tool_invocations (
                id, message_id, tool_name, server_name, arguments, result,
                error, duration_ms, timestamp, cost
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&invocation.id)
        .bind("") // message_id will be set by the caller
        .bind(&invocation.tool_name)
        .bind(&invocation.server_name)
        .bind(&arguments_json)
        .bind(&result_json)
        .bind(&invocation.error)
        .bind(invocation.duration_ms.map(|d| d as i32))
        .bind(invocation.timestamp.timestamp())
        .bind("0.00") // cost - for future use
        .execute(&self.pool)
        .await?;

        debug!("Successfully created tool invocation: {}", invocation.id);
        Ok(())
    }

    /// Retrieve messages for a conversation
    pub async fn get_messages(&self, conversation_id: &str) -> Result<Vec<ChatMessage>> {
        debug!("Retrieving messages for conversation: {}", conversation_id);

        let rows = sqlx::query(
            r#"
            SELECT id, conversation_id, role, content, content_type, timestamp,
                   model_used, provider, input_tokens, output_tokens, cost,
                   processing_time_ms, metadata
            FROM messages 
            WHERE conversation_id = ?
            ORDER BY timestamp ASC
            "#
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            let role_str: String = row.get("role");
            let role = match role_str.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                "tool" => MessageRole::Tool,
                _ => MessageRole::User,
            };

            let content_str: String = row.get("content");
            let content_type: String = row.get("content_type");
            let content = match content_type.as_str() {
                "text" => MessageContent::Text(content_str),
                "multimodal" => MessageContent::MultiModal {
                    text: Some(content_str),
                    attachments: Vec::new(), // TODO: implement attachment storage
                },
                "tool_call" => {
                    let json: serde_json::Value = serde_json::from_str(&content_str)
                        .unwrap_or(serde_json::json!({}));
                    MessageContent::ToolCall {
                        tool_name: json.get("tool_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        arguments: json.get("arguments").cloned().unwrap_or(serde_json::json!({})),
                        call_id: json.get("call_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    }
                }
                "tool_result" => {
                    let json: serde_json::Value = serde_json::from_str(&content_str)
                        .unwrap_or(serde_json::json!({}));
                    MessageContent::ToolResult {
                        call_id: json.get("call_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        result: json.get("result").cloned().unwrap_or(serde_json::json!({})),
                        is_error: json.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false),
                    }
                }
                _ => MessageContent::Text(content_str),
            };

            let timestamp_unix: i64 = row.get("timestamp");
            let timestamp = DateTime::from_timestamp(timestamp_unix, 0)
                .unwrap_or_else(|| Utc::now());

            let metadata_json: String = row.get("metadata");
            let metadata: HashMap<String, serde_json::Value> = serde_json::from_str(&metadata_json)
                .unwrap_or_default();

            let mut message = ChatMessage::new(
                conversation_id.to_string(),
                role,
                content,
            );

            message.id = row.get("id");
            message.timestamp = timestamp;
            message.metadata = metadata;

            // Get tool invocations for this message
            message.tool_invocations = self.get_tool_invocations(&message.id).await?;

            messages.push(message);
        }

        debug!("Retrieved {} messages for conversation: {}", messages.len(), conversation_id);
        Ok(messages)
    }

    /// Get tool invocations for a message
    async fn get_tool_invocations(&self, message_id: &str) -> Result<Vec<ToolInvocation>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tool_name, server_name, arguments, result, error,
                   duration_ms, timestamp, cost
            FROM tool_invocations 
            WHERE message_id = ?
            ORDER BY timestamp ASC
            "#
        )
        .bind(message_id)
        .fetch_all(&self.pool)
        .await?;

        let mut invocations = Vec::new();
        for row in rows {
            let arguments_json: String = row.get("arguments");
            let arguments: serde_json::Value = serde_json::from_str(&arguments_json)
                .unwrap_or(serde_json::json!({}));

            let result_json: Option<String> = row.get("result");
            let result = match result_json {
                Some(json_str) => serde_json::from_str(&json_str).ok(),
                None => None,
            };

            let timestamp_unix: i64 = row.get("timestamp");
            let timestamp = DateTime::from_timestamp(timestamp_unix, 0)
                .unwrap_or_else(|| Utc::now());

            let mut invocation = ToolInvocation::new(
                row.get::<String, _>("tool_name"),
                row.get::<String, _>("server_name"),
                arguments,
            );

            invocation.id = row.get("id");
            invocation.result = result;
            invocation.error = row.get("error");
            invocation.duration_ms = row.get::<Option<i32>, _>("duration_ms").map(|d| d as u64);
            invocation.timestamp = timestamp;

            invocations.push(invocation);
        }

        Ok(invocations)
    }

    /// Update conversation title
    pub async fn update_conversation_title(&self, conversation_id: &str, title: &str) -> Result<()> {
        debug!("Updating conversation title: {} -> {}", conversation_id, title);

        let rows_affected = sqlx::query(
            "UPDATE conversations SET title = ?, updated_at = ? WHERE id = ?"
        )
        .bind(title)
        .bind(Utc::now().timestamp())
        .bind(conversation_id)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows_affected == 0 {
            return Err(Error::Database(sqlx::Error::RowNotFound));
        }

        debug!("Successfully updated conversation title: {}", conversation_id);
        Ok(())
    }

    /// Get conversation statistics
    pub async fn get_conversation_statistics(&self) -> Result<ConversationStatistics> {
        debug!("Getting conversation statistics");

        let total_conversations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations WHERE status != 'deleted'"
        )
        .fetch_one(&self.pool)
        .await?;

        let active_conversations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations WHERE status = 'active'"
        )
        .fetch_one(&self.pool)
        .await?;

        let total_messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&self.pool)
            .await?;

        let total_tool_invocations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tool_invocations")
            .fetch_one(&self.pool)
            .await?;

        Ok(ConversationStatistics {
            total_conversations: total_conversations as u64,
            active_conversations: active_conversations as u64,
            total_messages: total_messages as u64,
            total_tool_invocations: total_tool_invocations as u64,
        })
    }
}

/// Statistics about conversations
#[derive(Debug, Clone)]
pub struct ConversationStatistics {
    pub total_conversations: u64,
    pub active_conversations: u64,
    pub total_messages: u64,
    pub total_tool_invocations: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::{SessionSettings, MessageContent};
    use tempfile::TempDir;
    use crate::storage::Database;
    use crate::platform::AppPaths;

    async fn create_test_repository() -> (ConversationRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let paths = AppPaths::with_data_dir(temp_dir.path()).unwrap();
        let db = Database::new(&paths).await.unwrap();
        let repo = ConversationRepository::new(db.pool().clone());
        (repo, temp_dir)
    }

    #[tokio::test]
    async fn test_create_and_get_conversation() {
        let (repo, _temp_dir) = create_test_repository().await;

        let mut session = ChatSession::new("Test Chat", "openai", "gpt-4");
        session.system_prompt = Some("You are a helpful assistant".to_string());

        // Create conversation
        repo.create_conversation(&session).await.unwrap();

        // Retrieve conversation
        let retrieved = repo.get_conversation(&session.id).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved_session = retrieved.unwrap();
        assert_eq!(retrieved_session.id, session.id);
        assert_eq!(retrieved_session.title, session.title);
        assert_eq!(retrieved_session.model_provider, session.model_provider);
        assert_eq!(retrieved_session.system_prompt, session.system_prompt);
    }

    #[tokio::test]
    async fn test_create_and_get_message() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Create a conversation first
        let session = ChatSession::new("Test Chat", "openai", "gpt-4");
        repo.create_conversation(&session).await.unwrap();

        // Create a message
        let message = ChatMessage::new(
            session.id.clone(),
            MessageRole::User,
            MessageContent::text("Hello, world!"),
        );

        repo.create_message(&message).await.unwrap();

        // Retrieve messages
        let messages = repo.get_messages(&session.id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, message.id);
        assert_eq!(messages[0].role, MessageRole::User);
    }

    #[tokio::test]
    async fn test_list_conversations() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Create multiple conversations
        for i in 0..3 {
            let session = ChatSession::new(format!("Test Chat {}", i), "openai", "gpt-4");
            repo.create_conversation(&session).await.unwrap();
        }

        // List conversations
        let conversations = repo.list_conversations(Some(10), Some(0)).await.unwrap();
        assert_eq!(conversations.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let (repo, _temp_dir) = create_test_repository().await;

        let session = ChatSession::new("Test Chat", "openai", "gpt-4");
        repo.create_conversation(&session).await.unwrap();

        // Verify it exists
        let retrieved = repo.get_conversation(&session.id).await.unwrap();
        assert!(retrieved.is_some());

        // Delete it
        repo.delete_conversation(&session.id).await.unwrap();

        // Verify it's gone
        let retrieved = repo.get_conversation(&session.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_conversation_statistics() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Create some test data
        let session = ChatSession::new("Test Chat", "openai", "gpt-4");
        repo.create_conversation(&session).await.unwrap();

        let message = ChatMessage::new(
            session.id.clone(),
            MessageRole::User,
            MessageContent::text("Hello!"),
        );
        repo.create_message(&message).await.unwrap();

        // Get statistics
        let stats = repo.get_conversation_statistics().await.unwrap();
        assert_eq!(stats.total_conversations, 1);
        assert_eq!(stats.active_conversations, 1);
        assert_eq!(stats.total_messages, 1);
    }
}