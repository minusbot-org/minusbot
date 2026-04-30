use minus_api::traits::{Command, CommandDefinition, CommandContext};
use anyhow::Result;
use async_trait::async_trait;

pub struct ModelCommand;

#[async_trait]
impl Command for ModelCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "models".into(),
            description: "List or set the default text model for the current provider".into(),
            aliases: vec!["m".into()],
            usage: "/models [model_id]".into(),
            category: "config".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let provider_id = ctx.providers.get_default_provider_id().await?;
        if provider_id.is_empty() {
            return Ok("No provider configured. Use `/providers <id>` first.".into());
        }

        if args.is_empty() {
            // List text models for the current provider
            let models = ctx.providers.list_text_models().await?;
            let current = ctx.providers.get_default_text_model().await?;

            if models.is_empty() {
                return Ok(format!("No text models available for '{}'.", provider_id));
            }

            let lines: Vec<String> = models.iter()
                .map(|m| {
                    let marker = if m == &current { "*" } else { " " };
                    format!("{} {}", marker, m)
                })
                .collect();
            return Ok(format!("Text models for '{}':\n{}", provider_id, lines.join("\n")));
        }

        let model = &args[0];
        ctx.providers.set_default_text_model(model).await?;
        Ok(format!("Default text model set to: {}", model))
    }
}
