use anyhow::Result;
use sqlx::Row;

use crate::Database;

#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub details_json: Option<String>,
    pub created_at: String,
}

impl Database {
    pub async fn log_audit(
        &self,
        id: &str,
        actor: &str,
        action: &str,
        target: Option<&str>,
        details_json: Option<&str>,
        created_at: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_events (id, actor, action, target, details_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(actor)
        .bind(action)
        .bind(target)
        .bind(details_json)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn tail_audit(&self, limit: i64) -> Result<Vec<AuditRecord>> {
        let rows = sqlx::query(
            "SELECT id, actor, action, target, details_json, created_at
             FROM audit_events ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| AuditRecord {
                id: r.get("id"),
                actor: r.get("actor"),
                action: r.get("action"),
                target: r.get("target"),
                details_json: r.get("details_json"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}