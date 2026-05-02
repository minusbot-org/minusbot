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

    pub fn list_secrets(&self) -> Result<Vec<String>> {
        let all = self.vault.list_keys();
        let filtered = all.into_iter()
            .filter(|k| k.starts_with(&self.prefix))
            .map(|k| {
                // Strip the prefix so the caller just sees their isolated keys
                k.strip_prefix(&self.prefix).unwrap_or(&k).to_string()
            })
            .collect();
        Ok(filtered)
    }

    pub fn has_secret(&self, key: &str) -> bool {
        self.vault.has_secret(&self.prefixed_key(key))
    }
}

#[minus_api::async_trait]
impl minus_api::MinusSecretStore for SubVault {
    async fn get_secret(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match self.vault.get_secret(&self.prefixed_key(key)) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    async fn put_secret(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        self.vault.put_secret(&self.prefixed_key(key), value)
    }

    async fn delete_secret(&self, key: &str) -> anyhow::Result<()> {
        self.vault.delete_secret(&self.prefixed_key(key))?;
        Ok(())
    }

    async fn list_secrets(&self) -> anyhow::Result<Vec<String>> {
        self.list_secrets()
    }

    async fn has_secret(&self, key: &str) -> anyhow::Result<bool> {
        Ok(self.has_secret(key))
    }
}
