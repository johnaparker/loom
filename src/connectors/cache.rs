//! Shared cache utilities for connectors.
//!
//! Provides common caching functionality used by Linear, GitHub, and Claude connectors.
//! All functions accept a base directory parameter, allowing the cache location
//! to be configured via Config::cache_dir().
//!
//! ## Cache Consistency
//!
//! For read-modify-write operations, use `with_lock_modify()` to prevent race conditions.
//! This acquires an exclusive file lock before reading, modifying, and writing the file.
//!
//! Lock strategy:
//! - Uses a separate `.lock` file (e.g., `claude.json.lock`) to avoid locking the file being renamed
//! - Exclusive locks only (simpler, hooks are fast)
//! - Locks timeout after ~310ms total (5 attempts with exponential backoff: 10, 20, 40, 80, 160ms)
//! - On timeout, proceeds unlocked (best effort - hooks must not crash)
//! - `.lock` files may be left behind (harmless)

use anyhow::Result;
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

/// Maximum number of lock acquisition attempts
const LOCK_MAX_ATTEMPTS: u32 = 5;

/// Initial backoff delay in milliseconds
const LOCK_INITIAL_BACKOFF_MS: u64 = 10;

/// Guard that holds an exclusive file lock and releases it on drop.
pub struct LockGuard {
    _file: File,
}

impl LockGuard {
    /// Create a new lock guard from a locked file.
    fn new(file: File) -> Self {
        Self { _file: file }
    }
}

// Lock is released automatically when File is dropped

/// Acquire an exclusive lock on a lock file, with retry and exponential backoff.
///
/// Returns `Some(LockGuard)` on success, `None` if lock couldn't be acquired after all retries.
/// The lock file is created if it doesn't exist.
pub fn acquire_lock(lock_path: &Path) -> Option<LockGuard> {
    // Ensure parent directory exists
    if let Some(parent) = lock_path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return None;
    }

    // Open or create the lock file
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
    {
        Ok(f) => f,
        Err(_) => return None,
    };

    // Try to acquire lock with exponential backoff
    let mut backoff_ms = LOCK_INITIAL_BACKOFF_MS;
    for _ in 0..LOCK_MAX_ATTEMPTS {
        match file.try_lock_exclusive() {
            Ok(()) => return Some(LockGuard::new(file)),
            Err(_) => {
                thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms *= 2; // Exponential backoff: 10, 20, 40, 80, 160ms
            }
        }
    }

    // Failed to acquire lock after all retries
    None
}

/// Atomically read, modify, and write a JSON cache file with file locking.
///
/// This function:
/// 1. Acquires an exclusive lock on `<filename>.lock`
/// 2. Reads the current JSON data (or `None` if file doesn't exist)
/// 3. Calls the modifier function to produce the new value
/// 4. Writes the new value atomically (temp file + rename)
/// 5. Releases the lock (automatically on drop)
///
/// If the lock cannot be acquired after retries (~310ms), the operation proceeds
/// without locking (best effort - hooks must not fail loudly).
pub fn with_lock_modify<T, F>(
    base: &Path,
    project_name: &str,
    worktree_name: &str,
    filename: &str,
    modifier: F,
) -> Result<()>
where
    T: DeserializeOwned + Serialize,
    F: FnOnce(Option<T>) -> T,
{
    let dir = worktree_dir(base, project_name, worktree_name);
    fs::create_dir_all(&dir)?;

    let lock_path = dir.join(format!("{}.lock", filename));

    // Try to acquire lock (proceeds without lock on timeout)
    let _guard = acquire_lock(&lock_path);

    // Read current value
    let current: Option<T> = read_json(base, project_name, worktree_name, filename)?;

    // Apply modifier
    let new_value = modifier(current);

    // Write new value
    write_json(base, project_name, worktree_name, filename, &new_value)
}

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
    // Use a unique temp file name (PID + thread hash) to avoid races when locks timeout.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    let thread_hash = hasher.finish() as u32;
    let unique_id = std::process::id() ^ thread_hash;
    let temp_name = format!("{}.{}.tmp", filename, unique_id);
    let temp_path = dir.join(&temp_name);
    fs::write(&temp_path, &content)?;
    fs::rename(&temp_path, &path)?;

    Ok(())
}

/// Delete a specific cache file.
///
/// Returns `Ok(())` even if the file doesn't exist.
pub fn delete_file(
    base: &Path,
    project_name: &str,
    worktree_name: &str,
    filename: &str,
) -> Result<()> {
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
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Counter {
        value: u32,
    }

    #[test]
    fn test_worktree_dir() {
        let base = Path::new("/tmp/loom-cache");
        let path = worktree_dir(base, "my-project", "my-worktree");
        assert_eq!(
            path,
            PathBuf::from("/tmp/loom-cache/my-project/my-worktree")
        );
    }

    #[test]
    fn test_file_path() {
        let base = Path::new("/tmp/loom-cache");
        let path = file_path(base, "my-project", "my-worktree", "test.json");
        assert_eq!(
            path,
            PathBuf::from("/tmp/loom-cache/my-project/my-worktree/test.json")
        );
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

        let result: Result<Option<serde_json::Value>> =
            read_json(base, "proj", "wt", "missing.json");
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

    #[test]
    fn test_with_lock_modify_basic() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        // Modify non-existent file
        with_lock_modify::<Counter, _>(base, "proj", "wt", "counter.json", |current| {
            assert!(current.is_none());
            Counter { value: 1 }
        })
        .unwrap();

        // Read back
        let counter: Option<Counter> = read_json(base, "proj", "wt", "counter.json").unwrap();
        assert_eq!(counter, Some(Counter { value: 1 }));

        // Modify existing
        with_lock_modify::<Counter, _>(base, "proj", "wt", "counter.json", |current| {
            let mut c = current.unwrap();
            c.value += 1;
            c
        })
        .unwrap();

        let counter: Option<Counter> = read_json(base, "proj", "wt", "counter.json").unwrap();
        assert_eq!(counter, Some(Counter { value: 2 }));
    }

    #[test]
    fn test_with_lock_modify_concurrent() {
        // Test concurrent updates with moderate contention.
        // In real-world usage (Claude hooks), we have 2-3 processes with infrequent updates.
        // Using 4 threads with 25 operations each is realistic and should reliably lock.
        let temp = TempDir::new().unwrap();
        let base = temp.path().to_path_buf();

        const NUM_THREADS: u32 = 4;
        const INCREMENTS_PER_THREAD: u32 = 25;

        // Initialize counter
        write_json(&base, "proj", "wt", "counter.json", &Counter { value: 0 }).unwrap();

        // Use scoped threads to ensure temp dir lives long enough
        std::thread::scope(|s| {
            for _ in 0..NUM_THREADS {
                let base = &base;
                s.spawn(move || {
                    for _ in 0..INCREMENTS_PER_THREAD {
                        with_lock_modify::<Counter, _>(
                            base,
                            "proj",
                            "wt",
                            "counter.json",
                            |current| {
                                let mut c = current.unwrap_or(Counter { value: 0 });
                                c.value += 1;
                                c
                            },
                        )
                        .unwrap();
                    }
                });
            }
        });

        // Check final value
        let counter: Option<Counter> = read_json(&base, "proj", "wt", "counter.json").unwrap();
        let expected = NUM_THREADS * INCREMENTS_PER_THREAD;
        let actual = counter.as_ref().map(|c| c.value);
        assert_eq!(
            counter,
            Some(Counter { value: expected }),
            "Expected {} but got {:?} - indicates lost updates due to race condition",
            expected,
            actual
        );
    }

    #[test]
    fn test_acquire_lock() {
        let temp = TempDir::new().unwrap();
        let lock_path = temp.path().join("test.lock");

        // First lock should succeed
        let guard1 = acquire_lock(&lock_path);
        assert!(guard1.is_some(), "First lock should succeed");

        // Second lock should fail (we still hold the first)
        let guard2 = acquire_lock(&lock_path);
        assert!(
            guard2.is_none(),
            "Second lock should fail while first is held"
        );

        // Drop first lock
        drop(guard1);

        // Now lock should succeed again
        let guard3 = acquire_lock(&lock_path);
        assert!(
            guard3.is_some(),
            "Lock should succeed after first is released"
        );
    }
}
