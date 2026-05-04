use crate::{ArgSpec, CommandSpec, ValueType};
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{ChatId, Command, CommandContext, NotificationPacket, NotificationSeverity};

pub struct ChatListCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new(
        "chat",
        "/chat <list|new|switch|rename|read>",
        "Manage chats",
    )
    .category("chat")
    .command_alias("c")
    .strict_subcommands()
    .handler(|args, ctx| Box::pin(async move { ChatListCommand.execute(args, ctx).await }))
    .subcommand(
        CommandSpec::new("list", "/chat list", "List chats").handler(|args, ctx| {
            Box::pin(async move {
                let mut full = vec!["list".to_string()];
                full.extend(args);
                ChatListCommand.execute(full, ctx).await
            })
        }),
    )
    .subcommand(
        CommandSpec::new("switch", "/chat switch <id>", "Switch to a chat")
            .arg(ArgSpec::required("id", "Chat id"))
            .handler(|args, ctx| {
                Box::pin(async move {
                    let mut full = vec!["switch".to_string()];
                    full.extend(args);
                    ChatListCommand.execute(full, ctx).await
                })
            }),
    )
    .subcommand(
        CommandSpec::new("s", "/chat s <id>", "Switch to a chat")
            .arg(ArgSpec::required("id", "Chat id"))
            .handler(|args, ctx| {
                Box::pin(async move {
                    let mut full = vec!["s".to_string()];
                    full.extend(args);
                    ChatListCommand.execute(full, ctx).await
                })
            }),
    )
    .subcommand(
        CommandSpec::new("new", "/chat new [title]", "Create a new chat")
            .arg(ArgSpec::optional("title", "Chat title").variadic())
            .handler(|args, ctx| {
                Box::pin(async move {
                    let mut full = vec!["new".to_string()];
                    full.extend(args);
                    ChatListCommand.execute(full, ctx).await
                })
            }),
    )
    .subcommand(
        CommandSpec::new("rename", "/chat rename <title>", "Rename current chat")
            .arg(ArgSpec::required("title", "New chat title").variadic())
            .handler(|args, ctx| {
                Box::pin(async move {
                    let mut full = vec!["rename".to_string()];
                    full.extend(args);
                    ChatListCommand.execute(full, ctx).await
                })
            }),
    )
    .subcommand(
        CommandSpec::new("read", "/chat read [id] [n]", "Read messages from a chat")
            .arg(ArgSpec::optional("id", "Chat id"))
            .arg(ArgSpec::optional("n", "Message count").value_type(ValueType::Integer))
            .handler(|args, ctx| {
                Box::pin(async move {
                    let mut full = vec!["read".to_string()];
                    full.extend(args);
                    ChatListCommand.execute(full, ctx).await
                })
            }),
    )
}

#[async_trait]
impl Command for ChatListCommand {
    fn spec(&self) -> CommandSpec {
        spec()
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
                let lines: Vec<String> = chats
                    .iter()
                    .map(|c| {
                        let marker = if c.id == ctx.chat_id.0 { "*" } else { " " };
                        format!(
                            "{} {} — {}",
                            marker,
                            c.id,
                            c.title.as_deref().unwrap_or("(no title)")
                        )
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
                        // Fetch message history and notify channel about chat switch
                        let messages = db.get_messages(&c.id, 50).await.unwrap_or_default();
                        ctx.channel
                            .on_chat_switch(&ChatId(c.id.clone()), messages)
                            .await?;

                        // Attempt to update channel config
                        let provider_id = format!("channel.{}", ctx.channel_id.0);
                        if let Some(channel_config) =
                            ctx.config_registry.get_provider(&provider_id).await
                        {
                            if let Err(e) = channel_config.set_config("chat", &c.id).await {
                                let _ = ctx
                                    .channel
                                    .send_notification(NotificationPacket {
                                        chat_id: ctx.chat_id.clone(),
                                        severity: NotificationSeverity::Warning,
                                        content: format!(
                                            "Failed to save default chat to config: {}",
                                            e
                                        ),
                                    })
                                    .await;
                            }
                        }

                        Ok(format!(
                            "Switched to chat: {}",
                            c.title.as_deref().unwrap_or(&c.id)
                        ))
                    }
                    None => Ok(format!("Chat '{}' not found.", id)),
                }
            }
            "new" => {
                let title = if args.len() > 1 {
                    Some(args[1..].join(" "))
                } else {
                    None
                };
                let chats = db.list_chats().await?;
                let id = format!("chat-{}", chats.len() + 1);
                db.ensure_chat(&id, &ctx.channel_id.0, &id, title.as_deref())
                    .await?;

                // Notify channel about new chat
                ctx.channel
                    .send_notification(NotificationPacket {
                        chat_id: ctx.chat_id.clone(),
                        severity: NotificationSeverity::Success,
                        content: format!("Created new chat: {}", id),
                    })
                    .await?;

                Ok(format!(
                    "Created new chat: {}",
                    title.unwrap_or_else(|| id.clone())
                ))
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
                let limit = if args.len() > 2 {
                    args[2].parse().unwrap_or(10)
                } else {
                    10
                };
                let target_id = if args.len() > 1 {
                    &args[1]
                } else {
                    &ctx.chat_id.0
                };
                let messages = db.get_messages(target_id, limit).await?;
                let lines: Vec<String> = messages
                    .iter()
                    .rev()
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
