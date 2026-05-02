use minus_api::{Command, CommandContext, CommandDefinition};
use anyhow::Result;
use async_trait::async_trait;

pub struct ProviderCommand;

#[async_trait]
impl Command for ProviderCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "providers".into(),
            description: "Manage AI providers".into(),
            aliases: vec!["p".into()],
            usage: "/providers [id]".into(),
            category: "config".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        if args.is_empty() {
            let providers = ctx.providers.list_providers().await?;
            let default_id = ctx.providers.get_default_provider_id().await?;
            
            let lines: Vec<String> = providers.iter()
                .map(|(id, name)| {
                    let marker = if id == &default_id { "*" } else { " " };
                    format!("{} {} — {}", marker, id, name)
                })
                .collect();
            return Ok(format!("Available providers:\n{}", lines.join("\n")));
        }

        let id = &args[0];
        ctx.providers.set_default_provider(id).await?;
        Ok(format!("Default provider set to: {}", id))
    }
}
