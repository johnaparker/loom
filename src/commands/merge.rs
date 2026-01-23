use anyhow::Result;
use colored::Colorize;

use super::ui;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::WorktreeManager;
use crate::linear;
use crate::output::{dry_run_action, dry_run_footer, dry_run_header};

pub fn merge(name: &str, force: bool, dry_run: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;

    // Merge to local main is only available in push workflow
    if config.is_pull_workflow() {
        return Err(GwtError::MergeNotAllowedInPullWorkflow.into());
    }

    let project_name = config.project_name(&manager.project_name()?);
    let cache_dir = config.cache_dir()?;

    // Find the worktree
    let worktree = manager
        .get_worktree(name)?
        .ok_or_else(|| GwtError::WorktreeNotFound {
            name: name.to_string(),
        })?;

    if worktree.is_main {
        Err(GwtError::CannotMergeMain)?;
    }

    let branch = worktree
        .branch
        .as_ref()
        .ok_or(GwtError::NoBranch)?
        .clone();

    let main_branch = manager.main_branch_name()?;

    // Check for Linear issue metadata (used for status updates)
    // Fallback: if no cache, try to extract issue from branch name
    let linear_issue = linear::read_metadata(&cache_dir, &project_name, name)
        .ok()
        .flatten()
        .or_else(|| {
            let branch = worktree.branch.as_deref()?;
            let prefix = config.linear_prefix()?;
            let api_key = config.linear_api_key()?;
            let issue_id = linear::extract_issue_id(branch, prefix)?;
            linear::get_issue(api_key, &issue_id).ok()
        });

    // Dry run mode - preview actions
    if dry_run {
        dry_run_header();
        dry_run_action(&format!(
            "Merge branch '{}' into '{}'",
            branch.green(),
            main_branch.yellow()
        ));
        dry_run_action(&format!(
            "Remove worktree at {}",
            worktree.path.display().to_string().dimmed()
        ));
        dry_run_action(&format!("Offer to delete branch '{}'", branch.green()));
        if config.linear_auto_update_status() && config.linear_api_key().is_some() {
            if let Some(ref issue) = linear_issue {
                dry_run_action(&format!(
                    "Update Linear issue {} to Done",
                    issue.id.cyan()
                ));
            }
        }
        dry_run_footer();
        return Ok(());
    }

    println!(
        "{} Merging '{}' into '{}'",
        "→".blue(),
        branch.green(),
        main_branch.yellow()
    );

    // Merge the branch to main
    manager.merge_to_main(&branch)?;
    println!("{} Merged successfully", "✓".green());

    // Update Linear issue to "Done" (non-critical)
    if config.linear_auto_update_status() {
        if let Some(api_key) = config.linear_api_key() {
            if let Some(ref issue) = linear_issue {
                match linear::update_issue_status(api_key, &issue.id, "completed") {
                    Ok(()) => println!("{} Updated Linear issue {} to Done", "✓".green(), issue.id.cyan()),
                    Err(e) => eprintln!("{} Could not update Linear status: {}", "⚠".yellow(), e),
                }
            }
        }
    }

    // Remove the worktree
    println!("{} Removing worktree", "→".blue());
    manager.remove_worktree(&worktree.path, force)?;
    println!("{} Worktree removed", "✓".green());

    // Prompt to delete branch
    if ui::confirm_delete_branch(&branch)? {
        manager.delete_branch(&branch, true)?;
        println!("{} Branch '{}' deleted", "✓".green(), branch);
    } else {
        println!("{} Branch '{}' kept", "→".blue(), branch);
    }

    // Clean up Linear cache
    let _ = linear::delete_metadata(&cache_dir, &project_name, name);

    println!();
    println!(
        "{} Successfully merged and cleaned up '{}'",
        "✓".green().bold(),
        name
    );

    Ok(())
}
