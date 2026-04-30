use minus_core::AddonManifest;
use std::collections::HashMap;

/// Registry for addon manifests. In MVP, addons are registered statically.
/// Future: support WASM, Lua, and dynamic Rust plugins.
pub struct AddonManager {
    manifests: HashMap<String, AddonManifest>,
}

impl AddonManager {
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
        }
    }

    pub fn register(&mut self, manifest: AddonManifest) {
        tracing::info!(addon_id = %manifest.id, "Registered addon");
        self.manifests.insert(manifest.id.clone(), manifest);
    }

    pub fn list(&self) -> Vec<&AddonManifest> {
        let mut addons: Vec<_> = self.manifests.values().collect();
        addons.sort_by(|a, b| a.id.cmp(&b.id));
        addons
    }

    pub fn get(&self, id: &str) -> Option<&AddonManifest> {
        self.manifests.get(id)
    }
}

impl Default for AddonManager {
    fn default() -> Self {
        Self::new()
    }
}
