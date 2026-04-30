use anyhow::{bail, Result};
use async_trait::async_trait;
use minus_core::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use minus_db::Database;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

/// Tool: memory.short_save
pub struct MemoryShortSaveTool;

#[async_trait]
impl Tool for MemoryShortSaveTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_short_save".into(),
            description: "Save a short fact (max 120 chars) that will always be visible to the assistant.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique identifier for this fact (e.g. 'user_age')" },
                    "fact": { "type": "string", "maxLength": 120, "description": "The fact to save" }
                },
                "required": ["id", "fact"]
            }),
            risk: ToolRisk::Low,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Arc<Database>>())
            .context("Database not found in ToolContext")?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;
        let fact = call.arguments["fact"].as_str().context("Missing fact")?;

        db.save_memory(id, "short", fact, None).await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "memory_short_save".into(),
            content: format!("Fact '{}' saved to short-term memory.", id),
            is_error: false,
        })
    }
}

/// Tool: memory.long_save
pub struct MemoryLongSaveTool;

#[async_trait]
impl Tool for MemoryLongSaveTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_long_save".into(),
            description: "Save a detailed long-term memory. Provide a brief summary (brief) and the full content.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique ID for this memory (optional, will generate Uuid if not provided)" },
                    "brief": { "type": "string", "description": "A short summary visible in the system prompt" },
                    "content": { "type": "string", "description": "The full detailed content" }
                },
                "required": ["brief", "content"]
            }),
            risk: ToolRisk::Low,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Arc<Database>>())
            .context("Database not found in ToolContext")?;
        
        let id = call.arguments["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| Uuid::new_v4().to_string());
        let brief = call.arguments["brief"].as_str().context("Missing brief")?;
        let content = call.arguments["content"].as_str().context("Missing content")?;

        db.save_memory(&id, "long", brief, Some(content)).await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "memory_long_save".into(),
            content: format!("Long-term memory '{}' saved.", id),
            is_error: false,
        })
    }
}

/// Tool: memory.search
pub struct MemorySearchTool;

#[async_trait]
impl Tool for MemorySearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".into(),
            description: "Search for long-term memories using keywords.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "terms": { 
                        "type": "array", 
                        "items": { "type": "string" },
                        "description": "Keywords to search for"
                    }
                },
                "required": ["terms"]
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Arc<Database>>())
            .context("Database not found in ToolContext")?;
        
        let terms: Vec<String> = call.arguments["terms"].as_array()
            .context("Missing terms")?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        let results = db.search_memories(&terms).await?;
        let content = if results.is_empty() {
            "No matching memories found.".into()
        } else {
            let lines: Vec<String> = results.iter()
                .map(|r| format!("[{}] Brief: {}\nContent: {}", r.id, r.brief, r.content.as_deref().unwrap_or("(no content)")))
                .collect();
            lines.join("\n---\n")
        };

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "memory_search".into(),
            content,
            is_error: false,
        })
    }
}

/// Tool: memory.manage
pub struct MemoryManageTool;

#[async_trait]
impl Tool for MemoryManageTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_manage".into(),
            description: "Update, append to, or delete a memory.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The ID of the memory to manage" },
                    "action": { 
                        "type": "string", 
                        "enum": ["replace", "append", "delete"],
                        "description": "What to do with the memory"
                    },
                    "brief": { "type": "string", "description": "New brief summary (optional)" },
                    "content": { "type": "string", "description": "New content or content to append (optional)" }
                },
                "required": ["id", "action"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Arc<Database>>())
            .context("Database not found in ToolContext")?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;
        let action = call.arguments["action"].as_str().context("Missing action")?;

        match action {
            "delete" => {
                if db.delete_memory(id).await? {
                    Ok(ToolResult {
                        tool_call_id: call.id,
                        name: "memory_manage".into(),
                        content: format!("Memory '{}' deleted.", id),
                        is_error: false,
                    })
                } else {
                    Ok(ToolResult {
                        tool_call_id: call.id,
                        name: "memory_manage".into(),
                        content: format!("Memory '{}' not found.", id),
                        is_error: true,
                    })
                }
            }
            "replace" => {
                let brief = call.arguments["brief"].as_str().context("Missing brief for replace")?;
                let content = call.arguments["content"].as_str();
                let existing = db.get_memory(id).await?;
                if let Some(m) = existing {
                    db.save_memory(id, &m.kind, brief, content).await?;
                    Ok(ToolResult {
                        tool_call_id: call.id,
                        name: "memory_manage".into(),
                        content: format!("Memory '{}' replaced.", id),
                        is_error: false,
                    })
                } else {
                    bail!("Memory not found: {}", id);
                }
            }
            "append" => {
                let content_to_add = call.arguments["content"].as_str().context("Missing content for append")?;
                let existing = db.get_memory(id).await?;
                if let Some(m) = existing {
                    let new_content = match m.content {
                        Some(old) => format!("{}\n{}", old, content_to_add),
                        None => content_to_add.to_string(),
                    };
                    db.save_memory(id, &m.kind, &m.brief, Some(&new_content)).await?;
                    Ok(ToolResult {
                        tool_call_id: call.id,
                        name: "memory_manage".into(),
                        content: format!("Content appended to memory '{}'.", id),
                        is_error: false,
                    })
                } else {
                    bail!("Memory not found: {}", id);
                }
            }
            _ => bail!("Unsupported action: {}", action),
        }
    }
}

use anyhow::Context;
