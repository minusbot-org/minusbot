use anyhow::Result;
use minus_agent::Agent;
use minus_api::traits::MinusDatabase;
use minus_core::*;
use minus_db::Database;
use minus_env::{AppConfig, SecretsManager};
use minus_policy::PolicyEngine;
use minus_providers::ProviderRegistry;
use minus_scheduler::Scheduler;
use minus_skills::SkillManager;
use minus_tools::ToolRegistry;
use minus_vault::Vault;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Runtime {
    pub db: Database,
    pub config: Arc<RwLock<AppConfig>>,
    pub config_path: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
    pub secrets: Arc<RwLock<SecretsManager>>,
    pub vault: Option<Arc<Vault>>,
    pub policy: Arc<PolicyEngine>,
    pub providers: Arc<RwLock<ProviderRegistry>>,
    pub tools: Arc<RwLock<ToolRegistry>>,
    pub skills: Arc<SkillManager>,
    pub scheduler: Arc<Scheduler>,
    pub agent: Arc<Agent>,
    pub commands: Arc<RwLock<minus_commands::CommandRegistry>>,
    pub config_providers: Arc<RwLock<std::collections::HashMap<String, Arc<dyn minus_api::traits::ConfigProvider>>>>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl Runtime {
    /// Process an incoming message — either a slash command or an agent chat.
    pub async fn process_message(
        runtime: Arc<Runtime>,
        incoming: &IncomingMessage,
        channel: Arc<dyn minus_api::traits::Channel>,
    ) -> Result<String> {
        let content = incoming.content.trim();

        // Auto-create chat if it doesn't exist
        let _ = runtime.db.ensure_chat(
            &incoming.chat_id.0,
            &incoming.channel_id.0,
            &incoming.chat_id.0,
            None
        ).await;

        if minus_commands::is_slash_command(content) {
            let parsed = minus_commands::parse_slash_command(content);

            // Check registry
            let reg = runtime.commands.read().await;
            if let Some(cmd) = reg.get(&parsed.name) {
                let runtime_arc = runtime.clone();

                let ctx = CommandContext {
                    chat_id: incoming.chat_id.clone(),
                    channel_id: incoming.channel_id.clone(),
                    db: runtime_arc.clone(),
                    providers: runtime_arc.clone(),
                    config: runtime_arc.clone(),
                    config_registry: runtime_arc.clone(),
                    secrets: runtime_arc.clone(),
                    channel: channel.clone(),
                    all_commands: reg.list(),
                    shutdown_trigger: Some(runtime.shutdown_tx.clone()),
                };
                return cmd.execute(parsed.args, ctx).await;
            }

            Ok(format!("Unknown command: /{}", parsed.name))
        } else {
            runtime.agent.handle_message(incoming).await
        }
    }

    pub async fn status_text(&self) -> Result<String> {
        let providers = self.providers.read().await;
        let tools = self.tools.read().await;

        let provider_str = providers.default_id().unwrap_or("(none)");
        let model_str = providers.default_model().unwrap_or("(none)");
        let tool_count = tools.list_names().len();
        let vault_status = if self.vault.is_some() {
            "initialized"
        } else {
            "not initialized"
        };

        Ok(format!(
            "minusbot status:\n  provider: {}\n  model: {}\n  tools: {}\n  vault: {}",
            provider_str, model_str, tool_count, vault_status,
        ))
    }

    pub async fn audit(&self, actor: &str, action: &str, target: Option<&str>) {
        let event = AuditEvent::new(actor, action, target.map(|s| s.to_string()), None);
        if let Err(e) = self
            .db
            .log_audit(
                &event.id,
                &event.actor,
                &event.action,
                event.target.as_deref(),
                None,
                &event.created_at.to_rfc3339(),
            )
            .await
        {
            tracing::error!(error = %e, "Failed to log audit event");
        }
    }

    pub async fn get_chat_history_summary(&self, chat_id: &str) -> Result<String> {
        let max_messages = self.config.read().await.agent.max_messages as i64;
        let messages = self.db.get_messages(chat_id, max_messages).await?;

        let total_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE chat_id = ?")
                .bind(chat_id)
                .fetch_one(&self.db.pool)
                .await?;

        let omitted = total_count.saturating_sub(messages.len() as i64);

        let mut summary = String::new();
        if omitted > 0 {
            summary.push_str(&format!(
                "\x1b[90m({} additional messages omitted)\x1b[0m\n",
                omitted
            ));
        }

        for m in messages.iter().rev() {
            let color = match m.role.as_str() {
                "user" => "\x1b[1;34muser\x1b[0m",
                "assistant" => "\x1b[1;35mbot\x1b[0m ",
                "system" => "\x1b[1;33msys\x1b[0m  ",
                _ => "\x1b[1;36mtool\x1b[0m ",
            };
            summary.push_str(&format!("{} \x1b[90m›\x1b[0m {}\n", color, m.content));
        }

        Ok(summary)
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusDatabase for Runtime {
    async fn list_chats(&self) -> Result<Vec<minus_api::Chat>> {
        minus_api::traits::MinusDatabase::list_chats(&self.db).await
    }
    async fn get_chat(&self, id: &str) -> Result<Option<minus_api::Chat>> {
        minus_api::traits::MinusDatabase::get_chat(&self.db, id).await
    }
    async fn ensure_chat(
        &self,
        id: &str,
        channel_type: &str,
        channel_id: &str,
        title: Option<&str>,
    ) -> Result<()> {
        minus_api::traits::MinusDatabase::ensure_chat(&self.db, id, channel_type, channel_id, title)
            .await
    }
    async fn rename_chat(&self, id: &str, title: &str) -> Result<()> {
        minus_api::traits::MinusDatabase::rename_chat(&self.db, id, title).await
    }
    async fn delete_chat(&self, id: &str) -> Result<()> {
        minus_api::traits::MinusDatabase::delete_chat(&self.db, id).await
    }
    async fn get_messages(&self, chat_id: &str, limit: i64) -> Result<Vec<minus_api::Message>> {
        minus_api::traits::MinusDatabase::get_messages(&self.db, chat_id, limit).await
    }
    async fn delete_messages(&self, chat_id: &str) -> Result<()> {
        minus_api::traits::MinusDatabase::delete_messages(&self.db, chat_id).await
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
        minus_api::traits::MinusDatabase::log_audit(
            &self.db, id, actor, action, target, metadata, created_at,
        )
        .await
    }
    async fn tail_audit(&self, limit: i64) -> Result<Vec<minus_api::AuditEvent>> {
        minus_api::traits::MinusDatabase::tail_audit(&self.db, limit).await
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusSecrets for Runtime {
    async fn list_secret_declarations(&self) -> Result<Vec<minus_api::SecretDeclarationStatus>> {
        let env_secrets = self.secrets.read().await.list_redacted();
        let db_records = self.db.list_secret_declarations().await.unwrap_or_default();
        let mut declarations = Vec::new();

        for (key, _) in env_secrets {
            let db_record = db_records.iter().find(|r| r.key == key);
            
            let (comp_id, desc, mut approved) = if key.starts_with("PROVIDER_") {
                let parts: Vec<&str> = key.splitn(3, '_').collect();
                let comp = if parts.len() >= 2 { format!("provider:{}", parts[1].to_lowercase()) } else { "provider".to_string() };
                (comp, "Auto-declared (Provider)", true)
            } else if key.starts_with("INTEGRATION_") {
                let parts: Vec<&str> = key.splitn(3, '_').collect();
                let comp = if parts.len() >= 2 { format!("integration:{}", parts[1].to_lowercase()) } else { "integration".to_string() };
                (comp, "Auto-declared (Integration)", true)
            } else if key.starts_with("CHANNEL_") {
                let parts: Vec<&str> = key.splitn(3, '_').collect();
                let comp = if parts.len() >= 2 { format!("channel:{}", parts[1].to_lowercase()) } else { "channel".to_string() };
                (comp, "Auto-declared (Channel)", true)
            } else {
                ("system".to_string(), "Orphan / Custom", false)
            };

            // Override approval if it exists in DB
            if let Some(r) = db_record {
                approved = r.approved;
            }

            declarations.push(minus_api::types::SecretDeclaration {
                id: db_record.map(|r| r.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                component_id: minus_api::types::ComponentId(db_record.map(|r| r.component_id.clone()).unwrap_or(comp_id)),
                key: key.clone(),
                description: db_record.map(|r| r.description.clone()).unwrap_or_else(|| desc.to_string()),
                required: db_record.map(|r| r.required).unwrap_or(false),
                permissions: vec![],
                approved,
                created_at: db_record.map(|r| chrono::DateTime::parse_from_rfc3339(&r.created_at).unwrap_or_default().with_timezone(&chrono::Utc)).unwrap_or_else(chrono::Utc::now),
            });
        }

        Ok(declarations)
    }

    async fn approve_secret(&self, component_id: &str, key: &str) -> Result<bool> {
        self.db
            .set_secret_declaration_approval(component_id, key, true)
            .await
    }

    async fn deny_secret(&self, component_id: &str, key: &str) -> Result<bool> {
        self.db
            .set_secret_declaration_approval(component_id, key, false)
            .await
    }

    async fn get_store(
        &self,
        prefix: &str,
    ) -> Result<Arc<dyn minus_api::traits::MinusSecretStore>> {
        let vault = self
            .vault
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Vault not initialized"))?;
        Ok(Arc::new(RuntimeSecretStore {
            vault,
            prefix: prefix.to_string(),
        }))
    }
}

pub struct RuntimeSecretStore {
    vault: Arc<minus_vault::Vault>,
    prefix: String,
}

#[minus_api::async_trait]
impl minus_api::traits::MinusSecretStore for RuntimeSecretStore {
    async fn put_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        let full_key = format!("{}{}", self.prefix, key);
        self.vault.put_secret(&full_key, value)
    }
    async fn get_secret(&self, key: &str) -> Result<Vec<u8>> {
        let full_key = format!("{}{}", self.prefix, key);
        self.vault.get_secret(&full_key)
    }
    async fn delete_secret(&self, key: &str) -> Result<bool> {
        let full_key = format!("{}{}", self.prefix, key);
        self.vault.delete_secret(&full_key)
    }
    async fn has_secret(&self, key: &str) -> Result<bool> {
        let full_key = format!("{}{}", self.prefix, key);
        Ok(self.vault.has_secret(&full_key))
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusProviders for Runtime {
    async fn list_providers(&self) -> Result<Vec<(String, String)>> {
        Ok(self.providers.read().await.list())
    }

    async fn set_default_provider(&self, id: &str) -> Result<()> {
        self.providers.write().await.set_default(id)?;

        let mut cfg = self.config.write().await;
        cfg.provider.default = Some(id.to_string());
        cfg.save(&self.config_path)?;

        // Load previously-saved text_model for this provider
        let providers = self.providers.read().await;
        if let Some(p) = providers.get(id) {
            if let Some(cp) = p.config() {
                if let Ok(Some(model)) = cp.read_config("text_model").await {
                    self.providers.write().await.set_default_model(&model);
                }
            }
        }
        Ok(())
    }

    async fn list_text_models(&self) -> Result<Vec<String>> {
        let provider_id = self.get_default_provider_id().await?;
        let key_name = format!("PROVIDER_{}_API_KEY", provider_id.to_uppercase());

        let secret_key = if let Some(vault) = &self.vault {
            match vault.get_secret(&key_name) {
                Ok(bytes) => Some(String::from_utf8_lossy(&bytes).to_string()),
                Err(_) => {
                    let sec = self.secrets.read().await;
                    sec.get(&key_name).map(|s| s.to_string())
                }
            }
        } else {
            let sec = self.secrets.read().await;
            sec.get(&key_name).map(|s| s.to_string())
        };

        self.providers.read().await.get_text_models(secret_key).await
    }

    async fn get_default_provider_id(&self) -> Result<String> {
        Ok(self.providers.read().await.default_id().unwrap_or("").to_string())
    }

    async fn get_default_text_model(&self) -> Result<String> {
        Ok(self.providers.read().await.default_model().unwrap_or("").to_string())
    }

    async fn set_default_text_model(&self, model: &str) -> Result<()> {
        self.providers.write().await.set_default_model(model);

        // Persist to provider's own config file
        let providers = self.providers.read().await;
        if let Some(p) = providers.default_provider() {
            if let Some(cp) = p.config() {
                cp.set_config("text_model", model).await?;
                return Ok(());
            }
        }

        // Fallback: save to main config
        let mut cfg = self.config.write().await;
        cfg.provider.text_model = Some(model.to_string());
        cfg.save(&self.config_path)?;
        Ok(())
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusConfig for Runtime {
    async fn get_config(&self) -> Result<serde_json::Value> {
        let cfg = self.config.read().await;
        Ok(serde_json::to_value(&*cfg)?)
    }
    async fn update_config(&self, patch: serde_json::Value) -> Result<()> {
        let mut cfg = self.config.write().await;
        let mut current = serde_json::to_value(&*cfg)?;
        if let Some(obj) = patch.as_object() {
            for (k, v) in obj {
                current[k] = v.clone();
            }
        }
        *cfg = serde_json::from_value(current)?;
        cfg.save(&self.config_path)?;
        Ok(())
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusConfigRegistry for Runtime {
    async fn get_provider(&self, id: &str) -> Option<Arc<dyn minus_api::traits::ConfigProvider>> {
        self.config_providers.read().await.get(id).cloned()
    }

    async fn list_providers(&self) -> Vec<Arc<dyn minus_api::traits::ConfigProvider>> {
        self.config_providers.read().await.values().cloned().collect()
    }

    async fn register(&self, provider: Arc<dyn minus_api::traits::ConfigProvider>) {
        let id = provider.id().to_string();
        self.config_providers.write().await.insert(id, provider);
    }
}
