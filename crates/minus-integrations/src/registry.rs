use minus_core::Integration;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of available integrations.
pub struct IntegrationRegistry {
    integrations: HashMap<String, Arc<dyn Integration>>,
}

impl IntegrationRegistry {
    pub fn new() -> Self {
        Self {
            integrations: HashMap::new(),
        }
    }

    pub fn register(&mut self, integration: Arc<dyn Integration>) {
        let id = integration.id().to_string();
        tracing::info!(integration_id = %id, "Registered integration");
        self.integrations.insert(id, integration);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Integration>> {
        self.integrations.get(id).cloned()
    }

    pub fn list(&self) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .integrations
            .values()
            .map(|i| (i.id().to_string(), i.name().to_string()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    pub fn all(&self) -> Vec<Arc<dyn Integration>> {
        self.integrations.values().cloned().collect()
    }

    pub fn all_tools(&self) -> Vec<minus_core::ToolDefinition> {
        self.integrations
            .values()
            .flat_map(|i| i.tools())
            .collect()
    }
}

impl Default for IntegrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}
