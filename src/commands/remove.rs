use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::git::WorktreeManager;
use crate::sesh;

pub fn remove(name: &str, force: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    // Find the worktree
    let worktree = manager
        .get_worktree(name)?
        .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", name))?;

    if worktree.is_main {
        return Err(anyhow::anyhow!("Cannot remove the main worktree"));
    }

    println!(
        "{} Removing worktree '{}'",
        "→".blue(),
        name.yellow()
    );

    // Remove the worktree
    manager.remove_worktree(&worktree.path, force)?;
    println!("{} Worktree removed", "✓".green());

    // Unregister from sesh
    if sesh::unregister_worktree(&project_name, name)? {
        println!("{} Unregistered sesh session", "✓".green());
    }

    Ok(())
}
