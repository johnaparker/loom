//! Shared cache utilities for connectors.
//!
//! Provides common caching functionality used by Linear, GitHub, and Claude connectors.
//! All functions accept a base directory parameter, allowing the cache location
//! to be configured via Config::cache_dir().

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Get the cache directory for a specific project and worktree.
///
/// Returns `<base>/<project>/<worktree>/`
pub fn worktree_dir(base: &Path, project_name: &str, worktree_name: &str) -> PathBuf {
    base.join(project_name).join(worktree_name)
}

/// Get the path to a specific cache file.
///
/// Returns `<base>/<project>/<worktree>/<filename>`
pub fn file_path(base: &Path, project_name: &str, worktree_name: &str, filename: &str) -> PathBuf {
    worktree_dir(base, project_name, worktree_name).join(filename)
}

/// Read JSON data from a cache file.
///
/// Returns `Ok(None)` if the file doesn't exist, `Ok(Some(data))` if it does,
/// or an error if reading/parsing fails.
pub fn read_json<T: DeserializeOwned>(
    base: &Path,
    project_name: &str,
    worktree_name: &str,
    filename: &str,
) -> Result<Option<T>> {
    let path = file_path(base, project_name, worktree_name, filename);

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
/// Uses atomic write (write to temp file, then rename) to prevent
/// corruption from concurrent reads during write operations.
pub fn write_json<T: Serialize>(
    base: &Path,
    project_name: &str,
    worktree_name: &str,
    filename: &str,
    data: &T,
) -> Result<()> {
    let dir = worktree_dir(base, project_name, worktree_name);
    fs::create_dir_all(&dir)?;

    let path = dir.join(filename);
    let content = serde_json::to_string_pretty(data)?;

    // Atomic write: write to temp file, then rename.
    // rename() is atomic on POSIX systems, ensuring readers always see
    // either the old complete file or the new complete file, never partial data.
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, &content)?;
    fs::rename(&temp_path, &path)?;

    Ok(())
}

/// Delete a specific cache file.
///
/// Returns `Ok(())` even if the file doesn't exist.
pub fn delete_file(base: &Path, project_name: &str, worktree_name: &str, filename: &str) -> Result<()> {
    let path = file_path(base, project_name, worktree_name, filename);

    if path.exists() {
        fs::remove_file(&path)?;
    }

    Ok(())
}

/// Delete the entire cache directory for a worktree.
///
/// Returns `Ok(())` even if the directory doesn't exist.
pub fn delete_worktree_cache(base: &Path, project_name: &str, worktree_name: &str) -> Result<()> {
    let dir = worktree_dir(base, project_name, worktree_name);

    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_worktree_dir() {
        let base = Path::new("/tmp/gwt-cache");
        let path = worktree_dir(base, "my-project", "my-worktree");
        assert_eq!(path, PathBuf::from("/tmp/gwt-cache/my-project/my-worktree"));
    }

    #[test]
    fn test_file_path() {
        let base = Path::new("/tmp/gwt-cache");
        let path = file_path(base, "my-project", "my-worktree", "test.json");
        assert_eq!(path, PathBuf::from("/tmp/gwt-cache/my-project/my-worktree/test.json"));
    }

    #[test]
    fn test_read_write_json() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Write some data
        let data = serde_json::json!({"key": "value"});
        write_json(base, "proj", "wt", "test.json", &data).unwrap();

        // Read it back
        let read: Option<serde_json::Value> = read_json(base, "proj", "wt", "test.json").unwrap();
        assert_eq!(read, Some(data));
    }

    #[test]
    fn test_read_nonexistent() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        let result: Result<Option<serde_json::Value>> = read_json(base, "proj", "wt", "missing.json");
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_delete_file() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Write then delete
        let data = serde_json::json!({"key": "value"});
        write_json(base, "proj", "wt", "test.json", &data).unwrap();
        delete_file(base, "proj", "wt", "test.json").unwrap();

        // Should be gone
        let path = file_path(base, "proj", "wt", "test.json");
        assert!(!path.exists());
    }

    #[test]
    fn test_delete_worktree_cache() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Write some files
        write_json(base, "proj", "wt", "a.json", &"a").unwrap();
        write_json(base, "proj", "wt", "b.json", &"b").unwrap();

        // Delete entire worktree cache
        delete_worktree_cache(base, "proj", "wt").unwrap();

        // Directory should be gone
        let dir = worktree_dir(base, "proj", "wt");
        assert!(!dir.exists());
    }
}
