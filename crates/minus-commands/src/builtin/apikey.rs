use crate::{ArgSpec, CommandSpec};
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct ApikeyCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new(
        "apikey",
        "/apikey <provider> <key>",
        "Set a provider API key",
    )
    .category("setup")
    .arg(ArgSpec::required("provider", "Provider id"))
    .arg(ArgSpec::required("key", "API key value"))
    .example("/apikey openrouter sk-or-v1-...")
    .handler(|args, ctx| Box::pin(async move { ApikeyCommand.execute(args, ctx).await }))
}

#[async_trait]
impl Command for ApikeyCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        if args.len() < 2 {
            return Ok("Usage: /apikey <provider> <key> (e.g., /apikey openrouter sk-...)".into());
        }
        let provider = args[0].to_lowercase();
        let key_value = &args[1];

        let secret_key = format!("PROVIDER_{}_API_KEY", provider.to_uppercase());

        let store = ctx.secrets.get_store("").await?;
        store.put_secret(&secret_key, key_value.as_bytes()).await?;

        Ok(format!(
            "Successfully set API key for provider: {}",
            provider
        ))
    }
}
