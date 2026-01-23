//! Shared worktree operation logic.
//!
//! This module contains the core logic for worktree operations that is shared
//! between CLI commands and the TUI dashboard. Functions here focus on the
//! operations themselves without output formatting.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::Category;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::{PushResult, WorktreeManager, WorktreeStats};
use crate::linear::{self, LinearIssue};
use crate::sesh;
use crate::sync;
use crate::tmux;

/// Result of a sync operation with remote.
#[derive(Debug)]
pub enum SyncResult {
    /// Worktree is already synced with remote
    AlreadySynced { worktree_name: String },
    /// Pushed commits to remote
    Pushed { worktree_name: String, commits: u32 },
    /// Created a new remote branch and pushed
    CreatedRemoteBranch { worktree_name: String },
    /// Pulled commits from remote
    Pulled { worktree_name: String, commits: u32 },
    /// Cannot sync due to some condition
    CannotSync { reason: String },
}

impl SyncResult {
    /// Convert the result to a human-readable message
    pub fn message(&self) -> String {
        match self {
            SyncResult::AlreadySynced { worktree_name } => {
                format!("'{}' already synced with remote", worktree_name)
            }
            SyncResult::Pushed { worktree_name, commits } => {
                format!("Pushed {} commit(s) for '{}'", commits, worktree_name)
            }
            SyncResult::CreatedRemoteBranch { worktree_name } => {
                format!("Created remote branch and pushed '{}'", worktree_name)
            }
            SyncResult::Pulled { worktree_name, commits } => {
                format!("Pulled {} commit(s) for '{}'", commits, worktree_name)
            }
            SyncResult::CannotSync { reason } => reason.clone(),
        }
    }

    /// Whether this result represents success
    pub fn is_success(&self) -> bool {
        !matches!(self, SyncResult::CannotSync { .. })
    }
}

/// Sync a worktree with its remote tracking branch.
///
/// This encapsulates the decision tree for syncing:
/// - If uncommitted changes present: cannot sync
/// - If both ahead and behind: cannot sync (needs manual resolution)
/// - If no tracking branch: push with -u to create it
/// - If behind: pull
/// - If ahead: push
/// - If synced: report already synced
pub fn sync_with_remote(
    manager: &WorktreeManager,
    worktree: &WorktreeStats,
) -> SyncResult {
    let branch = worktree.info.branch.as_deref().unwrap_or(&worktree.info.name);
    let worktree_name = worktree.info.name.clone();

    // Check for uncommitted changes first
    if worktree.has_uncommitted_changes() {
        return SyncResult::CannotSync {
            reason: "Cannot sync: uncommitted changes present".to_string(),
        };
    }

    let ahead = worktree.tracking_ahead.unwrap_or(0);
    let behind = worktree.tracking_behind.unwrap_or(0);
    let has_tracking = worktree.tracking_ahead.is_some();

    // Both ahead and behind - needs manual resolution
    if ahead > 0 && behind > 0 {
        return SyncResult::CannotSync {
            reason: format!(
                "Cannot sync: ↑{} ahead, ↓{} behind. Resolve manually",
                ahead, behind
            ),
        };
    }

    // No tracking branch - push with -u to create it
    if !has_tracking {
        match manager.push_to_remote(&worktree.info.path, branch) {
            Ok(PushResult::CreatedRemoteBranch) => {
                return SyncResult::CreatedRemoteBranch { worktree_name };
            }
            Ok(PushResult::Success) => {
                return SyncResult::Pushed {
                    worktree_name,
                    commits: ahead,
                };
            }
            Ok(PushResult::Rejected { reason: _ }) => {
                return SyncResult::CannotSync {
                    reason: "Push rejected. Pull first with 'P'".to_string(),
                };
            }
            Err(e) => {
                return SyncResult::CannotSync {
                    reason: format!("Failed to push: {}", e),
                };
            }
        }
    }

    // Behind tracking - pull
    if behind > 0 {
        match manager.pull_from_remote(&worktree.info.path, branch) {
            Ok(()) => {
                return SyncResult::Pulled {
                    worktree_name,
                    commits: behind,
                };
            }
            Err(e) => {
                return SyncResult::CannotSync {
                    reason: format!("Failed to pull: {}", e),
                };
            }
        }
    }

    // Ahead of tracking - push
    if ahead > 0 {
        match manager.push_to_remote(&worktree.info.path, branch) {
            Ok(PushResult::Success) | Ok(PushResult::CreatedRemoteBranch) => {
                return SyncResult::Pushed {
                    worktree_name,
                    commits: ahead,
                };
            }
            Ok(PushResult::Rejected { reason: _ }) => {
                return SyncResult::CannotSync {
                    reason: "Push rejected. Pull first with 'P'".to_string(),
                };
            }
            Err(e) => {
                return SyncResult::CannotSync {
                    reason: format!("Failed to push: {}", e),
                };
            }
        }
    }

    // Already synced
    SyncResult::AlreadySynced { worktree_name }
}

/// Result of a merge operation.
#[derive(Debug)]
pub enum MergeResult {
    /// Merge succeeded, cleanup completed
    Success(CleanupResult),
    /// Cannot merge due to conflicts
    Conflicts { files: Vec<String> },
}

/// Clean up all resources associated with a worktree.
///
/// This handles:
/// - Unregistering from sesh
/// - Deleting Linear metadata cache
/// - Killing the tmux session if it exists (LAST - may terminate current process)
///
/// Call this AFTER removing the worktree itself.
/// IMPORTANT: tmux kill is done last because if running from within the tmux session
/// being killed, the process will terminate and nothing after will execute.
pub fn cleanup_worktree_resources(cache_dir: &Path, project_name: &str, worktree_name: &str) -> Result<CleanupResult> {
    let session_name = sesh::session_name(project_name, worktree_name);

    // Do these BEFORE killing tmux - tmux kill may terminate this process
    let sesh_unregistered = sesh::unregister_worktree(project_name, worktree_name)?;
    let linear_deleted = linear::delete_metadata(cache_dir, project_name, worktree_name).is_ok();

    // Kill tmux LAST - this may kill the current process if running from within the session
    let tmux_killed = tmux::kill_session(&session_name);

    Ok(CleanupResult {
        tmux_killed,
        sesh_unregistered,
        linear_deleted,
        linear_updated: false,
    })
}

/// Result of cleanup operations for reporting.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CleanupResult {
    pub tmux_killed: bool,
    pub sesh_unregistered: bool,
    pub linear_deleted: bool,
    /// Whether Linear issue status was updated (e.g., to "Done" on merge)
    pub linear_updated: bool,
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
/// Checks for conflicts first and returns `MergeResult::Conflicts` if any exist.
///
/// Linear status is updated BEFORE cleanup because cleanup kills the tmux session,
/// which may terminate the process if running from within that session.
pub fn merge_worktree_to_main(
    manager: &WorktreeManager,
    cache_dir: &Path,
    project_name: &str,
    worktree_name: &str,
    worktree_path: &Path,
    branch: &str,
    delete_branch: bool,
    linear_issue: Option<&LinearIssue>,
    linear_api_key: Option<&str>,
    linear_auto_update: bool,
) -> Result<MergeResult> {
    // Check for conflicts first
    match manager.check_merge_conflicts(branch) {
        Ok(Some(conflicts)) => {
            return Ok(MergeResult::Conflicts { files: conflicts });
        }
        Ok(None) => {
            // No conflicts, proceed with merge
        }
        Err(e) => {
            // Error checking conflicts, warn but allow merge attempt
            eprintln!("Warning: Could not check for conflicts: {}", e);
        }
    }

    // Merge to main
    manager.merge_to_main(branch)?;

    // Remove the worktree
    manager.remove_worktree(worktree_path, false)?;

    // Delete branch if requested (before cleanup in case we get killed)
    if delete_branch {
        manager.delete_branch(branch, true)?;
    }

    // Update Linear issue to "Done" BEFORE cleanup (cleanup may kill our process)
    let linear_updated = if linear_auto_update {
        linear_api_key.and_then(|api_key| {
            linear_issue.and_then(|issue| {
                linear::update_issue_status(api_key, &issue.id, "completed").ok()
            })
        }).is_some()
    } else {
        false
    };

    // Clean up associated resources (tmux kill is LAST and may terminate this process)
    let mut result = cleanup_worktree_resources(cache_dir, project_name, worktree_name)?;
    result.linear_updated = linear_updated;

    Ok(MergeResult::Success(result))
}

/// Result of worktree creation for reporting.
#[derive(Debug)]
#[allow(dead_code)]
pub struct CreateWorktreeResult {
    /// Name of the worktree directory
    pub worktree_name: String,
    /// Git branch name
    pub git_branch: String,
    /// Full path to the worktree
    pub worktree_path: PathBuf,
    /// Session name for tmux/sesh
    pub session_name: String,
    /// Whether the worktree tracks a remote branch
    pub tracked_remote: bool,
    /// Linear issue if linked
    pub issue: Option<LinearIssue>,
    /// Whether Linear status was updated to "In Progress"
    pub linear_status_updated: bool,
    /// Files that were synced from main
    pub synced_files: Vec<String>,
    /// Whether direnv allow was run
    pub direnv_allowed: bool,
    /// Whether registered with sesh
    pub sesh_registered: bool,
}

/// Create a new worktree with all associated setup.
///
/// This is the complete create operation used by both CLI and TUI.
/// It handles:
/// - Fetching from origin to ensure we have the latest refs
/// - Resolving Linear issue IDs to branch names
/// - Creating the worktree (tracking remote or new branch)
/// - Workflow-aware branch start point (origin/main for pull, local main for push)
/// - Writing Linear metadata cache
/// - Updating Linear issue status to "In Progress"
/// - Syncing files from main (.env, .envrc, .claude/)
/// - Running direnv allow if needed
/// - Registering with sesh
///
/// Callers are responsible for:
/// - Printing output (CLI) or showing results (TUI)
/// - Switching to the tmux session
pub fn create_worktree(
    manager: &WorktreeManager,
    config: &Config,
    project_name: &str,
    branch: &str,
    category: Category,
    cache_dir: &Path,
) -> Result<CreateWorktreeResult> {
    let worktree_root = config.worktree_root()?;

    // Fetch from origin to ensure we have the latest refs
    let _ = manager.fetch_origin(); // Ignore errors - we can still create from local refs

    // Resolve Linear input (issue ID, branch with issue ID, or regular branch)
    let resolved = linear::resolve_input(
        branch,
        config.linear_prefix(),
        config.linear_api_key(),
    )?;

    // Check if worktree already exists
    if manager.get_worktree(&resolved.worktree_name)?.is_some() {
        return Err(GwtError::WorktreeAlreadyExists {
            name: resolved.worktree_name,
        }
        .into());
    }

    // Build worktree path: ~/worktrees/{project}/{category}/{name}
    let worktree_path = worktree_root
        .join(project_name)
        .join(category.to_string())
        .join(&resolved.worktree_name);

    // Check if remote branch exists
    let track_remote = manager.remote_branch_exists(&resolved.git_branch);

    // Create the worktree - either tracking remote or creating new
    // For new branches, use remote start point for pull workflow, local for push workflow
    if track_remote {
        manager.create_worktree_tracking(&resolved.git_branch, &worktree_path)?;
    } else {
        manager.create_worktree(&resolved.git_branch, &worktree_path, config.is_pull_workflow())?;
    }

    // Write Linear metadata and update status if we have issue info
    let mut linear_status_updated = false;
    if let Some(ref issue) = resolved.issue {
        linear::write_metadata(cache_dir, project_name, &resolved.worktree_name, issue)?;

        // Update Linear issue to "In Progress" (non-critical)
        if config.linear_auto_update_status() {
            if let Some(api_key) = config.linear_api_key() {
                if linear::update_issue_status(api_key, &issue.id, "started").is_ok() {
                    linear_status_updated = true;
                }
            }
        }
    }

    // Sync files from main repo
    let patterns = config.sync_patterns();
    let mut synced_files = Vec::new();
    let mut direnv_allowed = false;

    if !patterns.is_empty() {
        synced_files = sync::sync_files(manager.repo_root(), &worktree_path, &patterns)?;

        // Run direnv allow if .envrc was synced
        if synced_files.iter().any(|p| p == ".envrc") {
            if sync::run_direnv_allow(&worktree_path).unwrap_or(false) {
                direnv_allowed = true;
            }
        }
    }

    // Register with sesh if enabled
    let session_name = sesh::session_name(project_name, &resolved.worktree_name);
    let mut sesh_registered = false;

    if config.sesh_auto_register() {
        if sesh::register_worktree(
            project_name,
            &resolved.worktree_name,
            worktree_path.to_str().unwrap(),
        )
        .is_ok()
        {
            sesh_registered = true;
        }
    }

    Ok(CreateWorktreeResult {
        worktree_name: resolved.worktree_name,
        git_branch: resolved.git_branch,
        worktree_path,
        session_name,
        tracked_remote: track_remote,
        issue: resolved.issue,
        linear_status_updated,
        synced_files,
        direnv_allowed,
        sesh_registered,
    })
}
