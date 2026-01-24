//! Data loading for the dashboard.
//!
//! # Thread Safety: Background Thread Functions
//!
//! Functions in this module spawn background threads for I/O operations that
//! would otherwise block the TUI event loop. They follow a consistent pattern:
//!
//! 1. Accept a `Sender<ResultType>` channel for returning results
//! 2. Spawn a `std::thread` to perform the blocking operation
//! 3. Send results back through the channel when complete
//!
//! The TUI event loop polls these channels using `try_recv()` (non-blocking)
//! to check for completed operations.
//!
//! ## Usage from TUI
//!
//! ```ignore
//! // Start async operation
//! fetch_github_pr_async(params, self.sender.clone());
//!
//! // Later, in event loop, poll for results (non-blocking)
//! while let Ok(result) = self.receiver.try_recv() {
//!     // Process completed result
//! }
//! ```
//!
//! ## Sync Helper Functions
//!
//! Some functions like `load_linear_issues()` are synchronous and read from
//! cache files. These are safe to call from background threads (via
//! `load_all_caches_async`) but should NOT be called directly from the event loop
//! as they perform file I/O.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;

use crate::claude::{self, ClaudeSession, ClaudeState};
use crate::git::{WorktreeManager, WorktreeStats};
use crate::github::{self, CachedPRState};
use crate::linear::{self, LinearIssue};

/// Result of async GitHub PR fetch
pub type GitHubPRResult = (String, CachedPRState); // (worktree_name, cached_state)

/// Result of async Linear issue fetch
pub type LinearIssueResult = (String, Option<LinearIssue>); // (worktree_name, issue)

/// Result of async git fetch
pub type GitFetchResult = Result<(), String>;

/// Result of async worktree stats loading
pub type WorktreeStatsResult = Result<Vec<WorktreeStats>, String>;

/// Result of async batch cache loading
/// Contains (linear_issues, github_prs, claude_states, has_active_claude)
pub type CacheLoadResult = (
    HashMap<String, LinearIssue>,
    HashMap<String, CachedPRState>,
    HashMap<String, ClaudeSession>,
    bool,
);

/// Load Linear issues from cache for all worktrees
pub fn load_linear_issues(
    cache_dir: &Path,
    project_name: &str,
    worktrees: &[WorktreeStats],
) -> HashMap<String, LinearIssue> {
    let mut issues = HashMap::new();
    for wt in worktrees {
        if let Ok(Some(issue)) = linear::read_metadata(cache_dir, project_name, &wt.info.name)
            && !issue.title.is_empty()
        {
            issues.insert(wt.info.name.clone(), issue);
        }
    }
    issues
}

/// Load GitHub PR states from cache only (lazy loading - API fetch happens when panel is viewed)
pub fn load_github_prs_from_cache(
    cache_dir: &Path,
    project_name: &str,
    worktrees: &[WorktreeStats],
) -> HashMap<String, CachedPRState> {
    let mut prs = HashMap::new();
    for wt in worktrees {
        if let Ok(Some(state)) = github::read_pr_cache(cache_dir, project_name, &wt.info.name) {
            prs.insert(wt.info.name.clone(), state);
        }
    }
    prs
}

/// Spawn async fetch of GitHub PR for a worktree.
///
/// # Thread Safety: Spawns Background Thread
///
/// This function spawns a `std::thread` to perform the HTTP request. The result
/// is sent back through the provided `sender` channel. Call from the TUI event
/// loop - it returns immediately without blocking.
pub fn fetch_github_pr_async(
    cache_dir: PathBuf,
    worktree_name: String,
    branch: String,
    repo_root: PathBuf,
    project_name: String,
    sender: Sender<GitHubPRResult>,
) {
    thread::spawn(move || {
        let pr = github::get_pr_for_branch(&repo_root, &branch)
            .ok()
            .flatten();

        // Convert to cached state
        let state = match pr {
            Some(pr) => CachedPRState::Found(pr),
            None => CachedPRState::NotFound,
        };

        // Cache the result (both Found and NotFound)
        let _ = github::write_pr_cache(&cache_dir, &project_name, &worktree_name, &state);

        // Send result back (ignore error if receiver is gone)
        let _ = sender.send((worktree_name, state));
    });
}

/// Spawn async fetch of Linear issue for a worktree.
///
/// # Thread Safety: Spawns Background Thread
///
/// This function spawns a `std::thread` to perform the HTTP request via
/// `linear::get_issue()` (which uses `reqwest::blocking`). The result is sent
/// back through the provided `sender` channel. Call from the TUI event loop -
/// it returns immediately without blocking.
pub fn fetch_linear_issue_async(
    cache_dir: PathBuf,
    worktree_name: String,
    issue_id: String,
    api_key: String,
    project_name: String,
    sender: Sender<LinearIssueResult>,
) {
    thread::spawn(move || {
        let issue = linear::get_issue(&api_key, &issue_id).ok();

        // Cache the result on success
        if let Some(ref issue) = issue {
            let _ = linear::write_metadata(&cache_dir, &project_name, &worktree_name, issue);
        }

        // Send result back (ignore error if receiver is gone)
        let _ = sender.send((worktree_name, issue));
    });
}

/// Refresh Claude session states for all worktrees
pub fn load_claude_states(
    cache_dir: &Path,
    project_name: &str,
    worktrees: &[WorktreeStats],
) -> (HashMap<String, ClaudeSession>, bool) {
    let mut states = HashMap::new();
    for wt in worktrees {
        if let Some(session) = claude::read_state(cache_dir, project_name, &wt.info.name) {
            // Always load sessions - effective_state() handles showing stale ones as Inactive
            states.insert(wt.info.name.clone(), session);
        }
    }

    // Track if any worktree has an active animation state
    let has_active = states.values().any(|s| {
        matches!(
            claude::effective_state(s),
            ClaudeState::Working | ClaudeState::WaitingPermission
        )
    });

    (states, has_active)
}

/// Spawn async git fetch for origin.
///
/// # Thread Safety: Spawns Background Thread
///
/// This function spawns a `std::thread` to run `git fetch origin`. The result
/// is sent back through the provided `sender` channel. Call from the TUI event
/// loop - it returns immediately without blocking.
pub fn fetch_origin_async(repo_root: PathBuf, sender: Sender<GitFetchResult>) {
    thread::spawn(move || {
        let result = std::process::Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(&repo_root)
            .output();

        let fetch_result = match result {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(String::from_utf8_lossy(&output.stderr).to_string()),
            Err(e) => Err(e.to_string()),
        };
        let _ = sender.send(fetch_result);
    });
}

/// Spawn async worktree stats loading.
///
/// # Thread Safety: Spawns Background Thread
///
/// This function spawns a `std::thread` to load worktree statistics. The result
/// is sent back through the provided `sender` channel. Call from the TUI event
/// loop - it returns immediately without blocking.
pub fn load_worktree_stats_async(repo_root: PathBuf, sender: Sender<WorktreeStatsResult>) {
    thread::spawn(move || {
        let result = WorktreeManager::open(&repo_root)
            .and_then(|m| m.list_worktrees_with_stats())
            .map_err(|e| e.to_string());
        let _ = sender.send(result);
    });
}

/// Spawn async batch cache loading for all caches.
///
/// # Thread Safety: Spawns Background Thread
///
/// This function spawns a `std::thread` to load Linear, GitHub, and Claude
/// caches from disk. The result is sent back through the provided `sender`
/// channel. Call from the TUI event loop - it returns immediately without
/// blocking.
pub fn load_all_caches_async(
    cache_dir: PathBuf,
    project_name: String,
    worktrees: Vec<WorktreeStats>,
    sender: Sender<CacheLoadResult>,
) {
    thread::spawn(move || {
        // Load all three caches
        let linear_issues = load_linear_issues(&cache_dir, &project_name, &worktrees);
        let github_prs = load_github_prs_from_cache(&cache_dir, &project_name, &worktrees);
        let (claude_states, has_active_claude) =
            load_claude_states(&cache_dir, &project_name, &worktrees);

        let _ = sender.send((linear_issues, github_prs, claude_states, has_active_claude));
    });
}
