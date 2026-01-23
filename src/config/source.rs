//! Configuration source abstraction for testability.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Trait for reading configuration files.
///
/// This abstraction enables unit testing of configuration loading
/// without requiring actual files on disk.
pub trait ConfigSource {
    /// Read configuration content from the given path.
    /// Returns None if the file doesn't exist, Ok(Some(content)) if it exists.
    fn read_config(&self, path: &Path) -> Result<Option<String>>;

    /// Check if a file exists at the given path.
    fn exists(&self, path: &Path) -> bool;

    /// Get the path to the global config file.
    fn global_config_path(&self) -> Result<PathBuf>;
}

/// Default implementation using the real filesystem.
#[derive(Default, Clone)]
pub struct FilesystemSource;

impl ConfigSource for FilesystemSource {
    fn read_config(&self, path: &Path) -> Result<Option<String>> {
        if path.exists() {
            Ok(Some(std::fs::read_to_string(path)?))
        } else {
            Ok(None)
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn global_config_path(&self) -> Result<PathBuf> {
        // Prefer XDG-style path (~/.config/gwt/config.toml)
        if let Some(home) = dirs::home_dir() {
            let xdg_path = home.join(".config").join("gwt").join("config.toml");
            if xdg_path.exists() {
                return Ok(xdg_path);
            }
        }

        // Fall back to platform default
        let config_dir =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(config_dir.join("gwt").join("config.toml"))
    }
}

/// In-memory config source for testing.
#[cfg(test)]
#[derive(Default, Clone)]
pub struct MemorySource {
    /// Content for the global config file
    pub global_config: Option<String>,
    /// Content for project configs, keyed by path string
    pub project_configs: std::collections::HashMap<PathBuf, String>,
    /// Custom global config path (for testing path resolution)
    pub global_config_path: Option<PathBuf>,
}

#[cfg(test)]
impl ConfigSource for MemorySource {
    fn read_config(&self, path: &Path) -> Result<Option<String>> {
        // Check if this is the global config path
        if let Some(ref gcp) = self.global_config_path {
            if path == gcp {
                return Ok(self.global_config.clone());
            }
        }

        // Check project configs
        if let Some(content) = self.project_configs.get(path) {
            return Ok(Some(content.clone()));
        }

        // Default: if no custom global path set, assume any config.toml is global
        if path.ends_with("gwt/config.toml") || path.ends_with("gwt\\config.toml") {
            return Ok(self.global_config.clone());
        }

        Ok(None)
    }

    fn exists(&self, path: &Path) -> bool {
        // Check global config path
        if let Some(ref gcp) = self.global_config_path {
            if path == gcp && self.global_config.is_some() {
                return true;
            }
        }

        // Check project configs
        if self.project_configs.contains_key(path) {
            return true;
        }

        // Check if it's the global config
        if (path.ends_with("gwt/config.toml") || path.ends_with("gwt\\config.toml"))
            && self.global_config.is_some()
        {
            return true;
        }

        false
    }

    fn global_config_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.global_config_path {
            Ok(path.clone())
        } else {
            // Return a fake path for testing
            Ok(PathBuf::from("/test/.config/gwt/config.toml"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_source_global_config() {
        let source = MemorySource {
            global_config: Some(r#"worktree_root = "/test""#.to_string()),
            ..Default::default()
        };

        let path = source.global_config_path().unwrap();
        let content = source.read_config(&path).unwrap();
        assert!(content.is_some());
        assert!(content.unwrap().contains("worktree_root"));
    }

    #[test]
    fn test_memory_source_no_config() {
        let source = MemorySource::default();

        let path = PathBuf::from("/nonexistent/config.toml");
        let content = source.read_config(&path).unwrap();
        assert!(content.is_none());
    }

    #[test]
    fn test_memory_source_project_config() {
        let mut project_configs = std::collections::HashMap::new();
        project_configs.insert(
            PathBuf::from("/repo/.gwt.toml"),
            r#"project_name = "test""#.to_string(),
        );

        let source = MemorySource {
            project_configs,
            ..Default::default()
        };

        let content = source.read_config(Path::new("/repo/.gwt.toml")).unwrap();
        assert!(content.is_some());
        assert!(content.unwrap().contains("project_name"));
    }

    #[test]
    fn test_memory_source_exists() {
        let source = MemorySource {
            global_config: Some("content".to_string()),
            ..Default::default()
        };

        let path = source.global_config_path().unwrap();
        assert!(source.exists(&path));
        assert!(!source.exists(Path::new("/nonexistent")));
    }

    #[test]
    fn test_filesystem_source_nonexistent() {
        let source = FilesystemSource;
        let content = source.read_config(Path::new("/nonexistent/path/config.toml"));
        assert!(content.is_ok());
        assert!(content.unwrap().is_none());
    }
}
