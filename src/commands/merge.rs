use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::git::WorktreeManager;
use crate::sesh;

pub fn merge(name: &str, force: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    // Find the worktree
    let worktree = manager
        .get_worktree(name)?
        .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", name))?;

    if worktree.is_main {
        return Err(anyhow::anyhow!("Cannot merge the main worktree"));
    }

    let branch = worktree
        .branch
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Worktree has no associated branch"))?
        .clone();

    let main_branch = manager.main_branch_name()?;

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

    // Delete the branch
    println!("{} Deleting branch '{}'", "→".blue(), branch);
    manager.delete_branch(&branch, true)?;
    println!("{} Branch deleted", "✓".green());

    // Unregister from sesh
    if sesh::unregister_worktree(&project_name, name)? {
        println!("{} Unregistered sesh session", "✓".green());
    }

    println!();
    println!(
        "{} Successfully merged and cleaned up '{}'",
        "✓".green().bold(),
        name
    );

    Ok(())
}
