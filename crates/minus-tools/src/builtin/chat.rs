use anyhow::{Result, Context};
use async_trait::async_trait;
use minus_api::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use serde_json::json;

/// Tool: chat_list
pub struct ChatListTool;

#[async_trait]
impl Tool for ChatListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "chat_list".into(),
            description: "List all available chat conversations.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let chats = ctx.db.list_chats().await?;
        let content = if chats.is_empty() {
            "No chats found.".into()
        } else {
            let lines: Vec<String> = chats.iter()
                .map(|c| format!("[{}] {} (Channel: {})", c.id, c.title.as_deref().unwrap_or("(no title)"), c.channel_id))
                .collect();
            lines.join("\n")
        };

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "chat_list".into(),
            content,
            is_error: false,
        })
    }
}

/// Tool: chat_read
pub struct ChatReadTool;

#[async_trait]
impl Tool for ChatReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "chat_read".into(),
            description: "Read recent messages from a specific chat conversation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "string", "description": "The ID of the chat to read" },
                    "limit": { "type": "integer", "default": 20, "description": "Number of messages to read" }
                },
                "required": ["chat_id"]
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let chat_id = call.arguments["chat_id"].as_str().context("Missing chat_id")?;
        let limit = call.arguments["limit"].as_i64().unwrap_or(20);

        let messages = ctx.db.get_messages(chat_id, limit).await?;
        let content = if messages.is_empty() {
            "No messages found in this chat.".into()
        } else {
            let lines: Vec<String> = messages.iter()
                .map(|m| format!("[{}] {}: {}", m.created_at, m.role, m.content))
                .collect();
            lines.join("\n")
        };

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "chat_read".into(),
            content,
            is_error: false,
        })
    }
}

/// Tool: chat_search — searches messages across all chats
pub struct ChatSearchTool;

#[async_trait]
impl Tool for ChatSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "chat_search".into(),
            description: "Search for messages across all conversations using keywords.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "terms": { 
                        "type": "array", 
                        "items": { "type": "string" },
                        "description": "Keywords to search for"
                    },
                    "limit": { "type": "integer", "default": 20 }
                },
                "required": ["terms"]
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let terms: Vec<String> = call.arguments["terms"].as_array()
            .context("Missing terms")?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
            .collect();

        // Fetch recent messages from all chats and filter client-side
        let chats = ctx.db.list_chats().await?;
        let mut results = Vec::new();
        for chat in &chats {
            let messages = ctx.db.get_messages(&chat.id, 100).await?;
            for m in messages {
                if terms.iter().any(|t| m.content.to_lowercase().contains(t)) {
                    results.push(m);
                }
            }
        }

        let content = if results.is_empty() {
            "No matching messages found.".into()
        } else {
            let lines: Vec<String> = results.iter().take(20)
                .map(|m| format!("[{}] Chat: {} | {}: {}", m.created_at, m.chat_id, m.role, m.content))
                .collect();
            lines.join("\n")
        };

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "chat_search".into(),
            content,
            is_error: false,
        })
    }
}
