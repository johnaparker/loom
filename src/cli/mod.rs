use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "gwt")]
#[command(about = "Git worktree manager with tmux/sesh integration")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new worktree
    New {
        /// Branch name for the new worktree
        branch: String,

        /// Category for the worktree (dev, review, demo)
        #[arg(short, long, value_enum, default_value = "dev")]
        category: Category,
    },

    /// List all worktrees for the current project
    List,

    /// Interactive fuzzy picker to switch worktrees
    Switch {
        /// Optional name to fuzzy match (skips TUI if provided)
        name: Option<String>,
    },

    /// Merge worktree branch to main and cleanup
    Merge {
        /// Name of the worktree to merge
        name: String,

        /// Force removal even if there are uncommitted changes
        #[arg(short, long)]
        force: bool,
    },

    /// Remove a worktree
    Remove {
        /// Name of the worktree to remove
        name: String,

        /// Force removal even if there are uncommitted changes
        #[arg(short, long)]
        force: bool,
    },

    /// Show status of all worktrees
    Status,

    /// Switch to the main branch tmux session
    Main,
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
