use anyhow::Result;
use async_trait::async_trait;
use minus_api::traits::{Command, CommandContext, CommandDefinition};

pub struct MemoryCommand;

#[async_trait]
impl Command for MemoryCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "memory".into(),
            description: "List all agent memories.".into(),
            usage: "/memory".into(),
            category: "system".into(),
            aliases: vec!["mem".into()],
            min_args: 0,
        }
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let memories = ctx.db.list_memories().await?;
        if memories.is_empty() {
            return Ok("No memories stored yet.".into());
        }

        let mut res = String::from("Agent Memories:\n");
        for m in memories {
            let importance = if m.is_important { " [IMPORTANT]" } else { "" };
            let date = m.created_at.format("%Y-%m-%d %H:%M").to_string();
            
            // Use brief if available and not empty, otherwise content
            let display_text = if !m.brief.trim().is_empty() {
                &m.brief
            } else {
                m.content.as_deref().unwrap_or("(no content)")
            };

            res.push_str(&format!("- {}: {}{}\n", date, display_text, importance));
        }
        Ok(res)
    }
}
