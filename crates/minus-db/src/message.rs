use anyhow::Result;
use chrono::Utc;
use sqlx::Row;

use crate::Database;

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub id: String,
    pub chat_id: String,
    pub role: String,
    pub content: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

impl Database {
    pub async fn save_message(
        &self,
        id: &str,
        chat_id: &str,
        role: &str,
        content: &str,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO messages (id, chat_id, role, content, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(chat_id)
        .bind(role)
        .bind(content)
        .bind(metadata_json)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_messages(&self, chat_id: &str, limit: i64) -> Result<Vec<MessageRecord>> {
        let rows = sqlx::query(
            "SELECT id, chat_id, role, content, metadata_json, created_at
             FROM messages WHERE chat_id = ?
             ORDER BY created_at ASC
             LIMIT ?",
        )
        .bind(chat_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| MessageRecord {
                id: r.get("id"),
                chat_id: r.get("chat_id"),
                role: r.get("role"),
                content: r.get("content"),
                metadata_json: r.get("metadata_json"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn search_messages(&self, terms: &[String], limit: i64) -> Result<Vec<MessageRecord>> {
        if terms.is_empty() {
            return Ok(vec![]);
        }

        let mut query_str = String::from("SELECT id, chat_id, role, content, metadata_json, created_at FROM messages WHERE ");
        for (i, _) in terms.iter().enumerate() {
            if i > 0 {
                query_str.push_str(" OR ");
            }
            query_str.push_str("content LIKE ?");
        }
        query_str.push_str(" ORDER BY created_at DESC LIMIT ?");

        let mut query = sqlx::query(&query_str);
        for term in terms {
            query = query.bind(format!("%{}%", term));
        }
        query = query.bind(limit);

        let rows = query.fetch_all(&self.pool).await?;

        Ok(rows
            .iter()
            .map(|r| MessageRecord {
                id: r.get("id"),
                chat_id: r.get("chat_id"),
                role: r.get("role"),
                content: r.get("content"),
                metadata_json: r.get("metadata_json"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn delete_messages(&self, chat_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages WHERE chat_id = ?")
            .bind(chat_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}