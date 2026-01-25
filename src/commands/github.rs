use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::error::GroveError;
use crate::git::WorktreeManager;
use crate::github;

pub fn github_cmd() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);
    let cache_dir = config.cache_dir()?;

    // Find worktree from current directory
    let worktrees = manager.list_worktrees()?;
    let worktree = worktrees
        .into_iter()
        .find(|w| current_dir.starts_with(&w.path))
        .ok_or(GroveError::NotInWorktree)?;

    // Get the branch name
    let branch = worktree.branch.ok_or(GroveError::NoBranch)?;

    // Don't try to open PR for main branch
    if worktree.is_main {
        println!("{} Cannot open PR for main branch", "!".yellow());
        return Ok(());
    }

    println!(
        "{} Looking for PR for branch '{}'...",
        "→".blue(),
        branch.cyan()
    );

    // Try to get PR info and cache it
    if let Ok(Some(pr)) = github::get_pr_for_branch(manager.repo_root(), &branch) {
        // Cache the PR info
        let _ = github::write_pr_cache(
            &cache_dir,
            &project_name,
            &worktree.name,
            &github::CachedPRState::Found(pr.clone()),
        );
        github::open_url(&pr.url)?;
        println!("{} Opening PR #{}: {}", "✓".green(), pr.number, pr.title);
    } else {
        // No PR exists - open create PR page
        let (owner, repo) = github::get_repo_info(manager.repo_root())?;
        let create_url = github::get_create_pr_url(&owner, &repo, &branch);
        github::open_url(&create_url)?;
        println!(
            "{} Opening create PR page for branch '{}'",
            "✓".green(),
            branch
        );
    }

    Ok(())
}
