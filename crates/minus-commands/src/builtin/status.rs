use crate::CommandSpec;
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct StatusCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new("status", "/status", "Show system status")
        .category("core")
        .command_alias("st")
        .handler(|args, ctx| Box::pin(async move { StatusCommand.execute(args, ctx).await }))
}

#[async_trait]
impl Command for StatusCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, _args: Vec<String>, _ctx: CommandContext) -> Result<String> {
        // This will be tricky because Runtime has the status logic
        // We might need to move that logic or pass more info in CommandContext.
        Ok("System status: Running".into())
    }
}
