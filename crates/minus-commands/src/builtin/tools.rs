use crate::CommandSpec;
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct ToolsCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new("tools", "/tools", "List registered tools")
        .category("system")
        .handler(|args, ctx| Box::pin(async move { ToolsCommand.execute(args, ctx).await }))
}

#[async_trait]
impl Command for ToolsCommand {
    fn spec(&self) -> CommandSpec {
        spec()
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
