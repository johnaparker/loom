use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

use crate::config::Config;
use crate::core::capitalize_first;
use crate::git::{WorktreeInfo, WorktreeManager};

pub fn list() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;

    let worktrees = manager.list_worktrees()?;

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    // Get configured categories
    let categories = config.categories();
    let default_category = config.default_category();

    // Separate main from others and group by category
    let (main_wt, others): (Vec<_>, Vec<_>) = worktrees.into_iter().partition(|wt| wt.is_main);

    // Build dynamic category groups
    let mut category_groups: HashMap<String, Vec<&WorktreeInfo>> = HashMap::new();
    for cat in &categories {
        category_groups.insert(cat.clone(), Vec::new());
    }

    for wt in &others {
        let cat = wt.category.as_deref().unwrap_or(default_category);
        // If the category exists in configured list, use it; otherwise use default
        if categories.contains(&cat.to_string()) {
            category_groups.entry(cat.to_string()).or_default().push(wt);
        } else {
            category_groups
                .entry(default_category.to_string())
                .or_default()
                .push(wt);
        }
    }

    // Print main first
    if let Some(wt) = main_wt.first() {
        println!("{}", "Main".bold().underline());
        print_worktree(wt);
        println!();
    }

    // Print each category in configured order
    for cat in &categories {
        if let Some(worktrees) = category_groups.get(cat)
            && !worktrees.is_empty()
        {
            let display_name = capitalize_first(cat);
            println!("{}", display_name.bold().underline());
            for wt in worktrees {
                print_worktree(wt);
            }
            println!();
        }
    }

    Ok(())
}

fn print_worktree(wt: &WorktreeInfo) {
    let name = if wt.is_main {
        wt.name.yellow().to_string()
    } else {
        wt.name.green().to_string()
    };

    let branch = wt
        .branch
        .as_ref()
        .map(|b| format!("[{}]", b).cyan().to_string())
        .unwrap_or_default();

    println!("  {} {}", name, branch);
    println!("    {}", wt.path.display().to_string().dimmed());
}
