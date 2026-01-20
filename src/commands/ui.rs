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
