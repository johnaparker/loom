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
    /// Configurable worktree categories (max 4, default: ["dev"])
    #[serde(default = "default_categories")]
    pub categories: Vec<String>,
    /// Cache directory (default: ~/.cache/gwt)
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub linear: LinearConfig,
    #[serde(default)]
    pub github: GitHubConfig,
    #[serde(default)]
    pub diffview: DiffviewConfig,
    /// Workflow mode for automatic git sync behavior
    #[serde(default)]
    pub workflow: SyncWorkflow,
    /// Special character icons for TUI dashboard
    #[serde(default)]
    pub icons: IconsConfig,
    /// Claude Code integration settings
    #[serde(default)]
    pub claude: ClaudeConfig,
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
pub struct LinearConfig {
    /// Whether Linear integration is enabled (default: false - must explicitly enable)
    #[serde(default)]
    pub enabled: bool,
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
            enabled: false,
            api_key: None,
            team_prefix: None,
            auto_update_status: true,
        }
    }
}

/// GitHub integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    /// Whether GitHub integration is enabled (default: false - must explicitly enable)
    #[serde(default)]
    pub enabled: bool,
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// Diffview integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffviewConfig {
    /// Whether Diffview integration is enabled (default: false - must explicitly enable)
    #[serde(default)]
    pub enabled: bool,
    /// Command to open neovim with DiffView (default: "nvim")
    #[serde(default = "default_nvim_command")]
    pub command: String,
}

impl Default for DiffviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: default_nvim_command(),
        }
    }
}

fn default_nvim_command() -> String {
    "nvim".to_string()
}

/// Claude Code integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    /// Enable sandbox mode for new worktrees (default: false)
    #[serde(default)]
    pub sandbox: bool,
    /// Auto-approve sandboxed bash commands (default: true)
    #[serde(default = "default_true")]
    pub sandbox_auto_allow_bash: bool,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            sandbox: false,
            sandbox_auto_allow_bash: true,
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
    "~/.worktrees".to_string()
}

fn default_category() -> String {
    "dev".to_string()
}

fn default_categories() -> Vec<String> {
    vec!["dev".to_string()]
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
            categories: default_categories(),
            cache_dir: default_cache_dir(),
            sync: SyncConfig::default(),
            linear: LinearConfig::default(),
            github: GitHubConfig::default(),
            diffview: DiffviewConfig::default(),
            workflow: SyncWorkflow::default(),
            icons: IconsConfig::default(),
            claude: ClaudeConfig::default(),
        }
    }
}

impl GlobalConfig {
    /// Get the path to the global config file
    /// Prefers ~/.config/gwt/config.toml (XDG-style) if it exists,
    /// otherwise falls back to platform default
    pub fn config_path() -> Result<PathBuf> {
        FilesystemSource.global_config_path()
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
    use crate::config::MemorySource;

    #[test]
    fn test_load_with_source_invalid_toml() {
        let source = MemorySource {
            global_config: Some("invalid { toml [[[".to_string()),
            ..Default::default()
        };
        let result = GlobalConfig::load_with_source(&source);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_config() {
        let config = GlobalConfig::default();
        assert_eq!(config.worktree_root, "~/.worktrees");
        assert_eq!(config.default_category, "dev");
        assert_eq!(config.categories, vec!["dev".to_string()]);
        assert_eq!(config.cache_dir, "~/.cache/gwt");
        assert!(config.sync.patterns.contains(&".env".to_string()));
        assert!(config.sync.patterns.contains(&".envrc".to_string()));
        // Integrations disabled by default
        assert!(!config.linear.enabled);
        assert!(!config.github.enabled);
        assert!(!config.diffview.enabled);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
worktree_root = "/custom/path"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.worktree_root, "/custom/path");
        assert_eq!(config.default_category, "dev"); // default
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
worktree_root = "/custom/path"
default_category = "review"

[sync]
patterns = [".env", "custom-file.txt"]

[linear]
api_key = "test-key"
team_prefix = "ABC"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.worktree_root, "/custom/path");
        assert_eq!(config.default_category, "review");
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

    #[test]
    fn test_parse_integration_configs() {
        let toml_str = r#"
worktree_root = "/custom/path"

[linear]
enabled = true
api_key = "lin_api_123"
team_prefix = "JOH"

[github]
enabled = true

[diffview]
enabled = true
command = "nvim"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert!(config.linear.enabled);
        assert_eq!(config.linear.api_key, Some("lin_api_123".to_string()));
        assert_eq!(config.linear.team_prefix, Some("JOH".to_string()));
        assert!(config.github.enabled);
        assert!(config.diffview.enabled);
        assert_eq!(config.diffview.command, "nvim");
    }

    #[test]
    fn test_integration_defaults_disabled() {
        let toml_str = r#"
worktree_root = "/custom/path"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        // All integrations should be disabled by default
        assert!(!config.linear.enabled);
        assert!(!config.github.enabled);
        assert!(!config.diffview.enabled);
    }

    #[test]
    fn test_parse_categories() {
        let toml_str = r#"
worktree_root = "/custom/path"
categories = ["dev", "feature", "review", "hotfix"]
default_category = "feature"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.categories, vec!["dev", "feature", "review", "hotfix"]);
        assert_eq!(config.default_category, "feature");
    }

    #[test]
    fn test_categories_default() {
        let toml_str = r#"
worktree_root = "/custom/path"
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        // Should default to ["dev"]
        assert_eq!(config.categories, vec!["dev".to_string()]);
    }

    #[test]
    fn test_claude_config_defaults() {
        let config = GlobalConfig::default();
        // Claude sandbox disabled by default
        assert!(!config.claude.sandbox);
        // Auto-allow bash enabled by default
        assert!(config.claude.sandbox_auto_allow_bash);
    }

    #[test]
    fn test_parse_claude_config() {
        let toml_str = r#"
worktree_root = "/custom/path"

[claude]
sandbox = true
sandbox_auto_allow_bash = false
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert!(config.claude.sandbox);
        assert!(!config.claude.sandbox_auto_allow_bash);
    }

    #[test]
    fn test_parse_claude_config_partial() {
        let toml_str = r#"
worktree_root = "/custom/path"

[claude]
sandbox = true
"#;
        let config: GlobalConfig = toml::from_str(toml_str).unwrap();
        assert!(config.claude.sandbox);
        // sandbox_auto_allow_bash should default to true
        assert!(config.claude.sandbox_auto_allow_bash);
    }
}
