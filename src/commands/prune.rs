//! Prune command - delete worktrees with merged PRs.
//!
//! Only available in pull workflow since it relies on GitHub PR status.

use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

use super::operations;
use super::ui;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::{WorktreeInfo, WorktreeManager};
use crate::github::{self, GitHubPR};
use crate::output::{dry_run_action, dry_run_footer, dry_run_header, dry_run_warning};

/// Information about a worktree with a merged PR
#[derive(Debug, Clone)]
pub struct MergedWorktreeInfo {
    pub worktree: WorktreeInfo,
    pub pr: GitHubPR,
    pub has_uncommitted: bool,
    pub uncommitted_added: u32,
    pub uncommitted_removed: u32,
}

pub fn prune(force: bool, dry_run: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);
    let cache_dir = config.cache_dir()?;

    // Check that we're in pull workflow
    if !config.is_pull_workflow() {
        return Err(GwtError::PruneNotAllowedInPushWorkflow.into());
    }

    // Check that gh CLI is available
    github::check_gh_cli()?;

    println!("{} Checking PRs for merged status...", "->".cyan());
    io::stdout().flush()?;

    // Find all worktrees with merged PRs
    let worktrees = manager.list_worktrees()?;
    let merged = find_merged_worktrees(&manager, &worktrees)?;

    if merged.is_empty() {
        println!("{} No worktrees with merged PRs found.", "✓".green());
        return Ok(());
    }

    // Check for dirty worktrees
    let dirty_count = merged.iter().filter(|m| m.has_uncommitted).count();

    // Dry run mode - preview actions
    if dry_run {
        dry_run_header();
        for m in &merged {
            if m.has_uncommitted {
                dry_run_warning(&format!(
                    "Worktree '{}' has uncommitted changes: {} {} lines",
                    m.worktree.name.yellow(),
                    format!("+{}", m.uncommitted_added).green(),
                    format!("-{}", m.uncommitted_removed).red()
                ));
            }
            dry_run_action(&format!(
                "Delete worktree '{}' (PR #{} merged)",
                m.worktree.name.yellow(),
                m.pr.number
            ));
            if let Some(ref branch) = m.worktree.branch {
                dry_run_action(&format!("Delete branch '{}'", branch.green()));
            }
        }
        dry_run_footer();
        return Ok(());
    }

    // Show what will be pruned
    println!();
    println!(
        "Found {} worktree(s) with merged PRs:",
        merged.len().to_string().cyan()
    );
    println!();

    for m in &merged {
        let dirty_indicator = if m.has_uncommitted {
            format!(
                " {} uncommitted: {} {}",
                "!".yellow(),
                format!("+{}", m.uncommitted_added).green(),
                format!("-{}", m.uncommitted_removed).red()
            )
        } else {
            String::new()
        };

        println!(
            "  {} {} (PR #{}){dirty_indicator}",
            "-".dimmed(),
            m.worktree.name.yellow(),
            m.pr.number.to_string().cyan()
        );
    }
    println!();

    // If dirty worktrees exist and not forced, require stricter confirmation
    if dirty_count > 0 && !force {
        let warning = format!(
            "{} worktree(s) have uncommitted changes that will be lost!",
            dirty_count
        );
        if !ui::confirm_destructive(&warning, "Use --force to skip this check next time")? {
            println!("Cancelled.");
            return Ok(());
        }
    } else {
        // Normal confirmation
        let prompt = format!("Delete {} worktree(s)?", merged.len());
        if !ui::confirm(&prompt)? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Delete each worktree
    let mut deleted = 0;
    let total = merged.len();

    for m in &merged {
        print!(
            "{} Deleting '{}' (PR #{})...",
            "->".blue(),
            m.worktree.name.yellow(),
            m.pr.number
        );
        io::stdout().flush()?;

        let use_force = force || m.has_uncommitted;

        match operations::delete_worktree(
            &manager,
            &cache_dir,
            &project_name,
            &m.worktree.name,
            &m.worktree.path,
            m.worktree.branch.as_deref(),
            true, // Always delete branch for merged PRs
            use_force,
        ) {
            Ok(_) => {
                println!(" {}", "✓".green());
                deleted += 1;
            }
            Err(e) => {
                println!(" {} {}", "✗".red(), e);
            }
        }
    }

    println!();
    if deleted == total {
        println!(
            "{} Pruned {} worktree(s)",
            "✓".green(),
            deleted.to_string().cyan()
        );
    } else {
        println!(
            "{} Pruned {} of {} worktree(s)",
            "!".yellow(),
            deleted.to_string().cyan(),
            total.to_string().cyan()
        );
    }

    Ok(())
}

/// Find all worktrees that have merged PRs
fn find_merged_worktrees(
    manager: &WorktreeManager,
    worktrees: &[WorktreeInfo],
) -> Result<Vec<MergedWorktreeInfo>> {
    let mut merged = Vec::new();

    for wt in worktrees {
        // Skip main worktree
        if wt.is_main {
            continue;
        }

        // Skip worktrees without branches
        let Some(ref branch) = wt.branch else {
            continue;
        };

        // Check PR status
        match github::get_pr_for_branch(manager.repo_root(), branch) {
            Ok(Some(pr)) if pr.state == "MERGED" => {
                let (uncommitted_added, uncommitted_removed) =
                    manager.uncommitted_stats(&wt.path);
                let has_uncommitted = uncommitted_added > 0 || uncommitted_removed > 0;

                merged.push(MergedWorktreeInfo {
                    worktree: wt.clone(),
                    pr,
                    has_uncommitted,
                    uncommitted_added,
                    uncommitted_removed,
                });
            }
            Ok(Some(_)) | Ok(None) | Err(_) => {
                // Not merged or no PR - skip
            }
        }
    }

    Ok(merged)
}
