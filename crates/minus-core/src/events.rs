use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub details: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl AuditEvent {
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        target: Option<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            actor: actor.into(),
            action: action.into(),
            target,
            details,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    MessageReceived {
        chat_id: crate::ChatId,
        content: String,
    },
    MessageSent {
        chat_id: crate::ChatId,
        content: String,
    },
    ToolCalled {
        name: String,
        args: serde_json::Value,
    },
    JobTriggered {
        job_id: crate::JobId,
        name: String,
    },
    ProviderError {
        error: String,
    },
}