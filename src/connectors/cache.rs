//! Shared cache utilities for connectors.
//!
//! Provides common caching functionality used by Linear, GitHub, and Claude connectors.

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::PathBuf;

/// Get the base cache directory for gwt.
///
/// Returns `~/.cache/gwt` on Linux/macOS, or the platform's cache directory.
pub fn base_dir() -> Result<PathBuf> {
    let cache_base = dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .ok_or_else(|| anyhow::anyhow!("Could not find cache directory"))?;

    Ok(cache_base.join("gwt"))
}

/// Get the cache directory for a specific project and worktree.
///
/// Returns `~/.cache/gwt/<project>/<worktree>/`
pub fn worktree_dir(project_name: &str, worktree_name: &str) -> Result<PathBuf> {
    Ok(base_dir()?.join(project_name).join(worktree_name))
}

/// Get the path to a specific cache file.
///
/// Returns `~/.cache/gwt/<project>/<worktree>/<filename>`
pub fn file_path(project_name: &str, worktree_name: &str, filename: &str) -> Result<PathBuf> {
    Ok(worktree_dir(project_name, worktree_name)?.join(filename))
}

/// Read JSON data from a cache file.
///
/// Returns `Ok(None)` if the file doesn't exist, `Ok(Some(data))` if it does,
/// or an error if reading/parsing fails.
pub fn read_json<T: DeserializeOwned>(
    project_name: &str,
    worktree_name: &str,
    filename: &str,
) -> Result<Option<T>> {
    let path = file_path(project_name, worktree_name, filename)?;

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    let data: T = serde_json::from_str(&content)?;
    Ok(Some(data))
}

/// Write JSON data to a cache file.
///
/// Creates the parent directories if they don't exist.
pub fn write_json<T: Serialize>(
    project_name: &str,
    worktree_name: &str,
    filename: &str,
    data: &T,
) -> Result<()> {
    let dir = worktree_dir(project_name, worktree_name)?;
    fs::create_dir_all(&dir)?;

    let path = dir.join(filename);
    let content = serde_json::to_string_pretty(data)?;
    fs::write(&path, content)?;

    Ok(())
}

/// Delete a specific cache file.
///
/// Returns `Ok(())` even if the file doesn't exist.
pub fn delete_file(project_name: &str, worktree_name: &str, filename: &str) -> Result<()> {
    let path = file_path(project_name, worktree_name, filename)?;

    if path.exists() {
        fs::remove_file(&path)?;
    }

    Ok(())
}

/// Delete the entire cache directory for a worktree.
///
/// Returns `Ok(())` even if the directory doesn't exist.
pub fn delete_worktree_cache(project_name: &str, worktree_name: &str) -> Result<()> {
    let dir = worktree_dir(project_name, worktree_name)?;

    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_dir() {
        let dir = base_dir();
        assert!(dir.is_ok());
        let path = dir.unwrap();
        assert!(path.ends_with("gwt"));
    }

    #[test]
    fn test_worktree_dir() {
        let dir = worktree_dir("my-project", "my-worktree");
        assert!(dir.is_ok());
        let path = dir.unwrap();
        assert!(path.ends_with("gwt/my-project/my-worktree"));
    }
}
