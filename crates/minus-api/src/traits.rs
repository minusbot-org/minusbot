use crate::types::*;
use crate::events::*;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait Channel: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    async fn start(&self, ctx: ChannelContext) -> Result<()>;
    async fn send(&self, msg: OutgoingMessage) -> Result<()>;

    async fn send_message(&self, chat_id: &ChatId, msg: &str) -> Result<()> {
        self.send(OutgoingMessage::new(chat_id.clone(), msg)).await
    }

    async fn send_warning(&self, chat_id: &ChatId, msg: &str) -> Result<()> {
        let mut out = OutgoingMessage::new(chat_id.clone(), format!("WARNING: {}", msg));
        if let Some(ref mut meta) = out.metadata {
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("kind".to_string(), serde_json::Value::String("warning".to_string()));
            }
        } else {
            out.metadata = Some(serde_json::json!({"kind": "warning"}));
        }
        self.send(out).await
    }

    async fn send_error(&self, chat_id: &ChatId, msg: &str) -> Result<()> {
        let mut out = OutgoingMessage::new(chat_id.clone(), format!("ERROR: {}", msg));
        if let Some(ref mut meta) = out.metadata {
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("kind".to_string(), serde_json::Value::String("error".to_string()));
            }
        } else {
            out.metadata = Some(serde_json::json!({"kind": "error"}));
        }
        self.send(out).await
    }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        None
    }
}

/// Base trait all providers implement.
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    async fn is_ready(&self) -> Result<()>;

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        None
    }

    /// Downcast helper: returns Some(&dyn TextProvider) if this provider supports text generation.
    fn as_text_provider(&self) -> Option<&dyn TextProvider> {
        None
    }
}

/// Trait for providers that support text/LLM generation.
#[async_trait]
pub trait TextProvider: Provider {
    async fn complete_text(&self, request: ProviderRequest) -> Result<ProviderResponse>;
    async fn get_text_models(&self, secret_key: Option<String>) -> Result<Vec<String>>;
}


#[async_trait]
pub trait Integration: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn required_secrets(&self) -> Vec<SecretDeclaration>;
    fn tools(&self) -> Vec<ToolDefinition>;
    async fn call_tool(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult>;
    
    // Commands provided by integration
    fn commands(&self) -> Vec<CommandDefinition> { Vec::new() }
    async fn execute_command(&self, name: String, _args: Vec<String>, _ctx: CommandContext) -> Result<String> {
        anyhow::bail!("Command {} not implemented for integration {}", name, self.id())
    }
}

#[async_trait]
pub trait Addon: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn manifest(&self) -> AddonManifest;
    async fn register(&self, registry: &mut AddonRegistry) -> Result<()>;
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn call(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult>;
}

#[async_trait]
pub trait Command: Send + Sync {
    fn definition(&self) -> CommandDefinition;
    async fn execute(&self, args: Vec<String>, ctx: CommandContext) -> Result<String>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub usage: String,
    pub category: String,
    pub min_args: usize,
}

pub struct CommandContext {
    pub chat_id: ChatId,
    pub channel_id: ChannelId,
    pub db: Arc<dyn MinusDatabase>,
    pub providers: Arc<dyn MinusProviders>,
    pub config: Arc<dyn MinusConfig>,
    pub config_registry: Arc<dyn MinusConfigRegistry>,
    pub secrets: Arc<dyn MinusSecrets>,
    pub channel: Arc<dyn Channel>,
    pub all_commands: Vec<CommandDefinition>,
    pub shutdown_trigger: Option<tokio::sync::broadcast::Sender<()>>,
}

/// Thin registry passed during addon registration
pub struct AddonRegistry {
    pub tools: Vec<Box<dyn Tool>>,
    pub integrations: Vec<Box<dyn Integration>>,
    pub commands: Vec<Box<dyn Command>>,
}

impl AddonRegistry {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            integrations: Vec::new(),
            commands: Vec::new(),
        }
    }
}

impl Default for AddonRegistry {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
pub trait MinusDatabase: Send + Sync {
    // Chat management
    async fn list_chats(&self) -> Result<Vec<Chat>>;
    async fn get_chat(&self, id: &str) -> Result<Option<Chat>>;
    async fn ensure_chat(&self, id: &str, channel_type: &str, channel_id: &str, title: Option<&str>) -> Result<()>;
    async fn rename_chat(&self, id: &str, title: &str) -> Result<()>;
    async fn delete_chat(&self, id: &str) -> Result<()>;

    // Message management
    async fn get_messages(&self, chat_id: &str, limit: i64) -> Result<Vec<Message>>;
    async fn delete_messages(&self, chat_id: &str) -> Result<()>;

    // Audit management
    async fn log_audit(&self, id: &str, actor: &str, action: &str, target: Option<&str>, metadata: Option<&str>, created_at: &str) -> Result<()>;
    async fn tail_audit(&self, limit: i64) -> Result<Vec<AuditEvent>>;
    
}

#[async_trait]
pub trait MinusProviders: Send + Sync {
    async fn list_providers(&self) -> Result<Vec<(String, String)>>;
    async fn set_default_provider(&self, id: &str) -> Result<()>;
    async fn list_text_models(&self) -> Result<Vec<String>>;
    async fn get_default_provider_id(&self) -> Result<String>;
    async fn get_default_text_model(&self) -> Result<String>;
    async fn set_default_text_model(&self, model: &str) -> Result<()>;
}

#[async_trait]
pub trait MinusConfig: Send + Sync {
    async fn get_config(&self) -> Result<serde_json::Value>;
    async fn update_config(&self, patch: serde_json::Value) -> Result<()>;
}

#[async_trait]
pub trait MinusSecretStore: Send + Sync {
    async fn put_secret(&self, key: &str, value: &[u8]) -> Result<()>;
    async fn get_secret(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete_secret(&self, key: &str) -> Result<bool>;
    async fn has_secret(&self, key: &str) -> Result<bool>;
}

#[async_trait]
pub trait MinusSecrets: Send + Sync {
    async fn list_secret_declarations(&self) -> Result<Vec<SecretDeclarationStatus>>;
    async fn approve_secret(&self, component_id: &str, key: &str) -> Result<bool>;
    async fn deny_secret(&self, component_id: &str, key: &str) -> Result<bool>;
    async fn get_store(&self, prefix: &str) -> Result<std::sync::Arc<dyn MinusSecretStore>>;
}

#[async_trait]
pub trait ConfigProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn list_keys(&self) -> Vec<String>;
    async fn read_config(&self, key: &str) -> Result<Option<String>>;
    async fn set_config(&self, key: &str, value: &str) -> Result<()>;
}

#[async_trait]
pub trait MinusConfigRegistry: Send + Sync {
    async fn get_provider(&self, id: &str) -> Option<Arc<dyn ConfigProvider>>;
    async fn list_providers(&self) -> Vec<Arc<dyn ConfigProvider>>;
    async fn register(&self, provider: Arc<dyn ConfigProvider>);
}

/// A reusable implementation of ConfigProvider that stores data in a TOML file.
pub struct FileConfigProvider {
    pub id: String,
    pub path: std::path::PathBuf,
}

impl FileConfigProvider {
    pub fn new(id: impl Into<String>, path: std::path::PathBuf) -> Self {
        Self {
            id: id.into(),
            path,
        }
    }
}

#[async_trait]
impl ConfigProvider for FileConfigProvider {
    fn id(&self) -> &'static str {
        // We can't return a 'static str from a dynamic String easily,
        // but for these built-in ones we can leak it or use a box.
        // For simplicity, we'll assume the IDs are short-lived or static-ish.
        Box::leak(self.id.clone().into_boxed_str())
    }

    fn list_keys(&self) -> Vec<String> {
        if !self.path.exists() {
            return vec![];
        }
        let content = std::fs::read_to_string(&self.path).unwrap_or_default();
        let val: toml::Value = toml::from_str(&content).unwrap_or(toml::Value::Table(Default::default()));
        if let Some(table) = val.as_table() {
            table.keys().cloned().collect()
        } else {
            vec![]
        }
    }

    async fn read_config(&self, key: &str) -> Result<Option<String>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&self.path)?;
        let val: toml::Value = toml::from_str(&content)?;
        if let Some(v) = val.get(key) {
            match v {
                toml::Value::String(s) => Ok(Some(s.clone())),
                _ => Ok(Some(v.to_string())),
            }
        } else {
            Ok(None)
        }
    }

    async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let mut val: toml::Value = if self.path.exists() {
            let content = std::fs::read_to_string(&self.path)?;
            toml::from_str(&content).unwrap_or(toml::Value::Table(Default::default()))
        } else {
            toml::Value::Table(Default::default())
        };

        if let Some(table) = val.as_table_mut() {
            table.insert(key.to_string(), toml::Value::String(value.to_string()));
            let new_content = toml::to_string_pretty(&val)?;
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&self.path, new_content)?;
        }
        Ok(())
    }
}

