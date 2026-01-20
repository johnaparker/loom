//! Shared worktree operation logic.
//!
//! This module contains the core logic for worktree operations that is shared
//! between CLI commands and the TUI dashboard. Functions here focus on the
//! operations themselves without output formatting.

use anyhow::Result;
use std::path::Path;

use crate::git::WorktreeManager;
use crate::linear;
use crate::sesh;
use crate::tmux;

/// Clean up all resources associated with a worktree.
///
/// This handles:
/// - Killing the tmux session if it exists
/// - Unregistering from sesh
/// - Deleting Linear metadata cache
///
/// Call this AFTER removing the worktree itself.
pub fn cleanup_worktree_resources(cache_dir: &Path, project_name: &str, worktree_name: &str) -> Result<CleanupResult> {
    let session_name = sesh::session_name(project_name, worktree_name);

    let tmux_killed = tmux::kill_session(&session_name);
    let sesh_unregistered = sesh::unregister_worktree(project_name, worktree_name)?;
    let linear_deleted = linear::delete_metadata(cache_dir, project_name, worktree_name).is_ok();

    Ok(CleanupResult {
        tmux_killed,
        sesh_unregistered,
        linear_deleted,
    })
}

/// Result of cleanup operations for reporting.
#[derive(Debug, Default)]
pub struct CleanupResult {
    pub tmux_killed: bool,
    pub sesh_unregistered: bool,
    pub linear_deleted: bool,
}

/// Delete a worktree and clean up all associated resources.
///
/// This is the complete delete operation used by both CLI and TUI.
pub fn delete_worktree(
    manager: &WorktreeManager,
    cache_dir: &Path,
    project_name: &str,
    worktree_name: &str,
    worktree_path: &Path,
    branch: Option<&str>,
    delete_branch: bool,
    force: bool,
) -> Result<CleanupResult> {
    // Remove the worktree
    manager.remove_worktree(worktree_path, force)?;

    // Clean up associated resources
    let result = cleanup_worktree_resources(cache_dir, project_name, worktree_name)?;

    // Delete branch if requested
    if delete_branch {
        if let Some(branch) = branch {
            manager.delete_branch(branch, true)?;
        }
    }

    Ok(result)
}

/// Merge a worktree's branch to main and clean up.
///
/// This is the complete merge operation used by both CLI and TUI.
pub fn merge_worktree_to_main(
    manager: &WorktreeManager,
    cache_dir: &Path,
    project_name: &str,
    worktree_name: &str,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
) -> Result<CleanupResult> {
    // Merge to main
    manager.merge_to_main(branch)?;

    // Remove the worktree
    manager.remove_worktree(worktree_path, false)?;

    // Clean up associated resources
    let result = cleanup_worktree_resources(cache_dir, project_name, worktree_name)?;

    // Delete branch if requested
    if delete_branch {
        manager.delete_branch(branch, true)?;
    }

    Ok(result)
}

/// Sync a worktree with main branch.
pub fn sync_worktree_with_main(
    manager: &WorktreeManager,
    worktree_path: &Path,
) -> Result<()> {
    // Fetch from origin first
    let _ = manager.fetch_origin(); // Ignore errors - we can still try sync

    // Get sync source ref
    let source_ref = manager
        .get_sync_source_ref()
        .ok_or_else(|| anyhow::anyhow!("No main branch found to sync from"))?;

    // Perform sync
    manager.sync_branch_with_main(worktree_path, &source_ref)?;

    Ok(())
}
