use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::GwtError;

/// GitHub PR information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPR {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: String, // "OPEN", "MERGED", "CLOSED"
    pub draft: bool,
    pub head_branch: String,
    pub base_branch: String,
    pub checks_status: Option<ChecksStatus>,
    pub comments: Vec<PRComment>,
}

/// PR checks status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksStatus {
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
}

/// A comment on a PR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Type of GitHub URL
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubUrlType {
    PullRequest(u32),
    Branch(String),
    Tree(String), // branch/path in tree view
}

/// Parsed GitHub URL information
#[derive(Debug, Clone)]
pub struct GitHubUrlInfo {
    pub owner: String,
    pub repo: String,
    pub url_type: GitHubUrlType,
}

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

/// Check if a string looks like a GitHub URL
pub fn is_github_url(input: &str) -> bool {
    input.starts_with("https://github.com/") || input.starts_with("http://github.com/")
}

/// Parse a GitHub URL to extract owner, repo, and URL type
/// Supports:
/// - https://github.com/owner/repo/pull/123
/// - https://github.com/owner/repo/tree/branch-name
/// - https://github.com/owner/repo/tree/branch-name/path/to/file
pub fn parse_github_url(url: &str) -> Option<GitHubUrlInfo> {
    let url = url.trim();

    // Remove protocol
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 {
        return None;
    }

    let owner = parts[0].to_string();
    let repo = parts[1].to_string();

    let url_type = if parts.len() >= 4 {
        match parts[2] {
            "pull" => {
                let pr_num: u32 = parts[3].parse().ok()?;
                GitHubUrlType::PullRequest(pr_num)
            }
            "tree" => {
                // For now, take the first segment as the branch
                // (Branch names with slashes would need more context to parse correctly)
                GitHubUrlType::Tree(parts[3].to_string())
            }
            _ => return None,
        }
    } else {
        return None;
    };

    Some(GitHubUrlInfo {
        owner,
        repo,
        url_type,
    })
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
            "number,title,url,state,isDraft,headRefName,baseRefName,statusCheckRollup,comments",
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

        for check in checks {
            total += 1;
            match check["conclusion"].as_str() {
                Some("SUCCESS") => passing += 1,
                Some("FAILURE") | Some("ERROR") | Some("CANCELLED") => failing += 1,
                Some("PENDING") | None => pending += 1,
                _ => {}
            }
        }

        Some(ChecksStatus {
            total,
            passing,
            failing,
            pending,
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
    }))
}

/// Get the URL to create a new PR for a branch
pub fn get_create_pr_url(owner: &str, repo: &str, branch: &str) -> String {
    format!(
        "https://github.com/{}/{}/compare/{}?expand=1",
        owner, repo, branch
    )
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
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn()?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()?;
    }

    Ok(())
}

// === Caching ===

/// Get the cache directory for a worktree's GitHub metadata
/// Returns ~/.cache/gwt/<project>/<worktree_name>/
fn cache_dir(project_name: &str, worktree_name: &str) -> Result<std::path::PathBuf> {
    let cache_base = dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .ok_or_else(|| anyhow::anyhow!("Could not find cache directory"))?;

    Ok(cache_base
        .join("gwt")
        .join(project_name)
        .join(worktree_name))
}

/// Write GitHub PR metadata to cache directory
pub fn write_pr_cache(project_name: &str, worktree_name: &str, pr: &GitHubPR) -> Result<()> {
    let cache_path = cache_dir(project_name, worktree_name)?;
    fs::create_dir_all(&cache_path)?;

    fs::write(
        cache_path.join("github.json"),
        serde_json::to_string_pretty(pr)?,
    )?;
    Ok(())
}

/// Read GitHub PR metadata from cache directory
pub fn read_pr_cache(project_name: &str, worktree_name: &str) -> Result<Option<GitHubPR>> {
    let cache_path = cache_dir(project_name, worktree_name)?;
    let file_path = cache_path.join("github.json");

    if !file_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&file_path)?;
    let pr: GitHubPR = serde_json::from_str(&content)?;
    Ok(Some(pr))
}

/// Delete GitHub PR metadata from cache directory
pub fn delete_pr_cache(project_name: &str, worktree_name: &str) -> Result<()> {
    let cache_path = cache_dir(project_name, worktree_name)?;
    let file_path = cache_path.join("github.json");
    if file_path.exists() {
        fs::remove_file(&file_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_github_url() {
        assert!(is_github_url("https://github.com/user/repo/pull/123"));
        assert!(is_github_url("https://github.com/user/repo/tree/branch"));
        assert!(is_github_url("http://github.com/user/repo"));
        assert!(!is_github_url("https://gitlab.com/user/repo"));
        assert!(!is_github_url("my-branch-name"));
    }

    #[test]
    fn test_parse_github_url_pr() {
        let info = parse_github_url("https://github.com/anthropics/claude/pull/123").unwrap();
        assert_eq!(info.owner, "anthropics");
        assert_eq!(info.repo, "claude");
        assert_eq!(info.url_type, GitHubUrlType::PullRequest(123));
    }

    #[test]
    fn test_parse_github_url_tree() {
        let info = parse_github_url("https://github.com/user/repo/tree/feature-branch").unwrap();
        assert_eq!(info.owner, "user");
        assert_eq!(info.repo, "repo");
        assert_eq!(info.url_type, GitHubUrlType::Tree("feature-branch".to_string()));
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("https://github.com/user").is_none());
        assert!(parse_github_url("https://gitlab.com/user/repo").is_none());
        assert!(parse_github_url("not-a-url").is_none());
    }

    #[test]
    fn test_get_create_pr_url() {
        let url = get_create_pr_url("owner", "repo", "feature-branch");
        assert_eq!(
            url,
            "https://github.com/owner/repo/compare/feature-branch?expand=1"
        );
    }
}
