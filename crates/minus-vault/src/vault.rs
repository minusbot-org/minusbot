use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Metadata about the vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMeta {
    pub version: u32,
    pub created_at: String,
    #[serde(default)]
    pub secrets: HashMap<String, VaultSecretInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSecretInfo {
    pub key: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct Vault {
    vault_dir: PathBuf,
    master_key: [u8; 32],
}

impl Vault {
    /// Create a new Vault instance.
    /// `master_key_b64` is the base64-encoded 32-byte master key.
    pub fn open(vault_dir: &Path, master_key_b64: &str) -> Result<Self> {
        let key_bytes = BASE64
            .decode(master_key_b64)
            .context("Invalid vault master key (not valid base64)")?;
        if key_bytes.len() != 32 {
            bail!("Vault master key must be exactly 32 bytes (got {})", key_bytes.len());
        }
        let mut master_key = [0u8; 32];
        master_key.copy_from_slice(&key_bytes);

        std::fs::create_dir_all(vault_dir.join("secrets"))?;

        // Ensure vault.meta.json exists
        let meta_path = vault_dir.join("vault.meta.json");
        if !meta_path.exists() {
            let meta = VaultMeta {
                version: 1,
                created_at: chrono::Utc::now().to_rfc3339(),
                secrets: HashMap::new(),
            };
            let content = serde_json::to_string_pretty(&meta)?;
            std::fs::write(&meta_path, content)?;
        }

        Ok(Self {
            vault_dir: vault_dir.to_path_buf(),
            master_key,
        })
    }

    /// Generate a new random master key and return it as base64.
    pub fn generate_master_key() -> String {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        BASE64.encode(key)
    }

    /// Store an encrypted secret.
    pub fn put_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        let cipher = Aes256Gcm::new((&self.master_key).into());

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, value)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Store as: nonce (12 bytes) || ciphertext
        let mut data = Vec::with_capacity(12 + ciphertext.len());
        data.extend_from_slice(&nonce_bytes);
        data.extend_from_slice(&ciphertext);

        let encoded = BASE64.encode(&data);
        let secret_path = self.secret_file_path(key);
        std::fs::write(&secret_path, encoded)
            .with_context(|| format!("Failed to write secret file for {}", key))?;

        // Update meta
        self.update_meta(key)?;

        tracing::info!(key = key, "Vault secret stored");
        Ok(())
    }

    /// Retrieve a decrypted secret.
    pub fn get_secret(&self, key: &str) -> Result<Vec<u8>> {
        let secret_path = self.secret_file_path(key);
        if !secret_path.exists() {
            bail!("Secret '{}' not found in vault", key);
        }

        let encoded = std::fs::read_to_string(&secret_path)
            .with_context(|| format!("Failed to read secret file for {}", key))?;
        let data = BASE64
            .decode(encoded.trim())
            .context("Failed to decode secret data")?;

        if data.len() < 12 {
            bail!("Invalid secret data for '{}'", key);
        }

        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        let cipher = Aes256Gcm::new((&self.master_key).into());
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed for '{}': {}", key, e))?;

        Ok(plaintext)
    }

    /// Delete a secret from the vault.
    pub fn delete_secret(&self, key: &str) -> Result<bool> {
        let secret_path = self.secret_file_path(key);
        if secret_path.exists() {
            std::fs::remove_file(&secret_path)?;
            // Remove from meta
            let meta_path = self.vault_dir.join("vault.meta.json");
            if meta_path.exists() {
                let content = std::fs::read_to_string(&meta_path)?;
                let mut meta: VaultMeta = serde_json::from_str(&content)?;
                meta.secrets.remove(key);
                let content = serde_json::to_string_pretty(&meta)?;
                std::fs::write(&meta_path, content)?;
            }
            tracing::info!(key = key, "Vault secret deleted");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all secret keys in the vault.
    pub fn list_secrets(&self) -> Result<Vec<VaultSecretInfo>> {
        let meta_path = self.vault_dir.join("vault.meta.json");
        if !meta_path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&meta_path)?;
        let meta: VaultMeta = serde_json::from_str(&content)?;
        let mut entries: Vec<_> = meta.secrets.into_values().collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(entries)
    }

    /// Check if a secret exists.
    pub fn has_secret(&self, key: &str) -> bool {
        self.secret_file_path(key).exists()
    }

    fn secret_file_path(&self, key: &str) -> PathBuf {
        self.vault_dir.join("secrets").join(format!("{}.enc", key))
    }

    fn update_meta(&self, key: &str) -> Result<()> {
        let meta_path = self.vault_dir.join("vault.meta.json");
        let mut meta = if meta_path.exists() {
            let content = std::fs::read_to_string(&meta_path)?;
            serde_json::from_str(&content)?
        } else {
            VaultMeta {
                version: 1,
                created_at: chrono::Utc::now().to_rfc3339(),
                secrets: HashMap::new(),
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        let info = meta.secrets.entry(key.to_string()).or_insert(VaultSecretInfo {
            key: key.to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
        info.updated_at = chrono::Utc::now().to_rfc3339();

        let content = serde_json::to_string_pretty(&meta)?;
        std::fs::write(&meta_path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_vault_encrypt_decrypt() {
        let dir = std::env::temp_dir().join("minusbot_vault_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let master_key = Vault::generate_master_key();
        let vault = Vault::open(&dir, &master_key).unwrap();

        vault.put_secret("TEST_KEY", b"hello world").unwrap();
        let result = vault.get_secret("TEST_KEY").unwrap();
        assert_eq!(result, b"hello world");

        assert!(vault.has_secret("TEST_KEY"));
        assert!(!vault.has_secret("NONEXISTENT"));

        let secrets = vault.list_secrets().unwrap();
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].key, "TEST_KEY");

        vault.delete_secret("TEST_KEY").unwrap();
        assert!(!vault.has_secret("TEST_KEY"));

        let _ = fs::remove_dir_all(&dir);
    }
}
