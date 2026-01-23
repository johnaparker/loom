//! Cache operations for Claude session state.

use anyhow::Result;
use std::path::Path;

use crate::connectors::cache as shared_cache;
use super::types::ClaudeSession;

const CACHE_FILENAME: &str = "claude.json";

/// Read the Claude session state from cache.
/// Returns None if the cache file doesn't exist or is corrupted.
/// Errors are silently ignored since cache corruption is transient -
/// the next hook event will write a fresh state.
pub fn read_state(base: &Path, project: &str, worktree: &str) -> Option<ClaudeSession> {
    shared_cache::read_json(base, project, worktree, CACHE_FILENAME)
        .ok()
        .flatten()
}

/// Write the Claude session state to cache
pub fn write_state(base: &Path, project: &str, worktree: &str, session: &ClaudeSession) -> Result<()> {
    shared_cache::write_json(base, project, worktree, CACHE_FILENAME, session)
}

/// Atomically modify the Claude session state with file locking.
///
/// This function handles the read-modify-write cycle atomically to prevent
/// race conditions when multiple hook processes update the state simultaneously.
///
/// The modifier function receives the current session (or None if no state exists)
/// and returns the new session state to write.
pub fn modify_state<F>(base: &Path, project: &str, worktree: &str, modifier: F) -> Result<()>
where
    F: FnOnce(Option<ClaudeSession>) -> ClaudeSession,
{
    shared_cache::with_lock_modify(base, project, worktree, CACHE_FILENAME, modifier)
}
