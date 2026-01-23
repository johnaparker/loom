mod global;
mod project;
mod source;

pub use global::{GlobalConfig, SyncWorkflow};
pub use project::ProjectConfig;
pub use source::{ConfigSource, FilesystemSource};

#[cfg(test)]
pub use source::MemorySource;

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Combined configuration from global and project sources
#[derive(Debug, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    pub project: Option<ProjectConfig>,
}

impl Config {
    /// Load configuration from global config and optional project config.
    /// Checks current working directory for .gwt.toml first, then falls back to repo root.
    pub fn load(repo_root: Option<&Path>) -> Result<Self> {
        Self::load_with_source(repo_root, &FilesystemSource)
    }

    /// Load configuration using a custom source (for testing).
    pub fn load_with_source<S: ConfigSource>(repo_root: Option<&Path>, source: &S) -> Result<Self> {
        let global = GlobalConfig::load_with_source(source)?;
        let project = if let Some(root) = repo_root {
            // For now, project config loading still uses filesystem
            // because it has more complex path resolution logic
            let current_dir = std::env::current_dir().ok();
            ProjectConfig::load(current_dir.as_deref(), root)?
        } else {
            None
        };
        Ok(Self { global, project })
    }

    /// Create a Config from pre-built components (for testing).
    #[cfg(test)]
    pub fn from_parts(global: GlobalConfig, project: Option<ProjectConfig>) -> Self {
        Self { global, project }
    }

    /// Get the worktree root directory, expanding ~ to home
    pub fn worktree_root(&self) -> Result<std::path::PathBuf> {
        let root = &self.global.worktree_root;
        if root.starts_with("~") {
            let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
            Ok(home.join(root.strip_prefix("~/").unwrap_or(root)))
        } else {
            Ok(std::path::PathBuf::from(root))
        }
    }

    /// Get the default category for new worktrees
    pub fn default_category(&self) -> &str {
        &self.global.default_category
    }

    /// Get the project name (from project config or provided default)
    pub fn project_name(&self, default: &str) -> String {
        self.project
            .as_ref()
            .and_then(|p| p.project_name.clone())
            .unwrap_or_else(|| default.to_string())
    }

    /// Get sync patterns (merged from global and project)
    pub fn sync_patterns(&self) -> Vec<String> {
        let mut patterns = self.global.sync.patterns.clone();
        if let Some(project) = &self.project {
            for pattern in &project.sync.patterns {
                if !patterns.contains(pattern) {
                    patterns.push(pattern.clone());
                }
            }
        }
        patterns
    }

    /// Whether to auto-register with sesh
    pub fn sesh_auto_register(&self) -> bool {
        self.global.sesh.auto_register
    }

    /// Get the Linear team prefix (e.g., "ABC")
    pub fn linear_prefix(&self) -> Option<&str> {
        self.global.linear.team_prefix.as_deref()
    }

    /// Get the Linear API key
    pub fn linear_api_key(&self) -> Option<&str> {
        self.global.linear.api_key.as_deref()
    }

    /// Whether to automatically update Linear issue status
    pub fn linear_auto_update_status(&self) -> bool {
        self.global.linear.auto_update_status
    }

    /// Get the cache directory, expanding ~ to home
    ///
    /// Priority: GWT_CACHE_DIR env var > config file > default (~/.cache/gwt)
    pub fn cache_dir(&self) -> Result<PathBuf> {
        // Environment variable takes precedence
        if let Ok(env_dir) = std::env::var("GWT_CACHE_DIR") {
            return Ok(PathBuf::from(env_dir));
        }

        // Use config value, expanding ~ to home
        let dir = &self.global.cache_dir;
        if dir.starts_with("~") {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
            Ok(home.join(dir.strip_prefix("~/").unwrap_or(dir.strip_prefix("~").unwrap_or(dir))))
        } else {
            Ok(PathBuf::from(dir))
        }
    }

    /// Get the current workflow mode (project config overrides global)
    pub fn workflow(&self) -> global::SyncWorkflow {
        self.project
            .as_ref()
            .and_then(|p| p.git.workflow)
            .unwrap_or(self.global.workflow)
    }

    /// Whether we're in push workflow mode (local-first)
    pub fn is_push_workflow(&self) -> bool {
        self.workflow() == global::SyncWorkflow::Push
    }

    /// Whether we're in pull workflow mode (team/PR-based)
    pub fn is_pull_workflow(&self) -> bool {
        self.workflow() == global::SyncWorkflow::Pull
    }

    /// Get the Linear icon (displayed before Linear issue titles)
    pub fn linear_icon(&self) -> Option<&str> {
        self.global.icons.linear.as_deref()
    }

    /// Get the GitHub icon (displayed before GitHub PR info)
    pub fn github_icon(&self) -> Option<&str> {
        self.global.icons.github.as_deref()
    }

    /// Get the branch icon (displayed before branch names)
    pub fn branch_icon(&self) -> Option<&str> {
        self.global.icons.branch.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::{ProjectGitConfig, ProjectSyncConfig};

    fn make_config(global_workflow: SyncWorkflow, project_workflow: Option<SyncWorkflow>) -> Config {
        let global = GlobalConfig {
            workflow: global_workflow,
            ..GlobalConfig::default()
        };
        let project = project_workflow.map(|wf| ProjectConfig {
            project_name: None,
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig { workflow: Some(wf) },
        });
        Config { global, project }
    }

    #[test]
    fn test_workflow_uses_global_when_no_project() {
        let config = Config {
            global: GlobalConfig {
                workflow: SyncWorkflow::Pull,
                ..GlobalConfig::default()
            },
            project: None,
        };
        assert_eq!(config.workflow(), SyncWorkflow::Pull);
        assert!(config.is_pull_workflow());
        assert!(!config.is_push_workflow());
    }

    #[test]
    fn test_workflow_uses_global_when_project_has_no_workflow() {
        let config = Config {
            global: GlobalConfig {
                workflow: SyncWorkflow::Push,
                ..GlobalConfig::default()
            },
            project: Some(ProjectConfig {
                project_name: Some("test".to_string()),
                sync: ProjectSyncConfig::default(),
                git: ProjectGitConfig { workflow: None },
            }),
        };
        assert_eq!(config.workflow(), SyncWorkflow::Push);
        assert!(config.is_push_workflow());
    }

    #[test]
    fn test_workflow_project_overrides_global() {
        // Global is push, project overrides to pull
        let config = make_config(SyncWorkflow::Push, Some(SyncWorkflow::Pull));
        assert_eq!(config.workflow(), SyncWorkflow::Pull);
        assert!(config.is_pull_workflow());
        assert!(!config.is_push_workflow());

        // Global is pull, project overrides to push
        let config = make_config(SyncWorkflow::Pull, Some(SyncWorkflow::Push));
        assert_eq!(config.workflow(), SyncWorkflow::Push);
        assert!(config.is_push_workflow());
        assert!(!config.is_pull_workflow());
    }

    // Tests demonstrating new injection patterns

    #[test]
    fn test_config_from_parts_direct_construction() {
        // Test Config from pre-built parts (no filesystem access)
        let config = Config::from_parts(
            GlobalConfig {
                workflow: SyncWorkflow::Pull,
                worktree_root: "/custom/path".to_string(),
                ..GlobalConfig::default()
            },
            None,
        );

        assert!(config.is_pull_workflow());
        assert_eq!(config.worktree_root().unwrap(), PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_config_load_with_memory_source() {
        // Test config loading with in-memory source (no filesystem)
        let source = MemorySource {
            global_config: Some(
                r#"
worktree_root = "/test/worktrees"
default_category = "feature"
workflow = "pull"
"#
                .to_string(),
            ),
            ..Default::default()
        };

        let config = Config::load_with_source(None, &source).unwrap();
        assert_eq!(config.global.worktree_root, "/test/worktrees");
        assert_eq!(config.default_category(), "feature");
        assert!(config.is_pull_workflow());
    }

    #[test]
    fn test_config_load_with_missing_global_uses_defaults() {
        // Test that missing global config returns defaults
        let source = MemorySource::default();

        let config = Config::load_with_source(None, &source).unwrap();
        assert_eq!(config.global.worktree_root, "~/worktrees");
        assert_eq!(config.default_category(), "dev");
        assert!(config.is_push_workflow()); // Push is default
    }

    #[test]
    fn test_config_from_parts_with_project() {
        // Test Config from parts with project config
        let config = Config::from_parts(
            GlobalConfig {
                workflow: SyncWorkflow::Push,
                ..GlobalConfig::default()
            },
            Some(ProjectConfig {
                project_name: Some("test-project".to_string()),
                sync: ProjectSyncConfig {
                    patterns: vec!["custom.txt".to_string()],
                },
                git: ProjectGitConfig {
                    workflow: Some(SyncWorkflow::Pull), // Override global
                },
            }),
        );

        // Project workflow should override global
        assert!(config.is_pull_workflow());
        assert_eq!(config.project_name("default"), "test-project");

        // Sync patterns should merge
        let patterns = config.sync_patterns();
        assert!(patterns.contains(&"custom.txt".to_string()));
    }
}
