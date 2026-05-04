use anyhow::Result;
use minus_agent::Agent;
use minus_api::traits::*;
use minus_api::types::*;
use minus_db::Database;
use minus_env::{AppConfig, SecretsManager};
use minus_policy::PolicyEngine;
use minus_providers::ProviderRegistry;
use minus_scheduler::Scheduler;
use minus_skills::SkillManager;
use minus_tools::ToolRegistry;
use minus_vault::{SubVault, Vault};
use std::collections::HashMap;
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
    pub config_providers: Arc<RwLock<HashMap<String, Arc<dyn ConfigProvider>>>>,
    pub channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    pub integrations: Arc<RwLock<Vec<Arc<dyn Integration>>>>,
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
            let (shutdown_tx, _) = tokio::sync::mpsc::channel(1);
            let ctx = CommandContext {
                chat_id: msg.chat_id.clone(),
                channel_id: msg.channel_id.clone(),
                channel: channel.clone(),
                db: Arc::new(runtime.db.clone()) as Arc<dyn MinusDatabase>,
                config_registry: runtime.clone(),
                tools: runtime.clone(),
                secrets: runtime.clone(),
                providers: runtime.clone(),
                scheduler: runtime.clone(),
                channels: runtime.clone(),
                shutdown_trigger: Some(shutdown_tx),
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

        runtime.agent.clone().handle_message(msg, channel).await
    }

    /// Register a config provider (e.g., from a channel or provider)
    pub async fn register(&self, provider: Arc<dyn ConfigProvider>) {
        let id = provider.id().to_string();
        self.config_providers.write().await.insert(id, provider);
    }

    /// Register a channel
    pub async fn register_channel(&self, channel: Arc<dyn Channel>) {
        let id = Channel::id(channel.as_ref()).to_string();
        // If channel has a config provider, register that too
        if let Some(config) = channel.config() {
            self.register(config).await;
        }
        self.channels.write().await.insert(id, channel);
    }

    /// Register an integration
    pub async fn register_integration(&self, integration: Arc<dyn Integration>) {
        self.integrations.write().await.push(integration);
    }

    /// Stop all channels gracefully
    pub async fn stop(&self) {
        let channels = self.channels.read().await;
        for (id, chan) in channels.iter() {
            tracing::info!(channel_id = %id, "Stopping channel");
            if let Err(e) = chan.stop().await {
                tracing::error!(channel_id = %id, error = %e, "Failed to stop channel");
            }
        }
    }

    /// Resolve a secret value by checking vault first, then env secrets.
    async fn resolve_secret_value(&self, key: &str) -> Result<Option<String>> {
        // Check vault first
        if let Some(vault) = &self.vault {
            if vault.has_secret(key) {
                if let Ok(bytes) = vault.get_secret(key) {
                    return Ok(Some(String::from_utf8_lossy(&bytes).to_string()));
                }
            }
        }
        // Fallback to env secrets
        let sec = self.secrets.read().await;
        Ok(sec.get(key).map(|s| s.to_string()))
    }
}

// =============================================================================
// MinusScheduler — delegate to Scheduler
// =============================================================================

#[minus_api::async_trait]
impl MinusScheduler for Runtime {
    async fn list_tasks(&self) -> Result<Vec<SchedulerTask>> {
        minus_api::traits::MinusScheduler::list_tasks(self.scheduler.as_ref()).await
    }

    async fn delete_task(&self, id: &str) -> Result<bool> {
        minus_api::traits::MinusScheduler::delete_task(self.scheduler.as_ref(), id).await
    }

    async fn create_task(&self, name: &str, schedule: &str, prompt: &str, target_chat_id: Option<&str>, generate: bool) -> Result<String> {
        minus_api::traits::MinusScheduler::create_task(self.scheduler.as_ref(), name, schedule, prompt, target_chat_id, generate).await
    }
}

// =============================================================================
// MinusChannels — backed by in-memory HashMap (absorbed from minus-channels)
// =============================================================================

#[minus_api::async_trait]
impl MinusChannels for Runtime {
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
        let chan = self.get_channel(id).await
            .ok_or_else(|| anyhow::anyhow!("Channel '{}' not found", id))?;
        chan.set_enabled(enabled).await
    }
}

// =============================================================================
// MinusSecrets — uses Vault's SubVault for prefix-isolated access
// =============================================================================

#[minus_api::async_trait]
impl MinusSecrets for Runtime {
    async fn get_store(&self, component_id: &str) -> Result<Arc<dyn MinusSecretStore>> {
        let vault = self.vault.clone()
            .ok_or_else(|| anyhow::anyhow!("Vault not initialized"))?;
        let prefix = if component_id.is_empty() {
            "".to_string()
        } else {
            format!("{}:", component_id)
        };
        Ok(Arc::new(SubVault::new(vault, prefix)))
    }

    async fn list_secret_declarations(&self) -> Result<Vec<SecretDeclaration>> {
        let env_secrets = self.secrets.read().await.list_redacted();
        let db_records = self.db.list_secret_declarations().await.unwrap_or_default();
        let mut declarations = Vec::new();

        for (key, _) in env_secrets {
            let db_record = db_records.iter().find(|r| r.key == key);
            let (comp_id, desc, mut approved) = if key.starts_with("PROVIDER_") {
                let parts: Vec<&str> = key.splitn(3, '_').collect();
                let comp = if parts.len() >= 2 {
                    format!("provider:{}", parts[1].to_lowercase())
                } else {
                    "provider".to_string()
                };
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
                created_at: db_record.map(|r| {
                    chrono::DateTime::parse_from_rfc3339(&r.created_at)
                        .unwrap_or_default().with_timezone(&chrono::Utc)
                }).unwrap_or_else(chrono::Utc::now),
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

    async fn resolve_secret(&self, key: &str) -> Result<Option<String>> {
        self.resolve_secret_value(key).await
    }
}

// =============================================================================
// MinusProviders — delegate to ProviderRegistry, with unified secret resolution
// =============================================================================

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
        let secret_key = self.resolve_secret_value(&key_name).await?;
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
        // Also persist to provider-specific config
        if let Some(p) = self.providers.read().await.default_provider() {
            if let Some(cp) = p.config() {
                cp.set_config("text_model", model).await?;
            }
        }
        Ok(())
    }
    async fn resolve_api_key(&self, provider_id: &str) -> Result<Option<String>> {
        let key_name = format!("PROVIDER_{}_API_KEY", provider_id.to_uppercase());
        self.resolve_secret_value(&key_name).await
    }
    async fn get_provider(&self, id: &str) -> Result<Option<Arc<dyn Provider>>> {
        Ok(self.providers.read().await.get(id))
    }
}

// =============================================================================
// MinusConfigRegistry
// =============================================================================

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

// =============================================================================
// MinusTools — delegate to ToolRegistry
// =============================================================================

#[minus_api::async_trait]
impl MinusTools for Runtime {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().await.definitions()
    }
}
