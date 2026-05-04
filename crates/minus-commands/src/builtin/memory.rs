use crate::{ArgSpec, CommandSpec, ValueType};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct MemoryCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new(
        "memory",
        "/memory <list|search|delete|create> [args]",
        "Manage agent memories",
    )
    .category("system")
    .command_alias("mem")
    .strict_subcommands()
    .handler(|args, ctx| Box::pin(async move { MemoryCommand.execute(args, ctx).await }))
    .subcommand(sub("list", "/memory list", "List memories"))
    .subcommand(
        sub("search", "/memory search <term>", "Search memories")
            .arg(ArgSpec::required("term", "Search term")),
    )
    .subcommand(
        sub("delete", "/memory delete <id>", "Delete a memory")
            .arg(ArgSpec::required("id", "Memory id")),
    )
    .subcommand(
        sub(
            "create",
            "/memory create <id> <brief> [content] [important]",
            "Create a memory",
        )
        .arg(ArgSpec::required("id", "Memory id"))
        .arg(ArgSpec::required("brief", "Short memory summary"))
        .arg(ArgSpec::optional("content", "Memory content"))
        .arg(ArgSpec::optional("important", "true or false").value_type(ValueType::Boolean)),
    )
}

fn sub(name: &'static str, usage: &'static str, about: &'static str) -> CommandSpec {
    CommandSpec::new(name, usage, about).handler(move |args, ctx| {
        Box::pin(async move {
            let mut full = vec![name.to_string()];
            full.extend(args);
            MemoryCommand.execute(full, ctx).await
        })
    })
}

#[async_trait]
impl Command for MemoryCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let subcommand = args.get(0).map(|s| s.as_str()).unwrap_or("list");

        match subcommand {
            "list" => {
                let memories = ctx.db.list_memories().await?;
                if memories.is_empty() {
                    return Ok("No memories stored yet.".into());
                }

                let mut res = String::from("Agent Memories:\n");
                for m in memories {
                    let importance = if m.is_important { " [IMPORTANT]" } else { "" };
                    let date = m.created_at.format("%Y-%m-%d %H:%M").to_string();
                    let display_text = if !m.brief.trim().is_empty() {
                        &m.brief
                    } else {
                        m.content.as_deref().unwrap_or("(no content)")
                    };
                    res.push_str(&format!(
                        "- [{}] {}: {}{}\n",
                        m.id, date, display_text, importance
                    ));
                }
                Ok(res)
            }
            "search" => {
                let term = args
                    .get(1)
                    .context("Missing search term: /memory search <term>")?;
                let results = ctx.db.list_memories().await?; // Basic search for now
                let filtered: Vec<_> = results
                    .into_iter()
                    .filter(|m| {
                        m.brief.contains(term) || m.content.as_deref().unwrap_or("").contains(term)
                    })
                    .collect();

                if filtered.is_empty() {
                    return Ok(format!("No memories found matching '{}'.", term));
                }

                let mut res = format!("Search results for '{}':\n", term);
                for m in filtered {
                    res.push_str(&format!("- [{}] Brief: {}\n", m.id, m.brief));
                }
                Ok(res)
            }
            "delete" => {
                let id = args
                    .get(1)
                    .context("Missing memory ID: /memory delete <id>")?;
                if ctx.db.delete_memory(id).await? {
                    Ok(format!("Memory '{}' deleted.", id))
                } else {
                    Ok(format!("Memory '{}' not found.", id))
                }
            }
            "create" => {
                let id = args.get(1).context(
                    "Missing ID: /memory create <id> <brief> [content] [important:true|false]",
                )?;
                let brief = args
                    .get(2)
                    .context("Missing brief: /memory create <id> <brief> [content]")?;
                let content = args.get(3).map(|s| s.as_str());
                let important = args.get(4).map(|s| s == "true").unwrap_or(false);

                ctx.db
                    .ensure_chat("system", "cli", "system", Some("System Chat"))
                    .await?;
                ctx.db
                    .save_memory(id, "long", brief, content, important)
                    .await?;
                Ok(format!("Memory '{}' created successfully.", id))
            }
            _ => bail!(
                "Unknown subcommand: {}. Use list, search, delete, or create.",
                subcommand
            ),
        }
    }
}
