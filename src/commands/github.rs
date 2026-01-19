use anyhow::Result;
use colored::Colorize;

use crate::error::GwtError;
use crate::git::WorktreeManager;
use crate::github;

pub fn github_cmd() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;

    // Find worktree from current directory
    let worktrees = manager.list_worktrees()?;
    let worktree = worktrees
        .into_iter()
        .find(|w| current_dir.starts_with(&w.path))
        .ok_or(GwtError::NotInWorktree)?;

    // Get the branch name
    let branch = worktree.branch.ok_or(GwtError::NoBranch)?;

    // Don't try to open PR for main branch
    if worktree.is_main {
        println!(
            "{} Cannot open PR for main branch",
            "!".yellow()
        );
        return Ok(());
    }

    println!(
        "{} Looking for PR for branch '{}'...",
        "→".blue(),
        branch.cyan()
    );

    // Open PR or create-PR page
    let message = github::open_pr_or_create(manager.repo_root(), &branch)?;
    println!("{} {}", "✓".green(), message);

    Ok(())
}
