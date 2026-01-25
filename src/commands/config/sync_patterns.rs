//! Sync patterns subcommand implementation.
//!
//! Interactive menu for managing file patterns to sync from main to worktrees.

use anyhow::Result;
use colored::Colorize;

use super::ConfigTarget;
use crate::cli::ConfigScope;
use crate::commands::ui::{MenuAction, display_numbered_list, interactive_list_menu};
use crate::config::{GlobalConfig, ProjectConfig};

/// Run the sync-patterns interactive menu.
pub fn run(scope: ConfigScope) -> Result<()> {
    let target = ConfigTarget::resolve(scope)?;

    if target.is_project_scope {
        run_project_scope(&target)
    } else {
        run_user_scope()
    }
}

fn run_user_scope() -> Result<()> {
    let mut config = GlobalConfig::load()?;

    println!();
    println!("{}", "Sync Patterns (user scope)".bold());
    println!("{}", "─".repeat(40));

    loop {
        println!();
        println!("Current patterns:");
        display_numbered_list(&config.sync.patterns, None);

        match interactive_list_menu(&config.sync.patterns, None, false)? {
            MenuAction::Add(pattern) => {
                if config.sync.patterns.contains(&pattern) {
                    println!("{} Pattern '{}' already exists", "✗".red(), pattern);
                    continue;
                }
                config.sync.patterns.push(pattern.clone());
                println!("{} Added pattern '{}'", "✓".green(), pattern.cyan());
            }
            MenuAction::Delete(idx) => {
                let removed = config.sync.patterns.remove(idx);
                println!("{} Removed pattern '{}'", "✓".green(), removed.cyan());
            }
            MenuAction::SetDefault(_) => unreachable!(),
            MenuAction::Done => break,
        }
    }

    config.save()?;
    println!();
    println!("{} Sync patterns saved", "✓".green());

    Ok(())
}

fn run_project_scope(target: &ConfigTarget) -> Result<()> {
    let repo_root = target
        .repo_root
        .as_ref()
        .expect("project scope should have repo_root");

    // Load existing config or create new one
    let mut config = ProjectConfig::load(None, repo_root)?.unwrap_or_else(|| ProjectConfig {
        project_name: None,
        sync: Default::default(),
        git: Default::default(),
        linear: None,
        github: None,
        diffview: None,
        claude: None,
    });

    println!();
    println!("{}", "Sync Patterns (project scope)".bold());
    println!("{}", "─".repeat(40));
    println!(
        "{}",
        "Project patterns are additive to user patterns.".dimmed()
    );

    loop {
        println!();
        println!("Current project patterns:");
        if config.sync.patterns.is_empty() {
            println!("  {}", "(none)".dimmed());
        } else {
            display_numbered_list(&config.sync.patterns, None);
        }

        match interactive_list_menu(&config.sync.patterns, None, false)? {
            MenuAction::Add(pattern) => {
                if config.sync.patterns.contains(&pattern) {
                    println!("{} Pattern '{}' already exists", "✗".red(), pattern);
                    continue;
                }
                config.sync.patterns.push(pattern.clone());
                println!("{} Added pattern '{}'", "✓".green(), pattern.cyan());
            }
            MenuAction::Delete(idx) => {
                if idx < config.sync.patterns.len() {
                    let removed = config.sync.patterns.remove(idx);
                    println!("{} Removed pattern '{}'", "✓".green(), removed.cyan());
                } else {
                    println!("{} Invalid index", "✗".red());
                }
            }
            MenuAction::SetDefault(_) => unreachable!(),
            MenuAction::Done => break,
        }
    }

    config.save(repo_root)?;
    println!();
    println!("{} Project sync patterns saved", "✓".green());

    Ok(())
}
