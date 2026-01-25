//! Type definitions for Linear integration.

use serde::{Deserialize, Serialize};

/// Linear issue metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssue {
    pub id: String,
    pub branch_name: String,
    pub title: String,
    pub url: String,
    pub description: Option<String>,
}

/// Linear team metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearTeam {
    /// Team key/prefix (e.g., "JOH", "ENG")
    pub key: String,
    /// Team name (e.g., "John's Team", "Engineering")
    pub name: String,
}

/// Result of resolving user input for Linear integration
#[derive(Debug, Clone)]
pub struct ResolvedInput {
    /// Folder name for the worktree (e.g., "ABC-123")
    pub worktree_name: String,
    /// Git branch name (e.g., "john/joh-209-feature")
    pub git_branch: String,
    /// Linear issue metadata (if available from API)
    pub issue: Option<LinearIssue>,
}
