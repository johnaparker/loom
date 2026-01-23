use anyhow::Result;
use colored::Colorize;

use super::operations;
use crate::cli::Category;
use crate::config::Config;
use crate::git::WorktreeManager;
use crate::github::{self, GitHubUrlType};
use crate::tmux;

pub fn new(branch: &str, category: Category) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;

    let project_name = config.project_name(&manager.project_name()?);
    let cache_dir = config.cache_dir()?;

    // Pre-resolve GitHub URLs (CLI-specific feature with output)
    let resolved_branch = resolve_github_url(branch, &manager)?;

    // Use shared worktree creation logic
    let result = operations::create_worktree(
        &manager,
        &config,
        &project_name,
        &resolved_branch,
        category,
        &cache_dir,
    )?;

    // Print results
    println!(
        "{} Creating worktree for branch '{}' at {}",
        "→".blue(),
        result.git_branch.green(),
        result.worktree_path.display()
    );

    if result.tracked_remote {
        println!(
            "{} Tracking existing remote branch origin/{}",
            "→".blue(),
            result.git_branch.cyan()
        );
    }
    println!("{} Worktree created", "✓".green());

    if let Some(ref issue) = result.issue {
        println!(
            "{} Cached Linear metadata for {}",
            "✓".green(),
            issue.id.cyan()
        );

        if result.linear_status_updated {
            println!("{} Updated Linear issue to In Progress", "✓".green());
        }
    }

    if !result.synced_files.is_empty() {
        println!(
            "{} Synced files: {}",
            "✓".green(),
            result.synced_files.join(", ")
        );
    }

    if result.direnv_allowed {
        println!("{} Ran direnv allow", "✓".green());
    }

    // Auto-switch to the new tmux session
    println!(
        "{} Switching to session '{}'",
        "→".blue(),
        result.session_name.green()
    );
    tmux::switch_to_session(&result.session_name, result.worktree_path.to_str().unwrap())?;

    Ok(())
}

/// Resolve GitHub URLs to branch names (CLI-specific with output)
/// Returns the original input if not a GitHub URL
fn resolve_github_url(input: &str, manager: &WorktreeManager) -> Result<String> {
    if !github::is_github_url(input) {
        return Ok(input.to_string());
    }

    if let Some(url_info) = github::parse_github_url(input) {
        let branch = match url_info.url_type {
            GitHubUrlType::PullRequest(pr_num) => {
                println!(
                    "{} Fetching branch from PR #{}...",
                    "→".blue(),
                    pr_num
                );
                github::get_pr_branch(manager.repo_root(), pr_num)?
            }
            GitHubUrlType::Branch(ref b) | GitHubUrlType::Tree(ref b) => b.clone(),
        };
        return Ok(branch);
    }

    // Couldn't parse, return as-is
    Ok(input.to_string())
}
