use anyhow::Result;
use chrono::Utc;
use sqlx::Row;
use crate::Database;

#[derive(Debug, Clone)]
pub struct MemoryRecord {
    pub id: String,
    pub kind: String,
    pub brief: String,
    pub content: Option<String>,
    pub is_important: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
    pub async fn save_memory(
        &self,
        id: &str,
        kind: &str,
        brief: &str,
        content: Option<&str>,
        is_important: bool,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO memories (id, kind, brief, content, is_important, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                brief = excluded.brief,
                content = excluded.content,
                is_important = excluded.is_important,
                updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(kind)
        .bind(brief)
        .bind(content)
        .bind(is_important as i32)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_memories_by_kind(&self, kind: &str) -> Result<Vec<MemoryRecord>> {
        let rows = sqlx::query(
            "SELECT id, kind, brief, content, is_important, created_at, updated_at FROM memories WHERE kind = ?",
        )
        .bind(kind)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| MemoryRecord {
                id: r.get("id"),
                kind: r.get("kind"),
                brief: r.get("brief"),
                content: r.get("content"),
                is_important: r.get::<i32, _>("is_important") != 0,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn get_important_memories(&self) -> Result<Vec<MemoryRecord>> {
        let rows = sqlx::query(
            "SELECT id, kind, brief, content, is_important, created_at, updated_at FROM memories WHERE is_important = 1",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| MemoryRecord {
                id: r.get("id"),
                kind: r.get("kind"),
                brief: r.get("brief"),
                content: r.get("content"),
                is_important: true,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn search_memories(&self, terms: &[String]) -> Result<Vec<MemoryRecord>> {
        if terms.is_empty() {
            return Ok(vec![]);
        }

        let mut query = String::from("SELECT id, kind, brief, content, is_important, created_at, updated_at FROM memories WHERE ");
        for (i, _) in terms.iter().enumerate() {
            if i > 0 {
                query.push_str(" OR ");
            }
            query.push_str("(brief LIKE ? OR content LIKE ?)");
        }

        let mut sql = sqlx::query(&query);
        for term in terms {
            let pattern = format!("%{}%", term);
            sql = sql.bind(pattern.clone()).bind(pattern);
        }

        let rows = sql.fetch_all(&self.pool).await?;

        Ok(rows
            .iter()
            .map(|r| MemoryRecord {
                id: r.get("id"),
                kind: r.get("kind"),
                brief: r.get("brief"),
                content: r.get("content"),
                is_important: r.get::<i32, _>("is_important") != 0,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn get_memory(&self, id: &str) -> Result<Option<MemoryRecord>> {
        let row = sqlx::query("SELECT id, kind, brief, content, is_important, created_at, updated_at FROM memories WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| MemoryRecord {
            id: r.get("id"),
            kind: r.get("kind"),
            brief: r.get("brief"),
            content: r.get("content"),
            is_important: r.get::<i32, _>("is_important") != 0,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn list_memories(&self) -> Result<Vec<MemoryRecord>> {
        let rows = sqlx::query(
            "SELECT id, kind, brief, content, is_important, created_at, updated_at FROM memories ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| MemoryRecord {
                id: r.get("id"),
                kind: r.get("kind"),
                brief: r.get("brief"),
                content: r.get("content"),
                is_important: r.get::<i32, _>("is_important") != 0,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }
}
