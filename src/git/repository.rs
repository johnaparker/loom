//! Repository discovery abstraction for testability.

use anyhow::Result;
use git2::Repository;
use std::path::Path;

/// Trait for discovering git repositories.
///
/// This abstraction enables unit testing of code that needs to open git repositories
/// without requiring actual git repos on disk.
pub trait RepositoryProvider {
    /// Discover a repository by searching upward from the given path.
    fn discover(&self, path: &Path) -> Result<Repository>;

    /// Open a repository at the exact given path.
    fn open(&self, path: &Path) -> Result<Repository>;
}

/// Default implementation using git2 directly.
#[derive(Default, Clone)]
pub struct Git2Provider;

impl RepositoryProvider for Git2Provider {
    fn discover(&self, path: &Path) -> Result<Repository> {
        Ok(Repository::discover(path)?)
    }

    fn open(&self, path: &Path) -> Result<Repository> {
        Ok(Repository::open(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_git2_provider_discover_repo() {
        let temp = TempDir::new().unwrap();
        // Initialize a git repo
        Repository::init(temp.path()).unwrap();

        let provider = Git2Provider;
        let repo = provider.discover(temp.path());
        assert!(repo.is_ok());
    }

    #[test]
    fn test_git2_provider_open_repo() {
        let temp = TempDir::new().unwrap();
        Repository::init(temp.path()).unwrap();

        let provider = Git2Provider;
        let repo = provider.open(temp.path());
        assert!(repo.is_ok());
    }

    #[test]
    fn test_git2_provider_discover_nonexistent() {
        let temp = TempDir::new().unwrap();
        let provider = Git2Provider;
        let result = provider.discover(temp.path());
        assert!(result.is_err());
    }
}
