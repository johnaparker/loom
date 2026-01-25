use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::global::SyncWorkflow;
use super::{ClaudeOverride, IntegrationOverride};

/// Project-specific configuration stored at .grove.toml in repo root
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_name: Option<String>,
    #[serde(default)]
    pub sync: ProjectSyncConfig,
    /// Git section for workflow settings
    #[serde(default)]
    pub git: ProjectGitConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectGitConfig {
    /// Workflow mode override for this project
    pub workflow: Option<SyncWorkflow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSyncConfig {
    #[serde(default)]
    pub patterns: Vec<String>,
}

impl ProjectConfig {
    /// Load config from a specific path if it exists
    fn load_from_path(path: &Path) -> Result<Option<Self>> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: ProjectConfig = toml::from_str(&content)?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Load project config, checking worktree directory first then repo root.
    /// This allows per-worktree config overrides.
    pub fn load(worktree_dir: Option<&Path>, repo_root: &Path) -> Result<Option<Self>> {
        // Check worktree directory first (allows per-worktree overrides)
        if let Some(wt_dir) = worktree_dir
            && wt_dir != repo_root
            && let Some(config) = Self::load_from_path(&wt_dir.join(".grove.toml"))?
        {
            return Ok(Some(config));
        }

        // Fall back to repo root
        Self::load_from_path(&repo_root.join(".grove.toml"))
    }

    /// Save project config to repo root
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let path = repo_root.join(".grove.toml");
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
    fn test_parse_git_workflow_push() {
        let toml_str = r#"
[git]
workflow = "push"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.git.workflow, Some(SyncWorkflow::Push));
    }

    #[test]
    fn test_parse_git_workflow_pull() {
        let toml_str = r#"
[git]
workflow = "pull"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.git.workflow, Some(SyncWorkflow::Pull));
    }

    #[test]
    fn test_parse_git_workflow_default_none() {
        let toml_str = r#"
project_name = "test"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.git.workflow, None);
    }

    #[test]
    fn test_load_nonexistent_config() {
        let temp_dir = TempDir::new().unwrap();
        let result = ProjectConfig::load(None, temp_dir.path()).unwrap();
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
            git: ProjectGitConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: None,
        };

        config.save(temp_dir.path()).unwrap();

        let loaded = ProjectConfig::load(None, temp_dir.path()).unwrap().unwrap();
        assert_eq!(loaded.project_name, Some("test-project".to_string()));
        assert_eq!(loaded.sync.patterns.len(), 2);
    }

    #[test]
    fn test_worktree_config_takes_precedence() {
        let repo_root = TempDir::new().unwrap();
        let worktree_dir = TempDir::new().unwrap();

        // Create config in repo root
        let repo_config = ProjectConfig {
            project_name: Some("repo-project".to_string()),
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: None,
        };
        repo_config.save(repo_root.path()).unwrap();

        // Create config in worktree
        let wt_config = ProjectConfig {
            project_name: Some("worktree-project".to_string()),
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: None,
        };
        wt_config.save(worktree_dir.path()).unwrap();

        // Worktree config should take precedence
        let loaded = ProjectConfig::load(Some(worktree_dir.path()), repo_root.path())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.project_name, Some("worktree-project".to_string()));
    }

    #[test]
    fn test_fallback_to_repo_root_when_no_worktree_config() {
        let repo_root = TempDir::new().unwrap();
        let worktree_dir = TempDir::new().unwrap();

        // Only create config in repo root
        let repo_config = ProjectConfig {
            project_name: Some("repo-project".to_string()),
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: None,
        };
        repo_config.save(repo_root.path()).unwrap();

        // Should fall back to repo root config
        let loaded = ProjectConfig::load(Some(worktree_dir.path()), repo_root.path())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.project_name, Some("repo-project".to_string()));
    }

    #[test]
    fn test_parse_integration_overrides() {
        let toml_str = r#"
project_name = "my-project"

[linear]
enabled = false

[github]
enabled = true

[diffview]
enabled = false
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.linear.as_ref().and_then(|l| l.enabled), Some(false));
        assert_eq!(config.github.as_ref().and_then(|g| g.enabled), Some(true));
        assert_eq!(
            config.diffview.as_ref().and_then(|d| d.enabled),
            Some(false)
        );
    }

    #[test]
    fn test_partial_integration_overrides() {
        let toml_str = r#"
project_name = "my-project"

[github]
enabled = false
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert!(config.linear.is_none());
        assert_eq!(config.github.as_ref().and_then(|g| g.enabled), Some(false));
        assert!(config.diffview.is_none());
    }

    #[test]
    fn test_parse_claude_override() {
        let toml_str = r#"
project_name = "my-project"

[claude]
sandbox = false
sandbox_auto_allow_bash = false
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        let claude = config.claude.as_ref().unwrap();
        assert_eq!(claude.sandbox, Some(false));
        assert_eq!(claude.sandbox_auto_allow_bash, Some(false));
    }

    #[test]
    fn test_parse_claude_override_partial() {
        let toml_str = r#"
project_name = "my-project"

[claude]
sandbox = true
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        let claude = config.claude.as_ref().unwrap();
        assert_eq!(claude.sandbox, Some(true));
        assert!(claude.sandbox_auto_allow_bash.is_none());
    }
}
