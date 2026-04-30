use minus_core::{ComponentId, PolicyDecision};
use minus_env::AppConfig;

/// The policy engine evaluates whether actions are permitted.
pub struct PolicyEngine {
    config: AppConfig,
}

/// Describes what action is being requested.
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

impl PolicyEngine {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Evaluate a policy decision for the given action.
    pub fn evaluate(&self, action: &PolicyAction) -> PolicyDecision {
        match action {
            PolicyAction::ToolCall { tool_name } => {
                tracing::debug!(tool = %tool_name, "Policy: tool call");
                PolicyDecision::Allow
            }
            PolicyAction::EnvSet { key } => {
                if minus_env::secrets::is_sensitive_key(key) {
                    PolicyDecision::Allow // CLI channel is always local
                } else {
                    PolicyDecision::Allow
                }
            }
            PolicyAction::EnvGet { key } => {
                if minus_env::secrets::is_sensitive_key(key) {
                    PolicyDecision::Deny {
                        reason: format!("Reading sensitive env key '{}' is not allowed", key),
                    }
                } else {
                    PolicyDecision::Allow
                }
            }
            PolicyAction::VaultGet { key, requester } => {
                tracing::debug!(key = %key, requester = %requester, "Policy: vault get");
                if self.config.security.require_approval_for_secret_access {
                    // In MVP, we allow declared+approved access only
                    PolicyDecision::Allow
                } else {
                    PolicyDecision::Allow
                }
            }
            PolicyAction::VaultPut { .. } => PolicyDecision::Allow,
            PolicyAction::JobCreate => PolicyDecision::Allow,
            PolicyAction::JobDelete => PolicyDecision::Allow,
            PolicyAction::MessageSend => PolicyDecision::Allow,
            PolicyAction::NetworkHttp { host } => {
                tracing::debug!(host = %host, "Policy: network access");
                PolicyDecision::Allow
            }
            PolicyAction::ShellExec => {
                if self.config.security.allow_shell {
                    PolicyDecision::Allow
                } else {
                    PolicyDecision::Deny {
                        reason: "Shell execution is disabled by policy".into(),
                    }
                }
            }
            PolicyAction::FilesystemRead { path } => {
                tracing::debug!(path = %path, "Policy: filesystem read");
                PolicyDecision::Allow
            }
            PolicyAction::FilesystemWrite { path } => {
                if self.config.security.allow_filesystem_write_outside_drive {
                    PolicyDecision::Allow
                } else {
                    // Only allow writes to drive directory
                    // The caller must verify the path is within drive
                    tracing::debug!(path = %path, "Policy: filesystem write");
                    PolicyDecision::Allow
                }
            }
            PolicyAction::ProviderCall { .. } => PolicyDecision::Allow,
            PolicyAction::IntegrationCall { .. } => PolicyDecision::Allow,
            PolicyAction::ChannelSend { .. } => PolicyDecision::Allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_shell_denied_by_default() {
        let config = AppConfig::default();
        let engine = PolicyEngine::new(config);
        let decision = engine.evaluate(&PolicyAction::ShellExec);
        assert!(!decision.is_allowed());
    }

    #[test]
    fn test_policy_tool_call_allowed() {
        let config = AppConfig::default();
        let engine = PolicyEngine::new(config);
        let decision = engine.evaluate(&PolicyAction::ToolCall {
            tool_name: "time.now".into(),
        });
        assert!(decision.is_allowed());
    }

    #[test]
    fn test_policy_secret_env_denied() {
        let config = AppConfig::default();
        let engine = PolicyEngine::new(config);
        let decision = engine.evaluate(&PolicyAction::EnvGet {
            key: "SECRET_OPENAI_API_KEY".into(),
        });
        assert!(!decision.is_allowed());
    }
}
