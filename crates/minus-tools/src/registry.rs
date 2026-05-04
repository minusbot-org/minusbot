use anyhow::Result;
use minus_api::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;

/// Central tool registry that collects tools from all sources.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let def = tool.definition();
        let name = def.name.clone();
        self.tools.insert(name, Arc::from(tool));
    }

    /// Get all tool definitions for the LLM.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// List all registered tool names.
    pub fn list_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    /// Execute a tool call.
    pub async fn execute(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        if let Some(tool) = self.tools.get(&call.name) {
            tool.call(call, ctx).await
        } else {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: call.name.clone(),
                content: format!("Unknown tool: {}", call.name),
                is_error: true,
            })
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
