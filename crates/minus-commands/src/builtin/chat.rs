use minus_api::{NotificationKind, traits::{Command, CommandDefinition, CommandContext, MinusDatabase}};
use anyhow::{Result, Context};
use async_trait::async_trait;
use std::sync::Arc;

pub struct ChatListCommand;

#[async_trait]
impl Command for ChatListCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "chat".into(),
            description: "Manage chats".into(),
            aliases: vec!["c".into()],
            usage: "/chat <list|new|switch|rename|read>".into(),
            category: "chat".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let db = &ctx.db;

        if args.is_empty() {
            return Ok(self.help_text());
        }

        match args[0].as_str() {
            "list" => {
                let chats = db.list_chats().await?;
                if chats.is_empty() {
                    return Ok("No chats found.".into());
                }
                let lines: Vec<String> = chats.iter()
                    .map(|c| {
                        let marker = if c.id == ctx.chat_id.0 { "*" } else { " " };
                        format!("{} {} — {}", marker, c.id, c.title.as_deref().unwrap_or("(no title)"))
                    })
                    .collect();
                Ok(format!("Chats:\n{}", lines.join("\n")))
            }
            "switch" | "s" => {
                if args.len() < 2 {
                    return Ok("Usage: /chat switch <id>".into());
                }
                let id = &args[1];
                let chat = db.get_chat(id).await?;
                match chat {
                    Some(c) => {
                        // Notify channel about chat switch
                        ctx.channel.send_notification(&ctx.chat_id, NotificationKind::SwitchChat, &c.id).await?;

                        // Attempt to update channel config
                        let provider_id = format!("channel.{}", ctx.channel_id.0);
                        if let Some(channel_config) = ctx.config_registry.get_provider(&provider_id).await {
                            if let Err(e) = channel_config.set_config("chat", &c.id).await {
                                let _ = ctx.channel.send_warning(&ctx.chat_id, &format!("Failed to save default chat to config: {}", e)).await;
                            }
                        }
                        
                        Ok(format!("Switched to chat: {}", c.title.as_deref().unwrap_or(&c.id)))
                    }
                    None => Ok(format!("Chat '{}' not found.", id)),
                }
            }
            "new" => {
                let title = if args.len() > 1 { Some(args[1..].join(" ")) } else { None };
                let chats = db.list_chats().await?;
                let id = format!("chat-{}", chats.len() + 1);
                db.ensure_chat(&id, &ctx.channel_id.0, &id, title.as_deref()).await?;
                
                // Notify channel about new chat
                ctx.channel.send_notification(&ctx.chat_id, NotificationKind::NewChat, &id).await?;
                
                Ok(format!("Created new chat: {}", title.unwrap_or_else(|| id.clone())))
            }
            "rename" => {
                if args.len() < 2 {
                    return Ok("Usage: /chat rename <new title>".into());
                }
                let title = args[1..].join(" ");
                db.rename_chat(&ctx.chat_id.0, &title).await?;
                Ok(format!("Chat renamed to: {}", title))
            }
            "read" => {
                let limit = if args.len() > 2 { args[2].parse().unwrap_or(10) } else { 10 };
                let target_id = if args.len() > 1 { &args[1] } else { &ctx.chat_id.0 };
                let messages = db.get_messages(target_id, limit).await?;
                let lines: Vec<String> = messages.iter().rev()
                    .map(|m| format!("[{}] {}: {}", m.created_at, m.role, m.content))
                    .collect();
                Ok(format!("History for {}:\n{}", target_id, lines.join("\n")))
            }
            _ => Ok(format!("Unknown chat subcommand: {}", args[0])),
        }
    }
}

impl ChatListCommand {
    fn help_text(&self) -> String {
        let mut help = String::from("Chat management commands:\n");
        help.push_str("  /chat list            List all chats\n");
        help.push_str("  /chat switch <id>     Switch to a chat\n");
        help.push_str("  /chat new [title]     Create a new chat\n");
        help.push_str("  /chat rename <title>  Rename current chat\n");
        help.push_str("  /chat read [id] [n]   Read messages from a chat\n");
        help
    }
}
