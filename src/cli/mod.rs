use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "gwt")]
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

    /// Show status of all worktrees
    #[command(visible_alias = "st")]
    Status,

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
}
