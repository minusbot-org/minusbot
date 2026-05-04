use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn open(database_url: &str) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(path) = database_url.strip_prefix("sqlite://") {
            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }

        use sqlx::sqlite::SqliteConnectOptions;
        use std::str::FromStr;

        let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        let migration_sql = include_str!("../migrations/001_initial.sql");
        // Run each statement individually
        for stmt in migration_sql.split(';') {
            let stmt = stmt.trim();
            if !stmt.is_empty() {
                sqlx::query(stmt).execute(&self.pool).await?;
            }
        }

        // Migration: Add 'kind' to memories if missing
        let rows = sqlx::query("PRAGMA table_info(memories)")
            .fetch_all(&self.pool)
            .await?;

        use sqlx::Row;
        let has_kind = rows.iter().any(|r| r.get::<String, _>("name") == "kind");
        if !has_kind {
            tracing::info!("Migrating memories table: adding kind, brief, updated_at columns");
            sqlx::query("ALTER TABLE memories ADD COLUMN kind TEXT NOT NULL DEFAULT 'long'")
                .execute(&self.pool)
                .await?;
            sqlx::query("ALTER TABLE memories ADD COLUMN brief TEXT NOT NULL DEFAULT ''")
                .execute(&self.pool)
                .await?;
            sqlx::query("ALTER TABLE memories ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''")
                .execute(&self.pool)
                .await?;
            // Copy content to brief for existing ones
            sqlx::query("UPDATE memories SET brief = SUBSTR(content, 1, 100)")
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusDatabase for Database {
    async fn list_chats(&self) -> Result<Vec<minus_api::Chat>> {
        let records = self.list_chats().await?;
        Ok(records
            .into_iter()
            .map(|r| minus_api::Chat {
                id: r.id,
                channel_id: r.channel_id,
                external_id: r.external_id,
                title: r.title,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }
    async fn get_chat(&self, id: &str) -> Result<Option<minus_api::Chat>> {
        let record = self.get_chat(id).await?;
        Ok(record.map(|r| minus_api::Chat {
            id: r.id,
            channel_id: r.channel_id,
            external_id: r.external_id,
            title: r.title,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }
    async fn ensure_chat(
        &self,
        id: &str,
        channel_id: &str,
        external_id: &str,
        title: Option<&str>,
    ) -> Result<()> {
        let _ = self.ensure_chat(id, channel_id, external_id, title).await?;
        Ok(())
    }
    async fn rename_chat(&self, id: &str, title: &str) -> Result<()> {
        self.rename_chat(id, title).await
    }
    async fn delete_chat(&self, id: &str) -> Result<()> {
        self.delete_chat(id).await
    }
    async fn get_messages(&self, chat_id: &str, limit: i64) -> Result<Vec<minus_api::Message>> {
        let records = self.get_messages(chat_id, limit).await?;
        Ok(records
            .into_iter()
            .map(|r| minus_api::Message {
                id: r.id,
                chat_id: r.chat_id,
                role: r.role,
                content: r.content,
                metadata_json: r.metadata_json,
                created_at: r.created_at,
            })
            .collect())
    }
    async fn save_message(
        &self,
        id: &str,
        chat_id: &str,
        role: &str,
        content: &str,
        metadata: Option<&str>,
    ) -> Result<()> {
        self.save_message(id, chat_id, role, content, metadata)
            .await
    }
    async fn delete_messages(&self, chat_id: &str) -> Result<()> {
        self.delete_messages(chat_id).await
    }
    async fn log_audit(
        &self,
        id: &str,
        actor: &str,
        action: &str,
        target: Option<&str>,
        metadata: Option<&str>,
        created_at: &str,
    ) -> Result<()> {
        self.log_audit(id, actor, action, target, metadata, created_at)
            .await
    }
    async fn tail_audit(&self, limit: i64) -> Result<Vec<minus_api::AuditEvent>> {
        let records = self.tail_audit(limit).await?;
        Ok(records
            .into_iter()
            .map(|r| minus_api::AuditEvent {
                id: r.id,
                actor: r.actor,
                action: r.action,
                target: r.target,
                details: r.details_json.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
            })
            .collect())
    }

    async fn list_memories(&self) -> Result<Vec<minus_api::Memory>> {
        let records = self.list_memories().await?;
        Ok(records
            .into_iter()
            .map(|r| minus_api::Memory {
                id: r.id,
                kind: r.kind,
                brief: r.brief,
                content: r.content,
                is_important: r.is_important,
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
            })
            .collect())
    }

    async fn get_important_memories(&self) -> Result<Vec<minus_api::Memory>> {
        let records = self.get_important_memories().await?;
        Ok(records
            .into_iter()
            .map(|r| minus_api::Memory {
                id: r.id,
                kind: r.kind,
                brief: r.brief,
                content: r.content,
                is_important: r.is_important,
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
            })
            .collect())
    }

    async fn save_memory(
        &self,
        id: &str,
        kind: &str,
        brief: &str,
        content: Option<&str>,
        is_important: bool,
    ) -> Result<()> {
        self.save_memory(id, kind, brief, content, is_important)
            .await
    }

    async fn delete_memory(&self, id: &str) -> Result<bool> {
        self.delete_memory(id).await
    }
}
