use serde::{Deserialize, Serialize};

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