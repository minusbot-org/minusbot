use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::{ChatId, Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use serde_json::json;

/// Tool: agent_list
pub struct AgentListTool;

#[async_trait]
impl Tool for AgentListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "agent_list".into(),
            description: "List all available utility agents and their roles.".into(),
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
        let agents = ctx.agent.clone().list_agents().await?;
        let content = if agents.is_empty() {
            "No utility agents found. You are the only agent available.".into()
        } else {
            let lines: Vec<String> = agents
                .iter()
                .map(|a| format!("- {} (ID: {})", a.name, a.id))
                .collect();
            format!("Available agents:\n{}", lines.join("\n"))
        };

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "agent_list".into(),
            content,
            is_error: false,
        })
    }
}

/// Tool: agent_call
pub struct AgentCallTool;

#[async_trait]
impl Tool for AgentCallTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "agent_call".into(),
            description: "Invoke another agent to process a specific request or content.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "The ID of the agent to call" },
                    "content": { "type": "string", "description": "The message or request to send to the agent" },
                    "chat_id": { "type": "string", "description": "Optional: target chat ID. Defaults to current chat." }
                },
                "required": ["agent_id", "content"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let agent_id = call.arguments["agent_id"]
            .as_str()
            .context("Missing agent_id")?;
        let content = call.arguments["content"]
            .as_str()
            .context("Missing content")?;
        let target_chat_id = call.arguments["chat_id"]
            .as_str()
            .map(|s| ChatId(s.to_string()))
            .unwrap_or(ctx.chat_id.clone());

        let response = ctx
            .agent
            .clone()
            .call_agent(agent_id, content, target_chat_id, ctx.channel_id.clone())
            .await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "agent_call".into(),
            content: response,
            is_error: false,
        })
    }
}
