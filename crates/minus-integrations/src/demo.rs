use anyhow::Result;
use async_trait::async_trait;
use minus_core::*;

/// A demo integration to prove the integration system works.
pub struct DemoIntegration;

#[async_trait]
impl Integration for DemoIntegration {
    fn id(&self) -> &'static str {
        "demo"
    }

    fn name(&self) -> &'static str {
        "Demo Integration"
    }

    fn required_secrets(&self) -> Vec<SecretDeclaration> {
        vec![] // Demo doesn't need secrets
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "demo.echo".into(),
                description: "Echo back the given message.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string", "description": "Message to echo" }
                    },
                    "required": ["message"]
                }),
                risk: ToolRisk::Low,
                side_effect: false,
            },
            ToolDefinition {
                name: "demo.random_number".into(),
                description: "Generate a random number between min and max.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "min": { "type": "integer", "description": "Minimum value (inclusive)" },
                        "max": { "type": "integer", "description": "Maximum value (inclusive)" }
                    },
                    "required": ["min", "max"]
                }),
                risk: ToolRisk::Low,
                side_effect: false,
            },
        ]
    }

    async fn call_tool(&self, call: ToolCall, _ctx: ToolContext) -> Result<ToolResult> {
        match call.name.as_str() {
            "demo.echo" => {
                let msg = call.arguments["message"]
                    .as_str()
                    .unwrap_or("(no message)");
                Ok(ToolResult {
                    tool_call_id: call.id,
                    name: call.name,
                    content: msg.to_string(),
                    is_error: false,
                })
            }
            "demo.random_number" => {
                let min = call.arguments["min"].as_i64().unwrap_or(0);
                let max = call.arguments["max"].as_i64().unwrap_or(100);
                let num = if min >= max {
                    min
                } else {
                    use rand::Rng;
                    rand::thread_rng().gen_range(min..=max)
                };
                Ok(ToolResult {
                    tool_call_id: call.id,
                    name: call.name,
                    content: num.to_string(),
                    is_error: false,
                })
            }
            _ => Ok(ToolResult {
                tool_call_id: call.id,
                name: call.name.clone(),
                content: format!("Unknown demo tool: {}", call.name),
                is_error: true,
            }),
        }
    }
}
