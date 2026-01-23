use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::source::{ConfigSource, FilesystemSource};

/// Global configuration stored at ~/.config/gwt/config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_worktree_root")]
    pub worktree_root: String,
    #[serde(default = "default_category")]
    pub default_category: String,
    /// Cache directory (default: ~/.cache/gwt)
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub sesh: SeshConfig,
    #[serde(default)]
    pub linear: LinearConfig,
    /// Workflow mode for automatic git sync behavior
    #[serde(default)]
    pub workflow: SyncWorkflow,
    /// Special character icons for TUI dashboard
    #[serde(default)]
    pub icons: IconsConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearConfig {
    /// API key for Linear API access (optional)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Team prefix to detect Linear issue patterns (e.g., "ABC")
    #[serde(default)]
    pub team_prefix: Option<String>,
    /// Automatically update Linear issue status on gwt new/merge (default: true)
    #[serde(default = "default_true")]
    pub auto_update_status: bool,
}

impl Default for LinearConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            team_prefix: None,
            auto_update_status: true,
        }
    }
}

/// Configuration for special character icons in TUI dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconsConfig {
    /// Icon displayed before Linear issue title
    #[serde(default)]
    pub linear: Option<String>,
    /// Icon displayed before GitHub PR info
    #[serde(default)]
    pub github: Option<String>,
    /// Icon displayed before branch name
    #[serde(default)]
    pub branch: Option<String>,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self {
            linear: None,
            github: None,
            branch: None,
        }
    }
}

/// Workflow mode for git synchronization.
///
/// Determines automatic sync behavior based on how you work:
/// - `push`: Local-first workflow. After merging to main, auto-push. Before `gwt new`, auto-push main if ahead.
/// - `pull`: Team/PR-based workflow. Auto-pull main on switch, auto-pull branches when behind tracking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncWorkflow {
    /// Local-first workflow: auto-push after merge, push main before new if ahead
    #[default]
    Push,
    /// Team/PR workflow: auto-fetch before new, auto-pull when behind
    Pull,
}

fn default_worktree_root() -> String {
    "~/worktrees".to_string()
}

fn default_category() -> String {
    "dev".to_string()
}

fn default_cache_dir() -> String {
    "~/.cache/gwt".to_string()
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
            cache_dir: default_cache_dir(),
            sync: SyncConfig::default(),
            sesh: SeshConfig::default(),
            linear: LinearConfig::default(),
            workflow: SyncWorkflow::default(),
            icons: IconsConfig::default(),
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
        Self::load_with_source(&FilesystemSource)
    }

    /// Load global config using a custom source (for testing).
    pub fn load_with_source<S: ConfigSource>(source: &S) -> Result<Self> {
        let path = source.global_config_path()?;
        if let Some(content) = source.read_config(&path)? {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GlobalConfig::default();
        assert_eq!(config.worktree_root, "~/worktrees");
        assert_eq!(config.default_category, "dev");
        assert_eq!(config.cache_dir, "~/.cache/gwt");
        assert!(config.sesh.auto_register);
        assert!(config.sync.patterns.contains(&".env".to_string()));
        assert!(config.sync.patterns.contains(&".envrc".to_string()));
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
worktree_root = "/custom/path"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.worktree_root, "/custom/path");
        assert_eq!(config.default_category, "dev"); // default
        assert!(config.sesh.auto_register); // default
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
worktree_root = "/custom/path"
default_category = "review"

[sync]
patterns = [".env", "custom-file.txt"]

[sesh]
auto_register = false

[linear]
api_key = "test-key"
team_prefix = "ABC"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.worktree_root, "/custom/path");
        assert_eq!(config.default_category, "review");
        assert!(!config.sesh.auto_register);
        assert_eq!(config.sync.patterns.len(), 2);
        assert_eq!(config.linear.api_key, Some("test-key".to_string()));
        assert_eq!(config.linear.team_prefix, Some("ABC".to_string()));
    }

    #[test]
    fn test_serialize_config() {
        let config = GlobalConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("worktree_root"));
        assert!(serialized.contains("default_category"));

        // Should be able to round-trip
        let deserialized: GlobalConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.worktree_root, config.worktree_root);
    }

    #[test]
    fn test_parse_icons_config() {
        let toml_str = r#"
worktree_root = "/custom/path"

[icons]
linear = ""
github = ""
branch = ""
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.icons.linear, Some("".to_string()));
        assert_eq!(config.icons.github, Some("".to_string()));
        assert_eq!(config.icons.branch, Some("".to_string()));
    }

    #[test]
    fn test_icons_default_none() {
        let config = GlobalConfig::default();
        assert!(config.icons.linear.is_none());
        assert!(config.icons.github.is_none());
        assert!(config.icons.branch.is_none());
    }

    #[test]
    fn test_icons_partial_config() {
        let toml_str = r#"
worktree_root = "/custom/path"

[icons]
linear = "🎫"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.icons.linear, Some("🎫".to_string()));
        assert!(config.icons.github.is_none());
        assert!(config.icons.branch.is_none());
    }
}
