use minus_api::traits::{Command, CommandDefinition, CommandContext, MinusDatabase};
use anyhow::{Result, Context};
use async_trait::async_trait;
use std::sync::Arc;

pub struct ShutdownCommand;

#[async_trait]
impl Command for ShutdownCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "shutdown".into(),
            description: "Shutdown the daemon gracefully".into(),
            aliases: vec!["exit-daemon".into()],
            usage: "/shutdown".into(),
            category: "core".into(),
            min_args: 0,
        }
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

#[async_trait]
impl Command for ClearCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "clear".into(),
            description: "Clear chat history".into(),
            aliases: vec![],
            usage: "/clear".into(),
            category: "chat".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let db = &ctx.db;

        db.delete_messages(&ctx.chat_id.0).await?;
        Ok("Chat history cleared.".into())
    }
}
