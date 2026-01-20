//! Cache operations for Linear issue metadata.

use anyhow::Result;
use std::path::Path;

use crate::connectors::cache as shared_cache;
use super::types::LinearIssue;

const CACHE_FILENAME: &str = "linear.json";

/// Write Linear issue metadata to cache directory
pub fn write_metadata(base: &Path, project_name: &str, worktree_name: &str, issue: &LinearIssue) -> Result<()> {
    let metadata = serde_json::json!({
        "id": issue.id,
        "title": issue.title,
        "url": issue.url,
        "branch": issue.branch_name,
        "description": issue.description
    });
    shared_cache::write_json(base, project_name, worktree_name, CACHE_FILENAME, &metadata)
}

/// Read Linear issue metadata from cache directory
pub fn read_metadata(base: &Path, project_name: &str, worktree_name: &str) -> Result<Option<LinearIssue>> {
    let value: Option<serde_json::Value> =
        shared_cache::read_json(base, project_name, worktree_name, CACHE_FILENAME)?;

    let Some(value) = value else {
        return Ok(None);
    };

    Ok(Some(LinearIssue {
        id: value.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        url: value.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        branch_name: value.get("branch").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        // Handle missing description for old caches
        description: value
            .get("description")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    }))
}

/// Delete Linear issue metadata from cache directory
pub fn delete_metadata(base: &Path, project_name: &str, worktree_name: &str) -> Result<()> {
    shared_cache::delete_worktree_cache(base, project_name, worktree_name)
}
