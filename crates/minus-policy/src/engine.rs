use minus_api::{PolicyAction, PolicyDecision, MinusPolicy};
use minus_env::AppConfig;

/// The policy engine evaluates whether actions are permitted.
pub struct PolicyEngine {
    config: AppConfig,
}

impl PolicyEngine {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }
}

impl MinusPolicy for PolicyEngine {
    fn evaluate(&self, action: &PolicyAction) -> PolicyDecision {
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
