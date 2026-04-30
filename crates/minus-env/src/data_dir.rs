use anyhow::{Context, Result};
use std::path::PathBuf;

/// Manages the OS-native user data directory for minusbot.
#[derive(Debug, Clone)]
pub struct DataDir {
    pub root: PathBuf,
}

impl DataDir {
    /// Resolve the data directory. Uses `MINUSBOT_DATA_DIR` env var if set,
    /// otherwise falls back to OS-native data directory.
    pub fn resolve() -> Result<Self> {
        let root = if let Ok(custom) = std::env::var("MINUSBOT_DATA_DIR") {
            PathBuf::from(custom)
        } else {
            dirs::data_dir()
                .context("Could not determine OS data directory")?
                .join("minusbot")
        };
        Ok(Self { root })
    }

    /// Create all required subdirectories on first run.
    pub fn ensure_dirs(&self) -> Result<()> {
        let dirs = [
            self.root.as_path(),
            &self.skills_dir(),
            &self.drive_dir(),
            &self.logs_dir(),
            &self.vault_dir(),
            &self.vault_secrets_dir(),
            &self.addons_dir(),
            &self.cache_dir(),
        ];
        for d in &dirs {
            std::fs::create_dir_all(d)
                .with_context(|| format!("Failed to create directory: {}", d.display()))?;
        }
        Ok(())
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn secrets_env_path(&self) -> PathBuf {
        self.root.join("secrets.env")
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join("data.sqlite")
    }

    pub fn database_url(&self) -> String {
        format!("sqlite://{}", self.database_path().display())
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    pub fn drive_dir(&self) -> PathBuf {
        self.root.join("drive")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn vault_dir(&self) -> PathBuf {
        self.root.join("vault")
    }

    pub fn vault_secrets_dir(&self) -> PathBuf {
        self.root.join("vault").join("secrets")
    }

    pub fn addons_dir(&self) -> PathBuf {
        self.root.join("addons")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn log_file_today(&self) -> PathBuf {
        let date = chrono::Local::now().format("%d-%m-%Y").to_string();
        self.logs_dir().join(format!("{}.log", date))
    }

    /// Check if this is the first run (config.toml doesn't exist yet).
    pub fn is_first_run(&self) -> bool {
        !self.config_path().exists()
    }
}
