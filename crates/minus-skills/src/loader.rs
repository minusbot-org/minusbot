use anyhow::{bail, Context, Result};
use minus_db::Database;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Metadata parsed from the YAML frontmatter of a skill Markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Manages loading/unloading skills from Markdown files.
pub struct SkillManager {
    skills_dir: PathBuf,
    db: Database,
}

impl SkillManager {
    pub fn new(skills_dir: PathBuf, db: Database) -> Self {
        Self { skills_dir, db }
    }

    /// Scan the skills directory and index all skills into the database.
    pub async fn scan_and_index(&self) -> Result<usize> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir)?;
            return Ok(0);
        }

        let mut count = 0;
        let entries = std::fs::read_dir(&self.skills_dir)
            .with_context(|| format!("Failed to read skills dir: {}", self.skills_dir.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "md") {
                match self.index_skill(&path).await {
                    Ok(_) => count += 1,
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "Failed to index skill");
                    }
                }
            }
        }

        tracing::info!(count = count, "Skills indexed");
        Ok(count)
    }

    /// Parse a single skill file and upsert it into the database.
    async fn index_skill(&self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

        let (meta, body) = parse_skill_frontmatter(&content)?;
        let tags_json = serde_json::to_string(&meta.tags)?;

        self.db
            .upsert_skill(
                &meta.id,
                &meta.name,
                meta.description.as_deref(),
                Some(&tags_json),
                &body,
                Some(&path.display().to_string()),
            )
            .await?;

        Ok(())
    }

    /// Load a skill for a specific chat.
    pub async fn load_for_chat(&self, chat_id: &str, skill_name: &str) -> Result<String> {
        let skill = self
            .db
            .get_skill_by_name(skill_name)
            .await?
            .with_context(|| format!("Skill '{}' not found", skill_name))?;
        self.db.load_skill_for_chat(chat_id, &skill.id).await?;
        Ok(format!("Loaded skill \"{}\"", skill_name))
    }

    /// Unload a skill from a specific chat.
    pub async fn unload_from_chat(&self, chat_id: &str, skill_name: &str) -> Result<String> {
        let skill = self
            .db
            .get_skill_by_name(skill_name)
            .await?
            .with_context(|| format!("Skill '{}' not found", skill_name))?;
        let removed = self.db.unload_skill_for_chat(chat_id, &skill.id).await?;
        if removed {
            Ok(format!("Unloaded skill \"{}\"", skill_name))
        } else {
            Ok(format!("Skill \"{}\" was not loaded for this chat", skill_name))
        }
    }

    /// List all available skills.
    pub async fn list(&self) -> Result<Vec<SkillMeta>> {
        let records = self.db.list_skills().await?;
        Ok(records
            .into_iter()
            .map(|r| {
                let tags: Vec<String> = r
                    .tags_json
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
                SkillMeta {
                    id: r.id,
                    name: r.name,
                    description: r.description,
                    tags,
                }
            })
            .collect())
    }

    /// Get the content of skills loaded for a given chat.
    pub async fn get_loaded_content(&self, chat_id: &str) -> Result<Vec<String>> {
        let records = self.db.get_loaded_skills(chat_id).await?;
        Ok(records.into_iter().map(|r| r.content).collect())
    }

    /// Simple text search across skills.
    pub async fn search(&self, query: &str) -> Result<Vec<SkillMeta>> {
        let all = self.list().await?;
        let query_lower = query.to_lowercase();
        Ok(all
            .into_iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description
                        .as_ref()
                        .map_or(false, |d| d.to_lowercase().contains(&query_lower))
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect())
    }
}

/// Parse YAML frontmatter from a Markdown skill file.
/// Expects the format:
/// ```md
/// ---
/// id: ...
/// name: ...
/// ---
/// # Content...
/// ```
pub fn parse_skill_frontmatter(content: &str) -> Result<(SkillMeta, String)> {
    let content = content.trim();
    if !content.starts_with("---") {
        // No frontmatter — derive metadata from filename
        bail!("Skill file must start with YAML frontmatter (---)")
    }

    let rest = &content[3..];
    let end = rest
        .find("---")
        .context("Missing closing --- for frontmatter")?;

    let yaml = &rest[..end].trim();
    let body = rest[end + 3..].trim().to_string();

    // Parse YAML manually since we only need simple fields
    let meta = parse_yaml_meta(yaml)?;
    Ok((meta, body))
}

fn parse_yaml_meta(yaml: &str) -> Result<SkillMeta> {
    let mut id = None;
    let mut name = None;
    let mut description = None;
    let mut tags = Vec::new();

    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "id" => id = Some(val.to_string()),
                "name" => name = Some(val.to_string()),
                "description" => description = Some(val.to_string()),
                "tags" => {
                    // Parse [tag1, tag2, tag3]
                    let val = val.trim_start_matches('[').trim_end_matches(']');
                    tags = val
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    let id = id.context("Skill frontmatter missing 'id'")?;
    let name = name.context("Skill frontmatter missing 'name'")?;

    Ok(SkillMeta {
        id,
        name,
        description,
        tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_frontmatter() {
        let content = r#"---
id: rust-nes-emulator
name: Rust NES Emulator
description: Knowledge about building accurate NES emulators in Rust.
tags: [rust, emulation, nes]
---

# Rust NES Emulator

This skill covers...
"#;
        let (meta, body) = parse_skill_frontmatter(content).unwrap();
        assert_eq!(meta.id, "rust-nes-emulator");
        assert_eq!(meta.name, "Rust NES Emulator");
        assert_eq!(
            meta.description.as_deref(),
            Some("Knowledge about building accurate NES emulators in Rust.")
        );
        assert_eq!(meta.tags, vec!["rust", "emulation", "nes"]);
        assert!(body.contains("# Rust NES Emulator"));
    }
}
