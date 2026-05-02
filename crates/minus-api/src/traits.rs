use crate::types::*;
use crate::events::*;
use crate::permissions::*;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

// =============================================================================
// Component traits — implemented by channels, providers, integrations
// =============================================================================

#[async_trait]
pub trait Channel: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    
    // Status
    fn is_enabled(&self) -> bool;
    async fn set_enabled(&self, flag: bool) -> Result<bool>;
    async fn is_ready(&self) -> bool;
    
    // Chat context
    async fn get_active_chat(&self) -> Option<ChatId>;
    async fn set_active_chat(&self, chat_id: ChatId) -> Result<()>;

    // Lifecycle
    async fn start(&self, ctx: ChannelContext) -> Result<()>;
    async fn stop(&self) -> Result<()> { Ok(()) }

    // Triggers (Outgoing from daemon to channel)
    async fn send_message(&self, packet: MessagePacket) -> Result<()>;
    async fn send_notification(&self, packet: NotificationPacket) -> Result<()>;
    async fn send_tool_call(&self, packet: ToolCallPacket) -> Result<()>;
    async fn send_command_feedback(&self, feedback: CommandFeedback) -> Result<()>;
    
    // Platform features
    async fn register_commands(&self, _commands: Vec<CommandDefinition>) -> Result<()> { Ok(()) }

    // Events (Informing channel about state changes)
    async fn on_chat_switch(&self, _chat_id: &ChatId, _messages: Vec<Message>) -> Result<()> { Ok(()) }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        None
    }
}

/// Base trait all providers implement.
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;

    fn as_text_provider(&self) -> Option<&dyn TextProvider> {
        None
    }

    async fn is_ready(&self) -> bool { true }

    fn config(&self) -> Option<Arc<dyn ConfigProvider>> {
        None
    }
}

#[async_trait]
pub trait TextProvider: Provider {
    async fn generate_text(&self, request: ProviderRequest) -> Result<ProviderResponse>;
    async fn get_text_models(&self, secret_key: Option<String>) -> Result<Vec<String>>;
}

#[async_trait]
pub trait Integration: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn required_secrets(&self) -> Vec<SecretDeclaration>;
    fn tools(&self) -> Vec<ToolDefinition>;
    async fn call_tool(&self, call: ToolCall, ctx: ToolContext) -> Result<ToolResult>;
    
    fn commands(&self) -> Vec<CommandDefinition> { Vec::new() }
    async fn execute_command(&self, name: String, _args: Vec<String>, _ctx: CommandContext) -> Result<String> {
        anyhow::bail!("Command {} not implemented for integration {}", name, self.id())
    }
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

#[async_trait]
pub trait ConfigProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn list_keys(&self) -> Vec<String>;
    async fn read_config(&self, key: &str) -> Result<Option<String>>;
    async fn set_config(&self, key: &str, value: &str) -> Result<()>;
}

// =============================================================================
// Context structs — passed to commands, tools, skills
// =============================================================================

#[derive(Clone, Debug)]
pub struct ChannelContext {
    pub channel_id: ChannelId,
    pub config_dir: std::path::PathBuf,
}

pub struct ToolContext {
    pub chat_id: ChatId,
    pub channel_id: ChannelId,
    pub component_id: ComponentId,
    pub db: Arc<dyn MinusDatabase>,
    pub scheduler: Arc<dyn MinusScheduler>,
}

pub struct CommandContext {
    pub chat_id: ChatId,
    pub channel_id: ChannelId,
    pub channel: Arc<dyn Channel>,
    pub db: Arc<dyn MinusDatabase>,
    pub config_registry: Arc<dyn MinusConfigRegistry>,
    pub tools: Arc<dyn MinusTools>,
    pub secrets: Arc<dyn MinusSecrets>,
    pub providers: Arc<dyn MinusProviders>,
    pub scheduler: Arc<dyn MinusScheduler>,
    pub channels: Arc<dyn MinusChannels>,
    pub shutdown_trigger: Option<tokio::sync::mpsc::Sender<()>>,
    pub all_commands: Vec<CommandDefinition>,
}

// =============================================================================
// Service traits — facades over subsystem implementations
// =============================================================================

#[async_trait]
pub trait MinusDatabase: Send + Sync {
    // Chat management
    async fn list_chats(&self) -> Result<Vec<Chat>>;
    async fn get_chat(&self, id: &str) -> Result<Option<Chat>>;
    async fn ensure_chat(&self, id: &str, channel_id: &str, external_id: &str, title: Option<&str>) -> Result<()>;
    async fn rename_chat(&self, id: &str, title: &str) -> Result<()>;
    async fn delete_chat(&self, id: &str) -> Result<()>;

    // Message management
    async fn get_messages(&self, chat_id: &str, limit: i64) -> Result<Vec<Message>>;
    async fn save_message(&self, id: &str, chat_id: &str, role: &str, content: &str, metadata: Option<&str>) -> Result<()>;
    async fn delete_messages(&self, chat_id: &str) -> Result<()>;

    // Audit management
    async fn log_audit(&self, id: &str, actor: &str, action: &str, target: Option<&str>, metadata: Option<&str>, created_at: &str) -> Result<()>;
    async fn tail_audit(&self, limit: i64) -> Result<Vec<AuditEvent>>;

    // Memory management
    async fn list_memories(&self) -> Result<Vec<Memory>>;
    async fn get_important_memories(&self) -> Result<Vec<Memory>>;
    async fn save_memory(&self, id: &str, kind: &str, brief: &str, content: Option<&str>, is_important: bool) -> Result<()>;
    async fn delete_memory(&self, id: &str) -> Result<bool>;
}

#[async_trait]
pub trait MinusChannels: Send + Sync {
    async fn list_channels(&self) -> Vec<ChannelStatus>;
    async fn get_channel(&self, id: &str) -> Option<Arc<dyn Channel>>;
    async fn set_channel_enabled(&self, id: &str, enabled: bool) -> Result<bool>;
}

#[async_trait]
pub trait MinusScheduler: Send + Sync {
    async fn list_tasks(&self) -> Result<Vec<SchedulerTask>>;
    async fn delete_task(&self, id: &str) -> Result<bool>;
    async fn create_task(&self, name: &str, schedule: &str, prompt: &str, target_chat_id: Option<&str>) -> Result<String>;
}

#[async_trait]
pub trait MinusConfigRegistry: Send + Sync {
    async fn get_provider(&self, provider_id: &str) -> Option<Arc<dyn ConfigProvider>>;
    async fn list_providers(&self) -> Vec<Arc<dyn ConfigProvider>>;
    async fn register(&self, provider: Arc<dyn ConfigProvider>);
}

#[async_trait]
pub trait MinusSecretStore: Send + Sync {
    async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put_secret(&self, key: &str, value: &[u8]) -> Result<()>;
    async fn delete_secret(&self, key: &str) -> Result<()>;
    async fn list_secrets(&self) -> Result<Vec<String>>;
    async fn has_secret(&self, key: &str) -> Result<bool>;
}

#[async_trait]
pub trait MinusSecrets: Send + Sync {
    async fn get_store(&self, component_id: &str) -> Result<Arc<dyn MinusSecretStore>>;
    async fn list_secret_declarations(&self) -> Result<Vec<SecretDeclaration>>;
    async fn approve_secret(&self, component_id: &str, key: &str) -> Result<()>;
    async fn deny_secret(&self, component_id: &str, key: &str) -> Result<()>;
    /// Resolve a secret value by key. Checks vault first, then env secrets.
    async fn resolve_secret(&self, key: &str) -> Result<Option<String>>;
}

#[async_trait]
pub trait MinusTools: Send + Sync {
    async fn list_tools(&self) -> Vec<ToolDefinition>;
}

#[async_trait]
pub trait MinusProviders: Send + Sync {
    async fn list_providers(&self) -> Result<Vec<(String, String)>>;
    async fn set_default_provider(&self, id: &str) -> Result<()>;
    async fn list_text_models(&self) -> Result<Vec<String>>;
    async fn get_default_provider_id(&self) -> Result<String>;
    async fn get_default_text_model(&self) -> Result<String>;
    async fn set_default_text_model(&self, model: &str) -> Result<()>;
    /// Resolve the API key for a provider, checking vault then env.
    async fn resolve_api_key(&self, provider_id: &str) -> Result<Option<String>>;
}

/// Policy evaluation trait.
pub trait MinusPolicy: Send + Sync {
    fn evaluate(&self, action: &PolicyAction) -> PolicyDecision;
}

// =============================================================================
// Utility implementations — FileConfigProvider
// =============================================================================

pub struct FileConfigProvider {
    id: &'static str,
    path: std::path::PathBuf,
}

impl FileConfigProvider {
    pub fn new(id: &'static str, path: std::path::PathBuf) -> Self {
        Self { id, path }
    }
}

#[async_trait]
impl ConfigProvider for FileConfigProvider {
    fn id(&self) -> &'static str {
        self.id
    }
    fn list_keys(&self) -> Vec<String> {
        if let Ok(content) = std::fs::read_to_string(&self.path) {
            if let Ok(val) = toml::from_str::<toml::Value>(&content) {
                if let Some(table) = val.as_table() {
                    return table.keys().cloned().collect();
                }
            }
        }
        Vec::new()
    }
    async fn read_config(&self, key: &str) -> Result<Option<String>> {
        if let Ok(content) = std::fs::read_to_string(&self.path) {
            if let Ok(val) = toml::from_str::<toml::Value>(&content) {
                if let Some(v) = val.get(key) {
                    return Ok(Some(v.to_string().trim_matches('"').to_string()));
                }
            }
        }
        Ok(None)
    }
    async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let mut val = if self.path.exists() {
            let content = std::fs::read_to_string(&self.path)?;
            toml::from_str::<toml::Value>(&content)?
        } else {
            toml::Value::Table(toml::map::Map::new())
        };

        if let Some(table) = val.as_table_mut() {
            table.insert(key.to_string(), toml::Value::String(value.to_string()));
            let content = toml::to_string_pretty(&val)?;
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&self.path, content)?;
        }
        Ok(())
    }
}
