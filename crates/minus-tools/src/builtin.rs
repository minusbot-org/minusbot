use anyhow::Result;
use async_trait::async_trait;
use minus_core::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};

mod chat;
mod memory;

pub use chat::*;
pub use memory::*;

/// Built-in tool: time.now
pub struct TimeNowTool;

#[async_trait]
impl Tool for TimeNowTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "time_now".into(),
            description: "Get the current date and time in UTC.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, _call: ToolCall, _ctx: ToolContext) -> Result<ToolResult> {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(ToolResult {
            tool_call_id: _call.id,
            name: "time_now".into(),
            content: now,
            is_error: false,
        })
    }
}

/// Returns all built-in tool instances.
pub fn all_builtin_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(TimeNowTool),
        Box::new(MemoryShortSaveTool),
        Box::new(MemoryLongSaveTool),
        Box::new(MemorySearchTool),
        Box::new(MemoryManageTool),
        Box::new(ChatListTool),
        Box::new(ChatReadTool),
        Box::new(ChatSearchTool),
    ]
}
