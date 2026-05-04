use anyhow::Result;
use chrono::Utc;
use sqlx::Row;

use crate::Database;

#[derive(Debug, Clone)]
pub struct ChatRecord {
    pub id: String,
    pub channel_id: String,
    pub external_id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
    pub async fn ensure_chat(
        &self,
        id: &str,
        channel_id: &str,
        external_id: &str,
        title: Option<&str>,
    ) -> Result<ChatRecord> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT OR IGNORE INTO chats (id, channel_id, external_id, title, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(channel_id)
        .bind(external_id)
        .bind(title)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query(
            "SELECT id, channel_id, external_id, title, created_at, updated_at FROM chats WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(ChatRecord {
            id: row.get("id"),
            channel_id: row.get("channel_id"),
            external_id: row.get("external_id"),
            title: row.get("title"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn list_chats(&self) -> Result<Vec<ChatRecord>> {
        let rows = sqlx::query(
            "SELECT id, channel_id, external_id, title, created_at, updated_at FROM chats ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| ChatRecord {
                id: r.get("id"),
                channel_id: r.get("channel_id"),
                external_id: r.get("external_id"),
                title: r.get("title"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn rename_chat(&self, id: &str, title: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE chats SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_chat(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM messages WHERE chat_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM chats WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_chat(&self, id: &str) -> Result<Option<ChatRecord>> {
        let row = sqlx::query(
            "SELECT id, channel_id, external_id, title, created_at, updated_at FROM chats WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| ChatRecord {
            id: r.get("id"),
            channel_id: r.get("channel_id"),
            external_id: r.get("external_id"),
            title: r.get("title"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }
}
