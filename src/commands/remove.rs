use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

use crate::config::Config;
use crate::core::FuzzyMatcher;
use crate::error::GwtError;
use crate::git::{WorktreeInfo, WorktreeManager};
use crate::linear;
use crate::output::{dry_run_action, dry_run_footer, dry_run_header};
use crate::sesh;
use crate::tmux;
use crate::tui::Picker;

pub fn remove(name: Option<&str>, force: bool, dry_run: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    // Fetch from origin to ensure we have accurate lag information (only for TUI picker)
    if name.is_none() && !dry_run {
        print!("{} Fetching from origin...", "→".blue());
        io::stdout().flush()?;
        let fetch_result = manager.fetch_origin();
        // Clear the fetching message
        print!("\r{}\r", " ".repeat(30));
        io::stdout().flush()?;

        if fetch_result.is_err() {
            println!(
                "{} Could not fetch from origin (working offline)",
                "!".yellow()
            );
        }
    }

    let worktrees = manager.list_worktrees()?;

    // Filter out main worktree
    let removable: Vec<_> = worktrees.into_iter().filter(|wt| !wt.is_main).collect();

    if removable.is_empty() {
        println!("No worktrees to remove.");
        return Ok(());
    }

    // Determine which worktree to remove
    let worktree = if let Some(query) = name {
        // Fuzzy match
        let matched = fuzzy_match_worktree(&removable, query)?;

        if !dry_run {
            // Confirm with user (skip confirmation in dry run mode)
            print!(
                "Remove worktree '{}' at {}? [y/N] ",
                matched.name.yellow(),
                matched.path.display().to_string().dimmed()
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Cancelled.");
                return Ok(());
            }
        }

        matched
    } else {
        // Show picker (skip in dry run mode - require name for dry run)
        if dry_run {
            return Err(anyhow::anyhow!(
                "Dry run requires a worktree name. Usage: gwt remove <name> --dry-run"
            ));
        }

        let items: Vec<(String, String, String)> = removable
            .iter()
            .map(|wt| {
                // Calculate commits behind for lag indicator
                let commits_behind = if let Some(ref branch) = wt.branch {
                    manager.commits_behind_remote_main(&wt.path, branch)
                } else {
                    None
                };

                let lag_indicator = match commits_behind {
                    Some(n) if n > 0 => format!(" ↓{}", n),
                    _ => String::new(),
                };

                let details = format!(
                    "{}{}{}",
                    wt.branch
                        .as_ref()
                        .map(|b| format!("[{}]", b))
                        .unwrap_or_default(),
                    wt.category
                        .as_ref()
                        .map(|c| format!(" ({})", c))
                        .unwrap_or_default(),
                    lag_indicator
                );
                let path = wt.path.to_string_lossy().to_string();
                (wt.name.clone(), details, path)
            })
            .collect();

        let mut picker = Picker::new(items)?;
        let selected = picker.run()?;

        match selected {
            Some(index) => removable.into_iter().nth(index).unwrap(),
            None => return Ok(()),
        }
    };

    // Check for uncommitted changes
    let (uncommitted_added, uncommitted_removed) = manager.uncommitted_stats(&worktree.path);
    let has_uncommitted = uncommitted_added > 0 || uncommitted_removed > 0;
    let needs_force = has_uncommitted && !force;

    // If dirty and not already forced, require stricter confirmation
    if needs_force && !dry_run {
        println!();
        println!(
            "{} This worktree has uncommitted changes: {} {} lines",
            "⚠ Warning:".yellow().bold(),
            format!("+{}", uncommitted_added).green(),
            format!("-{}", uncommitted_removed).red()
        );
        println!(
            "{}",
            "These changes will be permanently lost!".yellow()
        );
        println!();
        print!(
            "Type '{}' to confirm deletion: ",
            "yes".red().bold()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let session_name = sesh::session_name(&project_name, &worktree.name);
    let use_force = force || has_uncommitted;

    // Dry run mode - preview actions
    if dry_run {
        dry_run_header();
        if has_uncommitted {
            dry_run_action(&format!(
                "{} Worktree has uncommitted changes: {} {} lines",
                "⚠".yellow(),
                format!("+{}", uncommitted_added).green(),
                format!("-{}", uncommitted_removed).red()
            ));
            dry_run_action(&format!(
                "Force remove worktree '{}' at {}",
                worktree.name.yellow(),
                worktree.path.display().to_string().dimmed()
            ));
        } else {
            dry_run_action(&format!(
                "Remove worktree '{}' at {}",
                worktree.name.yellow(),
                worktree.path.display().to_string().dimmed()
            ));
        }
        if tmux::session_exists(&session_name) {
            dry_run_action(&format!("Kill tmux session '{}'", session_name.cyan()));
        }
        dry_run_action(&format!(
            "Unregister sesh session '{}'",
            session_name.cyan()
        ));
        if let Some(ref branch) = worktree.branch {
            dry_run_action(&format!("Offer to delete branch '{}'", branch.green()));
        }
        dry_run_footer();
        return Ok(());
    }

    println!(
        "{} Removing worktree '{}'{}",
        "→".blue(),
        worktree.name.yellow(),
        if use_force { " (force)" } else { "" }
    );

    // Remove the worktree
    manager.remove_worktree(&worktree.path, use_force)?;
    println!("{} Worktree removed", "✓".green());

    // Kill tmux session if it exists
    if tmux::kill_session(&session_name) {
        println!("{} Killed tmux session", "✓".green());
    }

    // Unregister from sesh
    if sesh::unregister_worktree(&project_name, &worktree.name)? {
        println!("{} Unregistered sesh session", "✓".green());
    }

    // Clean up Linear cache
    let _ = linear::delete_metadata(&project_name, &worktree.name);

    // Offer to delete the branch
    if let Some(ref branch) = worktree.branch {
        print!("Delete branch '{}'? [y/N] ", branch.yellow());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            manager.delete_branch(branch, use_force)?;
            println!("{} Branch '{}' deleted", "✓".green(), branch);
        } else {
            println!("{} Branch '{}' kept", "→".blue(), branch);
        }
    }

    Ok(())
}

fn fuzzy_match_worktree(worktrees: &[WorktreeInfo], query: &str) -> Result<WorktreeInfo> {
    let mut matcher = FuzzyMatcher::new();

    let best_idx = matcher.best_match(worktrees, query, |wt| {
        format!("{} {}", wt.name, wt.branch.as_deref().unwrap_or(""))
    });

    match best_idx {
        Some(idx) => Ok(worktrees[idx].clone()),
        None => Err(GwtError::NoMatch {
            query: query.to_string(),
        }
        .into()),
    }
}
