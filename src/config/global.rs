use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Global configuration stored at ~/.config/gwt/config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_worktree_root")]
    pub worktree_root: String,
    #[serde(default = "default_category")]
    pub default_category: String,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub sesh: SeshConfig,
    #[serde(default)]
    pub linear: LinearConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default = "default_sync_patterns")]
    pub patterns: Vec<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            patterns: default_sync_patterns(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeshConfig {
    #[serde(default = "default_true")]
    pub auto_register: bool,
}

impl Default for SeshConfig {
    fn default() -> Self {
        Self { auto_register: true }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinearConfig {
    /// API key for Linear API access (optional)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Team prefix to detect Linear issue patterns (e.g., "ABC")
    #[serde(default)]
    pub team_prefix: Option<String>,
}

fn default_worktree_root() -> String {
    "~/worktrees".to_string()
}

fn default_category() -> String {
    "dev".to_string()
}

fn default_sync_patterns() -> Vec<String> {
    vec![
        ".env".to_string(),
        ".envrc".to_string(),
        ".claude/".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            worktree_root: default_worktree_root(),
            default_category: default_category(),
            sync: SyncConfig::default(),
            sesh: SeshConfig::default(),
            linear: LinearConfig::default(),
        }
    }
}

impl GlobalConfig {
    /// Get the path to the global config file
    /// Prefers ~/.config/gwt/config.toml (XDG-style) if it exists,
    /// otherwise falls back to platform default
    pub fn config_path() -> Result<PathBuf> {
        // Prefer XDG-style path (~/.config/gwt/config.toml)
        if let Some(home) = dirs::home_dir() {
            let xdg_path = home.join(".config").join("gwt").join("config.toml");
            if xdg_path.exists() {
                return Ok(xdg_path);
            }
        }

        // Fall back to platform default
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(config_dir.join("gwt").join("config.toml"))
    }

    /// Load global config from disk, or return defaults if not found
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: GlobalConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save the config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
