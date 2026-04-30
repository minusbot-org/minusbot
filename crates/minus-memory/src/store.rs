use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minus_db::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn add_memory(&self, memory: MemoryItem) -> Result<()>;
    async fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>>;
    async fn list_memories(&self, limit: usize) -> Result<Vec<MemoryItem>>;
}

/// SQLite-backed memory store. Simple text-based search for MVP.
/// Future: swap with vector DB for RAG.
pub struct SqliteMemoryStore {
    db: Database,
}

impl SqliteMemoryStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn add_memory(&self, memory: MemoryItem) -> Result<()> {
        let tags_json = serde_json::to_string(&memory.tags)?;
        let created_at = memory.created_at.to_rfc3339();
        sqlx::query(
            "INSERT INTO memories (id, content, source, tags_json, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&memory.id)
        .bind(&memory.content)
        .bind(&memory.source)
        .bind(&tags_json)
        .bind(&created_at)
        .execute(&self.db.pool)
        .await?;
        Ok(())
    }

    async fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        // Simple LIKE-based search for MVP. Replace with vector search later.
        let pattern = format!("%{}%", query);
        let rows = sqlx::query(
            "SELECT id, content, source, tags_json, created_at FROM memories WHERE content LIKE ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let tags_str: Option<String> = r.get("tags_json");
                let tags: Vec<String> = tags_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                MemoryItem {
                    id: r.get("id"),
                    content: r.get("content"),
                    source: r.get("source"),
                    tags,
                    created_at: Utc::now(), // simplified for MVP
                }
            })
            .collect())
    }

    async fn list_memories(&self, limit: usize) -> Result<Vec<MemoryItem>> {
        let rows = sqlx::query(
            "SELECT id, content, source, tags_json, created_at FROM memories ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                let tags_str: Option<String> = r.get("tags_json");
                let tags: Vec<String> = tags_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                MemoryItem {
                    id: r.get("id"),
                    content: r.get("content"),
                    source: r.get("source"),
                    tags,
                    created_at: Utc::now(),
                }
            })
            .collect())
    }
}
