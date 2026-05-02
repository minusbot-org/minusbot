use serde::{Deserialize, Serialize};
use crate::ComponentId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Permission {
    NetworkAccess { host: String },
    FilesystemRead { path: String },
    FilesystemWrite { path: String },
    ShellExec,
    VaultRead { key: String },
    VaultWrite { key: String },
    EnvRead { key: String },
    EnvWrite { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
    Ask { reason: String },
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PolicyDecision::Allow)
    }
}

/// Describes what action is being requested for policy evaluation.
#[derive(Debug, Clone)]
pub enum PolicyAction {
    ToolCall { tool_name: String },
    EnvSet { key: String },
    EnvGet { key: String },
    VaultGet { key: String, requester: ComponentId },
    VaultPut { key: String },
    JobCreate,
    JobDelete,
    MessageSend,
    NetworkHttp { host: String },
    ShellExec,
    FilesystemRead { path: String },
    FilesystemWrite { path: String },
    ProviderCall { provider_id: String },
    IntegrationCall { integration_id: String },
    ChannelSend { channel_id: String },
}