//! Data loading for the dashboard.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;

use crate::claude::{self, ClaudeSession, ClaudeState};
use crate::git::WorktreeStats;
use crate::github::{self, GitHubPR};
use crate::linear::{self, LinearIssue};

/// Result of async GitHub PR fetch
pub type GitHubPRResult = (String, Option<GitHubPR>); // (worktree_name, pr)

/// Result of async Linear issue fetch
pub type LinearIssueResult = (String, Option<LinearIssue>); // (worktree_name, issue)

/// Load Linear issues from cache for all worktrees
pub fn load_linear_issues(
    cache_dir: &Path,
    project_name: &str,
    worktrees: &[WorktreeStats],
) -> HashMap<String, LinearIssue> {
    let mut issues = HashMap::new();
    for wt in worktrees {
        if let Ok(Some(issue)) = linear::read_metadata(cache_dir, project_name, &wt.info.name) {
            if !issue.title.is_empty() {
                issues.insert(wt.info.name.clone(), issue);
            }
        }
    }
    issues
}

/// Load GitHub PRs from cache only (lazy loading - API fetch happens when panel is viewed)
pub fn load_github_prs_from_cache(
    cache_dir: &Path,
    project_name: &str,
    worktrees: &[WorktreeStats],
) -> HashMap<String, GitHubPR> {
    let mut prs = HashMap::new();
    for wt in worktrees {
        if let Ok(Some(pr)) = github::read_pr_cache(cache_dir, project_name, &wt.info.name) {
            prs.insert(wt.info.name.clone(), pr);
        }
    }
    prs
}

/// Spawn async fetch of GitHub PR for a worktree (non-blocking)
pub fn fetch_github_pr_async(
    cache_dir: PathBuf,
    worktree_name: String,
    branch: String,
    repo_root: PathBuf,
    project_name: String,
    sender: Sender<GitHubPRResult>,
) {
    thread::spawn(move || {
        let pr = github::get_pr_for_branch(&repo_root, &branch).ok().flatten();

        // Cache the result
        if let Some(ref pr) = pr {
            let _ = github::write_pr_cache(&cache_dir, &project_name, &worktree_name, pr);
        } else {
            let _ = github::delete_pr_cache(&cache_dir, &project_name, &worktree_name);
        }

        // Send result back (ignore error if receiver is gone)
        let _ = sender.send((worktree_name, pr));
    });
}

/// Spawn async fetch of Linear issue for a worktree (non-blocking)
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
            // Only include non-stale sessions (or explicitly inactive ones)
            if !session.is_stale() || session.state == ClaudeState::Inactive {
                states.insert(wt.info.name.clone(), session);
            }
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
