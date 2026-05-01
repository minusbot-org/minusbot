use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_core::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use minus_db::Database;
use serde_json::json;

/// Tool: memory_write
pub struct MemoryWriteTool;

#[async_trait]
impl Tool for MemoryWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_write".into(),
            description: "Save or update a memory entry. Use for facts, user preferences, or long-term information.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique identifier for the memory entry (e.g., 'user_birthday')" },
                    "brief": { "type": "string", "description": "A short summary of the information." },
                    "content": { "type": "string", "description": "Detailed information to store." },
                    "is_important": { "type": "boolean", "description": "If true, this memory will be prioritized in the assistant's context.", "default": false }
                },
                "required": ["id", "brief", "content"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Database>())
            .context("Database not found in ToolContext")?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;
        let brief = call.arguments["brief"].as_str().context("Missing brief")?;
        let content = call.arguments["content"].as_str().context("Missing content")?;
        let is_important = call.arguments["is_important"].as_bool().unwrap_or(false);

        db.save_memory(id, "long", brief, Some(content), is_important).await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "memory_write".into(),
            content: format!("Memory '{}' saved successfully.", id),
            is_error: false,
        })
    }
}

/// Tool: memory_read
pub struct MemoryReadTool;

#[async_trait]
impl Tool for MemoryReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_read".into(),
            description: "Retrieve a specific memory by its ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The unique identifier of the memory to read" }
                },
                "required": ["id"]
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Database>())
            .context("Database not found in ToolContext")?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;
        let memory = db.get_memory(id).await?;

        let content = match memory.as_ref() {
            Some(m) => format!("ID: {}\nBrief: {}\nContent: {}\nImportant: {}\nCreated: {}", 
                m.id, m.brief, m.content.as_deref().unwrap_or("(no content)"), m.is_important, m.created_at),
            None => format!("Memory with ID '{}' not found.", id),
        };

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "memory_read".into(),
            content,
            is_error: memory.is_none(),
        })
    }
}

/// Tool: memory_search
pub struct MemorySearchTool;

#[async_trait]
impl Tool for MemorySearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".into(),
            description: "Search for memories using keywords in their brief or content.".into(),
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
            .and_then(|s| s.downcast_ref::<Database>())
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

/// Tool: memory_delete
pub struct MemoryDeleteTool;

#[async_trait]
impl Tool for MemoryDeleteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_delete".into(),
            description: "Delete a memory entry permanently.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The unique identifier of the memory to delete" }
                },
                "required": ["id"]
            }),
            risk: ToolRisk::High,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let db = ctx.store.as_ref()
            .and_then(|s| s.downcast_ref::<Database>())
            .context("Database not found in ToolContext")?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;

        if db.delete_memory(id).await? {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: "memory_delete".into(),
                content: format!("Memory '{}' deleted successfully.", id),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: "memory_delete".into(),
                content: format!("Memory '{}' not found.", id),
                is_error: true,
            })
        }
    }
}
