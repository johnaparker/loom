use anyhow::Result;
use colored::Colorize;
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
use std::io::{self, Write};

use crate::config::Config;
use crate::git::{WorktreeInfo, WorktreeManager};
use crate::sesh;
use crate::tmux;
use crate::tui::Picker;

pub fn remove(name: Option<&str>, force: bool) -> Result<()> {
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
        // Fuzzy match and confirm
        let matched = fuzzy_match_worktree(&removable, query)?;

        // Confirm with user
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

        matched
    } else {
        // Show picker
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

    println!(
        "{} Removing worktree '{}'",
        "→".blue(),
        worktree.name.yellow()
    );

    // Remove the worktree
    manager.remove_worktree(&worktree.path, force)?;
    println!("{} Worktree removed", "✓".green());

    // Kill tmux session if it exists
    let session_name = sesh::session_name(&project_name, &worktree.name);
    if tmux::kill_session(&session_name) {
        println!("{} Killed tmux session", "✓".green());
    }

    // Unregister from sesh
    if sesh::unregister_worktree(&project_name, &worktree.name)? {
        println!("{} Unregistered sesh session", "✓".green());
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
        return Err(anyhow::anyhow!("No worktree matching '{}'", query));
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(worktrees[scored[0].0].clone())
}
