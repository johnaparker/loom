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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_project_config() {
        let toml_str = r#"
project_name = "my-project"

[sync]
patterns = ["custom-file.txt"]
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project_name, Some("my-project".to_string()));
        assert_eq!(config.sync.patterns, vec!["custom-file.txt"]);
    }

    #[test]
    fn test_parse_minimal_project_config() {
        let toml_str = r#"
project_name = "minimal"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project_name, Some("minimal".to_string()));
        assert!(config.sync.patterns.is_empty()); // default
    }

    #[test]
    fn test_load_nonexistent_config() {
        let temp_dir = TempDir::new().unwrap();
        let result = ProjectConfig::load(temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = ProjectConfig {
            project_name: Some("test-project".to_string()),
            sync: ProjectSyncConfig {
                patterns: vec!["file1.txt".to_string(), "file2.txt".to_string()],
            },
        };

        config.save(temp_dir.path()).unwrap();

        let loaded = ProjectConfig::load(temp_dir.path()).unwrap().unwrap();
        assert_eq!(loaded.project_name, Some("test-project".to_string()));
        assert_eq!(loaded.sync.patterns.len(), 2);
    }
}
