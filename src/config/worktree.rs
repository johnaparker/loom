//! Worktree-level configuration.
//!
//! Worktree configs are stored at `.loom.toml` in each worktree directory
//! (separate from the project-level `.loom.toml` at the repo root).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::{ClaudeOverride, IntegrationOverride};

/// Worktree-specific configuration stored at .loom.toml in the worktree directory.
///
/// This config only applies once the worktree exists - it cannot be used during
/// `loom new` since the directory doesn't exist yet.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorktreeConfig {
    /// Sync configuration overrides for this worktree
    #[serde(default)]
    pub sync: WorktreeSyncConfig,
    /// Linear integration override
    #[serde(default)]
    pub linear: Option<IntegrationOverride>,
    /// GitHub integration override
    #[serde(default)]
    pub github: Option<IntegrationOverride>,
    /// Diffview integration override
    #[serde(default)]
    pub diffview: Option<IntegrationOverride>,
    /// Claude Code integration override
    #[serde(default)]
    pub claude: Option<ClaudeOverride>,
}

/// Worktree-level sync configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorktreeSyncConfig {
    /// Additional patterns to sync to this worktree
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Patterns to exclude from syncing to this worktree
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

impl WorktreeConfig {
    /// Load worktree config from a worktree directory if it exists.
    ///
    /// The worktree config is separate from the project config - it's stored
    /// in the worktree directory itself, not the repo root.
    pub fn load(worktree_path: &Path) -> Result<Option<Self>> {
        let config_path = worktree_path.join(".loom.toml");
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            let config: WorktreeConfig = toml::from_str(&content)?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Save worktree config to the worktree directory.
    pub fn save(&self, worktree_path: &Path) -> Result<()> {
        let config_path = worktree_path.join(".loom.toml");
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_nonexistent_config() {
        let temp_dir = TempDir::new().unwrap();
        let result = WorktreeConfig::load(temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = WorktreeConfig {
            sync: WorktreeSyncConfig {
                patterns: vec!["extra.txt".to_string()],
                exclude_patterns: vec![".claude/".to_string()],
            },
            linear: Some(IntegrationOverride {
                enabled: Some(false),
            }),
            github: None,
            diffview: None,
            claude: None,
        };

        config.save(temp_dir.path()).unwrap();

        let loaded = WorktreeConfig::load(temp_dir.path()).unwrap().unwrap();
        assert_eq!(loaded.sync.patterns, vec!["extra.txt"]);
        assert_eq!(loaded.sync.exclude_patterns, vec![".claude/"]);
        assert_eq!(loaded.linear.as_ref().and_then(|l| l.enabled), Some(false));
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
[sync]
exclude_patterns = [".claude/"]
"#;
        let config: WorktreeConfig = toml::from_str(toml_str).unwrap();
        assert!(config.sync.patterns.is_empty());
        assert_eq!(config.sync.exclude_patterns, vec![".claude/"]);
    }

    #[test]
    fn test_parse_integration_overrides() {
        let toml_str = r#"
[linear]
enabled = true

[github]
enabled = false
"#;
        let config: WorktreeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.linear.as_ref().and_then(|l| l.enabled), Some(true));
        assert_eq!(config.github.as_ref().and_then(|g| g.enabled), Some(false));
        assert!(config.diffview.is_none());
    }

    #[test]
    fn test_default_config() {
        let config = WorktreeConfig::default();
        assert!(config.sync.patterns.is_empty());
        assert!(config.sync.exclude_patterns.is_empty());
        assert!(config.linear.is_none());
        assert!(config.github.is_none());
        assert!(config.diffview.is_none());
        assert!(config.claude.is_none());
    }

    #[test]
    fn test_parse_claude_override() {
        let toml_str = r#"
[claude]
sandbox = false
sandbox_auto_allow_bash = true
"#;
        let config: WorktreeConfig = toml::from_str(toml_str).unwrap();
        let claude = config.claude.as_ref().unwrap();
        assert_eq!(claude.sandbox, Some(false));
        assert_eq!(claude.sandbox_auto_allow_bash, Some(true));
    }
}
