//! Cache operations for Claude session state.

use anyhow::Result;
use std::path::Path;

use crate::connectors::cache as shared_cache;
use super::types::ClaudeSession;

const CACHE_FILENAME: &str = "claude.json";

/// Read the Claude session state from cache
pub fn read_state(base: &Path, project: &str, worktree: &str) -> Option<ClaudeSession> {
    match shared_cache::read_json(base, project, worktree, CACHE_FILENAME) {
        Ok(session) => session,
        Err(e) => {
            // Log read errors for debugging. This can happen due to corrupt files,
            // though with atomic writes it should be rare.
            eprintln!("gwt: Warning: Failed to read claude cache for {}/{}: {}", project, worktree, e);
            None
        }
    }
}

/// Write the Claude session state to cache
pub fn write_state(base: &Path, project: &str, worktree: &str, session: &ClaudeSession) -> Result<()> {
    shared_cache::write_json(base, project, worktree, CACHE_FILENAME, session)
}
