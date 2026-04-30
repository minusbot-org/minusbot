use crate::Vault;
use anyhow::Result;
use std::sync::Arc;

/// A SubVault provides isolated access to a subset of secrets in the main Vault,
/// utilizing a specific prefix (e.g., "PROVIDER_OPENROUTER_").
#[derive(Clone)]
pub struct SubVault {
    vault: Arc<Vault>,
    prefix: String,
}

impl SubVault {
    pub fn new(vault: Arc<Vault>, prefix: impl Into<String>) -> Self {
        Self {
            vault,
            prefix: prefix.into(),
        }
    }

    fn prefixed_key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }

    pub fn put_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        self.vault.put_secret(&self.prefixed_key(key), value)
    }

    pub fn get_secret(&self, key: &str) -> Result<Vec<u8>> {
        self.vault.get_secret(&self.prefixed_key(key))
    }

    pub fn delete_secret(&self, key: &str) -> Result<bool> {
        self.vault.delete_secret(&self.prefixed_key(key))
    }

    pub fn list_secrets(&self) -> Result<Vec<crate::vault::VaultSecretInfo>> {
        let all = self.vault.list_secrets()?;
        let filtered = all.into_iter()
            .filter(|s| s.key.starts_with(&self.prefix))
            .map(|mut s| {
                // Strip the prefix so the caller just sees their isolated keys
                s.key = s.key.strip_prefix(&self.prefix).unwrap_or(&s.key).to_string();
                s
            })
            .collect();
        Ok(filtered)
    }

    pub fn has_secret(&self, key: &str) -> bool {
        self.vault.has_secret(&self.prefixed_key(key))
    }
}
