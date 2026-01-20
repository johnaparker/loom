//! Sesh session configuration integration.
//!
//! Sesh (https://github.com/joshmedeski/sesh) is a terminal session manager.
//! This module provides integration for registering and unregistering worktrees
//! with sesh's configuration file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// A session entry in sesh.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeshSession {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_command: Option<String>,
}

/// The sesh.toml configuration file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeshConfig {
    #[serde(default)]
    pub session: Vec<SeshSession>,
}

impl SeshConfig {
    /// Get the path to sesh.toml
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(config_dir.join("sesh").join("sesh.toml"))
    }

    /// Load sesh config from disk
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: SeshConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save sesh config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Add a session entry
    pub fn add_session(&mut self, name: &str, path: &str) {
        // Remove existing entry with same name if present
        self.session.retain(|s| s.name != name);
        self.session.push(SeshSession {
            name: name.to_string(),
            path: path.to_string(),
            startup_command: None,
        });
    }

    /// Remove a session entry by name
    pub fn remove_session(&mut self, name: &str) -> bool {
        let len_before = self.session.len();
        self.session.retain(|s| s.name != name);
        self.session.len() < len_before
    }

    /// Check if a session exists
    pub fn has_session(&self, name: &str) -> bool {
        self.session.iter().any(|s| s.name == name)
    }

    /// Get a session by name
    pub fn get_session(&self, name: &str) -> Option<&SeshSession> {
        self.session.iter().find(|s| s.name == name)
    }
}

/// Register a worktree with sesh
pub fn register_worktree(project: &str, name: &str, path: &str) -> Result<()> {
    let session_name = format!("{}/{}", project, name);
    let mut config = SeshConfig::load()?;
    config.add_session(&session_name, path);
    config.save().context("Failed to save sesh config")?;
    Ok(())
}

/// Unregister a worktree from sesh
pub fn unregister_worktree(project: &str, name: &str) -> Result<bool> {
    let session_name = format!("{}/{}", project, name);
    let mut config = SeshConfig::load()?;
    let removed = config.remove_session(&session_name);
    if removed {
        config.save().context("Failed to save sesh config")?;
    }
    Ok(removed)
}

/// Get the session name for a worktree
pub fn session_name(project: &str, name: &str) -> String {
    format!("{}/{}", project, name)
}
