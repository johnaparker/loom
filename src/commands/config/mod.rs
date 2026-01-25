//! Configuration commands for enabling/disabling grove integrations.
//!
//! Provides `grove config enable/disable <feature>` commands to simplify
//! integration setup for Claude, GitHub, and Linear.

mod claude_setup;
mod disable;
mod enable;
mod github_setup;
mod linear_setup;

use anyhow::Result;
use std::path::PathBuf;

use crate::cli::{ConfigCommands, ConfigScope};
use crate::config::GlobalConfig;
use crate::error::GroveError;

/// Target paths for configuration changes.
///
/// Resolves the appropriate config file paths based on scope.
pub struct ConfigTarget {
    /// Path to grove config file (.grove.toml or ~/.config/grove/config.toml)
    pub grove_config_path: PathBuf,
    /// Path to Claude settings.json location
    pub claude_settings_path: PathBuf,
    /// Whether this is project scope (vs user scope)
    pub is_project_scope: bool,
    /// Repository root (only set for project scope)
    pub repo_root: Option<PathBuf>,
}

impl ConfigTarget {
    /// Resolve configuration target paths based on scope.
    pub fn resolve(scope: ConfigScope) -> Result<Self> {
        match scope {
            ConfigScope::User => Self::resolve_user_scope(),
            ConfigScope::Project => Self::resolve_project_scope(),
        }
    }

    fn resolve_user_scope() -> Result<Self> {
        let grove_config_path = GlobalConfig::config_path()?;
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let claude_settings_path = home.join(".claude").join("settings.json");

        Ok(Self {
            grove_config_path,
            claude_settings_path,
            is_project_scope: false,
            repo_root: None,
        })
    }

    fn resolve_project_scope() -> Result<Self> {
        // Find the repository root
        let cwd = std::env::current_dir()?;
        let repo = git2::Repository::discover(&cwd).map_err(|_| GroveError::NotGitRepo)?;
        let repo_root = repo.workdir().ok_or(GroveError::NotGitRepo)?.to_path_buf();

        // For project scope, we need to be at the repo root
        // (or we could just use the repo root regardless)
        let grove_config_path = repo_root.join(".grove.toml");
        let claude_settings_path = repo_root.join(".claude").join("settings.json");

        Ok(Self {
            grove_config_path,
            claude_settings_path,
            is_project_scope: true,
            repo_root: Some(repo_root),
        })
    }

    /// Get a display name for the scope
    pub fn scope_name(&self) -> &'static str {
        if self.is_project_scope {
            "project"
        } else {
            "user"
        }
    }
}

/// Main entry point for config commands.
pub fn run(command: ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Enable { feature, scope } => enable::run(feature, scope),
        ConfigCommands::Disable { feature, scope } => disable::run(feature, scope),
    }
}
