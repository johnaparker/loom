//! Shared CLI user interaction helpers.
//!
//! Centralizes confirmation prompts and other interactive elements
//! to ensure consistent UX across CLI commands.

use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

/// Result of an interactive menu action
pub enum MenuAction {
    /// Add a new item with the given name
    Add(String),
    /// Delete the item at the given index
    Delete(usize),
    /// Set the item at the given index as default (for categories)
    SetDefault(usize),
    /// User is done with the menu
    Done,
}

/// Prompt user for simple y/N confirmation.
///
/// Returns true if user confirms with 'y' or 'Y'.
pub fn confirm(prompt: &str) -> Result<bool> {
    print!("{} [y/N] ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Prompt user for destructive action confirmation.
///
/// Shows a warning and requires user to type "yes" to confirm.
/// Use for actions that cannot be undone (deleting with uncommitted changes, etc).
pub fn confirm_destructive(warning: &str, detail: &str) -> Result<bool> {
    println!();
    println!("{} {}", "⚠ Warning:".yellow().bold(), warning);
    println!("{}", detail.yellow());
    println!();
    print!("Type '{}' to confirm: ", "yes".red().bold());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim() == "yes")
}

/// Prompt user for branch deletion after worktree operation.
///
/// Returns true if user wants to delete the branch.
pub fn confirm_delete_branch(branch: &str) -> Result<bool> {
    confirm(&format!("Delete branch '{}'?", branch.yellow()))
}

/// Prompt for confirmation with default yes [Y/n].
///
/// Returns true if user confirms (Enter or 'y'), false if 'n'.
pub fn confirm_default_yes(prompt: &str) -> Result<bool> {
    print!("{} [Y/n] ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let trimmed = input.trim();
    // Default to yes if empty, otherwise check for 'n'
    Ok(trimmed.is_empty() || !trimmed.eq_ignore_ascii_case("n"))
}

/// Prompt for string input with optional default.
///
/// If a default is provided and user enters nothing, returns the default.
pub fn prompt_string(prompt: &str, default: Option<&str>) -> Result<String> {
    if let Some(def) = default {
        print!("{} [{}]: ", prompt, def.cyan());
    } else {
        print!("{}: ", prompt);
    }
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        if let Some(def) = default {
            Ok(def.to_string())
        } else {
            Ok(String::new())
        }
    } else {
        Ok(trimmed.to_string())
    }
}

/// Prompt for secret input (no echo).
///
/// Uses rpassword for secure input.
pub fn prompt_secret(prompt: &str) -> Result<String> {
    print!("{}: ", prompt);
    io::stdout().flush()?;
    let password = rpassword::read_password()?;
    Ok(password)
}

/// Display items in a numbered list format with optional default marker.
pub fn display_numbered_list(items: &[String], default_idx: Option<usize>) {
    for (i, item) in items.iter().enumerate() {
        let marker = if Some(i) == default_idx {
            " (default)".cyan().to_string()
        } else {
            String::new()
        };
        println!("  [{}] {}{}", (i + 1).to_string().cyan(), item, marker);
    }
}

/// Prompt for a menu action.
///
/// Returns the action the user selected based on input.
fn prompt_menu_action(items: &[String], allow_set_default: bool) -> Result<MenuAction> {
    println!();
    println!("Actions:");
    println!("  [{}] Add new item", "a".cyan());
    println!("  [{}] Delete item (enter number)", "d".cyan());
    if allow_set_default {
        println!("  [{}] Set default (enter number)", "s".cyan());
    }
    println!("  [{}] Done", "q".cyan());
    println!();

    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    match input.as_str() {
        "a" => {
            let name = prompt_string("Enter name", None)?;
            if name.is_empty() {
                println!("{}", "Name cannot be empty".red());
                return prompt_menu_action(items, allow_set_default);
            }
            Ok(MenuAction::Add(name))
        }
        "d" => {
            let idx_str = prompt_string("Enter item number to delete", None)?;
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx >= 1 && idx <= items.len() {
                    Ok(MenuAction::Delete(idx - 1))
                } else {
                    println!("{}", "Invalid item number".red());
                    prompt_menu_action(items, allow_set_default)
                }
            } else {
                println!("{}", "Please enter a valid number".red());
                prompt_menu_action(items, allow_set_default)
            }
        }
        "s" if allow_set_default => {
            let idx_str = prompt_string("Enter item number to set as default", None)?;
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx >= 1 && idx <= items.len() {
                    Ok(MenuAction::SetDefault(idx - 1))
                } else {
                    println!("{}", "Invalid item number".red());
                    prompt_menu_action(items, allow_set_default)
                }
            } else {
                println!("{}", "Please enter a valid number".red());
                prompt_menu_action(items, allow_set_default)
            }
        }
        "q" | "" => Ok(MenuAction::Done),
        _ => {
            println!("{}", "Unknown action".red());
            prompt_menu_action(items, allow_set_default)
        }
    }
}

/// Run an interactive menu for managing a list of items.
///
/// Shows numbered list with available actions and handles user input.
/// Returns the action the user selected.
pub fn interactive_list_menu(
    items: &[String],
    _default_idx: Option<usize>,
    allow_set_default: bool,
) -> Result<MenuAction> {
    prompt_menu_action(items, allow_set_default)
}
