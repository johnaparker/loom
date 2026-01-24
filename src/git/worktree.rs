use anyhow::{Context, Result};
use git2::{BranchType, Repository};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::repository::{Git2Provider, RepositoryProvider};
use crate::error::GwtError;

/// Result of a push operation
#[derive(Debug, Clone)]
pub enum PushResult {
    /// Push succeeded normally
    Success,
    /// Push succeeded and created a new remote branch with tracking
    CreatedRemoteBranch,
    /// Push was rejected (e.g., remote has new commits)
    Rejected { reason: String },
}

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
    /// Commits ahead of tracking branch (e.g., origin/<branch>)
    pub tracking_ahead: Option<u32>,
    /// Commits behind tracking branch (e.g., origin/<branch>)
    pub tracking_behind: Option<u32>,
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
    /// Always finds the main repository, even when called from a worktree
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_provider(path, &Git2Provider)
    }

    /// Open with a custom repository provider (for testing).
    pub fn open_with_provider<P: RepositoryProvider>(path: &Path, provider: &P) -> Result<Self> {
        let repo = provider.discover(path).map_err(|_| GwtError::NotGitRepo)?;
        let workdir = repo.workdir().ok_or(GwtError::NotGitRepo)?;

        // Check if we're in a worktree by looking for the common git dir
        // git rev-parse --git-common-dir returns the path to the main .git directory
        let output = Command::new("git")
            .args(["rev-parse", "--git-common-dir"])
            .current_dir(workdir)
            .output()
            .context("Failed to run git rev-parse")?;

        if !output.status.success() {
            Err(GwtError::NotGitRepo)?;
        }

        let common_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());

        // The main repo root is the parent of the .git directory
        // For worktrees, common_dir will be like /path/to/main/.git
        // For main repo, common_dir will be .git (relative) or /path/to/main/.git
        let repo_root = if common_dir.is_absolute() {
            common_dir.parent().unwrap_or(&common_dir).to_path_buf()
        } else {
            // Relative path means we're already in the main repo
            workdir.to_path_buf()
        };

        // Re-open repository from the main repo root to ensure consistent behavior
        let repo = provider
            .open(&repo_root)
            .map_err(|_| GwtError::NotGitRepo)?;

        Ok(Self { repo, repo_root })
    }

    /// Create from an already-opened repository (for testing).
    #[cfg(test)]
    pub fn from_repo(repo: Repository, repo_root: PathBuf) -> Self {
        Self { repo, repo_root }
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
        Err(GwtError::NoMainBranch)?
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
            Err(GwtError::GitCommandFailed {
                command: "git fetch origin".to_string(),
                stderr,
            })?;
        }

        Ok(())
    }

    /// Get the remote main branch ref (origin/main or origin/master)
    pub fn remote_main_ref(&self) -> Option<String> {
        for name in &["origin/main", "origin/master"] {
            if self.repo.find_branch(name, BranchType::Remote).is_ok() {
                return Some(name.to_string());
            }
        }
        None
    }

    /// Get the sync source ref (prefers origin/main, falls back to local main)
    pub fn get_sync_source_ref(&self) -> Option<String> {
        self.remote_main_ref()
            .or_else(|| self.main_branch_name().ok())
    }

    /// Get the best start point for a new branch
    ///
    /// - `use_remote`: If true (pull workflow), prefers origin/main for most up-to-date remote state.
    ///   If false (push workflow), uses local main to stay local-first.
    fn best_start_point(&self, use_remote: bool) -> Result<String> {
        if use_remote {
            // Pull workflow: prefer remote main branch (most up-to-date)
            if let Some(remote_main) = self.remote_main_ref() {
                return Ok(remote_main);
            }
        }

        // Push workflow or no remote: use local main branch
        self.main_branch_name()
    }

    /// Create a new worktree
    ///
    /// - `use_remote_start_point`: If true, new branches start from origin/main (pull workflow).
    ///   If false, new branches start from local main (push workflow).
    pub fn create_worktree(
        &self,
        branch: &str,
        path: &Path,
        use_remote_start_point: bool,
    ) -> Result<()> {
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
            let start_point = self.best_start_point(use_remote_start_point)?;
            Command::new("git")
                .args(["worktree", "add", "-b", branch, path_str, &start_point])
                .current_dir(&self.repo_root)
                .output()
                .context("Failed to run git worktree add")?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: "git worktree add".to_string(),
                stderr,
            })?;
        }

        Ok(())
    }

    /// Create a new worktree tracking a remote branch
    /// This creates a local branch that tracks origin/<branch>
    pub fn create_worktree_tracking(&self, branch: &str, path: &Path) -> Result<()> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let path_str = path.to_str().unwrap();
        let remote_ref = format!("origin/{}", branch);

        // Check if local branch already exists
        let local_branch_exists = self.repo.find_branch(branch, BranchType::Local).is_ok();

        let output = if local_branch_exists {
            // Local branch exists - use it directly without -b flag
            Command::new("git")
                .args(["worktree", "add", path_str, branch])
                .current_dir(&self.repo_root)
                .output()
                .context("Failed to run git worktree add")?
        } else {
            // Create new local branch tracking the remote
            // git worktree add -b <branch> <path> origin/<branch>
            Command::new("git")
                .args(["worktree", "add", "-b", branch, path_str, &remote_ref])
                .current_dir(&self.repo_root)
                .output()
                .context("Failed to run git worktree add")?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: "git worktree add".to_string(),
                stderr,
            })?;
        }

        // Set up tracking (in case git worktree add didn't do it automatically)
        let _ = Command::new("git")
            .args(["branch", "--set-upstream-to", &remote_ref, branch])
            .current_dir(path)
            .output();

        Ok(())
    }

    /// Check if a remote branch exists
    pub fn remote_branch_exists(&self, branch: &str) -> bool {
        let output = Command::new("git")
            .args(["ls-remote", "--heads", "origin", branch])
            .current_dir(&self.repo_root)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                !stdout.trim().is_empty()
            }
            _ => false,
        }
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
            Err(GwtError::GitCommandFailed {
                command: "git worktree remove".to_string(),
                stderr,
            })?;
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
            Err(GwtError::GitCommandFailed {
                command: format!("git checkout {}", main_branch),
                stderr,
            })?;
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
            Err(GwtError::GitCommandFailed {
                command: format!("git merge {}", branch),
                stderr,
            })?;
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
            Err(GwtError::GitCommandFailed {
                command: format!("git branch {} {}", flag, branch),
                stderr,
            })?;
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

        // For main branch: compare to origin/main to show unpushed commits
        let target = if branch == main_branch {
            self.remote_main_ref()?
        } else {
            main_branch
        };

        let output = Command::new("git")
            .args(["rev-list", "--count", &format!("{}..{}", target, branch)])
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
            .args(["rev-list", "--count", &format!("{}..{}", branch, target)])
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

    /// Get number of commits local main is ahead of origin/main
    pub fn main_ahead_of_origin(&self) -> Option<u32> {
        let main_branch = self.main_branch_name().ok()?;
        let remote_main = self.remote_main_ref()?;

        let output = Command::new("git")
            .args([
                "rev-list",
                "--count",
                &format!("{}..{}", remote_main, main_branch),
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

    /// Get the tracking branch for a given branch (origin/<branch>)
    /// Only returns a value if origin/<branch> exists - this ensures
    /// "vs self" compares to the same branch on the remote, not a
    /// different configured upstream (like origin/main for feature branches)
    fn get_tracking_branch(&self, path: &Path, branch: &str) -> Option<String> {
        // Only check if origin/<branch> exists (same branch name on remote)
        let remote_branch = format!("origin/{}", branch);
        let output = Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                &format!("refs/remotes/{}", remote_branch),
            ])
            .current_dir(path)
            .output()
            .ok()?;

        if output.status.success() {
            Some(remote_branch)
        } else {
            None
        }
    }

    /// Get commits ahead/behind vs tracking branch
    /// Returns (ahead, behind) count
    fn commits_vs_tracking(&self, path: &Path, branch: &str) -> Option<(u32, u32)> {
        let tracking = self.get_tracking_branch(path, branch)?;

        let output = Command::new("git")
            .args([
                "rev-list",
                "--left-right",
                "--count",
                &format!("{}...{}", branch, tracking),
            ])
            .current_dir(path)
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = stdout.trim().split('\t').collect();
            if parts.len() == 2 {
                let ahead = parts[0].parse().unwrap_or(0);
                let behind = parts[1].parse().unwrap_or(0);
                return Some((ahead, behind));
            }
        }

        None
    }

    /// Get diff stats vs main (lines added, lines removed)
    fn diff_stats_vs_main(&self, path: &Path, branch: &str) -> Option<(u32, u32)> {
        let main_branch = self.main_branch_name().ok()?;
        if branch == main_branch {
            return Some((0, 0));
        }

        let output = Command::new("git")
            .args([
                "diff",
                "--shortstat",
                &format!("{}...{}", main_branch, branch),
            ])
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
    /// For main branch, shows full commit history
    /// For worktrees, shows only commits ahead of main (branch-specific commits)
    fn recent_commits(&self, path: &Path, limit: usize, is_main: bool) -> Vec<CommitInfo> {
        let limit_str = limit.to_string();
        let mut args = vec!["log", "--format=%h|%s|%an|%cr", "-n", &limit_str];

        // For non-main worktrees, only show commits ahead of main
        let main_branch = self
            .main_branch_name()
            .unwrap_or_else(|_| "main".to_string());
        let range = format!("{}..HEAD", main_branch);
        if !is_main {
            args.push(&range);
        }

        let output = Command::new("git").args(&args).current_dir(path).output();

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

        // Get tracking branch comparison (commits ahead/behind origin/<branch>)
        let (tracking_ahead, tracking_behind) = if !branch.is_empty() {
            self.commits_vs_tracking(&info.path, branch)
                .map(|(a, b)| (Some(a), Some(b)))
                .unwrap_or((None, None))
        } else {
            (None, None)
        };

        let (uncommitted_added, uncommitted_removed) = self.uncommitted_stats(&info.path);
        let age_days = self.worktree_age_days(&info.path);
        let recent_commits = self.recent_commits(&info.path, 25, info.is_main);

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
            tracking_ahead,
            tracking_behind,
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

    /// Check if syncing a branch with main (merging main INTO branch) would cause conflicts
    /// Returns None if merge is clean, Some(conflicted_files) if there are conflicts
    pub fn check_sync_conflicts(&self, branch: &str) -> Result<Option<Vec<String>>> {
        let source_ref = self
            .get_sync_source_ref()
            .ok_or_else(|| anyhow::anyhow!("No main branch found to sync from"))?;

        // Use git merge-tree --write-tree to check for conflicts
        // Note: order is reversed from merge - we're merging source_ref INTO branch
        let output = Command::new("git")
            .args(["merge-tree", "--write-tree", branch, &source_ref])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git merge-tree")?;

        if output.status.success() {
            // Exit code 0 means clean merge possible
            Ok(None)
        } else {
            // Exit code non-zero means conflicts
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut conflicted_files = Vec::new();

            for line in stdout.lines() {
                if line.starts_with("CONFLICT")
                    && let Some(idx) = line.find(" in ")
                {
                    let filename = line[idx + 4..].trim();
                    conflicted_files.push(filename.to_string());
                }
            }

            if conflicted_files.is_empty() {
                for line in stdout.lines() {
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

            if conflicted_files.is_empty() {
                conflicted_files.push("(unable to determine specific files)".to_string());
            }

            Ok(Some(conflicted_files))
        }
    }

    /// Sync a branch with main by merging the sync source ref INTO the branch
    /// This updates the worktree's branch with the latest changes from main
    pub fn sync_branch_with_main(&self, worktree_path: &Path, source_ref: &str) -> Result<()> {
        let output = Command::new("git")
            .args([
                "merge",
                source_ref,
                "-m",
                &format!("Sync with {}", source_ref),
            ])
            .current_dir(worktree_path)
            .output()
            .context("Failed to run git merge")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: format!("git merge {}", source_ref),
                stderr,
            })?;
        }

        Ok(())
    }

    /// Push a branch to its remote tracking branch (origin/<branch>)
    /// If no tracking branch exists, creates one with -u flag
    pub fn push_to_remote(&self, path: &Path, branch: &str) -> Result<PushResult> {
        // Check if remote branch exists
        let tracking_exists = self.get_tracking_branch(path, branch).is_some();

        if tracking_exists {
            // Push to existing tracking branch
            let output = Command::new("git")
                .args(["push"])
                .current_dir(path)
                .output()
                .context("Failed to run git push")?;

            if output.status.success() {
                return Ok(PushResult::Success);
            }

            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Check if rejected (non-fast-forward)
            if stderr.contains("rejected")
                || stderr.contains("non-fast-forward")
                || stderr.contains("failed to push")
            {
                Ok(PushResult::Rejected { reason: stderr })
            } else {
                Err(GwtError::GitCommandFailed {
                    command: "git push".to_string(),
                    stderr,
                })?
            }
        } else {
            // Create remote branch with tracking
            let output = Command::new("git")
                .args(["push", "-u", "origin", branch])
                .current_dir(path)
                .output()
                .context("Failed to run git push -u")?;

            if output.status.success() {
                Ok(PushResult::CreatedRemoteBranch)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(GwtError::GitCommandFailed {
                    command: format!("git push -u origin {}", branch),
                    stderr,
                })?
            }
        }
    }

    /// Pull from the remote tracking branch into the current branch
    pub fn pull_from_remote(&self, path: &Path, branch: &str) -> Result<()> {
        // Check if tracking branch exists
        if self.get_tracking_branch(path, branch).is_none() {
            Err(GwtError::NoTrackingBranch {
                branch: branch.to_string(),
            })?;
        }

        let output = Command::new("git")
            .args(["pull"])
            .current_dir(path)
            .output()
            .context("Failed to run git pull")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: "git pull".to_string(),
                stderr,
            })?;
        }

        Ok(())
    }

    /// Push the main branch to origin
    pub fn push_main_to_remote(&self) -> Result<()> {
        let main_branch = self.main_branch_name()?;

        let output = Command::new("git")
            .args(["push", "origin", &main_branch])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git push")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: format!("git push origin {}", main_branch),
                stderr,
            })?;
        }

        Ok(())
    }

    /// Pull the main branch from origin
    pub fn pull_main_from_remote(&self) -> Result<()> {
        let main_branch = self.main_branch_name()?;

        // Make sure we're on main branch
        let output = Command::new("git")
            .args(["checkout", &main_branch])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to checkout main branch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: format!("git checkout {}", main_branch),
                stderr,
            })?;
        }

        // Pull from origin
        let output = Command::new("git")
            .args(["pull", "origin", &main_branch])
            .current_dir(&self.repo_root)
            .output()
            .context("Failed to run git pull")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GwtError::GitCommandFailed {
                command: format!("git pull origin {}", main_branch),
                stderr,
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    /// Helper to create a git repo with an initial commit
    fn create_test_repo(temp: &TempDir) -> Repository {
        let repo = Repository::init(temp.path()).unwrap();

        // Configure user for commits
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
            config.set_str("user.name", "Test User").unwrap();
        }

        // Create initial commit
        {
            let sig = repo.signature().unwrap();
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        // Ensure we have a main branch
        {
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            repo.branch("main", &commit, false).ok();
        }

        repo
    }

    #[test]
    fn test_worktree_manager_open_with_tempdir() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        let manager = WorktreeManager::open(temp.path()).unwrap();
        // Canonicalize paths to handle macOS /var -> /private/var symlink
        let expected = temp.path().canonicalize().unwrap();
        let actual = manager.repo_root().canonicalize().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_worktree_manager_from_repo() {
        let temp = TempDir::new().unwrap();
        let repo = create_test_repo(&temp);
        let repo_root = temp.path().to_path_buf();

        // Create WorktreeManager directly from repo (no filesystem discovery)
        let manager = WorktreeManager::from_repo(repo, repo_root.clone());

        assert_eq!(manager.repo_root(), &repo_root);
    }

    #[test]
    fn test_list_worktrees_main_only() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        let manager = WorktreeManager::open(temp.path()).unwrap();
        let worktrees = manager.list_worktrees().unwrap();

        // Should have exactly one worktree (main)
        assert_eq!(worktrees.len(), 1);
        assert!(worktrees[0].is_main);
        assert_eq!(worktrees[0].name, "main");
    }

    #[test]
    fn test_main_branch_name() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        let manager = WorktreeManager::open(temp.path()).unwrap();
        let main_branch = manager.main_branch_name().unwrap();

        // Should be "main" (created in create_test_repo)
        assert_eq!(main_branch, "main");
    }

    #[test]
    fn test_uncommitted_stats_clean_repo() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        let manager = WorktreeManager::open(temp.path()).unwrap();
        let (added, removed) = manager.uncommitted_stats(temp.path());

        // Clean repo should have no uncommitted changes
        assert_eq!(added, 0);
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_uncommitted_stats_with_changes() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        // Add a file with content and stage it so git diff --cached sees it
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        // Stage the file
        StdCommand::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let manager = WorktreeManager::open(temp.path()).unwrap();
        // Use canonicalized path to handle macOS symlinks
        let canonical_path = temp.path().canonicalize().unwrap();
        let (added, _removed) = manager.uncommitted_stats(&canonical_path);

        // Should have added lines (3 lines in the new file)
        assert!(added > 0);
    }

    #[test]
    fn test_open_with_provider_git2() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        // Use the Git2Provider explicitly
        let provider = Git2Provider;
        let manager = WorktreeManager::open_with_provider(temp.path(), &provider).unwrap();

        // Canonicalize paths to handle macOS /var -> /private/var symlink
        let expected = temp.path().canonicalize().unwrap();
        let actual = manager.repo_root().canonicalize().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_open_with_provider_not_a_repo() {
        let temp = TempDir::new().unwrap();
        // Don't initialize a git repo - just an empty directory

        let provider = Git2Provider;
        let result = WorktreeManager::open_with_provider(temp.path(), &provider);

        // Should fail with NotGitRepo error
        assert!(result.is_err());
    }

    #[test]
    fn test_project_name() {
        let temp = TempDir::new().unwrap();
        create_test_repo(&temp);

        let manager = WorktreeManager::open(temp.path()).unwrap();
        let project_name = manager.project_name().unwrap();

        // Project name should match temp dir name
        assert!(!project_name.is_empty());
    }

    #[test]
    fn test_parse_shortstat_empty() {
        let result = WorktreeManager::parse_shortstat("");
        assert_eq!(result, Some((0, 0)));
    }

    #[test]
    fn test_parse_shortstat_insertions_only() {
        let result = WorktreeManager::parse_shortstat("3 files changed, 100 insertions(+)");
        assert_eq!(result, Some((100, 0)));
    }

    #[test]
    fn test_parse_shortstat_deletions_only() {
        let result = WorktreeManager::parse_shortstat("2 files changed, 50 deletions(-)");
        assert_eq!(result, Some((0, 50)));
    }

    #[test]
    fn test_parse_shortstat_both() {
        let result =
            WorktreeManager::parse_shortstat("5 files changed, 100 insertions(+), 20 deletions(-)");
        assert_eq!(result, Some((100, 20)));
    }
}
