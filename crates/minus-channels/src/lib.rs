use minus_core::Channel;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of available channels.
pub struct ChannelRegistry {
    channels: HashMap<String, Arc<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    pub fn register(&mut self, channel: Arc<dyn Channel>) {
        let id = channel.id().to_string();
        tracing::info!(channel_id = %id, "Registered channel");
        self.channels.insert(id, channel);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Channel>> {
        self.channels.get(id).cloned()
    }

    pub fn list(&self) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .channels
            .values()
            .map(|c| (c.id().to_string(), c.name().to_string()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
