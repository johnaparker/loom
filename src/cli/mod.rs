use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Parser)]
#[command(name = "gwt")]
#[command(about = "Git worktree manager with tmux/sesh integration")]
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

        /// Category for the worktree (dev, review, demo)
        #[arg(short, long, value_enum, default_value = "dev")]
        category: Category,
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

    /// Switch to the main branch tmux session
    #[command(visible_alias = "ma")]
    Main,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum Category {
    Dev,
    Review,
    Demo,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Dev => write!(f, "dev"),
            Category::Review => write!(f, "review"),
            Category::Demo => write!(f, "demo"),
        }
    }
}
