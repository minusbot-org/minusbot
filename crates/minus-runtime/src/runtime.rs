use anyhow::Result;
use minus_agent::Agent;
use minus_api::traits::*;
use minus_api::types::*;
use minus_api::events::*;
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
    pub providers: Arc<RwLock<ProviderRegistry>>,
    pub tools: Arc<RwLock<ToolRegistry>>,
    pub skills: Arc<SkillManager>,
    pub scheduler: Arc<Scheduler>,
    pub policy: Arc<PolicyEngine>,
    pub agent: Arc<Agent>,
    pub commands: Arc<RwLock<minus_commands::CommandRegistry>>,
    pub config_providers: Arc<RwLock<std::collections::HashMap<String, Arc<dyn ConfigProvider>>>>,
    pub channels: Arc<RwLock<std::collections::HashMap<String, Arc<dyn Channel>>>>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl Runtime {
    pub async fn process_command(
        runtime: Arc<Self>,
        msg: &IncomingMessage,
        channel: Arc<dyn Channel>,
    ) -> Result<String> {
        let parsed = minus_commands::parse_slash_command(&msg.content);
        let registry = runtime.commands.read().await;
        
        if let Some(cmd) = registry.get(&parsed.name) {
            let ctx = CommandContext {
                chat_id: msg.chat_id.clone(),
                channel_id: msg.channel_id.clone(),
                channel: channel.clone(),
                db: runtime.clone(),
                config_registry: runtime.clone(),
                tools: runtime.clone(),
                secrets: runtime.clone(),
                providers: runtime.clone(),
                scheduler: runtime.clone(),
                channels: runtime.clone(),
                shutdown_trigger: None, // TODO: Connect to shutdown_tx
                all_commands: registry.list(),
            };
            
            cmd.execute(parsed.args, ctx).await
        } else {
            Ok(format!("Unknown command: /{}", parsed.name))
        }
    }

    pub async fn process_message(
        runtime: Arc<Self>,
        msg: &IncomingMessage,
        channel: Arc<dyn Channel>,
    ) -> Result<String> {
        if minus_commands::is_slash_command(&msg.content) {
            anyhow::bail!("Slash commands must be processed via process_command: {}", msg.content);
        }

        runtime.agent.handle_message(msg, channel).await
    }

    pub async fn get_store(&self, prefix: &str) -> Result<Arc<dyn MinusSecretStore>> {
        let vault = self.vault.clone().ok_or_else(|| anyhow::anyhow!("Vault not initialized"))?;
        Ok(Arc::new(RuntimeSecretStore {
            vault,
            prefix: prefix.to_string(),
        }))
    }
}

#[minus_api::async_trait]
impl MinusDatabase for Runtime {
    async fn list_chats(&self) -> Result<Vec<Chat>> {
        MinusDatabase::list_chats(&self.db).await
    }
    async fn get_chat(&self, id: &str) -> Result<Option<Chat>> {
        MinusDatabase::get_chat(&self.db, id).await
    }
    async fn ensure_chat(&self, id: &str, channel_id: &str, external_id: &str, title: Option<&str>) -> Result<()> {
        MinusDatabase::ensure_chat(&self.db, id, channel_id, external_id, title).await
    }
    async fn rename_chat(&self, id: &str, title: &str) -> Result<()> {
        MinusDatabase::rename_chat(&self.db, id, title).await
    }
    async fn delete_chat(&self, id: &str) -> Result<()> {
        MinusDatabase::delete_chat(&self.db, id).await
    }
    async fn get_messages(&self, chat_id: &str, limit: i64) -> Result<Vec<Message>> {
        MinusDatabase::get_messages(&self.db, chat_id, limit).await
    }
    async fn delete_messages(&self, chat_id: &str) -> Result<()> {
        MinusDatabase::delete_messages(&self.db, chat_id).await
    }
    async fn log_audit(&self, id: &str, actor: &str, action: &str, target: Option<&str>, metadata: Option<&str>, created_at: &str) -> Result<()> {
        MinusDatabase::log_audit(&self.db, id, actor, action, target, metadata, created_at).await
    }
    async fn tail_audit(&self, limit: i64) -> Result<Vec<AuditEvent>> {
        MinusDatabase::tail_audit(&self.db, limit).await
    }

    async fn list_memories(&self) -> Result<Vec<Memory>> {
        MinusDatabase::list_memories(&self.db).await
    }

    async fn save_memory(&self, id: &str, kind: &str, brief: &str, content: Option<&str>, is_important: bool) -> Result<()> {
        MinusDatabase::save_memory(&self.db, id, kind, brief, content, is_important).await
    }

    async fn delete_memory(&self, id: &str) -> Result<bool> {
        MinusDatabase::delete_memory(&self.db, id).await
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusScheduler for Runtime {
    async fn list_tasks(&self) -> Result<Vec<SchedulerTask>> {
        minus_api::traits::MinusScheduler::list_tasks(self.scheduler.as_ref()).await
    }

    async fn delete_task(&self, id: &str) -> Result<bool> {
        minus_api::traits::MinusScheduler::delete_task(self.scheduler.as_ref(), id).await
    }
}

#[minus_api::async_trait]
impl minus_api::traits::MinusChannels for Runtime {
    async fn list_channels(&self) -> Vec<ChannelStatus> {
        let channels = self.channels.read().await;
        let mut statuses = Vec::new();
        for chan in channels.values() {
            statuses.push(ChannelStatus {
                id: chan.id().to_string(),
                name: chan.name().to_string(),
                is_enabled: chan.is_enabled(),
                is_ready: chan.is_ready().await,
                active_chat_id: chan.get_active_chat().await,
            });
        }
        statuses
    }

    async fn get_channel(&self, id: &str) -> Option<Arc<dyn Channel>> {
        let channels = self.channels.read().await;
        channels.get(id).cloned()
    }

    async fn set_channel_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let chan = self.get_channel(id).await.ok_or_else(|| anyhow::anyhow!("Channel not found"))?;
        chan.set_enabled(enabled).await
    }
}

#[minus_api::async_trait]
impl MinusSecrets for Runtime {
    async fn get_store(&self, component_id: &str) -> Result<Arc<dyn MinusSecretStore>> {
        let prefix = format!("{}:", component_id);
        self.get_store(&prefix).await
    }

    async fn list_secret_declarations(&self) -> Result<Vec<SecretDeclaration>> {
        let env_secrets = self.secrets.read().await.list_redacted();
        let db_records = self.db.list_secret_declarations().await.unwrap_or_default();
        let mut declarations = Vec::new();

        for (key, _) in env_secrets {
            let db_record = db_records.iter().find(|r| r.key == key);
            let (comp_id, desc, mut approved) = if key.starts_with("PROVIDER_") {
                let parts: Vec<&str> = key.splitn(3, '_').collect();
                let comp = if parts.len() >= 2 { format!("provider:{}", parts[1].to_lowercase()) } else { "provider".to_string() };
                (comp, "Auto-declared (Provider)", true)
            } else {
                ("system".to_string(), "Orphan / Custom", false)
            };

            if let Some(r) = db_record { approved = r.approved; }

            declarations.push(SecretDeclaration {
                id: db_record.map(|r| r.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                component_id: ComponentId(db_record.map(|r| r.component_id.clone()).unwrap_or(comp_id)),
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

    async fn approve_secret(&self, component_id: &str, key: &str) -> Result<()> {
        self.db.set_secret_declaration_approval(component_id, key, true).await.map(|_| ())
    }

    async fn deny_secret(&self, component_id: &str, key: &str) -> Result<()> {
        self.db.set_secret_declaration_approval(component_id, key, false).await.map(|_| ())
    }
}

pub struct RuntimeSecretStore {
    vault: Arc<Vault>,
    prefix: String,
}

#[minus_api::async_trait]
impl MinusSecretStore for RuntimeSecretStore {
    async fn put_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        let full_key = format!("{}{}", self.prefix, key);
        self.vault.put_secret(&full_key, value)
    }
    async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let full_key = format!("{}{}", self.prefix, key);
        match self.vault.get_secret(&full_key) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }
    async fn delete_secret(&self, key: &str) -> Result<()> {
        let full_key = format!("{}{}", self.prefix, key);
        self.vault.delete_secret(&full_key).map(|_| ())
    }
    async fn list_secrets(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }
    async fn has_secret(&self, key: &str) -> Result<bool> {
        let full_key = format!("{}{}", self.prefix, key);
        Ok(self.vault.has_secret(&full_key))
    }
}

#[minus_api::async_trait]
impl MinusProviders for Runtime {
    async fn list_providers(&self) -> Result<Vec<(String, String)>> {
        Ok(self.providers.read().await.list())
    }
    async fn set_default_provider(&self, id: &str) -> Result<()> {
        self.providers.write().await.set_default(id)?;
        let mut cfg = self.config.write().await;
        cfg.provider.default = Some(id.to_string());
        cfg.save(&self.config_path)?;
        Ok(())
    }
    async fn list_text_models(&self) -> Result<Vec<String>> {
        let provider_id = self.get_default_provider_id().await?;
        let key_name = format!("PROVIDER_{}_API_KEY", provider_id.to_uppercase());
        let secret_key = if let Some(vault) = &self.vault {
            vault.get_secret(&key_name).ok().map(|b| String::from_utf8_lossy(&b).to_string())
                .or({
                    let sec = self.secrets.read().await;
                    sec.get(&key_name).map(|s| s.to_string())
                })
        } else {
            self.secrets.read().await.get(&key_name).map(|s| s.to_string())
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
        let mut cfg = self.config.write().await;
        cfg.provider.text_model = Some(model.to_string());
        cfg.save(&self.config_path)?;
        Ok(())
    }
}

#[minus_api::async_trait]
impl MinusConfigRegistry for Runtime {
    async fn get_provider(&self, id: &str) -> Option<Arc<dyn ConfigProvider>> {
        self.config_providers.read().await.get(id).cloned()
    }
    async fn list_providers(&self) -> Vec<Arc<dyn ConfigProvider>> {
        self.config_providers.read().await.values().cloned().collect()
    }
    async fn register(&self, provider: Arc<dyn ConfigProvider>) {
        let id = provider.id().to_string();
        self.config_providers.write().await.insert(id, provider);
    }
}

#[minus_api::async_trait]
impl MinusTools for Runtime {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().await.definitions()
    }
}
