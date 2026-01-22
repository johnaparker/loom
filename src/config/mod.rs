mod global;
mod project;

pub use global::{GlobalConfig, SyncWorkflow};
pub use project::ProjectConfig;

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
        let global = GlobalConfig::load()?;
        let project = if let Some(root) = repo_root {
            let current_dir = std::env::current_dir().ok();
            ProjectConfig::load(current_dir.as_deref(), root)?
        } else {
            None
        };
        Ok(Self { global, project })
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
}
