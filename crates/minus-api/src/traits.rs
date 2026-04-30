use crate::types::*;
use crate::events::*;
use anyhow::Result;
use async_trait::async_trait;
use std::any::Any;
use std::sync::Arc;

#[async_trait]
pub trait Channel: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    async fn start(&self, ctx: ChannelContext) -> Result<()>;
    async fn send(&self, msg: OutgoingMessage) -> Result<()>;
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse>;
    async fn list_models(&self, secret_key: Option<String>) -> Result<Vec<String>>;
    async fn is_ready(&self) -> Result<bool>;
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
    pub secrets: Arc<dyn MinusSecrets>,
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
    async fn list_models(&self, provider_id: &str) -> Result<Vec<String>>;
    async fn get_default_provider_id(&self) -> Result<String>;
    async fn get_default_model(&self) -> Result<String>;
    async fn set_default_model(&self, model: &str) -> Result<()>;
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

