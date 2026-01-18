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

    println!("{} Switching to '{}'", "→".blue(), session_name.green());
    tmux::switch_to_session(&session_name, &main_path)?;

    Ok(())
}
