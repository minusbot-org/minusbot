use crate::{ArgSpec, CommandSpec};
use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct LlmCommand;

#[async_trait]
impl Command for LlmCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let Some(subcommand) = args.get(0).map(|s| s.as_str()) else {
            return Ok(llm_help());
        };

        match subcommand {
            "help" => Ok(llm_help()),
            "provider" | "providers" => handle_provider(args[1..].to_vec(), ctx).await,
            "model" | "models" => handle_model(args[1..].to_vec(), ctx).await,
            _ => anyhow::bail!(
                "Unknown llm subcommand: {}. Use provider, providers, model, or models.",
                subcommand
            ),
        }
    }
}

pub fn spec() -> CommandSpec {
    CommandSpec::new(
        "llm",
        "/llm provider(s) <args>",
        "Manage LLM providers and text models",
    )
    .category("llm")
    .strict_subcommands()
    .handler(|_, _| Box::pin(async move { Ok(llm_help()) }))
    .subcommand(
        CommandSpec::new(
            "help",
            "/llm help",
            "Show LLM provider and model subcommands",
        )
        .handler(|_, _| Box::pin(async move { Ok(llm_help()) })),
    )
    .subcommand(
        CommandSpec::new(
            "provider",
            "/llm provider [list|<id>]",
            "List providers or set the active provider",
        )
        .alias("providers")
        .arg(ArgSpec::optional("id", "Provider id"))
        .handler(|args, ctx| Box::pin(async move { handle_provider(args, ctx).await }))
        .subcommand(
            CommandSpec::new("list", "/llm provider list", "List available providers")
                .handler(|_, ctx| Box::pin(async move { list_providers(ctx).await })),
        )
        .subcommand(
            CommandSpec::new("set", "/llm provider set <id>", "Set the active provider")
                .arg(ArgSpec::required("id", "Provider id"))
                .handler(|args, ctx| {
                    Box::pin(async move {
                        let id = args
                            .get(0)
                            .context("Missing provider ID: /llm provider <id>")?;
                        set_provider(id, ctx).await
                    })
                }),
        )
        .example("/llm provider openrouter"),
    )
    .subcommand(
        CommandSpec::new(
            "model",
            "/llm model [list|<id>]",
            "List or set the active provider's text model",
        )
        .alias("models")
        .arg(ArgSpec::optional("id", "Model id"))
        .handler(|args, ctx| Box::pin(async move { handle_model(args, ctx).await }))
        .subcommand(
            CommandSpec::new(
                "list",
                "/llm model list",
                "List models for the active provider",
            )
            .handler(|_, ctx| Box::pin(async move { list_models_for_active_provider(ctx).await })),
        )
        .subcommand(
            CommandSpec::new("set", "/llm model set <id>", "Set the text model")
                .arg(ArgSpec::required("id", "Model id"))
                .handler(|args, ctx| {
                    Box::pin(async move {
                        let model = args.get(0).context("Missing model ID: /llm model <id>")?;
                        set_model(model, ctx).await
                    })
                }),
        )
        .example("/llm model gpt-4o-mini"),
    )
    .subcommand(
        CommandSpec::new(
            "config",
            "/llm config [list|get|set] [key] [value]",
            "Manage configuration for the active provider",
        )
        .arg(ArgSpec::optional("key", "Configuration key"))
        .arg(ArgSpec::optional("value", "Configuration value"))
        .handler(|args, ctx| Box::pin(async move { provider_config(args, ctx).await }))
        .subcommand(
            CommandSpec::new(
                "list",
                "/llm config list",
                "List active provider configuration",
            )
            .handler(|_, ctx| Box::pin(async move { provider_config(Vec::new(), ctx).await })),
        )
        .subcommand(
            CommandSpec::new(
                "get",
                "/llm config get <key>",
                "Read active provider configuration",
            )
            .arg(ArgSpec::required("key", "Configuration key"))
            .handler(|args, ctx| {
                Box::pin(async move {
                    let key = args.get(0).context("Missing key: /llm config get <key>")?;
                    provider_config(vec!["get".to_string(), key.clone()], ctx).await
                })
            }),
        )
        .subcommand(
            CommandSpec::new(
                "set",
                "/llm config set <key> <value>",
                "Set active provider configuration",
            )
            .arg(ArgSpec::required("key", "Configuration key"))
            .arg(ArgSpec::required("value", "Configuration value"))
            .handler(|args, ctx| {
                Box::pin(async move {
                    let key = args
                        .get(0)
                        .context("Missing key: /llm config set <key> <value>")?;
                    let val = args
                        .get(1)
                        .context("Missing value: /llm config set <key> <value>")?;
                    provider_config(vec!["set".to_string(), key.clone(), val.clone()], ctx).await
                })
            }),
        ),
    )
    .example("/llm provider")
    .example("/llm provider openrouter")
    .example("/llm model")
    .example("/llm model openai/gpt-4o-mini")
    .example("/llm config set text_model gpt-4o-mini")
}

async fn handle_provider(args: Vec<String>, ctx: CommandContext) -> Result<String> {
    let subcommand = args.get(0).map(|s| s.as_str()).unwrap_or("list");

    match subcommand {
        "list" => list_providers(ctx).await,
        "set" => {
            let id = args
                .get(1)
                .context("Missing provider ID: /llm provider <id>")?;
            set_provider(id, ctx).await
        }
        "config" => {
            Ok("Use /llm config [list|get|set] [key] [value] for the active provider.".into())
        }
        id if args.len() == 1 => set_provider(id, ctx).await,
        _ => anyhow::bail!(
            "Unknown provider operation: {}. Use list, <id>, or config.",
            subcommand
        ),
    }
}

async fn list_providers(ctx: CommandContext) -> Result<String> {
    let providers = ctx.providers.list_providers().await?;
    let default_id = ctx.providers.get_default_provider_id().await?;

    let lines: Vec<String> = providers
        .iter()
        .map(|(id, name)| {
            let marker = if id == &default_id { "*" } else { " " };
            format!("{} {} - {}", marker, id, name)
        })
        .collect();
    Ok(format!("Available providers:\n{}", lines.join("\n")))
}

async fn set_provider(id: &str, ctx: CommandContext) -> Result<String> {
    ctx.providers.set_default_provider(id).await?;
    Ok(format!("Default provider set to: {}", id))
}

async fn provider_config(args: Vec<String>, ctx: CommandContext) -> Result<String> {
    let id = ctx.providers.get_default_provider_id().await?;
    if id.is_empty() {
        return Ok("No provider configured. Use `/llm provider <id>` first.".into());
    }
    let provider = ctx
        .providers
        .get_provider(&id)
        .await?
        .context("Provider not found")?;
    let config = provider
        .config()
        .context("Provider does not support configuration")?;

    let op = args.get(0).map(|s| s.as_str()).unwrap_or("list");
    match op {
        key if args.len() == 2 => {
            let val = &args[1];
            config.set_config(key, val).await?;
            Ok(format!(
                "Configuration updated for provider '{}': {} = {}",
                id, key, val
            ))
        }
        "list" => {
            let keys = config.list_keys();
            if keys.is_empty() {
                return Ok(format!("No configuration keys for provider '{}'.", id));
            }
            let mut res = format!("Configuration for provider '{}':\n", id);
            for key in keys {
                let val = config
                    .read_config(&key)
                    .await?
                    .unwrap_or_else(|| "(not set)".to_string());
                res.push_str(&format!("- {}: {}\n", key, val));
            }
            Ok(res)
        }
        "get" => {
            let key = args.get(1).context("Missing key: /llm config get <key>")?;
            let val = config
                .read_config(key)
                .await?
                .unwrap_or_else(|| "(not set)".to_string());
            Ok(format!("{} = {}", key, val))
        }
        "set" => {
            let key = args
                .get(1)
                .context("Missing key: /llm config set <key> <value>")?;
            let val = args
                .get(2)
                .context("Missing value: /llm config set <key> <value>")?;
            config.set_config(key, val).await?;
            Ok(format!(
                "Configuration updated for provider '{}': {} = {}",
                id, key, val
            ))
        }
        _ => anyhow::bail!("Unknown config operation: {}. Use list, get, or set.", op),
    }
}

async fn handle_model(args: Vec<String>, ctx: CommandContext) -> Result<String> {
    let provider_id = ctx.providers.get_default_provider_id().await?;
    if provider_id.is_empty() {
        return Ok("No provider configured. Use `/llm provider <id>` first.".into());
    }

    match args.get(0).map(|s| s.as_str()) {
        None | Some("list") => list_models(provider_id, ctx).await,
        Some("set") => {
            let model = args.get(1).context("Missing model ID: /llm model <id>")?;
            set_model(model, ctx).await
        }
        Some(model) if args.len() == 1 => set_model(model, ctx).await,
        Some(op) => anyhow::bail!("Unknown model operation: {}. Use list or <id>.", op),
    }
}

async fn list_models_for_active_provider(ctx: CommandContext) -> Result<String> {
    let provider_id = ctx.providers.get_default_provider_id().await?;
    if provider_id.is_empty() {
        return Ok("No provider configured. Use `/llm provider <id>` first.".into());
    }
    list_models(provider_id, ctx).await
}

async fn list_models(provider_id: String, ctx: CommandContext) -> Result<String> {
    let models = ctx.providers.list_text_models().await?;
    let current = ctx.providers.get_default_text_model().await?;

    if models.is_empty() {
        return Ok(format!("No text models available for '{}'.", provider_id));
    }

    let lines: Vec<String> = models
        .iter()
        .map(|m| {
            let marker = if m == &current { "*" } else { " " };
            format!("{} {}", marker, m)
        })
        .collect();
    Ok(format!(
        "Text models for '{}':\n{}",
        provider_id,
        lines.join("\n")
    ))
}

async fn set_model(model: &str, ctx: CommandContext) -> Result<String> {
    ctx.providers.set_default_text_model(model).await?;
    Ok(format!("Default text model set to: {}", model))
}

fn llm_help() -> String {
    [
        "Usage: /llm provider(s) <args>",
        "",
        "LLM (Text Model) subcommands:",
        "  /llm provider             List available providers",
        "  /llm provider <id>        Set the active provider",
        "  /llm model                List models for the active provider",
        "  /llm model <id>           Set the model for the active provider",
        "  /llm config <key> <value> Set config on the active provider",
    ]
    .join("\n")
}
