use minus_api::{Command, CommandContext, CommandDefinition};
use anyhow::Result;
use async_trait::async_trait;

pub struct ApikeyCommand;

#[async_trait]
impl Command for ApikeyCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "apikey".into(),
            description: "Shortcut to set provider API keys".into(),
            aliases: vec![],
            usage: "/apikey <provider> <key>".into(),
            category: "setup".into(),
            min_args: 2,
        }
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
        
        Ok(format!("Successfully set API key for provider: {}", provider))
    }
}
