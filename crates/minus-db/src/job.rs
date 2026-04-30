use anyhow::Result;
use chrono::Utc;
use sqlx::Row;

use crate::Database;

#[derive(Debug, Clone)]
pub struct JobRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub schedule_kind: String,
    pub schedule_expr: String,
    pub action_kind: String,
    pub action_json: String,
    pub target_chat_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub next_run_at: Option<String>,
}

impl Database {
    pub async fn create_job(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        schedule_kind: &str,
        schedule_expr: &str,
        action_kind: &str,
        action_json: &str,
        target_chat_id: Option<&str>,
        next_run_at: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO jobs (id, name, description, schedule_kind, schedule_expr, action_kind, action_json, target_chat_id, enabled, created_at, updated_at, next_run_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(schedule_kind)
        .bind(schedule_expr)
        .bind(action_kind)
        .bind(action_json)
        .bind(target_chat_id)
        .bind(&now)
        .bind(&now)
        .bind(next_run_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_jobs(&self) -> Result<Vec<JobRecord>> {
        let rows = sqlx::query(
            "SELECT id, name, description, schedule_kind, schedule_expr, action_kind, action_json, target_chat_id, enabled, created_at, updated_at, next_run_at
             FROM jobs ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| JobRecord {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                schedule_kind: r.get("schedule_kind"),
                schedule_expr: r.get("schedule_expr"),
                action_kind: r.get("action_kind"),
                action_json: r.get("action_json"),
                target_chat_id: r.get("target_chat_id"),
                enabled: r.get::<i64, _>("enabled") != 0,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                next_run_at: r.get("next_run_at"),
            })
            .collect())
    }

    pub async fn cancel_job(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE jobs SET enabled = 0, updated_at = ? WHERE id = ? AND enabled = 1")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_job_next_run(&self, id: &str, next_run_at: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE jobs SET next_run_at = ?, updated_at = ? WHERE id = ?")
            .bind(next_run_at)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}