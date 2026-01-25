use anyhow::Result;
use colored::Colorize;

use crate::error::GroveError;
use crate::git::WorktreeManager;
use crate::output::{dry_run_action, dry_run_footer, dry_run_header};

pub fn sync(name: Option<&str>, dry_run: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;

    // Find the worktree - either by name or from current directory
    let worktree = if let Some(name) = name {
        manager
            .get_worktree(name)?
            .ok_or_else(|| GroveError::WorktreeNotFound {
                name: name.to_string(),
            })?
    } else {
        // Find worktree from current directory
        let worktrees = manager.list_worktrees()?;
        worktrees
            .into_iter()
            .find(|w| current_dir.starts_with(&w.path))
            .ok_or(GroveError::NotInWorktree)?
    };

    let branch = worktree
        .branch
        .as_ref()
        .ok_or(GroveError::NoBranch)?
        .clone();

    // Get sync source ref
    let source_ref = manager
        .get_sync_source_ref()
        .ok_or_else(|| anyhow::anyhow!("No main branch found to sync from"))?;

    // Check if already up to date
    let commits_behind = manager.commits_behind_remote_main(&worktree.path, &branch);
    if commits_behind == Some(0) {
        println!(
            "{} Branch '{}' is already up to date with {}",
            "✓".green(),
            branch.green(),
            source_ref.yellow()
        );
        return Ok(());
    }

    // Dry run mode - preview actions
    if dry_run {
        dry_run_header();
        dry_run_action("Fetch from origin");
        dry_run_action(&format!(
            "Merge '{}' into '{}'",
            source_ref.yellow(),
            branch.green()
        ));
        if let Some(behind) = commits_behind {
            dry_run_action(&format!("Will merge {} commit(s)", behind));
        }
        dry_run_footer();
        return Ok(());
    }

    // Check for uncommitted changes
    let (uncommitted_added, uncommitted_removed) = manager.uncommitted_stats(&worktree.path);
    if uncommitted_added > 0 || uncommitted_removed > 0 {
        Err(GroveError::UncommittedChanges)?;
    }

    // Fetch from origin first
    println!("{} Fetching from origin", "→".blue());
    manager.fetch_origin()?;

    // Re-check commits behind after fetch
    let commits_behind = manager.commits_behind_remote_main(&worktree.path, &branch);
    if commits_behind == Some(0) {
        println!(
            "{} Branch '{}' is already up to date with {}",
            "✓".green(),
            branch.green(),
            source_ref.yellow()
        );
        return Ok(());
    }

    // Check for conflicts
    match manager.check_sync_conflicts(&branch) {
        Ok(Some(conflicts)) => {
            eprintln!(
                "{} Cannot sync: {} file(s) have conflicts",
                "✗".red(),
                conflicts.len()
            );
            for file in &conflicts {
                eprintln!("  {} {}", "-".red(), file);
            }
            return Err(anyhow::anyhow!("Merge conflicts detected"));
        }
        Ok(None) => {
            // No conflicts, proceed
        }
        Err(e) => {
            // Warning but allow attempt
            eprintln!(
                "{} Warning: Could not check for conflicts: {}",
                "!".yellow(),
                e
            );
        }
    }

    // Perform sync
    println!(
        "{} Syncing '{}' with '{}'",
        "→".blue(),
        branch.green(),
        source_ref.yellow()
    );
    manager.sync_branch_with_main(&worktree.path, &source_ref)?;

    println!();
    println!(
        "{} Successfully synced '{}' with {}",
        "✓".green().bold(),
        branch.green(),
        source_ref.yellow()
    );

    Ok(())
}
