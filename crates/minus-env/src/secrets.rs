use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Sensitive keywords — if a key contains any of these, the value is treated as secret.
const SENSITIVE_KEYWORDS: &[&str] = &["KEY", "TOKEN", "SECRET", "PASSWORD", "PRIVATE", "CREDENTIAL"];

/// Manages the secrets.env file containing private configuration.
#[derive(Debug, Clone)]
pub struct SecretsManager {
    values: HashMap<String, String>,
    path: std::path::PathBuf,
}

impl SecretsManager {
    /// Load from a secrets.env file. Creates the file if it doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, "# minusbot secrets — DO NOT SHARE\n")?;
        }

        let mut values = HashMap::new();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read secrets from {}", path.display()))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim().to_string();
                let val = val.trim().to_string();
                if !val.is_empty() {
                    values.insert(key, val);
                }
            }
        }

        Ok(Self {
            values,
            path: path.to_path_buf(),
        })
    }

    /// Get a secret value. Only for internal use by approved components.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    /// Set a secret value and persist to disk.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        self.values.insert(key.to_string(), value.to_string());
        self.persist()
    }

    /// Remove a secret and persist to disk.
    pub fn unset(&mut self, key: &str) -> Result<bool> {
        let existed = self.values.remove(key).is_some();
        if existed {
            self.persist()?;
        }
        Ok(existed)
    }

    /// List all secret keys with redacted values.
    pub fn list_redacted(&self) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .values
            .iter()
            .map(|(k, v)| (k.clone(), redact_value(k, v)))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    /// Persist all secrets back to the file.
    fn persist(&self) -> Result<()> {
        let mut content = String::from("# minusbot secrets — DO NOT SHARE\n");
        let mut keys: Vec<_> = self.values.keys().collect();
        keys.sort();
        for key in keys {
            let val = &self.values[key];
            content.push_str(&format!("{}={}\n", key, val));
        }
        std::fs::write(&self.path, content)
            .with_context(|| format!("Failed to write secrets to {}", self.path.display()))?;
        Ok(())
    }
}

/// Check if a key looks like it holds a sensitive value.
pub fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_uppercase();
    SENSITIVE_KEYWORDS.iter().any(|kw| upper.contains(kw))
}

/// Redact a value for display. Shows first 4 chars + "...redacted" for sensitive keys.
pub fn redact_value(key: &str, value: &str) -> String {
    if is_sensitive_key(key) {
        if value.len() > 6 {
            format!("{}...redacted", &value[..4])
        } else {
            "***redacted***".to_string()
        }
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sensitive_key() {
        assert!(is_sensitive_key("SECRET_OPENAI_API_KEY"));
        assert!(is_sensitive_key("GITHUB_TOKEN"));
        assert!(is_sensitive_key("DB_PASSWORD"));
        assert!(is_sensitive_key("PRIVATE_KEY"));
        assert!(!is_sensitive_key("MINUSBOT_DEFAULT_PROVIDER"));
        assert!(!is_sensitive_key("LOG_LEVEL"));
    }

    #[test]
    fn test_redact_value() {
        assert_eq!(
            redact_value("SECRET_OPENAI_API_KEY", "sk-or-v1-abcdef1234"),
            "sk-o...redacted"
        );
        assert_eq!(
            redact_value("SECRET_OPENAI_API_KEY", "abc"),
            "***redacted***"
        );
        assert_eq!(
            redact_value("MINUSBOT_DEFAULT_PROVIDER", "openrouter"),
            "openrouter"
        );
    }
}
