use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext, CommandDefinition};


pub struct ToolsCommand;

#[async_trait]
impl Command for ToolsCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "tools".into(),
            description: "List all registered tools.".into(),
            aliases: vec![],
            usage: "/tools".into(),
            category: "system".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let tools = ctx.tools.list_tools().await;
        if tools.is_empty() {
            return Ok("No tools registered.".into());
        }

        let mut res = String::from("Registered Tools:\n");
        for tool in tools {
            res.push_str(&format!("- {}: {}\n", tool.name, tool.description));
        }
        Ok(res)
    }
}
