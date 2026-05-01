use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_api::traits::{Command, CommandContext, CommandDefinition};

pub struct SchedulerCommand;

#[async_trait]
impl Command for SchedulerCommand {
    fn definition(&self) -> CommandDefinition {
        CommandDefinition {
            name: "scheduler".into(),
            description: "Manage scheduled tasks.".into(),
            usage: "/scheduler <list|delete|search> [args]".into(),
            category: "system".into(),
            aliases: vec!["sched".into()],
            min_args: 0,
        }
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
                    let next = t.next_run.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or("N/A".into());
                    res.push_str(&format!("- [{}] {}: {} ({}) | Next: {}\n", t.id, t.name, t.schedule, status, next));
                }
                Ok(res)
            }
            "delete" => {
                let id = args.get(1).context("Missing task ID: /scheduler delete <id>")?;
                if ctx.scheduler.delete_task(id).await? {
                    Ok(format!("Task '{}' deleted.", id))
                } else {
                    Ok(format!("Task '{}' not found.", id))
                }
            }
            "search" => {
                let term = args.get(1).context("Missing search term: /scheduler search <term>")?;
                let tasks = ctx.scheduler.list_tasks().await?;
                let filtered: Vec<_> = tasks.into_iter()
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
            _ => bail!("Unknown subcommand: {}. Use list, delete, or search.", subcommand),
        }
    }
}
