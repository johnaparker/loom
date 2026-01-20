//! GitHub URL parsing and generation utilities.

use super::types::{GitHubUrlInfo, GitHubUrlType};

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
                // Use all remaining segments as the branch/path
                // This handles branches with slashes like "feature/sub-feature"
                GitHubUrlType::Tree(parts[3..].join("/"))
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

/// Get the URL to create a new PR for a branch
pub fn get_create_pr_url(owner: &str, repo: &str, branch: &str) -> String {
    format!(
        "https://github.com/{}/{}/compare/{}?expand=1",
        owner,
        repo,
        urlencoding::encode(branch)
    )
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
        assert_eq!(
            info.url_type,
            GitHubUrlType::Tree("feature-branch".to_string())
        );

        // Test branches with slashes
        let info =
            parse_github_url("https://github.com/user/repo/tree/feature/sub-feature").unwrap();
        assert_eq!(
            info.url_type,
            GitHubUrlType::Tree("feature/sub-feature".to_string())
        );
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

        // Test URL encoding of special characters
        let url = get_create_pr_url("owner", "repo", "feature/branch#123");
        assert_eq!(
            url,
            "https://github.com/owner/repo/compare/feature%2Fbranch%23123?expand=1"
        );
    }
}
