use crate::{ArgSpec, CommandSpec};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_api::{Command, CommandContext};

pub struct SchedulerCommand;

pub fn spec() -> CommandSpec {
    CommandSpec::new(
        "scheduler",
        "/scheduler <list|delete|search> [args]",
        "Manage scheduled tasks",
    )
    .category("system")
    .command_alias("sched")
    .strict_subcommands()
    .handler(|args, ctx| Box::pin(async move { SchedulerCommand.execute(args, ctx).await }))
    .subcommand(sub("list", "/scheduler list", "List scheduled tasks"))
    .subcommand(
        sub(
            "delete",
            "/scheduler delete <id>",
            "Delete a scheduled task",
        )
        .arg(ArgSpec::required("id", "Task id")),
    )
    .subcommand(
        sub(
            "search",
            "/scheduler search <term>",
            "Search scheduled tasks",
        )
        .arg(ArgSpec::required("term", "Search term")),
    )
}

fn sub(name: &'static str, usage: &'static str, about: &'static str) -> CommandSpec {
    CommandSpec::new(name, usage, about).handler(move |args, ctx| {
        Box::pin(async move {
            let mut full = vec![name.to_string()];
            full.extend(args);
            SchedulerCommand.execute(full, ctx).await
        })
    })
}

#[async_trait]
impl Command for SchedulerCommand {
    fn spec(&self) -> CommandSpec {
        spec()
    }

    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String> {
        let subcommand = args.get(0).map(|s| s.as_str()).unwrap_or("list");

        match subcommand {
            "list" => {
                let tasks = ctx.scheduler.list_tasks().await?;
                if tasks.is_empty() {
                    return Ok("No tasks scheduled.".into());
                }

                let mut res = String::from("Scheduled Tasks:\n");
                for t in tasks {
                    let status = if t.enabled { "Enabled" } else { "Disabled" };
                    let next = t
                        .next_run
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or("N/A".into());
                    res.push_str(&format!(
                        "- [{}] {}: {} ({}) | Next: {}\n",
                        t.id, t.name, t.schedule, status, next
                    ));
                }
                Ok(res)
            }
            "delete" => {
                let id = args
                    .get(1)
                    .context("Missing task ID: /scheduler delete <id>")?;
                if ctx.scheduler.delete_task(id).await? {
                    Ok(format!("Task '{}' deleted.", id))
                } else {
                    Ok(format!("Task '{}' not found.", id))
                }
            }
            "search" => {
                let term = args
                    .get(1)
                    .context("Missing search term: /scheduler search <term>")?;
                let tasks = ctx.scheduler.list_tasks().await?;
                let filtered: Vec<_> = tasks
                    .into_iter()
                    .filter(|t| t.name.contains(term) || t.schedule.contains(term))
                    .collect();

                if filtered.is_empty() {
                    return Ok(format!("No tasks found matching '{}'.", term));
                }

                let mut res = format!("Search results for '{}':\n", term);
                for t in filtered {
                    res.push_str(&format!("- [{}] {}\n", t.id, t.name));
                }
                Ok(res)
            }
            _ => bail!(
                "Unknown subcommand: {}. Use list, delete, or search.",
                subcommand
            ),
        }
    }
}
