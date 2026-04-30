use minus_api::traits::{Command, CommandDefinition, CommandContext};
use anyhow::Result;
use async_trait::async_trait;

pub struct ModelCommand;

#[async_trait]
impl Command for ModelCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "models".into(),
            description: "Manage AI models".into(),
            aliases: vec!["m".into()],
            usage: "/models [id]".into(),
            category: "config".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let provider_id = ctx.providers.get_default_provider_id().await?;
        if provider_id.is_empty() {
            return Ok("No provider configured.".into());
        }

        if args.is_empty() {
            let models = ctx.providers.list_models(&provider_id).await?;
            let default_model = ctx.providers.get_default_model().await?;
            
            let lines: Vec<String> = models.iter()
                .map(|m| {
                    let marker = if m == &default_model { "*" } else { " " };
                    format!("{} {}", marker, m)
                })
                .collect();
            return Ok(format!("Available models for {}:\n{}", provider_id, lines.join("\n")));
        }

        let model = &args[0];
        ctx.providers.set_default_model(model).await?;
        Ok(format!("Default model set to: {}", model))
    }
}
