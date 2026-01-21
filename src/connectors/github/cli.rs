//! GitHub CLI (gh) wrapper functions.

use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::error::GwtError;
use super::types::{ChecksStatus, GitHubPR, PRComment};
use super::url::get_create_pr_url;

/// Check if the gh CLI is installed and authenticated
pub fn check_gh_cli() -> Result<()> {
    // Check if gh is installed
    let output = Command::new("gh")
        .args(["--version"])
        .output()
        .map_err(|_| GwtError::GitHubCliNotFound)?;

    if !output.status.success() {
        return Err(GwtError::GitHubCliNotFound.into());
    }

    // Check if gh is authenticated
    let output = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map_err(|_| GwtError::GitHubCliNotFound)?;

    if !output.status.success() {
        return Err(GwtError::GitHubNotAuthenticated.into());
    }

    Ok(())
}

/// Get the branch name from a PR number using gh CLI
pub fn get_pr_branch(repo_path: &Path, pr_number: u32) -> Result<String> {
    check_gh_cli()?;

    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_number.to_string(),
            "--json",
            "headRefName",
            "-q",
            ".headRefName",
        ])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GwtError::GitHubApiError {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GwtError::GitHubApiError {
            message: stderr.to_string(),
        }
        .into());
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Err(GwtError::GitHubApiError {
            message: format!("PR #{} not found or has no branch", pr_number),
        }
        .into());
    }

    Ok(branch)
}

/// Check if a remote branch exists
pub fn remote_branch_exists(repo_path: &Path, branch: &str) -> bool {
    let output = Command::new("git")
        .args(["ls-remote", "--heads", "origin", branch])
        .current_dir(repo_path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            !stdout.trim().is_empty()
        }
        _ => false,
    }
}

/// Get PR information for a branch using gh CLI
pub fn get_pr_for_branch(repo_path: &Path, branch: &str) -> Result<Option<GitHubPR>> {
    check_gh_cli()?;

    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "number,title,url,state,isDraft,headRefName,baseRefName,statusCheckRollup,comments,author,assignees,reviewRequests,reviewDecision",
        ])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GwtError::GitHubApiError {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        // PR not found is not an error - branch might not have a PR
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no pull requests found") || stderr.contains("Could not resolve") {
            return Ok(None);
        }
        return Err(GwtError::GitHubApiError {
            message: stderr.to_string(),
        }
        .into());
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        GwtError::GitHubApiError {
            message: format!("Failed to parse gh output: {}", e),
        }
    })?;

    let number = json["number"].as_u64().unwrap_or(0) as u32;
    let title = json["title"].as_str().unwrap_or("").to_string();
    let url = json["url"].as_str().unwrap_or("").to_string();
    let state = json["state"].as_str().unwrap_or("OPEN").to_string();
    let draft = json["isDraft"].as_bool().unwrap_or(false);
    let head_branch = json["headRefName"].as_str().unwrap_or("").to_string();
    let base_branch = json["baseRefName"].as_str().unwrap_or("main").to_string();

    // Parse status checks
    let checks_status = if let Some(checks) = json["statusCheckRollup"].as_array() {
        let mut total = 0u32;
        let mut passing = 0u32;
        let mut failing = 0u32;
        let mut pending = 0u32;
        let mut failing_names = Vec::new();

        for check in checks {
            total += 1;
            // Get check name from either "name" or "context" field
            let name = check["name"]
                .as_str()
                .or_else(|| check["context"].as_str())
                .unwrap_or("unknown")
                .to_string();

            match check["conclusion"].as_str() {
                Some("SUCCESS") => passing += 1,
                Some("FAILURE") | Some("ERROR") | Some("CANCELLED") => {
                    failing += 1;
                    failing_names.push(name);
                }
                // Empty string means in progress, None or "PENDING" also means pending
                Some("PENDING") | Some("") | None => pending += 1,
                _ => {}
            }
        }

        Some(ChecksStatus {
            total,
            passing,
            failing,
            pending,
            failing_names,
        })
    } else {
        None
    };

    // Parse comments (most recent first, limit to 5)
    let comments: Vec<PRComment> = if let Some(comments_arr) = json["comments"].as_array() {
        comments_arr
            .iter()
            .rev()
            .take(5)
            .filter_map(|c| {
                let author = c["author"]["login"].as_str()?;
                let body = c["body"].as_str()?;
                let created_at = c["createdAt"].as_str()?;
                Some(PRComment {
                    author: author.to_string(),
                    body: body.to_string(),
                    created_at: created_at.to_string(),
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    // Parse author
    let author = json["author"]["login"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Parse assignees
    let assignees: Vec<String> = json["assignees"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| a["login"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Parse requested reviewers
    let reviewers: Vec<String> = json["reviewRequests"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r["login"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Parse review decision
    let review_decision = json["reviewDecision"].as_str().map(|s| s.to_string());

    Ok(Some(GitHubPR {
        number,
        title,
        url,
        state,
        draft,
        head_branch,
        base_branch,
        checks_status,
        comments,
        author,
        assignees,
        reviewers,
        review_decision,
    }))
}

/// Get repo info (owner/name) from the current git repo using gh CLI
pub fn get_repo_info(repo_path: &Path) -> Result<(String, String)> {
    check_gh_cli()?;

    let output = Command::new("gh")
        .args(["repo", "view", "--json", "owner,name", "-q", ".owner.login,.name"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GwtError::GitHubApiError {
            message: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(GwtError::NoGitHubRemote.into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('\n').collect();
    if parts.len() != 2 {
        return Err(GwtError::NoGitHubRemote.into());
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Open GitHub PR or create-PR page for a branch
pub fn open_pr_or_create(repo_path: &Path, branch: &str) -> Result<String> {
    // First try to get existing PR
    if let Some(pr) = get_pr_for_branch(repo_path, branch)? {
        open_url(&pr.url)?;
        return Ok(format!("Opening PR #{}: {}", pr.number, pr.title));
    }

    // No PR exists - open create PR page
    let (owner, repo) = get_repo_info(repo_path)?;
    let create_url = get_create_pr_url(&owner, &repo, branch);
    open_url(&create_url)?;
    Ok(format!("Opening create PR page for branch '{}'", branch))
}

/// Open a URL in the default browser
pub fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(anyhow::anyhow!(
            "Cannot open URL: unsupported platform. Please open manually: {}",
            url
        ))
    }
}
