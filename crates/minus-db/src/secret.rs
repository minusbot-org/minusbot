use anyhow::Result;
use sqlx::Row;

use crate::Database;

#[derive(Debug, Clone)]
pub struct SecretDeclarationRecord {
    pub id: String,
    pub component_id: String,
    pub key: String,
    pub description: String,
    pub required: bool,
    pub permissions_json: String,
    pub approved: bool,
    pub created_at: String,
}

impl Database {
    pub async fn upsert_secret_declaration(
        &self,
        id: &str,
        component_id: &str,
        key: &str,
        description: &str,
        required: bool,
        permissions_json: &str,
        approved: bool,
        created_at: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO secret_declarations (id, component_id, key, description, required, permissions_json, approved, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(component_id)
        .bind(key)
        .bind(description)
        .bind(required as i64)
        .bind(permissions_json)
        .bind(approved as i64)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_secret_declarations(&self) -> Result<Vec<SecretDeclarationRecord>> {
        let rows = sqlx::query(
            "SELECT id, component_id, key, description, required, permissions_json, approved, created_at
             FROM secret_declarations ORDER BY component_id, key",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| SecretDeclarationRecord {
                id: r.get("id"),
                component_id: r.get("component_id"),
                key: r.get("key"),
                description: r.get("description"),
                required: r.get::<i64, _>("required") != 0,
                permissions_json: r.get("permissions_json"),
                approved: r.get::<i64, _>("approved") != 0,
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn set_secret_declaration_approval(
        &self,
        component_id: &str,
        key: &str,
        approved: bool,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE secret_declarations SET approved = ? WHERE component_id = ? AND key = ?",
        )
        .bind(approved as i64)
        .bind(component_id)
        .bind(key)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
