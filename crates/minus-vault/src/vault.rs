use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub struct Vault {
    vault_path: PathBuf,
    hwid: String,
    master_key: [u8; 32],
    // Store entries in memory for fast access
    entries: RwLock<HashMap<String, String>>,
}

impl Vault {
    /// Create a new Vault instance using the machine's HWID.
    pub fn open(vault_dir: &Path) -> Result<Self> {
        // 1. Get HWID
        let hwid =
            machine_uid::get().map_err(|e| anyhow::anyhow!("Failed to get machine HWID: {}", e))?;

        // 2. Derive 32-byte key from HWID
        let mut hasher = Sha256::new();
        hasher.update(hwid.as_bytes());
        let result = hasher.finalize();
        let mut master_key = [0u8; 32];
        master_key.copy_from_slice(&result);

        std::fs::create_dir_all(vault_dir)?;
        let vault_path = vault_dir.join("vault.env");

        let mut entries = HashMap::new();
        let mut dirty = false;

        if vault_path.exists() {
            let content = std::fs::read_to_string(&vault_path).with_context(|| {
                format!("Failed to read vault file at {}", vault_path.display())
            })?;

            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, val)) = line.split_once('=') {
                    let key = key.trim().to_string();
                    let val = val.trim().to_string();

                    if !val.starts_with("enc:") && !val.is_empty() {
                        // Unencrypted value found, encrypt it!
                        tracing::info!(key = key, "Auto-encrypting vault key");
                        let encrypted = Self::encrypt_value(&master_key, val.as_bytes())?;
                        entries.insert(key, format!("enc:{}", encrypted));
                        dirty = true;
                    } else {
                        entries.insert(key, val);
                    }
                }
            }
        }

        let vault = Self {
            vault_path,
            hwid,
            master_key,
            entries: RwLock::new(entries),
        };

        if dirty {
            vault.persist()?;
        }

        Ok(vault)
    }

    /// Store an encrypted secret.
    pub fn put_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        let encrypted = Self::encrypt_value(&self.master_key, value)?;
        {
            let mut entries = self
                .entries
                .write()
                .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            entries.insert(key.to_string(), format!("enc:{}", encrypted));
        }
        self.persist()?;

        tracing::info!(key = key, "Vault secret stored");
        Ok(())
    }

    /// Retrieve a decrypted secret.
    pub fn get_secret(&self, key: &str) -> Result<Vec<u8>> {
        let val = {
            let entries = self
                .entries
                .read()
                .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            entries.get(key).cloned()
        }
        .ok_or_else(|| anyhow::anyhow!("Secret '{}' not found in vault", key))?;

        if let Some(encrypted_data) = val.strip_prefix("enc:") {
            Self::decrypt_value(&self.master_key, encrypted_data)
        } else {
            // Treat as plaintext if not prefixed with enc: (should be rare due to auto-encrypt)
            Ok(val.as_bytes().to_vec())
        }
    }

    /// Delete a secret from the vault.
    pub fn delete_secret(&self, key: &str) -> Result<bool> {
        let removed = {
            let mut entries = self
                .entries
                .write()
                .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            entries.remove(key).is_some()
        };

        if removed {
            self.persist()?;
            tracing::info!(key = key, "Vault secret deleted");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all secret keys in the vault.
    pub fn list_keys(&self) -> Vec<String> {
        let entries = self.entries.read().unwrap();
        let mut keys: Vec<_> = entries.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Check if a secret exists.
    pub fn has_secret(&self, key: &str) -> bool {
        let entries = self.entries.read().unwrap();
        entries.contains_key(key)
    }

    /// Get the machine's HWID used by this vault.
    pub fn hwid(&self) -> &str {
        &self.hwid
    }

    fn encrypt_value(master_key: &[u8; 32], value: &[u8]) -> Result<String> {
        let cipher = Aes256Gcm::new(master_key.into());

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

        Ok(BASE64.encode(&data))
    }

    fn decrypt_value(master_key: &[u8; 32], encrypted_b64: &str) -> Result<Vec<u8>> {
        let data = BASE64
            .decode(encrypted_b64.trim())
            .context("Failed to decode secret data")?;

        if data.len() < 12 {
            bail!("Invalid secret data");
        }

        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        let cipher = Aes256Gcm::new(master_key.into());
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext)
    }

    fn persist(&self) -> Result<()> {
        let mut content = String::from("# minusbot vault — MACHINE ENCRYPTED\n");
        let entries = self
            .entries
            .read()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        let mut keys: Vec<_> = entries.keys().collect();
        keys.sort();
        for key in keys {
            let val = &entries[key];
            content.push_str(&format!("{}={}\n", key, val));
        }
        std::fs::write(&self.vault_path, content)
            .with_context(|| format!("Failed to write vault to {}", self.vault_path.display()))?;
        Ok(())
    }
}

#[minus_api::async_trait]
impl minus_api::MinusSecretStore for Vault {
    async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.get_secret(key) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    async fn put_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        self.put_secret(key, value)
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        self.delete_secret(key)?;
        Ok(())
    }

    async fn list_secrets(&self) -> Result<Vec<String>> {
        Ok(self.list_keys())
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        Ok(self.has_secret(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_vault_encrypt_decrypt() {
        let dir = std::env::temp_dir().join("minusbot_vault_hwid_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let vault = Vault::open(&dir).unwrap();

        vault.put_secret("TEST_KEY", b"hello world").unwrap();
        let result = vault.get_secret("TEST_KEY").unwrap();
        assert_eq!(result, b"hello world");

        assert!(vault.has_secret("TEST_KEY"));

        let keys = vault.list_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "TEST_KEY");

        // Test persistence and reload
        let vault2 = Vault::open(&dir).unwrap();
        let result2 = vault2.get_secret("TEST_KEY").unwrap();
        assert_eq!(result2, b"hello world");

        vault.delete_secret("TEST_KEY").unwrap();
        assert!(!vault.has_secret("TEST_KEY"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_vault_auto_encrypt() {
        let dir = std::env::temp_dir().join("minusbot_vault_auto_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let vault_path = dir.join("vault.env");
        std::fs::write(&vault_path, "RAW_KEY=secret_value\n").unwrap();

        // Load vault, should auto-encrypt RAW_KEY
        let vault = Vault::open(&dir).unwrap();
        assert!(vault.has_secret("RAW_KEY"));
        let result = vault.get_secret("RAW_KEY").unwrap();
        assert_eq!(result, b"secret_value");

        // Verify file content is now encrypted
        let content = std::fs::read_to_string(&vault_path).unwrap();
        assert!(content.contains("RAW_KEY=enc:"));
        assert!(!content.contains("RAW_KEY=secret_value"));

        let _ = fs::remove_dir_all(&dir);
    }
}
