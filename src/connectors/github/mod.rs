//! GitHub integration for PR and repository management.
//!
//! Provides functionality for:
//! - Parsing GitHub URLs (PRs, branches, tree paths)
//! - Interacting with the gh CLI for PR information
//! - Opening PRs and create-PR pages in the browser
//! - Caching PR metadata locally

mod cache;
mod cli;
mod types;
mod url;

// Re-export types
pub use types::{CachedPRState, ChecksStatus, GitHubPR, GitHubUrlInfo, GitHubUrlType, PRComment};

// Re-export URL functions
pub use url::{get_create_pr_url, is_github_url, parse_github_url};

// Re-export CLI functions
pub use cli::{
    check_gh_cli, get_pr_branch, get_pr_for_branch, get_repo_info, open_pr_or_create, open_url,
    remote_branch_exists,
};

// Re-export cache functions
pub use cache::{read_pr_cache, write_pr_cache};
