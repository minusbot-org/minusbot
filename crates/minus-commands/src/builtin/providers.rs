use minus_api::{Command, CommandContext, CommandDefinition};
use anyhow::{Context, Result};
use async_trait::async_trait;

pub struct ProviderCommand;

#[async_trait]
impl Command for ProviderCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "providers".into(),
            description: "Manage AI providers".into(),
            aliases: vec!["p".into()],
            usage: "/providers <list|set|config> [args]".into(),
            category: "config".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let subcommand = args.get(0).map(|s| s.as_str()).unwrap_or("list");

        match subcommand {
            "list" => {
                let providers = ctx.providers.list_providers().await?;
                let default_id = ctx.providers.get_default_provider_id().await?;
                
                let lines: Vec<String> = providers.iter()
                    .map(|(id, name)| {
                        let marker = if id == &default_id { "*" } else { " " };
                        format!("{} {} — {}", marker, id, name)
                    })
                    .collect();
                Ok(format!("Available providers:\n{}", lines.join("\n")))
            }
            "set" => {
                let id = args.get(1).context("Missing provider ID: /providers set <id>")?;
                ctx.providers.set_default_provider(id).await?;
                Ok(format!("Default provider set to: {}", id))
            }
            "config" => {
                let id = args.get(1).context("Missing provider ID: /providers config <id> <get|set> [key] [value]")?;
                let provider = ctx.providers.get_provider(id).await?.context("Provider not found")?;
                let config = provider.config().context("Provider does not support configuration")?;
                
                let op = args.get(2).map(|s| s.as_str()).unwrap_or("list");
                match op {
                    "list" => {
                        let keys = config.list_keys();
                        if keys.is_empty() {
                            return Ok(format!("No configuration keys for provider '{}'.", id));
                        }
                        let mut res = format!("Configuration for provider '{}':\n", id);
                        for key in keys {
                            let val = config.read_config(&key).await?.unwrap_or_else(|| "(not set)".to_string());
                            res.push_str(&format!("- {}: {}\n", key, val));
                        }
                        Ok(res)
                    }
                    "get" => {
                        let key = args.get(3).context("Missing key: /providers config <id> get <key>")?;
                        let val = config.read_config(key).await?.unwrap_or_else(|| "(not set)".to_string());
                        Ok(format!("{} = {}", key, val))
                    }
                    "set" => {
                        let key = args.get(3).context("Missing key: /providers config <id> set <key> <value>")?;
                        let val = args.get(4).context("Missing value: /providers config <id> set <key> <value>")?;
                        config.set_config(key, val).await?;
                        Ok(format!("Configuration updated for provider '{}': {} = {}", id, key, val))
                    }
                    _ => anyhow::bail!("Unknown config operation: {}. Use list, get, or set.", op),
                }
            }
            _ => {
                // If it's a single arg and not a subcommand, assume it's 'set' for backward compatibility
                if args.len() == 1 {
                    let id = &args[0];
                    ctx.providers.set_default_provider(id).await?;
                    return Ok(format!("Default provider set to: {}", id));
                }
                anyhow::bail!("Unknown subcommand: {}. Use list, set, or config.", subcommand)
            }
        }
    }
}
