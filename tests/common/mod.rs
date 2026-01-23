//! Test utilities for creating ephemeral git repositories and worktrees.
//!
//! This module provides helpers for setting up isolated test environments
//! that don't pollute the user's `~/worktrees` folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// A test repository with a custom worktree location.
///
/// When dropped, all temporary directories are automatically cleaned up.
pub struct TestRepo {
    /// The temporary directory containing the main repository
    pub repo_dir: TempDir,
    /// The temporary directory for worktrees (instead of ~/worktrees)
    pub worktree_dir: TempDir,
    /// Path to the config directory
    pub config_dir: TempDir,
}

impl TestRepo {
    /// Create a new test repository with initial setup.
    ///
    /// Creates:
    /// - An initialized git repo with an initial commit
    /// - A `main` branch
    /// - A custom config file pointing worktrees to a temp directory
    pub fn new() -> Self {
        let repo_dir = TempDir::new().expect("Failed to create temp repo dir");
        let worktree_dir = TempDir::new().expect("Failed to create temp worktree dir");
        let config_dir = TempDir::new().expect("Failed to create temp config dir");

        // Initialize git repo
        run_git(&repo_dir, &["init"]);
        run_git(&repo_dir, &["config", "user.email", "test@test.com"]);
        run_git(&repo_dir, &["config", "user.name", "Test User"]);

        // Create initial commit
        let readme = repo_dir.path().join("README.md");
        fs::write(&readme, "# Test Repository\n").expect("Failed to write README");
        run_git(&repo_dir, &["add", "."]);
        run_git(&repo_dir, &["commit", "-m", "Initial commit"]);

        // Rename branch to main (git might default to master)
        run_git(&repo_dir, &["branch", "-M", "main"]);

        // Create gwt config file
        let config_path = config_dir.path().join("gwt").join("config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).expect("Failed to create config dir");

        let config_content = format!(
            r#"worktree_root = "{}"
default_category = "dev"

[sync]
patterns = []

[linear]
"#,
            worktree_dir.path().display()
        );
        fs::write(&config_path, config_content).expect("Failed to write config");

        TestRepo {
            repo_dir,
            worktree_dir,
            config_dir,
        }
    }

    /// Get the path to the main repository
    pub fn path(&self) -> &Path {
        self.repo_dir.path()
    }

    /// Get the path to the worktree directory
    pub fn worktree_path(&self) -> &Path {
        self.worktree_dir.path()
    }

    /// Get the project name (directory name of the repo)
    pub fn project_name(&self) -> String {
        self.repo_dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    /// Create a new branch in the repository
    #[allow(dead_code)]
    pub fn create_branch(&self, name: &str) {
        run_git(&self.repo_dir, &["branch", name]);
    }

    /// Create a commit with a test file
    pub fn create_commit(&self, message: &str) {
        let file_path = self
            .repo_dir
            .path()
            .join(format!("file_{}.txt", rand_suffix()));
        fs::write(&file_path, format!("Content for: {}\n", message))
            .expect("Failed to write test file");
        run_git(&self.repo_dir, &["add", "."]);
        run_git(&self.repo_dir, &["commit", "-m", message]);
    }

    /// Create a worktree manually using git commands
    pub fn create_worktree(&self, branch: &str, category: &str) -> PathBuf {
        let project_name = self.project_name();
        let worktree_path = self
            .worktree_dir
            .path()
            .join(&project_name)
            .join(category)
            .join(branch);

        fs::create_dir_all(worktree_path.parent().unwrap()).expect("Failed to create worktree dir");

        run_git(
            &self.repo_dir,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                worktree_path.to_str().unwrap(),
            ],
        );

        worktree_path
    }

    /// Create a worktree for an existing branch
    #[allow(dead_code)]
    pub fn create_worktree_existing(&self, branch: &str, category: &str) -> PathBuf {
        let project_name = self.project_name();
        let worktree_path = self
            .worktree_dir
            .path()
            .join(&project_name)
            .join(category)
            .join(branch);

        fs::create_dir_all(worktree_path.parent().unwrap()).expect("Failed to create worktree dir");

        run_git(
            &self.repo_dir,
            &["worktree", "add", worktree_path.to_str().unwrap(), branch],
        );

        worktree_path
    }

    /// List all worktrees using git
    pub fn list_worktrees(&self) -> Vec<String> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(self.repo_dir.path())
            .output()
            .expect("Failed to run git worktree list");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();

        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                worktrees.push(path.to_string());
            }
        }

        worktrees
    }

    /// Run gwt command in this test repo with the test config
    #[allow(deprecated)]
    pub fn run_gwt(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut cmd = assert_cmd::Command::cargo_bin("gwt").expect("Failed to find gwt binary");
        cmd.current_dir(self.repo_dir.path())
            .env("XDG_CONFIG_HOME", self.config_dir.path())
            .env("HOME", self.config_dir.path())
            .args(args);
        cmd.assert()
    }

    /// Run gwt command and return output as string
    pub fn run_gwt_output(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_gwt"))
            .current_dir(self.repo_dir.path())
            .env("XDG_CONFIG_HOME", self.config_dir.path())
            .env("HOME", self.config_dir.path())
            .args(args)
            .output()
            .expect("Failed to run gwt");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        format!("{}{}", stdout, stderr)
    }

    /// Check if a worktree exists at the expected path
    pub fn worktree_exists(&self, branch: &str, category: &str) -> bool {
        let project_name = self.project_name();
        let worktree_path = self
            .worktree_dir
            .path()
            .join(&project_name)
            .join(category)
            .join(branch);
        worktree_path.exists()
    }

    /// Get the expected worktree path
    #[allow(dead_code)]
    pub fn expected_worktree_path(&self, branch: &str, category: &str) -> PathBuf {
        let project_name = self.project_name();
        self.worktree_dir
            .path()
            .join(&project_name)
            .join(category)
            .join(branch)
    }
}

/// Run a git command in the given directory
fn run_git(dir: &TempDir, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir.path())
        .output()
        .expect("Failed to run git command");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "Git command failed: git {}\nStderr: {}",
            args.join(" "),
            stderr
        );
    }
}

/// Generate a random suffix for unique file names
fn rand_suffix() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_creation() {
        let repo = TestRepo::new();
        assert!(repo.path().exists());
        assert!(repo.path().join(".git").exists());
        assert!(repo.worktree_path().exists());
    }

    #[test]
    fn test_create_worktree() {
        let repo = TestRepo::new();
        let wt_path = repo.create_worktree("feature-1", "dev");
        assert!(wt_path.exists());

        let worktrees = repo.list_worktrees();
        assert_eq!(worktrees.len(), 2); // main + feature-1
    }

    #[test]
    fn test_create_commit() {
        let repo = TestRepo::new();
        repo.create_commit("Test commit");

        let output = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(repo.path())
            .output()
            .expect("Failed to run git log");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Test commit"));
    }
}
