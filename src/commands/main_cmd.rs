use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::git::WorktreeManager;
use crate::tmux;

pub fn main_cmd() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;
    let project_name = config.project_name(&manager.project_name()?);

    let session_name = format!("{}/main", project_name);
    let main_path = manager.repo_root().to_string_lossy().to_string();

    // Check if already in main session
    if let Some(current) = tmux::current_session() {
        if current == session_name {
            println!("{} Already in main session", "!".yellow());
            return Ok(());
        }
    }

    // Check if we're in tmux
    if !tmux::in_tmux() {
        println!(
            "{} Not in a tmux session. Navigate to: {}",
            "!".yellow(),
            main_path.cyan()
        );
        return Ok(());
    }

    // Auto-pull from origin in pull workflow when main is behind
    if config.is_pull_workflow() {
        if let Some(behind) = manager.main_behind_origin() {
            if behind > 0 {
                println!(
                    "{} Main is {} commit(s) behind origin, pulling...",
                    "→".blue(),
                    behind
                );
                match manager.pull_main_from_remote() {
                    Ok(()) => println!("{} Pulled {} commit(s) from origin", "✓".green(), behind),
                    Err(e) => eprintln!("{} Could not auto-pull: {}", "⚠".yellow(), e),
                }
            }
        }
    }

    println!("{} Switching to '{}'", "→".blue(), session_name.green());
    tmux::switch_to_session(&session_name, &main_path)?;

    Ok(())
}
