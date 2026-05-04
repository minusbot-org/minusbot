use anyhow::Result;
use chrono::Utc;
use sqlx::Row;

use crate::Database;

#[derive(Debug, Clone)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags_json: Option<String>,
    pub content: String,
    pub file_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
    pub async fn upsert_skill(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        tags_json: Option<&str>,
        content: &str,
        file_path: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR REPLACE INTO skills (id, name, description, tags_json, content, file_path, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, COALESCE((SELECT created_at FROM skills WHERE id = ?), ?), ?)",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(tags_json)
        .bind(content)
        .bind(file_path)
        .bind(id)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_skills(&self) -> Result<Vec<SkillRecord>> {
        let rows = sqlx::query(
            "SELECT id, name, description, tags_json, content, file_path, created_at, updated_at FROM skills ORDER BY name ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| SkillRecord {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                tags_json: r.get("tags_json"),
                content: r.get("content"),
                file_path: r.get("file_path"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn get_skill_by_name(&self, name: &str) -> Result<Option<SkillRecord>> {
        let row = sqlx::query(
            "SELECT id, name, description, tags_json, content, file_path, created_at, updated_at FROM skills WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| SkillRecord {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            tags_json: r.get("tags_json"),
            content: r.get("content"),
            file_path: r.get("file_path"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn load_skill_for_chat(&self, chat_id: &str, skill_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO loaded_skills (chat_id, skill_id, loaded_at) VALUES (?, ?, ?)",
        )
        .bind(chat_id)
        .bind(skill_id)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unload_skill_for_chat(&self, chat_id: &str, skill_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM loaded_skills WHERE chat_id = ? AND skill_id = ?")
            .bind(chat_id)
            .bind(skill_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_loaded_skills(&self, chat_id: &str) -> Result<Vec<SkillRecord>> {
        let rows = sqlx::query(
            "SELECT s.id, s.name, s.description, s.tags_json, s.content, s.file_path, s.created_at, s.updated_at
             FROM skills s
             JOIN loaded_skills ls ON s.id = ls.skill_id
             WHERE ls.chat_id = ?",
        )
        .bind(chat_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| SkillRecord {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                tags_json: r.get("tags_json"),
                content: r.get("content"),
                file_path: r.get("file_path"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }
}
