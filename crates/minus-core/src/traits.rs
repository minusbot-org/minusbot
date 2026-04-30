use crate::types::*;
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
    pub store: Arc<dyn Any + Send + Sync>,
    pub all_commands: Vec<CommandDefinition>,
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