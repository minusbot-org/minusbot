use minus_api::traits::{Command, CommandDefinition, CommandContext};
use anyhow::Result;
use async_trait::async_trait;

pub struct SecretCommand;

#[async_trait]
impl Command for SecretCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "secrets".into(),
            description: "Manage programmatic secret declarations".into(),
            aliases: vec![],
            usage: "/secrets <list|approve|deny> <id>".into(),
            category: "security".into(),
            min_args: 0,
        }
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        if args.is_empty() || args[0] == "list" {
            let secrets = ctx.secrets.list_secret_declarations().await?;
            if secrets.is_empty() {
                return Ok("No secret declarations found.".into());
            }
            let lines: Vec<String> = secrets.iter()
                .map(|s| {
                    let status = if s.approved { "[OK]" } else { "[PENDING]" };
                    format!("{} {} — {}", status, s.key, s.description)
                })
                .collect();
            return Ok(format!("Secret declarations:\n{}", lines.join("\n")));
        }

        if args.len() < 2 {
            return Ok("Usage: /secrets <approve|deny> <component_id/key>".into());
        }

        let action = &args[0];
        let target = &args[1];
        let parts: Vec<&str> = target.split('/').collect();
        if parts.len() != 2 {
            return Ok("Invalid target format. Use component_id/key".into());
        }
        let comp_id = parts[0];
        let key = parts[1];

        match action.as_str() {
            "approve" => {
                ctx.secrets.approve_secret(comp_id, key).await?;
                Ok(format!("Approved secret: {}/{}", comp_id, key))
            }
            "deny" => {
                ctx.secrets.deny_secret(comp_id, key).await?;
                Ok(format!("Denied secret: {}/{}", comp_id, key))
            }
            _ => Ok(format!("Unknown secret subcommand: {}", action)),
        }
    }
}
