use anyhow::Result;
use colored::Colorize;

use super::ui;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::WorktreeManager;
use crate::linear;
use crate::output::{dry_run_action, dry_run_footer, dry_run_header};
use crate::sesh;

pub fn merge(name: &str, force: bool, dry_run: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    // Find the worktree
    let worktree = manager
        .get_worktree(name)?
        .ok_or_else(|| GwtError::WorktreeNotFound {
            name: name.to_string(),
        })?;

    if worktree.is_main {
        return Err(GwtError::CannotMergeMain.into());
    }

    let branch = worktree
        .branch
        .as_ref()
        .ok_or(GwtError::NoBranch)?
        .clone();

    let main_branch = manager.main_branch_name()?;
    let session_name = sesh::session_name(&project_name, name);

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
        dry_run_action(&format!("Unregister sesh session '{}'", session_name.cyan()));
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

    // Unregister from sesh
    if sesh::unregister_worktree(&project_name, name)? {
        println!("{} Unregistered sesh session", "✓".green());
    }

    // Clean up Linear cache
    let _ = linear::delete_metadata(&project_name, name);

    println!();
    println!(
        "{} Successfully merged and cleaned up '{}'",
        "✓".green().bold(),
        name
    );

    Ok(())
}
