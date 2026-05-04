use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
}

pub struct AgentRegistry {
    agents_dir: PathBuf,
    agents: Arc<RwLock<HashMap<String, AgentDefinition>>>,
}

impl AgentRegistry {
    pub fn new(agents_dir: PathBuf) -> Self {
        Self {
            agents_dir,
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn scan(&self) -> Result<()> {
        if !self.agents_dir.exists() {
            std::fs::create_dir_all(&self.agents_dir)?;
        }

        let mut agents = HashMap::new();

        // Add hardcoded default if it doesn't exist?
        // Actually, the user says "default" should be one of them.
        // We'll see if there is a default.md

        let entries = std::fs::read_dir(&self.agents_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let content = std::fs::read_to_string(&path)?;

                // Simple parsing: first line starting with # is the name, rest is prompt
                let lines = content.lines();
                let mut name = id.clone();
                let mut prompt_lines = Vec::new();

                for line in lines {
                    if line.starts_with("# ") {
                        name = line[2..].trim().to_string();
                    } else {
                        prompt_lines.push(line);
                    }
                }

                let system_prompt = prompt_lines.join("\n").trim().to_string();

                agents.insert(
                    id.clone(),
                    AgentDefinition {
                        id,
                        name,
                        system_prompt,
                    },
                );
            }
        }

        let mut lock = self.agents.write().await;
        *lock = agents;

        tracing::info!(count = lock.len(), "Scanned agents from directory");
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Option<AgentDefinition> {
        let lock = self.agents.read().await;
        lock.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<AgentDefinition> {
        let lock = self.agents.read().await;
        lock.values().cloned().collect()
    }
}
