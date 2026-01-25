//! Categories subcommand implementation.
//!
//! Interactive menu for managing worktree categories (max 4).
//! Categories are global (user scope only).

use anyhow::Result;
use colored::Colorize;

use crate::commands::ui::{MenuAction, display_numbered_list, interactive_list_menu};
use crate::config::GlobalConfig;

const MAX_CATEGORIES: usize = 4;

/// Run the categories interactive menu.
pub fn run() -> Result<()> {
    let mut config = GlobalConfig::load()?;

    println!();
    println!("{}", "Worktree Categories".bold());
    println!("{}", "─".repeat(40));

    loop {
        // Find the default category index
        let default_idx = config
            .categories
            .iter()
            .position(|c| c == &config.default_category);

        println!();
        println!("Current categories:");
        display_numbered_list(&config.categories, default_idx);

        match interactive_list_menu(&config.categories, true)? {
            MenuAction::Add(name) => {
                if config.categories.len() >= MAX_CATEGORIES {
                    println!(
                        "{} Maximum {} categories allowed",
                        "✗".red(),
                        MAX_CATEGORIES
                    );
                    continue;
                }
                if config.categories.contains(&name) {
                    println!("{} Category '{}' already exists", "✗".red(), name);
                    continue;
                }
                config.categories.push(name.clone());
                println!("{} Added category '{}'", "✓".green(), name.cyan());
            }
            MenuAction::Delete(idx) => {
                if config.categories.len() <= 1 {
                    println!("{} Cannot delete the last category", "✗".red());
                    continue;
                }
                if config.categories[idx] == config.default_category {
                    println!(
                        "{} Cannot delete the default category. Set a different default first.",
                        "✗".red()
                    );
                    continue;
                }
                let removed = config.categories.remove(idx);
                println!("{} Removed category '{}'", "✓".green(), removed.cyan());
            }
            MenuAction::SetDefault(idx) => {
                let new_default = config.categories[idx].clone();
                config.default_category = new_default.clone();
                println!("{} Set '{}' as default", "✓".green(), new_default.cyan());
            }
            MenuAction::Done => break,
        }
    }

    config.save()?;
    println!();
    println!("{} Categories saved", "✓".green());

    Ok(())
}
