//! Shared CLI user interaction helpers.
//!
//! Centralizes confirmation prompts and other interactive elements
//! to ensure consistent UX across CLI commands.

use anyhow::Result;
use colored::Colorize;
use std::io::{self, Write};

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
