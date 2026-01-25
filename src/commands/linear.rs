use anyhow::Result;
use colored::Colorize;
use std::process::Command;

use crate::config::Config;
use crate::error::LoomError;
use crate::git::WorktreeManager;
use crate::linear;

/// Convert a Linear web URL to the desktop app URL scheme
/// e.g., https://linear.app/team/issue/ABC-123 -> linear://team/issue/ABC-123
pub fn to_desktop_url(url: &str) -> String {
    url.replace("https://linear.app/", "linear://")
}

pub fn linear_cmd() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);
    let cache_dir = config.cache_dir()?;

    // Find worktree from current directory
    let worktrees = manager.list_worktrees()?;
    let worktree = worktrees
        .into_iter()
        .find(|w| current_dir.starts_with(&w.path))
        .ok_or(LoomError::NotInWorktree)?;

    // Read Linear metadata from cache
    let issue = linear::read_metadata(&cache_dir, &project_name, &worktree.name)?
        .ok_or(LoomError::NoLinearIssue)?;

    if issue.url.is_empty() {
        Err(LoomError::NoLinearIssue)?;
    }

    println!(
        "{} Opening {} ({})",
        "→".blue(),
        issue.id.green(),
        issue.title.dimmed()
    );

    // Convert to desktop app URL scheme
    let desktop_url = to_desktop_url(&issue.url);

    // Open URL with platform-specific command
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&desktop_url).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&desktop_url).spawn()?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", &desktop_url])
            .spawn()?;
    }

    Ok(())
}
