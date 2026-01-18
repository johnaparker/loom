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

/// Information about a single commit
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash_short: String,
    pub message: String,
    pub author: String,
    pub relative_time: String,
}

/// Statistics about a worktree for dashboard display
#[derive(Debug, Clone)]
pub struct WorktreeStats {
    pub info: WorktreeInfo,
    pub commits_ahead: Option<u32>,
    pub commits_behind: Option<u32>,
    pub diff_added: Option<u32>,
    pub diff_removed: Option<u32>,
    pub uncommitted_added: u32,
    pub uncommitted_removed: u32,
    pub age_days: Option<u32>,
    pub recent_commits: Vec<CommitInfo>,
}

impl WorktreeStats {
    /// Check if this worktree has any uncommitted changes
    pub fn has_uncommitted_changes(&self) -> bool {
        self.uncommitted_added > 0 || self.uncommitted_removed > 0
    }
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

    /// Fetch from origin to ensure we have the latest refs
    pub fn fetch_origin(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git fetch origin")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GwtError::GitCommandFailed {
                command: "git fetch origin".to_string(),
                stderr,
            }
            .into());
        }

        Ok(())
    }

    /// Get the remote main branch ref (origin/main or origin/master)
    fn remote_main_ref(&self) -> Option<String> {
        for name in &["origin/main", "origin/master"] {
            if self.repo.find_branch(name, BranchType::Remote).is_ok() {
                return Some(name.to_string());
            }
        }
        None
    }

    /// Get the best start point for a new branch (prefers origin/main over local main)
    fn best_start_point(&self) -> Result<String> {
        // Prefer remote main branch (most up-to-date)
        if let Some(remote_main) = self.remote_main_ref() {
            return Ok(remote_main);
        }

        // Fall back to local main branch
        self.main_branch_name()
    }

    /// Create a new worktree
    pub fn create_worktree(&self, branch: &str, path: &Path) -> Result<()> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Check if branch exists
        let branch_exists = self.repo.find_branch(branch, BranchType::Local).is_ok();
        let path_str = path.to_str().unwrap();

        let output = if branch_exists {
            // Use existing branch
            Command::new("git")
                .args(["worktree", "add", path_str, branch])
                .current_dir(&self.repo_root)
                .output()
                .context("Failed to run git worktree add")?
        } else {
            // Create new branch from origin/main (or fallback to local main)
            let start_point = self.best_start_point()?;
            Command::new("git")
                .args(["worktree", "add", "-b", branch, path_str, &start_point])
                .current_dir(&self.repo_root)
                .output()
                .context("Failed to run git worktree add")?
        };

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

    /// Get number of commits ahead of main branch
    fn commits_ahead_of_main(&self, path: &Path, branch: &str) -> Option<u32> {
        let main_branch = self.main_branch_name().ok()?;
        if branch == main_branch {
            return Some(0);
        }

        let output = Command::new("git")
            .args(["rev-list", "--count", &format!("{}..{}", main_branch, branch)])
            .current_dir(path)
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.trim().parse().ok()
        } else {
            None
        }
    }

    /// Get number of commits a branch is behind origin/main (or local main if no remote)
    /// For non-main branches: git rev-list --count {branch}..origin/main
    pub fn commits_behind_remote_main(&self, path: &Path, branch: &str) -> Option<u32> {
        // Prefer remote main, fall back to local main
        let target = self
            .remote_main_ref()
            .or_else(|| self.main_branch_name().ok())?;

        // Don't compare a branch to itself
        if branch == target {
            return Some(0);
        }

        let output = Command::new("git")
            .args([
                "rev-list",
                "--count",
                &format!("{}..{}", branch, target),
            ])
            .current_dir(path)
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.trim().parse().ok()
        } else {
            None
        }
    }

    /// Get number of commits local main is behind origin/main
    pub fn main_behind_origin(&self) -> Option<u32> {
        let main_branch = self.main_branch_name().ok()?;
        let remote_main = self.remote_main_ref()?;

        let output = Command::new("git")
            .args([
                "rev-list",
                "--count",
                &format!("{}..{}", main_branch, remote_main),
            ])
            .current_dir(&self.repo_root)
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.trim().parse().ok()
        } else {
            None
        }
    }

    /// Get diff stats vs main (lines added, lines removed)
    fn diff_stats_vs_main(&self, path: &Path, branch: &str) -> Option<(u32, u32)> {
        let main_branch = self.main_branch_name().ok()?;
        if branch == main_branch {
            return Some((0, 0));
        }

        let output = Command::new("git")
            .args(["diff", "--shortstat", &format!("{}...{}", main_branch, branch)])
            .current_dir(path)
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Self::parse_shortstat(&stdout)
        } else {
            None
        }
    }

    /// Parse git diff --shortstat output to extract insertions and deletions
    fn parse_shortstat(output: &str) -> Option<(u32, u32)> {
        let output = output.trim();
        if output.is_empty() {
            return Some((0, 0));
        }

        let mut added = 0u32;
        let mut removed = 0u32;

        // Parse strings like "5 files changed, 100 insertions(+), 20 deletions(-)"
        for part in output.split(',') {
            let part = part.trim();
            if part.contains("insertion")
                && let Some(num) = part.split_whitespace().next()
            {
                added = num.parse().unwrap_or(0);
            } else if part.contains("deletion")
                && let Some(num) = part.split_whitespace().next()
            {
                removed = num.parse().unwrap_or(0);
            }
        }

        Some((added, removed))
    }

    /// Get uncommitted changes stats (lines added, lines removed)
    pub fn uncommitted_stats(&self, path: &Path) -> (u32, u32) {
        let mut total_added = 0u32;
        let mut total_removed = 0u32;

        // Unstaged changes
        if let Ok(output) = Command::new("git")
            .args(["diff", "--shortstat"])
            .current_dir(path)
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some((added, removed)) = Self::parse_shortstat(&stdout) {
                total_added += added;
                total_removed += removed;
            }
        }

        // Staged changes
        if let Ok(output) = Command::new("git")
            .args(["diff", "--cached", "--shortstat"])
            .current_dir(path)
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some((added, removed)) = Self::parse_shortstat(&stdout) {
                total_added += added;
                total_removed += removed;
            }
        }

        (total_added, total_removed)
    }

    /// Get worktree age in days
    fn worktree_age_days(&self, path: &Path) -> Option<u32> {
        let metadata = std::fs::metadata(path).ok()?;
        let created = metadata.created().ok()?;
        let duration = std::time::SystemTime::now().duration_since(created).ok()?;
        Some((duration.as_secs() / 86400) as u32)
    }

    /// Get recent commits for a worktree
    fn recent_commits(&self, path: &Path, limit: usize) -> Vec<CommitInfo> {
        let output = Command::new("git")
            .args([
                "log",
                "--format=%h|%s|%an|%cr",
                "-n",
                &limit.to_string(),
            ])
            .current_dir(path)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout
                    .lines()
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.splitn(4, '|').collect();
                        if parts.len() == 4 {
                            Some(CommitInfo {
                                hash_short: parts[0].to_string(),
                                message: parts[1].to_string(),
                                author: parts[2].to_string(),
                                relative_time: parts[3].to_string(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get stats for a single worktree
    fn get_worktree_stats(&self, info: WorktreeInfo) -> WorktreeStats {
        let branch = info.branch.as_deref().unwrap_or("");

        let commits_ahead = if !branch.is_empty() {
            self.commits_ahead_of_main(&info.path, branch)
        } else {
            None
        };

        let commits_behind = if !branch.is_empty() {
            self.commits_behind_remote_main(&info.path, branch)
        } else {
            None
        };

        let (diff_added, diff_removed) = if !branch.is_empty() {
            self.diff_stats_vs_main(&info.path, branch)
                .map(|(a, r)| (Some(a), Some(r)))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };

        let (uncommitted_added, uncommitted_removed) = self.uncommitted_stats(&info.path);
        let age_days = self.worktree_age_days(&info.path);
        let recent_commits = self.recent_commits(&info.path, 10);

        WorktreeStats {
            info,
            commits_ahead,
            commits_behind,
            diff_added,
            diff_removed,
            uncommitted_added,
            uncommitted_removed,
            age_days,
            recent_commits,
        }
    }

    /// List all worktrees with their stats
    pub fn list_worktrees_with_stats(&self) -> Result<Vec<WorktreeStats>> {
        let worktrees = self.list_worktrees()?;
        Ok(worktrees
            .into_iter()
            .map(|info| self.get_worktree_stats(info))
            .collect())
    }

    /// Check if merging a branch into main would cause conflicts
    /// Returns None if merge is clean, Some(conflicted_files) if there are conflicts
    pub fn check_merge_conflicts(&self, branch: &str) -> Result<Option<Vec<String>>> {
        let main_branch = self.main_branch_name()?;

        // Use git merge-tree --write-tree to check for conflicts
        // This runs without modifying the working directory or index
        let output = Command::new("git")
            .args(["merge-tree", "--write-tree", &main_branch, branch])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git merge-tree")?;

        if output.status.success() {
            // Exit code 0 means clean merge possible
            Ok(None)
        } else {
            // Exit code non-zero means conflicts
            // Parse the output to find conflicted files
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut conflicted_files = Vec::new();

            // The output format includes lines like:
            // CONFLICT (content): Merge conflict in <filename>
            for line in stdout.lines() {
                if line.starts_with("CONFLICT") {
                    // Extract filename from the line
                    if let Some(idx) = line.find(" in ") {
                        let filename = line[idx + 4..].trim();
                        conflicted_files.push(filename.to_string());
                    }
                }
            }

            // If we didn't parse any specific files, try alternative parsing
            // or just indicate there are conflicts
            if conflicted_files.is_empty() {
                // Try parsing from the tree structure
                for line in stdout.lines() {
                    // Look for lines that indicate conflicts (e.g., with mode 100644 or similar)
                    if line.contains("100644") || line.contains("100755") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 4 {
                            let filename = parts.last().unwrap_or(&"unknown");
                            if !conflicted_files.contains(&filename.to_string()) {
                                conflicted_files.push(filename.to_string());
                            }
                        }
                    }
                }
            }

            // If still empty but we know there are conflicts, add a generic message
            if conflicted_files.is_empty() {
                conflicted_files.push("(unable to determine specific files)".to_string());
            }

            Ok(Some(conflicted_files))
        }
    }
}
