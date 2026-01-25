use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// Scope for configuration changes
#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum ConfigScope {
    /// User-wide settings (~/.config/grove/config.toml)
    #[default]
    User,
    /// Project-specific settings (.grove.toml in repo root)
    Project,
}

/// Workflow mode for git synchronization
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum WorkflowArg {
    /// Local-first workflow: auto-push after merge
    Push,
    /// Team/PR workflow: auto-fetch before new, auto-pull when behind
    Pull,
}

/// Features that can be enabled/disabled
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ConfigFeature {
    /// Claude Code integration (hooks for state tracking)
    Claude,
    /// GitHub integration (PR status, checks)
    Github,
    /// Linear integration (issue linking)
    Linear,
}

/// Config subcommands for enabling/disabling features
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Enable an integration
    Enable {
        /// The feature to enable
        feature: ConfigFeature,
        /// Configuration scope
        #[arg(long, short, default_value = "user")]
        scope: ConfigScope,
    },
    /// Disable an integration
    Disable {
        /// The feature to disable
        feature: ConfigFeature,
        /// Configuration scope
        #[arg(long, short, default_value = "user")]
        scope: ConfigScope,
    },
    /// Set git workflow mode (push or pull)
    SetWorkflow {
        /// Workflow mode
        #[arg(value_enum)]
        workflow: WorkflowArg,
        /// Configuration scope
        #[arg(long, short, default_value = "user")]
        scope: ConfigScope,
    },
    /// Manage worktree categories interactively
    Categories,
    /// Manage sync patterns interactively
    SyncPatterns {
        /// Configuration scope
        #[arg(long, short, default_value = "user")]
        scope: ConfigScope,
    },
}

#[derive(Parser)]
#[command(name = "grove")]
#[command(about = "Git worktree manager with tmux integration")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new worktree
    #[command(visible_alias = "n")]
    New {
        /// Branch name for the new worktree
        branch: String,

        /// Category for the worktree (uses configured categories, default from config)
        #[arg(short, long)]
        category: Option<String>,
    },

    /// List all worktrees for the current project
    #[command(visible_aliases = ["l", "ls"])]
    List,

    /// Interactive fuzzy picker to switch worktrees
    #[command(visible_alias = "s")]
    Switch {
        /// Optional name to fuzzy match (skips TUI if provided)
        name: Option<String>,
    },

    /// Merge worktree branch to main and cleanup
    #[command(visible_alias = "m")]
    Merge {
        /// Name of the worktree to merge
        name: String,

        /// Force removal even if there are uncommitted changes
        #[arg(short, long)]
        force: bool,

        /// Preview what would happen without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove a worktree
    #[command(visible_aliases = ["r", "rm"])]
    Remove {
        /// Name of the worktree to remove (fuzzy picker if omitted)
        name: Option<String>,

        /// Force removal even if there are uncommitted changes
        #[arg(short, long)]
        force: bool,

        /// Preview what would happen without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Sync worktree with main (merge origin/main into current branch)
    #[command(visible_alias = "sy")]
    Sync {
        /// Name of the worktree to sync (current worktree if omitted)
        name: Option<String>,

        /// Preview what would happen without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Review worktree changes vs main in neovim Diffview
    #[command(visible_alias = "rev")]
    Review,

    /// Switch to the main branch tmux session
    #[command(visible_alias = "ma")]
    Main,

    /// Open the Linear issue for the current worktree
    #[command(visible_alias = "li")]
    Linear,

    /// Open the GitHub PR for the current worktree (or create-PR page)
    #[command(visible_alias = "gh")]
    GitHub,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Handle Claude Code hook events (internal use)
    #[command(hide = true)]
    Hook {
        /// Event type: user-prompt, stop, notification, session-start, session-end, tool-use, tool-result
        event: String,
    },

    /// Prune worktrees with merged PRs (pull workflow only)
    #[command(visible_alias = "p")]
    Prune {
        /// Force removal even if there are uncommitted changes
        #[arg(short, long)]
        force: bool,

        /// Preview what would happen without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Configure grove integrations
    #[command(visible_alias = "cfg")]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}
