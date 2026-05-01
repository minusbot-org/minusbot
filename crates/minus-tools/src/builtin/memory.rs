use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_core::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use minus_db::Database;
use serde_json::json;

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

/// Tool: memory.manage
pub struct MemoryManageTool;

#[async_trait]
impl Tool for MemoryManageTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_manage".into(),
            description: "Save, update, append to, or delete a memory. Use 'save' for new entries (with optional brief).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique identifier for the memory entry" },
                    "action": { 
                        "type": "string", 
                        "enum": ["save", "replace", "append", "delete"],
                        "description": "What to do with the memory"
                    },
                    "content": { "type": "string", "description": "The information to store or append." },
                    "brief": { "type": "string", "description": "A short summary. Optional for 'save' if content < 128 chars." },
                    "important": { "type": "boolean", "description": "If true, this memory will be loaded into the system prompt briefs.", "default": false }
                },
                "required": ["id", "action"]
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
        let action = call.arguments["action"].as_str().context("Missing action")?;
        let is_important = call.arguments["important"].as_bool().unwrap_or(false);

        match action {
            "save" => {
                let content = call.arguments["content"].as_str().context("Missing content for save")?;
                let brief = call.arguments["brief"].as_str();

                let (kind, final_brief, final_content) = match brief {
                    Some(b) => ("long", b, Some(content)),
                    None => {
                        if content.len() >= 128 {
                            bail!("Content is too long (>= 128 chars). Please provide a 'brief' summary.");
                        }
                        ("short", content, None)
                    }
                };
                db.save_memory(id, kind, final_brief, final_content, is_important).await?;
                Ok(ToolResult {
                    tool_call_id: call.id,
                    name: "memory_manage".into(),
                    content: format!("Memory '{}' saved successfully as {}. Important: {}.", id, kind, is_important),
                    is_error: false,
                })
            }
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
                    db.save_memory(id, &m.kind, brief, content, is_important).await?;
                    Ok(ToolResult {
                        tool_call_id: call.id,
                        name: "memory_manage".into(),
                        content: format!("Memory '{}' replaced. Important: {}.", id, is_important),
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
                    db.save_memory(id, &m.kind, &m.brief, Some(&new_content), is_important).await?;
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
