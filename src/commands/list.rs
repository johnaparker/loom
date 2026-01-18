use anyhow::Result;
use colored::Colorize;

use crate::git::{WorktreeInfo, WorktreeManager};

pub fn list() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;

    let worktrees = manager.list_worktrees()?;

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    // Separate main from others and group by category
    let (main_wt, others): (Vec<_>, Vec<_>) = worktrees.into_iter().partition(|wt| wt.is_main);

    let mut dev: Vec<&WorktreeInfo> = Vec::new();
    let mut review: Vec<&WorktreeInfo> = Vec::new();
    let mut demo: Vec<&WorktreeInfo> = Vec::new();

    for wt in &others {
        match wt.category.as_deref() {
            Some("dev") => dev.push(wt),
            Some("review") => review.push(wt),
            Some("demo") => demo.push(wt),
            _ => dev.push(wt), // default to dev
        }
    }

    // Print main first
    if let Some(wt) = main_wt.first() {
        println!("{}", "Main".bold().underline());
        print_worktree(wt);
        println!();
    }

    // Print each category with header
    if !dev.is_empty() {
        println!("{}", "Dev".bold().underline());
        for wt in dev {
            print_worktree(wt);
        }
        println!();
    }

    if !review.is_empty() {
        println!("{}", "Review".bold().underline());
        for wt in review {
            print_worktree(wt);
        }
        println!();
    }

    if !demo.is_empty() {
        println!("{}", "Demo".bold().underline());
        for wt in demo {
            print_worktree(wt);
        }
        println!();
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
