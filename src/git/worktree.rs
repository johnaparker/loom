use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::GwtError;

/// Information about a git worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_main: bool,
    pub category: Option<String>,
}

/// Manager for git worktree operations
pub struct WorktreeManager {
    repo: Repository,
    repo_root: PathBuf,
}

impl WorktreeManager {
    /// Open a repository and create a worktree manager
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path).map_err(|_| GwtError::NotGitRepo)?;
        let repo_root = repo
            .workdir()
            .ok_or(GwtError::NotGitRepo)?
            .to_path_buf();
        Ok(Self { repo, repo_root })
    }

    /// Get the repository root path
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Get the project name (derived from repo directory name)
    pub fn project_name(&self) -> Result<String> {
        self.repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Could not determine project name"))
    }

    /// Get the main branch name (main or master)
    pub fn main_branch_name(&self) -> Result<String> {
        // Try 'main' first, then 'master'
        for name in &["main", "master"] {
            if self.repo.find_branch(name, BranchType::Local).is_ok() {
                return Ok(name.to_string());
            }
        }
        // Check remote branches
        for name in &["origin/main", "origin/master"] {
            if self.repo.find_branch(name, BranchType::Remote).is_ok() {
                return Ok(name.strip_prefix("origin/").unwrap().to_string());
            }
        }
        Err(GwtError::NoMainBranch.into())
    }

    /// List all worktrees for this repository
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let mut worktrees = Vec::new();

        // The main worktree
        let main_branch = self.main_branch_name().ok();
        worktrees.push(WorktreeInfo {
            name: "main".to_string(),
            path: self.repo_root.clone(),
            branch: main_branch.clone(),
            is_main: true,
            category: None,
        });

        // List all worktrees using git command (more reliable than git2)
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git worktree list")?;

        if !output.status.success() {
            return Ok(worktrees);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;

        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(path));
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch.to_string());
            } else if line.is_empty() {
                if let Some(path) = current_path.take() {
                    // Skip the main worktree
                    if path != self.repo_root {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        // Try to determine category from path
                        let category = path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string());

                        worktrees.push(WorktreeInfo {
                            name,
                            path,
                            branch: current_branch.take(),
                            is_main: false,
                            category,
                        });
                    }
                }
                current_branch = None;
            }
        }

        Ok(worktrees)
    }

    /// Create a new worktree
    pub fn create_worktree(&self, branch: &str, path: &Path) -> Result<()> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Check if branch exists
        let branch_exists = self.repo.find_branch(branch, BranchType::Local).is_ok();

        let mut args = vec!["worktree", "add"];

        if branch_exists {
            // Use existing branch
            args.push(path.to_str().unwrap());
            args.push(branch);
        } else {
            // Create new branch from main
            args.push("-b");
            args.push(branch);
            args.push(path.to_str().unwrap());
        }

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GwtError::GitCommandFailed {
                command: "git worktree add".to_string(),
                stderr,
            }
            .into());
        }

        Ok(())
    }

    /// Remove a worktree
    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<()> {
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(path.to_str().unwrap());

        let output = Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git worktree remove")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GwtError::GitCommandFailed {
                command: "git worktree remove".to_string(),
                stderr,
            }
            .into());
        }

        Ok(())
    }

    /// Get worktree by name
    pub fn get_worktree(&self, name: &str) -> Result<Option<WorktreeInfo>> {
        let worktrees = self.list_worktrees()?;
        Ok(worktrees.into_iter().find(|w| w.name == name))
    }

    /// Merge a branch into main
    pub fn merge_to_main(&self, branch: &str) -> Result<()> {
        let main_branch = self.main_branch_name()?;

        // First, checkout main
        let output = Command::new("git")
            .args(["checkout", &main_branch])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to checkout main branch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GwtError::GitCommandFailed {
                command: format!("git checkout {}", main_branch),
                stderr,
            }
            .into());
        }

        // Merge the branch
        let output = Command::new("git")
            .args([
                "merge",
                "--no-ff",
                branch,
                "-m",
                &format!("Merge branch '{}'", branch),
            ])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to merge branch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GwtError::GitCommandFailed {
                command: format!("git merge {}", branch),
                stderr,
            }
            .into());
        }

        Ok(())
    }

    /// Delete a branch
    pub fn delete_branch(&self, branch: &str, force: bool) -> Result<()> {
        let flag = if force { "-D" } else { "-d" };
        let output = Command::new("git")
            .args(["branch", flag, branch])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to delete branch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GwtError::GitCommandFailed {
                command: format!("git branch {} {}", flag, branch),
                stderr,
            }
            .into());
        }

        Ok(())
    }

    /// Get current branch name
    pub fn current_branch(&self) -> Result<Option<String>> {
        let head = self.repo.head()?;
        if head.is_branch() {
            Ok(head.shorthand().map(|s| s.to_string()))
        } else {
            Ok(None)
        }
    }
}
