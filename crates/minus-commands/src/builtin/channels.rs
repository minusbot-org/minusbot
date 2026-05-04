use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_api::{Command, CommandContext, CommandDefinition};

pub struct ChannelsCommand;

#[async_trait]
impl Command for ChannelsCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "channels".into(),
            description: "Manage communication channels.".into(),
            usage: "/channels <list|enable|disable|config|setup|setchat> [args]".into(),
            category: "system".into(),
            aliases: vec!["chan".into()],
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let subcommand = args.get(0).map(|s| s.as_str()).unwrap_or("list");

        match subcommand {
            "list" => {
                let channels = ctx.channels.list_channels().await;
                if channels.is_empty() {
                    return Ok("No channels registered.".into());
                }

                let mut res = String::from("Communication Channels:\n");
                for c in channels {
                    let enabled_str = if c.is_enabled { "ENABLED" } else { "DISABLED" };
                    let ready_str = if c.is_ready { "READY" } else { "NOT READY" };
                    let chat_str = c.active_chat_id.map(|id| id.0).unwrap_or_else(|| "none".to_string());
                    
                    res.push_str(&format!("- [{}] {}: {} | {} | Active Chat: {}\n", 
                        c.id, c.name, enabled_str, ready_str, chat_str));
                }
                Ok(res)
            }
            "enable" => {
                let id = args.get(1).context("Missing channel ID: /channels enable <id>")?;
                if ctx.channels.set_channel_enabled(id, true).await? {
                    Ok(format!("Channel '{}' enabled.", id))
                } else {
                    Ok(format!("Channel '{}' was already enabled.", id))
                }
            }
            "disable" => {
                let id = args.get(1).context("Missing channel ID: /channels disable <id>")?;
                if ctx.channels.set_channel_enabled(id, false).await? {
                    Ok(format!("Channel '{}' disabled.", id))
                } else {
                    Ok(format!("Channel '{}' was already disabled.", id))
                }
            }
            "config" => {
                let id = args.get(1).context("Missing channel ID: /channels config <id> <get|set> [key] [value]")?;
                let chan = ctx.channels.get_channel(id).await.context("Channel not found")?;
                let config = chan.config().context("Channel does not support configuration")?;
                
                let op = args.get(2).map(|s| s.as_str()).unwrap_or("list");
                match op {
                    "list" => {
                        let keys = config.list_keys();
                        if keys.is_empty() {
                            return Ok(format!("No configuration keys for channel '{}'.", id));
                        }
                        let mut res = format!("Configuration for channel '{}':\n", id);
                        for key in keys {
                            let val = config.read_config(&key).await?.unwrap_or_else(|| "(not set)".to_string());
                            res.push_str(&format!("- {}: {}\n", key, val));
                        }
                        Ok(res)
                    }
                    "get" => {
                        let key = args.get(3).context("Missing key: /channels config <id> get <key>")?;
                        let val = config.read_config(key).await?.unwrap_or_else(|| "(not set)".to_string());
                        Ok(format!("{} = {}", key, val))
                    }
                    "set" => {
                        let key = args.get(3).context("Missing key: /channels config <id> set <key> <value>")?;
                        let val = args.get(4).context("Missing value: /channels config <id> set <key> <value>")?;
                        config.set_config(key, val).await?;
                        Ok(format!("Configuration updated for channel '{}': {} = {}", id, key, val))
                    }
                    _ => bail!("Unknown config operation: {}. Use list, get, or set.", op),
                }
            }
            "setup" => {
                let id = args.get(1).context("Missing channel ID: /channels setup <id>")?;
                let chan = ctx.channels.get_channel(id).await.context("Channel not found")?;
                if !chan.has_available_setup() {
                    return Ok(format!("Channel '{}' does not support setup.", id));
                }
                let pin = chan.setup().await?;
                Ok(format!("Setup initiated for channel '{}'.\nUse the following PIN: {}", id, pin))
            }
            "setchat" => {
                let id = args.get(1).context("Missing channel ID: /channels setchat <id> <chat_id>")?;
                let chat_id = args.get(2).context("Missing chat ID: /channels setchat <id> <chat_id>")?;
                let chan = ctx.channels.get_channel(id).await.context("Channel not found")?;
                chan.set_active_chat(minus_api::ChatId(chat_id.to_string())).await?;
                Ok(format!("Active chat for channel '{}' set to '{}'.", id, chat_id))
            }
            _ => bail!("Unknown subcommand: {}. Use list, enable, disable, config, setup, or setchat.", subcommand),
        }
    }
}
