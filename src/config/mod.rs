mod global;
mod project;
mod source;
mod worktree;

pub use global::{ClaudeConfig, DiffviewConfig, GitHubConfig, GlobalConfig, LinearConfig, SyncWorkflow};
pub use project::ProjectConfig;
pub use source::{ConfigSource, FilesystemSource};
pub use worktree::{WorktreeConfig, WorktreeSyncConfig};

#[cfg(test)]
pub use source::MemorySource;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Shared integration override configuration.
///
/// Used by both project and worktree configs to override integration enabled state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrationOverride {
    /// Override the enabled state at this config level
    pub enabled: Option<bool>,
}

/// Claude Code integration override configuration.
///
/// Used by both project and worktree configs to override Claude settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeOverride {
    /// Override sandbox enabled state
    pub sandbox: Option<bool>,
    /// Override auto-allow bash when sandboxed
    pub sandbox_auto_allow_bash: Option<bool>,
}

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

    /// Get the configured categories (filtered, max 4).
    /// Filters out empty strings and takes only the first 4 categories.
    pub fn categories(&self) -> Vec<String> {
        self.global
            .categories
            .iter()
            .filter(|c| !c.is_empty())
            .take(4)
            .cloned()
            .collect()
    }

    /// Validate if a category is in the configured list.
    pub fn validate_category(&self, category: &str) -> bool {
        self.categories().iter().any(|c| c == category)
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

    /// Whether Claude sandbox mode is enabled for new worktrees
    pub fn claude_sandbox(&self) -> bool {
        self.global.claude.sandbox
    }

    /// Whether to auto-approve bash commands when sandboxed
    pub fn claude_sandbox_auto_allow_bash(&self) -> bool {
        self.global.claude.sandbox_auto_allow_bash
    }

    /// Resolve configuration for a specific worktree context.
    ///
    /// This merges all config levels with priority: worktree → project → global → defaults.
    /// If `worktree_path` is provided and a worktree config exists there, it will override
    /// project and global settings.
    pub fn resolve(&self, worktree_path: Option<&Path>) -> Result<ResolvedConfig> {
        // Load worktree config if path provided and exists
        let worktree_config = worktree_path
            .and_then(|p| WorktreeConfig::load(p).ok())
            .flatten();

        // Resolve Linear integration
        let linear_enabled = resolve_integration_enabled(
            worktree_config.as_ref().and_then(|w| w.linear.as_ref()).and_then(|i| i.enabled),
            self.project.as_ref().and_then(|p| p.linear.as_ref()).and_then(|i| i.enabled),
            self.global.linear.enabled,
        );

        // Resolve GitHub integration
        let github_enabled = resolve_integration_enabled(
            worktree_config.as_ref().and_then(|w| w.github.as_ref()).and_then(|i| i.enabled),
            self.project.as_ref().and_then(|p| p.github.as_ref()).and_then(|i| i.enabled),
            self.global.github.enabled,
        );

        // Resolve Diffview integration
        let diffview_enabled = resolve_integration_enabled(
            worktree_config.as_ref().and_then(|w| w.diffview.as_ref()).and_then(|i| i.enabled),
            self.project.as_ref().and_then(|p| p.diffview.as_ref()).and_then(|i| i.enabled),
            self.global.diffview.enabled,
        );

        // Resolve Claude sandbox settings
        let claude_sandbox = resolve_integration_enabled(
            worktree_config.as_ref().and_then(|w| w.claude.as_ref()).and_then(|c| c.sandbox),
            self.project.as_ref().and_then(|p| p.claude.as_ref()).and_then(|c| c.sandbox),
            self.global.claude.sandbox,
        );
        let claude_sandbox_auto_allow_bash = resolve_integration_enabled(
            worktree_config.as_ref().and_then(|w| w.claude.as_ref()).and_then(|c| c.sandbox_auto_allow_bash),
            self.project.as_ref().and_then(|p| p.claude.as_ref()).and_then(|c| c.sandbox_auto_allow_bash),
            self.global.claude.sandbox_auto_allow_bash,
        );

        // Resolve sync patterns: merge global + project, then apply worktree excludes
        let mut sync_patterns = self.sync_patterns();
        if let Some(ref wt_config) = worktree_config {
            // Add worktree-specific patterns
            for pattern in &wt_config.sync.patterns {
                if !sync_patterns.contains(pattern) {
                    sync_patterns.push(pattern.clone());
                }
            }
            // Remove excluded patterns
            sync_patterns.retain(|p| !wt_config.sync.exclude_patterns.contains(p));
        }

        Ok(ResolvedConfig {
            worktree_root: self.worktree_root()?,
            cache_dir: self.cache_dir()?,
            workflow: self.workflow(),
            linear: ResolvedLinearConfig {
                enabled: linear_enabled,
                api_key: self.global.linear.api_key.clone(),
                team_prefix: self.global.linear.team_prefix.clone(),
                auto_update_status: self.global.linear.auto_update_status,
            },
            github: ResolvedGitHubConfig {
                enabled: github_enabled,
            },
            diffview: ResolvedDiffviewConfig {
                enabled: diffview_enabled,
                command: self.global.diffview.command.clone(),
            },
            claude: ResolvedClaudeConfig {
                sandbox: claude_sandbox,
                sandbox_auto_allow_bash: claude_sandbox_auto_allow_bash,
            },
            sync_patterns,
            icons: ResolvedIconsConfig {
                linear: self.global.icons.linear.clone(),
                github: self.global.icons.github.clone(),
                branch: self.global.icons.branch.clone(),
            },
            default_category: self.global.default_category.clone(),
            categories: self.categories(),
        })
    }

    /// Check for migration warnings (e.g., old worktree path still in use)
    pub fn check_migration_warnings(&self) -> Vec<String> {
        let mut warnings = vec![];

        // Check if using new default but old path exists with worktrees
        if self.global.worktree_root == "~/.worktrees" {
            if let Some(home) = dirs::home_dir() {
                let old_path = home.join("worktrees");
                if old_path.exists() && old_path.is_dir() {
                    // Check if it contains any subdirectories (worktrees)
                    if let Ok(entries) = std::fs::read_dir(&old_path) {
                        if entries.filter_map(|e| e.ok()).any(|e| e.path().is_dir()) {
                            warnings.push(format!(
                                "Found worktrees at ~/worktrees but gwt now defaults to ~/.worktrees. \
                                To migrate, run: mv ~/worktrees ~/.worktrees && ln -s ~/.worktrees ~/worktrees"
                            ));
                        }
                    }
                }
            }
        }

        warnings
    }
}

/// Resolve optional value with priority: worktree → project → global
fn resolve_integration_enabled(
    worktree: Option<bool>,
    project: Option<bool>,
    global: bool,
) -> bool {
    worktree.or(project).unwrap_or(global)
}

/// Resolved configuration with all levels merged.
///
/// Priority: worktree → project → global → defaults
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub worktree_root: PathBuf,
    pub cache_dir: PathBuf,
    pub workflow: SyncWorkflow,
    pub linear: ResolvedLinearConfig,
    pub github: ResolvedGitHubConfig,
    pub diffview: ResolvedDiffviewConfig,
    pub claude: ResolvedClaudeConfig,
    pub sync_patterns: Vec<String>,
    pub icons: ResolvedIconsConfig,
    pub default_category: String,
    /// Configured worktree categories (max 4)
    pub categories: Vec<String>,
}

/// Resolved Linear integration configuration
#[derive(Debug, Clone)]
pub struct ResolvedLinearConfig {
    /// Whether Linear integration is enabled (merged from all levels)
    pub enabled: bool,
    /// API key (from global config)
    pub api_key: Option<String>,
    /// Team prefix for issue detection (from global config)
    pub team_prefix: Option<String>,
    /// Auto-update status on gwt new/merge
    pub auto_update_status: bool,
}

/// Resolved GitHub integration configuration
#[derive(Debug, Clone)]
pub struct ResolvedGitHubConfig {
    /// Whether GitHub integration is enabled (merged from all levels)
    pub enabled: bool,
}

/// Resolved Diffview integration configuration
#[derive(Debug, Clone)]
pub struct ResolvedDiffviewConfig {
    /// Whether Diffview integration is enabled (merged from all levels)
    pub enabled: bool,
    /// Command to run nvim (from global config)
    pub command: String,
}

/// Resolved icons configuration
#[derive(Debug, Clone)]
pub struct ResolvedIconsConfig {
    pub linear: Option<String>,
    pub github: Option<String>,
    pub branch: Option<String>,
}

/// Resolved Claude Code integration configuration
#[derive(Debug, Clone)]
pub struct ResolvedClaudeConfig {
    /// Whether sandbox mode is enabled for new worktrees
    pub sandbox: bool,
    /// Whether to auto-approve bash commands when sandboxed
    pub sandbox_auto_allow_bash: bool,
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
            linear: None,
            github: None,
            diffview: None,
            claude: None,
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
                linear: None,
                github: None,
                diffview: None,
                claude: None,
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
        assert_eq!(config.global.worktree_root, "~/.worktrees");
        assert_eq!(config.default_category(), "dev");
        assert!(config.is_push_workflow()); // Push is default
        // Integrations disabled by default
        assert!(!config.global.linear.enabled);
        assert!(!config.global.github.enabled);
        assert!(!config.global.diffview.enabled);
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
                linear: None,
                github: None,
                diffview: None,
                claude: None,
            }),
        );

        // Project workflow should override global
        assert!(config.is_pull_workflow());
        assert_eq!(config.project_name("default"), "test-project");

        // Sync patterns should merge
        let patterns = config.sync_patterns();
        assert!(patterns.contains(&"custom.txt".to_string()));
    }

    #[test]
    fn test_resolve_integration_enabled() {
        // Test the helper function directly
        assert!(!resolve_integration_enabled(None, None, false));
        assert!(resolve_integration_enabled(None, None, true));
        assert!(resolve_integration_enabled(None, Some(true), false));
        assert!(!resolve_integration_enabled(None, Some(false), true));
        assert!(resolve_integration_enabled(Some(true), Some(false), false));
        assert!(!resolve_integration_enabled(Some(false), Some(true), true));
    }

    #[test]
    fn test_resolved_config_defaults() {
        // Test that ResolvedConfig has correct defaults
        let config = Config::from_parts(GlobalConfig::default(), None);
        let resolved = config.resolve(None).unwrap();

        // All integrations disabled by default
        assert!(!resolved.linear.enabled);
        assert!(!resolved.github.enabled);
        assert!(!resolved.diffview.enabled);
        assert_eq!(resolved.diffview.command, "nvim");
    }

    #[test]
    fn test_resolved_config_global_enabled() {
        // Test that global enabled state is propagated
        let mut global = GlobalConfig::default();
        global.linear.enabled = true;
        global.github.enabled = true;
        global.diffview.enabled = true;

        let config = Config::from_parts(global, None);
        let resolved = config.resolve(None).unwrap();

        assert!(resolved.linear.enabled);
        assert!(resolved.github.enabled);
        assert!(resolved.diffview.enabled);
    }

    #[test]
    fn test_resolved_config_project_overrides_global() {
        // Test that project config overrides global
        let mut global = GlobalConfig::default();
        global.linear.enabled = true;
        global.github.enabled = true;

        let project = ProjectConfig {
            project_name: None,
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: Some(IntegrationOverride { enabled: Some(false) }),
            github: None, // No override
            diffview: Some(IntegrationOverride { enabled: Some(true) }),
            claude: None,
        };

        let config = Config::from_parts(global, Some(project));
        let resolved = config.resolve(None).unwrap();

        // Project override should win
        assert!(!resolved.linear.enabled); // Was true globally, disabled by project
        assert!(resolved.github.enabled);  // No project override, uses global (true)
        assert!(resolved.diffview.enabled); // Was false globally, enabled by project
    }

    #[test]
    fn test_resolved_config_worktree_overrides_all() {
        use tempfile::TempDir;
        use worktree::{WorktreeConfig, WorktreeSyncConfig};

        // Create temp worktree directory with config
        let temp_dir = TempDir::new().unwrap();
        let wt_config = WorktreeConfig {
            sync: WorktreeSyncConfig {
                patterns: vec!["worktree-only.txt".to_string()],
                exclude_patterns: vec![".env".to_string()], // Exclude from global patterns
            },
            linear: Some(IntegrationOverride { enabled: Some(true) }), // Override project's false
            github: Some(IntegrationOverride { enabled: Some(false) }), // Override global's true
            diffview: None, // No override - should use project's true
            claude: None,
        };
        wt_config.save(temp_dir.path()).unwrap();

        // Global: linear=false, github=true, diffview=false
        let mut global = GlobalConfig::default();
        global.github.enabled = true;

        // Project: linear=false, github=None (uses global), diffview=true
        let project = ProjectConfig {
            project_name: None,
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: Some(IntegrationOverride { enabled: Some(false) }),
            github: None,
            diffview: Some(IntegrationOverride { enabled: Some(true) }),
            claude: None,
        };

        let config = Config::from_parts(global, Some(project));
        let resolved = config.resolve(Some(temp_dir.path())).unwrap();

        // Worktree overrides should win
        assert!(resolved.linear.enabled); // Worktree true overrides project false
        assert!(!resolved.github.enabled); // Worktree false overrides global true
        assert!(resolved.diffview.enabled); // No worktree override, uses project true

        // Sync patterns: global patterns minus worktree excludes, plus worktree additions
        assert!(resolved.sync_patterns.contains(&"worktree-only.txt".to_string()));
        assert!(!resolved.sync_patterns.contains(&".env".to_string())); // Excluded by worktree
        assert!(resolved.sync_patterns.contains(&".envrc".to_string())); // Global pattern not excluded
    }

    #[test]
    fn test_claude_config_defaults() {
        let config = Config::from_parts(GlobalConfig::default(), None);
        // Claude sandbox disabled by default
        assert!(!config.claude_sandbox());
        // Auto-allow bash enabled by default
        assert!(config.claude_sandbox_auto_allow_bash());
    }

    #[test]
    fn test_resolved_claude_config_defaults() {
        let config = Config::from_parts(GlobalConfig::default(), None);
        let resolved = config.resolve(None).unwrap();

        // Claude sandbox disabled by default
        assert!(!resolved.claude.sandbox);
        // Auto-allow bash enabled by default
        assert!(resolved.claude.sandbox_auto_allow_bash);
    }

    #[test]
    fn test_resolved_claude_config_project_overrides() {
        let mut global = GlobalConfig::default();
        global.claude.sandbox = true;
        global.claude.sandbox_auto_allow_bash = true;

        let project = ProjectConfig {
            project_name: None,
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: Some(ClaudeOverride {
                sandbox: Some(false), // Override global
                sandbox_auto_allow_bash: None, // Use global
            }),
        };

        let config = Config::from_parts(global, Some(project));
        let resolved = config.resolve(None).unwrap();

        // Project override should win for sandbox
        assert!(!resolved.claude.sandbox);
        // No project override, uses global
        assert!(resolved.claude.sandbox_auto_allow_bash);
    }

    #[test]
    fn test_resolved_claude_config_worktree_overrides() {
        use tempfile::TempDir;
        use worktree::{WorktreeConfig, WorktreeSyncConfig};

        // Create temp worktree directory with config
        let temp_dir = TempDir::new().unwrap();
        let wt_config = WorktreeConfig {
            sync: WorktreeSyncConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: Some(ClaudeOverride {
                sandbox: Some(true), // Override project's false
                sandbox_auto_allow_bash: Some(false), // Override global's true
            }),
        };
        wt_config.save(temp_dir.path()).unwrap();

        // Global: sandbox=true, auto_allow=true
        let mut global = GlobalConfig::default();
        global.claude.sandbox = true;
        global.claude.sandbox_auto_allow_bash = true;

        // Project: sandbox=false
        let project = ProjectConfig {
            project_name: None,
            sync: ProjectSyncConfig::default(),
            git: ProjectGitConfig::default(),
            linear: None,
            github: None,
            diffview: None,
            claude: Some(ClaudeOverride {
                sandbox: Some(false),
                sandbox_auto_allow_bash: None,
            }),
        };

        let config = Config::from_parts(global, Some(project));
        let resolved = config.resolve(Some(temp_dir.path())).unwrap();

        // Worktree overrides should win
        assert!(resolved.claude.sandbox); // Worktree true overrides project false
        assert!(!resolved.claude.sandbox_auto_allow_bash); // Worktree false overrides global true
    }
}
