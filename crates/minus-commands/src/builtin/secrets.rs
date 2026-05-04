use crate::{ArgSpec, CommandSpec};
use anyhow::Result;
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct SecretCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new(
        "secrets",
        "/secrets <list|set|approve|deny> <component/key> [value]",
        "Manage programmatic secret declarations",
    )
    .category("security")
    .strict_subcommands()
    .handler(|args, ctx| Box::pin(async move { SecretCommand.execute(args, ctx).await }))
    .subcommand(sub("list", "/secrets list", "List secret declarations"))
    .subcommand(
        sub(
            "set",
            "/secrets set <component/key> <value>",
            "Set a secret",
        )
        .arg(ArgSpec::required("component/key", "Secret target"))
        .arg(ArgSpec::required("value", "Secret value")),
    )
    .subcommand(
        sub(
            "approve",
            "/secrets approve <component/key>",
            "Approve a secret declaration",
        )
        .arg(ArgSpec::required("component/key", "Secret target")),
    )
    .subcommand(
        sub(
            "deny",
            "/secrets deny <component/key>",
            "Deny a secret declaration",
        )
        .arg(ArgSpec::required("component/key", "Secret target")),
    )
}

fn sub(name: &'static str, usage: &'static str, about: &'static str) -> CommandSpec {
    CommandSpec::new(name, usage, about).handler(move |args, ctx| {
        Box::pin(async move {
            let mut full = vec![name.to_string()];
            full.extend(args);
            SecretCommand.execute(full, ctx).await
        })
    })
}

#[async_trait]
impl Command for SecretCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        if args.is_empty() || args[0] == "list" {
            let secrets = ctx.secrets.list_secret_declarations().await?;
            if secrets.is_empty() {
                return Ok("No secret declarations found.".into());
            }
            let lines: Vec<String> = secrets
                .iter()
                .map(|s| {
                    let status = if s.approved { "[OK]" } else { "[PENDING]" };
                    format!("{} {} — {}", status, s.key, s.description)
                })
                .collect();
            return Ok(format!("Secret declarations:\n{}", lines.join("\n")));
        }

        let action = &args[0];

        if action == "set" {
            if args.len() < 3 {
                return Ok("Usage: /secrets set <component/key> <value>".into());
            }
            let target = &args[1];
            let value = &args[2];

            let (comp_id, key) = if let Some(pos) = target.find('/') {
                (&target[..pos], &target[pos + 1..])
            } else {
                ("", target.as_str())
            };

            let store = ctx.secrets.get_store(comp_id).await?;
            store.put_secret(key, value.as_bytes()).await?;

            if comp_id.is_empty() {
                return Ok(format!("Successfully set root secret: {}", key));
            } else {
                return Ok(format!("Successfully set secret: {}/{}", comp_id, key));
            }
        }

        if args.len() < 2 {
            return Ok("Usage: /secrets <approve|deny> <component_id/key>".into());
        }

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
