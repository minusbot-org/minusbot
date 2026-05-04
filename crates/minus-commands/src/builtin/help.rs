use crate::CommandSpec;
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct HelpCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new("help", "/help", "Show available commands")
        .category("core")
        .command_alias("?")
        .command_alias("h")
        .handler(|args, ctx| Box::pin(async move { HelpCommand.execute(args, ctx).await }))
}

#[async_trait]
impl Command for HelpCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let mut help = String::from("Available commands:\n\n");
        let mut current_category = String::new();

        let mut commands = ctx.all_commands.clone();
        commands.sort_by_key(|c| category_sort_key(&c.category));

        for cmd in commands {
            if cmd.category != current_category {
                current_category = cmd.category.clone();
                help.push_str(&format!("\n[{}]\n", display_category(&current_category)));
            }

            let aliases = if cmd.aliases.is_empty() {
                String::new()
            } else {
                format!(" (aliases: {})", cmd.aliases.join(", "))
            };

            help.push_str(&format!(
                "  {:<25} — {}{}\n",
                cmd.usage, cmd.description, aliases
            ));

            if cmd.name == "llm" {
                help.push_str("  /llm provider             — List available providers\n");
                help.push_str("  /llm provider <id>        — Set the active provider\n");
                help.push_str(
                    "  /llm model                — List models for the active provider\n",
                );
                help.push_str(
                    "  /llm model <id>           — Set the model for the active provider\n",
                );
                help.push_str("  /llm config <key> <value> — Set config on the active provider\n");
            }
        }

        Ok(help)
    }
}

fn category_sort_key(category: &str) -> String {
    if category == "llm" {
        "config.llm".to_string()
    } else {
        category.to_string()
    }
}

fn display_category(category: &str) -> String {
    if category == "llm" {
        "LLM (Text Model)".to_string()
    } else {
        category.to_uppercase()
    }
}
