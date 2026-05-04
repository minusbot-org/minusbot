use anyhow::{Context, Result};
use async_trait::async_trait;
use minus_api::{Tool, ToolCall, ToolContext, ToolDefinition, ToolResult, ToolRisk};
use serde_json::json;

/// Tool: schedule_create
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
                    "content": { "type": "string", "description": "The message content to send." },
                    "generate": { "type": "boolean", "description": "If true, the assistant will process the message.", "default": false }
                },
                "required": ["name", "schedule", "content"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let name = call.arguments["name"].as_str().context("Missing name")?;
        let schedule_str = call.arguments["schedule"]
            .as_str()
            .context("Missing schedule")?;
        let content = call.arguments["content"]
            .as_str()
            .context("Missing content")?;

        let generate = call.arguments["generate"].as_bool().unwrap_or(false);

        // Use the MinusScheduler trait to create a task
        // The scheduler will be connected by the runtime
        let task_id = ctx
            .scheduler
            .create_task(name, schedule_str, content, Some(&ctx.chat_id.0), generate)
            .await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "schedule_create".into(),
            content: format!("Task '{}' scheduled successfully (ID: {}).", name, task_id),
            is_error: false,
        })
    }
}

/// Tool: schedule_list
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
        let tasks = ctx.scheduler.list_tasks().await?;
        if tasks.is_empty() {
            return Ok(ToolResult {
                tool_call_id: call.id,
                name: "schedule_list".into(),
                content: "No tasks scheduled.".into(),
                is_error: false,
            });
        }

        let mut res = String::from("Scheduled Tasks:\n");
        for task in tasks {
            let status = if task.enabled { "Enabled" } else { "Disabled" };
            let next = task
                .next_run
                .map(|d| d.to_rfc3339())
                .unwrap_or_else(|| "N/A".into());
            res.push_str(&format!(
                "- [{}] {}: {} ({}) - Next run: {}\n",
                task.id, task.name, task.schedule, status, next
            ));
        }

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "schedule_list".into(),
            content: res,
            is_error: false,
        })
    }
}

/// Tool: schedule_delete
pub struct ScheduleDeleteTool;

#[async_trait]
impl Tool for ScheduleDeleteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_delete".into(),
            description: "Cancel and delete a scheduled task.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The ID of the task to delete." }
                },
                "required": ["id"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let id = call.arguments["id"].as_str().context("Missing id")?;

        if ctx.scheduler.delete_task(id).await? {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: "schedule_delete".into(),
                content: format!("Task '{}' deleted successfully.", id),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                tool_call_id: call.id,
                name: "schedule_delete".into(),
                content: format!("Task '{}' not found or already deleted.", id),
                is_error: true,
            })
        }
    }
}

/// Tool: schedule_update — for now, delete + create since MinusScheduler trait is minimal
pub struct ScheduleUpdateTool;

#[async_trait]
impl Tool for ScheduleUpdateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "schedule_update".into(),
            description: "Update an existing scheduled task by deleting and recreating it.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The ID of the task to update." },
                    "name": { "type": "string", "description": "New name for the task." },
                    "schedule": {
                        "type": "string",
                        "description": "New schedule format: 'delay:10s', 'interval:1h', or 'cron:0 9 * * *'"
                    },
                    "content": { "type": "string", "description": "The message content to send." },
                    "generate": { "type": "boolean", "description": "If true, the assistant will process the message.", "default": false }
                },
                "required": ["id", "name", "schedule", "content"]
            }),
            risk: ToolRisk::Medium,
            side_effect: true,
        }
    }

    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult> {
        let id = call.arguments["id"].as_str().context("Missing id")?;
        let name = call.arguments["name"].as_str().context("Missing name")?;
        let schedule_str = call.arguments["schedule"]
            .as_str()
            .context("Missing schedule")?;
        let content = call.arguments["content"]
            .as_str()
            .context("Missing content")?;

        let generate = call.arguments["generate"].as_bool().unwrap_or(false);
        // Delete old
        ctx.scheduler.delete_task(id).await?;
        // Create new
        let new_id = ctx
            .scheduler
            .create_task(name, schedule_str, content, Some(&ctx.chat_id.0), generate)
            .await?;

        Ok(ToolResult {
            tool_call_id: call.id,
            name: "schedule_update".into(),
            content: format!("Task updated successfully (new ID: {}).", new_id),
            is_error: false,
        })
    }
}
