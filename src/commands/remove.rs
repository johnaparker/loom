use anyhow::Result;
use colored::Colorize;
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
use std::io::{self, Write};

use crate::config::Config;
use crate::error::GwtError;
use crate::git::{WorktreeInfo, WorktreeManager};
use crate::output::{dry_run_action, dry_run_footer, dry_run_header};
use crate::sesh;
use crate::tmux;
use crate::tui::Picker;

pub fn remove(name: Option<&str>, force: bool, dry_run: bool) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

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
                let details = format!(
                    "{} {}",
                    wt.branch
                        .as_ref()
                        .map(|b| format!("[{}]", b))
                        .unwrap_or_default(),
                    wt.category
                        .as_ref()
                        .map(|c| format!("({})", c))
                        .unwrap_or_default()
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

    let session_name = sesh::session_name(&project_name, &worktree.name);

    // Dry run mode - preview actions
    if dry_run {
        dry_run_header();
        dry_run_action(&format!(
            "Remove worktree '{}' at {}",
            worktree.name.yellow(),
            worktree.path.display().to_string().dimmed()
        ));
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
        "{} Removing worktree '{}'",
        "→".blue(),
        worktree.name.yellow()
    );

    // Remove the worktree
    manager.remove_worktree(&worktree.path, force)?;
    println!("{} Worktree removed", "✓".green());

    // Kill tmux session if it exists
    if tmux::kill_session(&session_name) {
        println!("{} Killed tmux session", "✓".green());
    }

    // Unregister from sesh
    if sesh::unregister_worktree(&project_name, &worktree.name)? {
        println!("{} Unregistered sesh session", "✓".green());
    }

    // Offer to delete the branch
    if let Some(ref branch) = worktree.branch {
        print!("Delete branch '{}'? [y/N] ", branch.yellow());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            manager.delete_branch(branch, force)?;
            println!("{} Branch '{}' deleted", "✓".green(), branch);
        } else {
            println!("{} Branch '{}' kept", "→".blue(), branch);
        }
    }

    Ok(())
}

fn fuzzy_match_worktree(worktrees: &[WorktreeInfo], query: &str) -> Result<WorktreeInfo> {
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);

    let mut scored: Vec<(usize, u16)> = worktrees
        .iter()
        .enumerate()
        .filter_map(|(i, wt)| {
            let haystack = format!("{} {}", wt.name, wt.branch.as_deref().unwrap_or(""));
            let mut haystack_buf = Vec::new();
            let haystack_str = Utf32Str::new(&haystack, &mut haystack_buf);
            let mut needle_buf = Vec::new();
            let needle_str = Utf32Str::new(query, &mut needle_buf);

            matcher
                .fuzzy_match(haystack_str, needle_str)
                .map(|score| (i, score))
        })
        .collect();

    if scored.is_empty() {
        return Err(GwtError::NoMatch {
            query: query.to_string(),
        }
        .into());
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(worktrees[scored[0].0].clone())
}
