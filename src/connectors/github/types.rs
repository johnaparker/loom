//! Type definitions for GitHub integration.

use serde::{Deserialize, Serialize};

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
    /// Usernames of assignees
    pub assignees: Vec<String>,
    /// Usernames of requested reviewers
    pub reviewers: Vec<String>,
    /// Review decision: APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED, or None
    pub review_decision: Option<String>,
}

/// PR checks status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksStatus {
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
    /// Names of failing checks
    pub failing_names: Vec<String>,
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
