mod global;
mod project;

pub use global::GlobalConfig;
pub use project::ProjectConfig;

use anyhow::Result;
use std::path::Path;

/// Combined configuration from global and project sources
#[derive(Debug, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    pub project: Option<ProjectConfig>,
}

impl Config {
    /// Load configuration from global config and optional project config
    pub fn load(repo_root: Option<&Path>) -> Result<Self> {
        let global = GlobalConfig::load()?;
        let project = if let Some(root) = repo_root {
            ProjectConfig::load(root)?
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
}
