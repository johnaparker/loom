//! Cache operations for GitHub PR metadata.

use anyhow::Result;

use crate::connectors::cache as shared_cache;
use super::types::GitHubPR;

const CACHE_FILENAME: &str = "github.json";

/// Write GitHub PR metadata to cache directory
pub fn write_pr_cache(project_name: &str, worktree_name: &str, pr: &GitHubPR) -> Result<()> {
    shared_cache::write_json(project_name, worktree_name, CACHE_FILENAME, pr)
}

/// Read GitHub PR metadata from cache directory
pub fn read_pr_cache(project_name: &str, worktree_name: &str) -> Result<Option<GitHubPR>> {
    shared_cache::read_json(project_name, worktree_name, CACHE_FILENAME)
}

/// Delete GitHub PR metadata from cache directory
pub fn delete_pr_cache(project_name: &str, worktree_name: &str) -> Result<()> {
    shared_cache::delete_file(project_name, worktree_name, CACHE_FILENAME)
}
