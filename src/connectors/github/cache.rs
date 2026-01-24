//! Cache operations for GitHub PR metadata.

use anyhow::Result;
use std::path::Path;

use super::types::CachedPRState;
use crate::connectors::cache as shared_cache;

const CACHE_FILENAME: &str = "github.json";

/// Write GitHub PR state to cache directory
pub fn write_pr_cache(
    base: &Path,
    project_name: &str,
    worktree_name: &str,
    state: &CachedPRState,
) -> Result<()> {
    shared_cache::write_json(base, project_name, worktree_name, CACHE_FILENAME, state)
}

/// Read GitHub PR state from cache directory
pub fn read_pr_cache(
    base: &Path,
    project_name: &str,
    worktree_name: &str,
) -> Result<Option<CachedPRState>> {
    shared_cache::read_json(base, project_name, worktree_name, CACHE_FILENAME)
}
