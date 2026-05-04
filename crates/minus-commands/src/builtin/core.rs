use crate::CommandSpec;
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct ShutdownCommand;

pub fn shutdown_spec() -> CommandSpec {
    CommandSpec::new("shutdown", "/shutdown", "Shutdown the daemon gracefully")
        .category("core")
        .command_alias("exit-daemon")
        .handler(|args, ctx| Box::pin(async move { ShutdownCommand.execute(args, ctx).await }))
}

#[async_trait]
impl Command for ShutdownCommand {
    fn spec(&self) -> CommandSpec {
        shutdown_spec()
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        if let Some(tx) = ctx.shutdown_trigger {
            tracing::info!("Exit command received, triggering graceful shutdown...");
            let _ = tx.send(());
            Ok("Graceful shutdown triggered. Goodbye!".into())
        } else {
            Ok("Shutdown not available in this context.".into())
        }
    }
}

pub struct ClearCommand;

pub fn clear_spec() -> CommandSpec {
    CommandSpec::new("clear", "/clear", "Clear chat history")
        .category("chat")
        .handler(|args, ctx| Box::pin(async move { ClearCommand.execute(args, ctx).await }))
}

#[async_trait]
impl Command for ClearCommand {
    fn spec(&self) -> CommandSpec {
        clear_spec()
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let db = &ctx.db;

        db.delete_messages(&ctx.chat_id.0).await?;
        Ok("Chat history cleared.".into())
    }
}
