use anyhow::Result;
use colored::Colorize;

use crate::cli::Category;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::WorktreeManager;
use crate::sesh;
use crate::sync;
use crate::tmux;

pub fn new(branch: &str, category: Category) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;

    let project_name = config.project_name(&manager.project_name()?);
    let worktree_root = config.worktree_root()?;

    // Sanitize branch name for filesystem
    let sanitized_name = branch.replace('/', "-");

    // Check if worktree already exists
    if let Some(_existing) = manager.get_worktree(&sanitized_name)? {
        return Err(GwtError::WorktreeAlreadyExists {
            name: sanitized_name,
        }
        .into());
    }

    // Build worktree path: ~/worktrees/{project}/{category}/{name}
    let worktree_path = worktree_root
        .join(&project_name)
        .join(category.to_string())
        .join(&sanitized_name);

    // Fetch from origin to ensure we have the latest refs
    println!("{} Fetching from origin...", "→".blue());
    manager.fetch_origin()?;
    println!("{} Fetched latest from origin", "✓".green());

    println!(
        "{} Creating worktree for branch '{}' at {}",
        "→".blue(),
        branch.green(),
        worktree_path.display()
    );

    // Create the worktree (branches from origin/main if available)
    manager.create_worktree(branch, &worktree_path)?;
    println!("{} Worktree created", "✓".green());

    // Sync files from main repo
    let patterns = config.sync_patterns();
    if !patterns.is_empty() {
        let synced = sync::sync_files(manager.repo_root(), &worktree_path, &patterns)?;
        if !synced.is_empty() {
            println!("{} Synced files: {}", "✓".green(), synced.join(", "));
        }

        // Run direnv allow if .envrc was synced
        if synced.iter().any(|p| p == ".envrc") {
            if sync::run_direnv_allow(&worktree_path)? {
                println!("{} Ran direnv allow", "✓".green());
            }
        }
    }

    // Register with sesh if enabled
    let session_name = sesh::session_name(&project_name, &sanitized_name);
    if config.sesh_auto_register() {
        sesh::register_worktree(
            &project_name,
            &sanitized_name,
            worktree_path.to_str().unwrap(),
        )?;
        println!("{} Registered sesh session: {}", "✓".green(), session_name);
    }

    // Auto-switch to the new tmux session
    println!("{} Switching to session '{}'", "→".blue(), session_name.green());
    tmux::switch_to_session(&session_name, worktree_path.to_str().unwrap())?;

    Ok(())
}
