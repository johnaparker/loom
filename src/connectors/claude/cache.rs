//! Cache operations for Claude session state.

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use super::types::ClaudeSession;

const CACHE_FILENAME: &str = "claude.json";

/// Get the cache directory for a project/worktree
pub fn cache_dir(project: &str, worktree: &str) -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("gwt")
        .join(project)
        .join(worktree)
}

/// Get the path to the claude.json cache file
pub fn cache_file(project: &str, worktree: &str) -> PathBuf {
    cache_dir(project, worktree).join(CACHE_FILENAME)
}

/// Read the Claude session state from cache
pub fn read_state(project: &str, worktree: &str) -> Option<ClaudeSession> {
    let path = cache_file(project, worktree);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write the Claude session state to cache
pub fn write_state(project: &str, worktree: &str, session: &ClaudeSession) -> Result<()> {
    let dir = cache_dir(project, worktree);
    fs::create_dir_all(&dir)?;

    let path = cache_file(project, worktree);
    let content = serde_json::to_string_pretty(session)?;
    fs::write(&path, content)?;

    Ok(())
}
