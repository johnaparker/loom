//! Dashboard state types and enums.

use crate::git::WorktreeStats;
use crate::tui::modals::{ActionResultModal, DeleteConfirmModal, MergeConfirmModal, NewWorktreeModal};

/// Result of the dashboard interaction
#[derive(Debug, Clone)]
pub enum DashboardResult {
    /// Switch to a worktree
    SwitchTo(WorktreeStats),
    /// Quit the dashboard
    Quit,
    /// Delete a worktree
    Delete {
        worktree: WorktreeStats,
        delete_branch: bool,
        force: bool,
    },
    /// Merge a worktree to main
    Merge {
        worktree: WorktreeStats,
        delete_branch: bool,
    },
    /// Sync a worktree with its remote tracking branch (contextual push/pull)
    SyncWithRemote { worktree: WorktreeStats },
    /// Review a worktree's changes vs main
    Review { worktree: WorktreeStats },
    /// Open Claude in the worktree
    Claude { worktree: WorktreeStats },
    /// Create a new worktree
    CreateNew { branch: String, category: String, auto_claude: bool, plan_mode: bool },
    /// Open Linear issue for a worktree
    Linear { worktree: WorktreeStats },
    /// Open GitHub PR for a worktree
    GitHub { worktree: WorktreeStats },
    /// Refresh the dashboard (after an action)
    Refresh,
}

/// Dashboard mode
pub enum DashboardMode {
    /// Normal navigation mode
    Normal,
    /// Search/filter mode
    Search,
    /// Showing delete confirmation modal
    ConfirmDelete(DeleteConfirmModal),
    /// Showing merge confirmation modal
    ConfirmMerge(MergeConfirmModal),
    /// Showing new worktree modal
    NewWorktree(NewWorktreeModal),
    /// Showing action result
    ActionResult(ActionResultModal),
}
