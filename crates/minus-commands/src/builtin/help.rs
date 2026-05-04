use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext, CommandDefinition};

pub struct HelpCommand;

#[async_trait]
impl Command for HelpCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "help".into(),
            description: "Show available commands".into(),
            aliases: vec!["?".into(), "h".into()],
            usage: "/help".into(),
            category: "core".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, _args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let mut help = String::from("Available commands:\n\n");
        let mut current_category = String::new();

        let mut commands = ctx.all_commands.clone();
        commands.sort_by_key(|c| c.category.clone());

        for cmd in commands {
            if cmd.category != current_category {
                current_category = cmd.category.clone();
                help.push_str(&format!("\n[{}]\n", current_category.to_uppercase()));
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
        }

        Ok(help)
    }
}
