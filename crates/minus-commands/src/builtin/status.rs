use minus_core::traits::{Command, CommandDefinition, CommandContext};
use anyhow::Result;
use async_trait::async_trait;

pub struct StatusCommand;

#[async_trait]
impl Command for StatusCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "status".into(),
            description: "Show system status".into(),
            aliases: vec!["st".into()],
            usage: "/status".into(),
            category: "core".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, _args: Vec<String>, _ctx: CommandContext) -> Result<String> {
        // This will be tricky because Runtime has the status logic
        // We might need to move that logic or pass more info in CommandContext.
        Ok("System status: Running".into())
    }
}
