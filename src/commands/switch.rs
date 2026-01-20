use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::core::FuzzyMatcher;
use crate::git::{WorktreeInfo, WorktreeManager};
use crate::tmux;
use crate::tui::Picker;

pub fn switch(name: Option<&str>) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    // Fetch from origin to ensure we have accurate lag information
    print!("{} Fetching from origin...", "→".blue());
    std::io::Write::flush(&mut std::io::stdout())?;
    let fetch_result = manager.fetch_origin();
    // Clear the fetching message
    print!("\r{}\r", " ".repeat(30));
    std::io::Write::flush(&mut std::io::stdout())?;

    if fetch_result.is_err() {
        println!(
            "{} Could not fetch from origin (working offline)",
            "!".yellow()
        );
    }

    let worktrees = manager.list_worktrees()?;

    // Pre-calculate commits behind for main worktree
    let main_behind = manager.main_behind_origin();

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    // If name provided, fuzzy match and switch directly
    if let Some(query) = name {
        return switch_by_name(&worktrees, &project_name, query);
    }

    // Create picker items
    let items: Vec<(String, String, String)> = worktrees
        .iter()
        .map(|wt| {
            let display_name = if wt.is_main {
                format!("{} (main repo)", wt.name)
            } else {
                wt.name.clone()
            };

            // Calculate commits behind for lag indicator
            let commits_behind = if wt.is_main {
                main_behind
            } else if let Some(ref branch) = wt.branch {
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

            (display_name, details, path)
        })
        .collect();

    // Run the picker
    let mut picker = Picker::new(items.clone())?;
    let selected = picker.run()?;

    if let Some(index) = selected {
        let worktree = &worktrees[index];
        let session_name = if worktree.is_main {
            format!("{}/main", project_name)
        } else {
            format!("{}/{}", project_name, worktree.name)
        };
        let path = &items[index].2;

        tmux::switch_to_session(&session_name, path)?;
    }

    Ok(())
}

fn switch_by_name(worktrees: &[WorktreeInfo], project_name: &str, query: &str) -> Result<()> {
    let mut matcher = FuzzyMatcher::new();

    // Find the best matching worktree
    let best_idx = matcher.best_match(worktrees, query, |wt| {
        format!("{} {}", wt.name, wt.branch.as_deref().unwrap_or(""))
    });

    let Some(idx) = best_idx else {
        println!("{} No worktree matching '{}'", "!".yellow(), query);
        return Ok(());
    };

    let best_match = &worktrees[idx];

    let session_name = if best_match.is_main {
        format!("{}/main", project_name)
    } else {
        format!("{}/{}", project_name, best_match.name)
    };
    let path = best_match.path.to_string_lossy().to_string();

    println!(
        "{} Switching to '{}' (matched '{}')",
        "→".blue(),
        session_name.green(),
        best_match.name
    );

    tmux::switch_to_session(&session_name, &path)?;
    Ok(())
}
