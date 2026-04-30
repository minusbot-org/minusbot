use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main application configuration stored in config.toml.
/// Contains only public/non-sensitive configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub core: CoreConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    #[serde(default)]
    pub first_run: bool,
    #[serde(default = "default_channel")]
    pub default_channel: String,
}

fn default_channel() -> String {
    "cli".into()
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            first_run: true,
            default_channel: default_channel(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_agent_name")]
    pub name: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: u32,
    #[serde(default = "default_max_messages")]
    pub max_messages: u32,
}

fn default_agent_name() -> String {
    "minusbot".into()
}

fn default_system_prompt() -> String {
    "You are minusbot, a self-hosted personal AI assistant.".into()
}

fn default_max_tool_iterations() -> u32 {
    5
}

fn default_max_messages() -> u32 {
    11
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            system_prompt: default_system_prompt(),
            max_tool_iterations: default_max_tool_iterations(),
            max_messages: default_max_messages(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            default: None,
            model: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub redact_secrets: bool,
    #[serde(default)]
    pub allow_shell: bool,
    #[serde(default)]
    pub allow_filesystem_write_outside_drive: bool,
    #[serde(default = "default_true")]
    pub require_approval_for_secret_access: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            redact_secrets: true,
            allow_shell: false,
            allow_filesystem_write_outside_drive: false,
            require_approval_for_secret_access: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_skills")]
    pub skills: String,
    #[serde(default = "default_drive")]
    pub drive: String,
    #[serde(default = "default_logs")]
    pub logs: String,
    #[serde(default = "default_addons")]
    pub addons: String,
    #[serde(default = "default_vault")]
    pub vault: String,
    #[serde(default = "default_cache")]
    pub cache: String,
    #[serde(default = "default_database")]
    pub database: String,
}

fn default_skills() -> String { "skills".into() }
fn default_drive() -> String { "drive".into() }
fn default_logs() -> String { "logs".into() }
fn default_addons() -> String { "addons".into() }
fn default_vault() -> String { "vault".into() }
fn default_cache() -> String { "cache".into() }
fn default_database() -> String { "data.sqlite".into() }

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            skills: default_skills(),
            drive: default_drive(),
            logs: default_logs(),
            addons: default_addons(),
            vault: default_vault(),
            cache: default_cache(),
            database: default_database(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            core: CoreConfig::default(),
            agent: AgentConfig::default(),
            provider: ProviderConfig::default(),
            security: SecurityConfig::default(),
            paths: PathsConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load config from a TOML file. If the file doesn't exist, returns defaults.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: AppConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;
        Ok(config)
    }

    /// Save config to a TOML file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    /// Get a config value by dotted path (e.g. "provider.default").
    pub fn get(&self, path: &str) -> Option<String> {
        let value = toml::Value::try_from(self).ok()?;
        let mut current = &value;
        for key in path.split('.') {
            current = current.get(key)?;
        }
        match current {
            toml::Value::String(s) => Some(s.clone()),
            toml::Value::Boolean(b) => Some(b.to_string()),
            toml::Value::Integer(i) => Some(i.to_string()),
            toml::Value::Float(f) => Some(f.to_string()),
            _ => Some(current.to_string()),
        }
    }

    /// Set a config value by dotted path. Returns true if the value was set.
    pub fn set(&mut self, path: &str, value: &str) -> Result<bool> {
        let mut toml_val = toml::Value::try_from(&*self)
            .context("Failed to serialize config to TOML value")?;

        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return Ok(false);
        }

        let mut current = &mut toml_val;
        for &key in &parts[..parts.len() - 1] {
            current = current
                .get_mut(key)
                .with_context(|| format!("Config path '{}' not found", path))?;
        }

        let last = parts.last().unwrap();
        if let Some(existing) = current.get(last) {
            // Preserve the existing type
            let new_val = match existing {
                toml::Value::Boolean(_) => toml::Value::Boolean(value.parse().unwrap_or(false)),
                toml::Value::Integer(_) => {
                    toml::Value::Integer(value.parse().unwrap_or(0))
                }
                _ => toml::Value::String(value.to_string()),
            };
            current[last] = new_val;
        } else {
            current[last] = toml::Value::String(value.to_string());
        }

        // Deserialize back
        let updated: AppConfig = toml_val.try_into()
            .context("Failed to deserialize updated config")?;
        *self = updated;
        Ok(true)
    }
}
