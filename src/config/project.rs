use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Project-specific configuration stored at .gwt.toml in repo root
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_name: Option<String>,
    #[serde(default)]
    pub sync: ProjectSyncConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSyncConfig {
    #[serde(default)]
    pub patterns: Vec<String>,
}

impl ProjectConfig {
    /// Load project config from repo root, returns None if not found
    pub fn load(repo_root: &Path) -> Result<Option<Self>> {
        let path = repo_root.join(".gwt.toml");
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: ProjectConfig = toml::from_str(&content)?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Save project config to repo root
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let path = repo_root.join(".gwt.toml");
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
