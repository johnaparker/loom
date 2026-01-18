use anyhow::Result;
use colored::Colorize;
use std::process::Command;

/// Get the current tmux session name
pub fn current_session() -> Option<String> {
    if std::env::var("TMUX").is_err() {
        return None;
    }

    Command::new("tmux")
        .args(["display-message", "-p", "#S"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Check if we're in a tmux session
pub fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Check if a tmux session exists
pub fn session_exists(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a tmux session (detached)
pub fn create_session(session_name: &str, path: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name, "-c", path])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to create tmux session: {}", stderr));
    }
    Ok(())
}

/// Switch to a tmux session (creates if needed)
pub fn switch_to_session(session_name: &str, path: &str) -> Result<()> {
    if !in_tmux() {
        println!(
            "{} Not in a tmux session. Navigate to: {}",
            "!".yellow(),
            path.cyan()
        );
        return Ok(());
    }

    if !session_exists(session_name) {
        create_session(session_name, path)?;
    }

    let output = Command::new("tmux")
        .args(["switch-client", "-t", session_name])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to switch tmux session: {}", stderr));
    }

    Ok(())
}
