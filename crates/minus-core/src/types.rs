use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;
use crate::permissions::Permission;

// --- ID newtypes ---

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntegrationId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentId(pub String);

impl ComponentId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for ChatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// --- Messages ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: MessageId,
    pub chat_id: ChatId,
    pub role: Role,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl ChatMessage {
    pub fn new(chat_id: ChatId, role: Role, content: impl Into<String>) -> Self {
        Self {
            id: MessageId(Uuid::new_v4().to_string()),
            chat_id,
            role,
            content: content.into(),
            metadata: None,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub chat_id: ChatId,
    pub channel_id: ChannelId,
    pub user_id: Option<UserId>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl IncomingMessage {
    pub fn new(chat_id: ChatId, channel_id: ChannelId, content: impl Into<String>) -> Self {
        Self {
            chat_id,
            channel_id,
            user_id: None,
            content: content.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub chat_id: ChatId,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
}

impl OutgoingMessage {
    pub fn new(chat_id: ChatId, content: impl Into<String>) -> Self {
        Self {
            chat_id,
            content: content.into(),
            metadata: None,
        }
    }
}

// --- Tools ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub risk: ToolRisk,
    pub side_effect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolRisk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
}

// --- Provider ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub model: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub secret_key: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMessage {
    pub role: Role,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<ProviderUsage>,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub llm: bool,
    pub tools: bool,
    pub vision: bool,
    pub embeddings: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            llm: true,
            tools: true,
            vision: false,
            embeddings: false,
        }
    }
}

// --- Context ---

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub chat_id: ChatId,
    pub channel_id: ChannelId,
    pub history: Vec<ChatMessage>,
    pub skills_content: Vec<String>,
    pub system_prompt: String,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub chat_id: ChatId,
    pub channel_id: ChannelId,
    pub component_id: ComponentId,
    pub store: Option<Arc<dyn Any + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub struct ChannelContext {
    pub channel_id: ChannelId,
    pub config_dir: std::path::PathBuf,
}

// --- Secret declarations ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretDeclaration {
    pub id: String,
    pub component_id: ComponentId,
    pub key: String,
    pub description: String,
    pub required: bool,
    pub permissions: Vec<Permission>,
    pub approved: bool,
    pub created_at: DateTime<Utc>,
}

impl SecretDeclaration {
    pub fn new(
        component_id: ComponentId,
        key: impl Into<String>,
        description: impl Into<String>,
        required: bool,
        permissions: Vec<Permission>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            component_id,
            key: key.into(),
            description: description.into(),
            required,
            permissions,
            approved: false,
            created_at: Utc::now(),
        }
    }
}

// --- Addon manifest ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub description: Option<String>,
}