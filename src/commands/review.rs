use anyhow::Result;

use crate::config::Config;
use crate::error::LoomError;
use crate::git::WorktreeManager;
use crate::tmux;

pub fn review() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    // Find worktree from current directory
    let worktrees = manager.list_worktrees()?;
    let worktree = worktrees
        .into_iter()
        .find(|w| current_dir.starts_with(&w.path))
        .ok_or(LoomError::NotInWorktree)?;

    // Can't review main worktree
    if worktree.is_main {
        Err(LoomError::CannotReviewMain)?;
    }

    let session = format!("{}/{}", project_name, &worktree.name);
    let path_str = worktree.path.to_str().unwrap();
    let main_branch = manager
        .main_branch_name()
        .unwrap_or_else(|_| "main".to_string());

    // Use merge-base to show only the worktree's changes since branching
    // This avoids showing changes main has that the worktree doesn't
    // Wrap in bash -c so command substitution is evaluated
    let nvim_command = format!(
        "bash -c 'nvim -c \"DiffviewOpen $(git merge-base {} HEAD)\"'",
        main_branch
    );

    if tmux::session_exists(&session) {
        // Session exists - create new window with diff command
        tmux::create_window(&session, "review", path_str, &nvim_command)?;
    } else {
        // Session doesn't exist - create it with diff as first window
        tmux::create_session_with_command(&session, path_str, &nvim_command)?;
    }

    // Switch to the session
    tmux::switch_to_session(&session, path_str)?;

    Ok(())
}
