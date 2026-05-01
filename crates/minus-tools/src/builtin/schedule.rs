use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use minus_core::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use minus_scheduler::{Scheduler, JobAction};
use serde_json::json;


/// Tool: schedule.create
pub struct ScheduleCreateTool;

#[async_trait]
impl Tool for ScheduleCreateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_create".into(),
            description: "Schedule a task to run in the future or periodically.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "A descriptive name for the task." },
                    "schedule": { 
                        "type": "string", 
                        "description": "Schedule format: 'delay:10s', 'interval:1h', or 'cron:0 9 * * *'" 
                    },
                    "action": {
                        "type": "string",
                        "enum": ["message_send"],
                        "description": "What to do when triggered."
                    },
                    "content": { "type": "string", "description": "The message content to send." },
                    "generate": { "type": "boolean", "description": "If true, the assistant will process the message and generate a new response.", "default": false }
                },
                "required": ["name", "schedule", "action", "content"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let scheduler_any = ctx.scheduler.context("Scheduler not found in ToolContext")?;
        let scheduler = scheduler_any.clone().downcast::<Scheduler>()
            .map_err(|_| anyhow::anyhow!("Failed to downcast scheduler"))?;
        
        let name = call.arguments["name"].as_str().context("Missing name")?;
        let schedule_str = call.arguments["schedule"].as_str().context("Missing schedule")?;
        let action_str = call.arguments["action"].as_str().context("Missing action")?;
        let content = call.arguments["content"].as_str().context("Missing content")?;
        let generate = call.arguments["generate"].as_bool().unwrap_or(false);

        if action_str != "message_send" {
            bail!("Unsupported action: {}", action_str);
        }

        let job_id = scheduler.create_job_v2(
            name,
            schedule_str,
            JobAction::MessageSend {
                content: content.to_string(),
                generate,
            },
            Some(&ctx.chat_id.0),
        ).await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "schedule_create".into(),
            content: format!("Task '{}' scheduled successfully (ID: {}).", name, job_id),
            is_error: false,
        })
    }
}

/// Tool: schedule.list
pub struct ScheduleListTool;

#[async_trait]
impl Tool for ScheduleListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_list".into(),
            description: "List all scheduled tasks.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            risk: ToolRisk::Low,
            side_effect: false,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let scheduler_any = ctx.scheduler.context("Scheduler not found in ToolContext")?;
        let scheduler = scheduler_any.clone().downcast::<Scheduler>()
            .map_err(|_| anyhow::anyhow!("Failed to downcast scheduler"))?;
        
        let jobs = scheduler.list_jobs().await?;
        if jobs.is_empty() {
            return Ok(ToolResult {
                tool_call_id: call.id,
                name: "schedule_list".into(),
                content: "No tasks scheduled.".into(),
                is_error: false,
            });
        }

        let mut res = String::from("Scheduled Tasks:\n");
        for job in jobs {
            let status = if job.enabled { "Enabled" } else { "Disabled" };
            res.push_str(&format!("- [{}] {}: {} ({}) - Next run: {}\n", 
                job.id, job.name, job.schedule_expr, status, job.next_run_at.as_deref().unwrap_or("N/A")));
        }

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "schedule_list".into(),
            content: res,
            is_error: false,
        })
    }
}

/// Tool: schedule.abort
pub struct ScheduleAbortTool;

#[async_trait]
impl Tool for ScheduleAbortTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_abort".into(),
            description: "Cancel a scheduled task.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The ID of the task to cancel." }
                },
                "required": ["id"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let scheduler_any = ctx.scheduler.context("Scheduler not found in ToolContext")?;
        let scheduler = scheduler_any.clone().downcast::<Scheduler>()
            .map_err(|_| anyhow::anyhow!("Failed to downcast scheduler"))?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;

        if scheduler.cancel_job(id).await? {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: "schedule_abort".into(),
                content: format!("Task '{}' cancelled.", id),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: "schedule_abort".into(),
                content: format!("Task '{}' not found or already cancelled.", id),
                is_error: true,
            })
        }
    }
}

/// Tool: schedule.update
pub struct ScheduleUpdateTool;

#[async_trait]
impl Tool for ScheduleUpdateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_update".into(),
            description: "Update an existing scheduled task.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The ID of the task to update." },
                    "name": { "type": "string", "description": "New name for the task." },
                    "schedule": { 
                        "type": "string", 
                        "description": "New schedule format: 'delay:10s', 'interval:1h', or 'cron:0 9 * * *'" 
                    },
                    "action": {
                        "type": "string",
                        "enum": ["message_send"],
                        "description": "What to do when triggered."
                    },
                    "content": { "type": "string", "description": "The message content to send." },
                    "generate": { "type": "boolean", "description": "If true, the assistant will process the message and generate a new response.", "default": false }
                },
                "required": ["id", "name", "schedule", "action", "content"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let scheduler_any = ctx.scheduler.context("Scheduler not found in ToolContext")?;
        let scheduler = scheduler_any.clone().downcast::<Scheduler>()
            .map_err(|_| anyhow::anyhow!("Failed to downcast scheduler"))?;
        
        let id = call.arguments["id"].as_str().context("Missing id")?;
        let name = call.arguments["name"].as_str().context("Missing name")?;
        let schedule_str = call.arguments["schedule"].as_str().context("Missing schedule")?;
        let action_str = call.arguments["action"].as_str().context("Missing action")?;
        let content = call.arguments["content"].as_str().context("Missing content")?;
        let generate = call.arguments["generate"].as_bool().unwrap_or(false);

        if action_str != "message_send" {
            bail!("Unsupported action: {}", action_str);
        }

        scheduler.update_job(
            id,
            name,
            schedule_str,
            JobAction::MessageSend {
                content: content.to_string(),
                generate,
            },
        ).await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "schedule_update".into(),
            content: format!("Task '{}' updated successfully.", id),
            is_error: false,
        })
    }
}
