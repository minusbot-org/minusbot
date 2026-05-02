use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_api::{Command, CommandContext, CommandDefinition};

pub struct MemoryCommand;

#[async_trait]
impl Command for MemoryCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "memory".into(),
            description: "Manage agent memories.".into(),
            usage: "/memory <list|search|delete|create> [args]".into(),
            category: "system".into(),
            aliases: vec!["mem".into()],
            min_args: 0,
        }
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
                    res.push_str(&format!("- [{}] {}: {}{}\n", m.id, date, display_text, importance));
                }
                Ok(res)
            }
            "search" => {
                let term = args.get(1).context("Missing search term: /memory search <term>")?;
                let results = ctx.db.list_memories().await?; // Basic search for now
                let filtered: Vec<_> = results.into_iter()
                    .filter(|m| m.brief.contains(term) || m.content.as_deref().unwrap_or("").contains(term))
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
                let id = args.get(1).context("Missing memory ID: /memory delete <id>")?;
                if ctx.db.delete_memory(id).await? {
                    Ok(format!("Memory '{}' deleted.", id))
                } else {
                    Ok(format!("Memory '{}' not found.", id))
                }
            }
            "create" => {
                let id = args.get(1).context("Missing ID: /memory create <id> <brief> [content] [important:true|false]")?;
                let brief = args.get(2).context("Missing brief: /memory create <id> <brief> [content]")?;
                let content = args.get(3).map(|s| s.as_str());
                let important = args.get(4).map(|s| s == "true").unwrap_or(false);

                ctx.db.ensure_chat("system", "cli", "system", Some("System Chat")).await?;
                ctx.db.save_memory(id, "long", brief, content, important).await?;
                Ok(format!("Memory '{}' created successfully.", id))
            }
            _ => bail!("Unknown subcommand: {}. Use list, search, delete, or create.", subcommand),
        }
    }
}
