//! Tmux session management.
//!
//! Provides functionality for creating, switching, and managing tmux sessions
//! for worktree navigation.

use anyhow::Result;
use colored::Colorize;
use std::process::Command;

/// Build a shell command to launch Claude with the appropriate mode.
///
/// - `plan_mode`: If true, uses `/plan` suffix for plan-first workflow.
///   If false, uses `--permission-mode acceptEdits` for immediate editing.
/// - `issue_id`: Optional Linear issue ID to include in the initial prompt.
///
/// Returns a bash command string suitable for tmux session creation.
pub fn build_claude_command(plan_mode: bool, issue_id: Option<&str>) -> String {
    match (plan_mode, issue_id) {
        (true, Some(id)) => format!("bash -c 'claude \"work on {} /plan\"; exec $SHELL'", id),
        (false, Some(id)) => format!(
            "bash -c 'claude --permission-mode acceptEdits \"work on {}\"; exec $SHELL'",
            id
        ),
        (true, None) => "bash -c 'claude; exec $SHELL'".to_string(),
        (false, None) => "bash -c 'claude --permission-mode acceptEdits; exec $SHELL'".to_string(),
    }
}

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

/// Create a tmux session with a specific command in the first window
pub fn create_session_with_command(session_name: &str, path: &str, command: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name, "-c", path, command])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to create tmux session: {}", stderr));
    }
    Ok(())
}

/// Create a new window in an existing tmux session
pub fn create_window(
    session_name: &str,
    window_name: &str,
    path: &str,
    command: &str,
) -> Result<()> {
    let output = Command::new("tmux")
        .args([
            "new-window",
            "-t",
            session_name,
            "-n",
            window_name,
            "-c",
            path,
            command,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to create tmux window: {}", stderr));
    }
    Ok(())
}

/// Kill a tmux session if it exists
pub fn kill_session(session_name: &str) -> bool {
    if !session_exists(session_name) {
        return false;
    }

    Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

/// Location of a pane within a tmux session
pub struct PaneLocation {
    pub window_index: u32,
    pub pane_index: u32,
}

/// Check if a character is in the Braille Unicode range (U+2800-U+28FF)
fn is_braille_char(c: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&c)
}

/// Find a pane in a session with a title containing the given text.
/// Searches ALL panes across ALL windows (regardless of window name).
/// Returns window and pane indices if found.
pub fn find_pane_with_title(session_name: &str, title_contains: &str) -> Option<PaneLocation> {
    find_pane_matching(session_name, |title| title.contains(title_contains))
}

/// Find a pane in a session with a title starting with any Braille character.
/// Claude Code uses various Braille patterns in its status line (U+2800-U+28FF).
pub fn find_pane_with_braille_title(session_name: &str) -> Option<PaneLocation> {
    find_pane_matching(session_name, |title| {
        title.chars().next().is_some_and(is_braille_char)
    })
}

/// Find a pane in a session matching a predicate on the pane title.
fn find_pane_matching<F>(session_name: &str, predicate: F) -> Option<PaneLocation>
where
    F: Fn(&str) -> bool,
{
    // List all panes with format: window_index:pane_index:pane_title
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            session_name,
            "-s", // all panes in session (not -a which is all sessions)
            "-F",
            "#{window_index}:#{pane_index}:#{pane_title}",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() == 3 && predicate(parts[2]) {
            let window_index = parts[0].parse().ok()?;
            let pane_index = parts[1].parse().ok()?;
            return Some(PaneLocation {
                window_index,
                pane_index,
            });
        }
    }
    None
}

/// Switch to a specific pane within a session.
/// Selects both the window and the pane.
pub fn switch_to_pane(session_name: &str, location: &PaneLocation) -> Result<()> {
    // Target format: session:window.pane
    let target = format!(
        "{}:{}.{}",
        session_name, location.window_index, location.pane_index
    );

    // First select the window
    let output = Command::new("tmux")
        .args([
            "select-window",
            "-t",
            &format!("{}:{}", session_name, location.window_index),
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to select window: {}", stderr));
    }

    // Then select the pane
    let output = Command::new("tmux")
        .args(["select-pane", "-t", &target])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to select pane: {}", stderr));
    }

    Ok(())
}

/// Find an active Claude pane in a session.
///
/// Claude uses various pane titles: "Claude" (normal), "✳" (multi-pane),
/// or Braille patterns (spinner/stats).
///
/// Returns the pane location if found, None otherwise.
pub fn find_active_claude_pane(session: &str) -> Option<PaneLocation> {
    find_pane_with_title(session, "Claude")
        .or_else(|| find_pane_with_title(session, "✳"))
        .or_else(|| find_pane_with_braille_title(session))
}

/// Build a bash command to open nvim with DiffviewOpen for reviewing changes.
///
/// Uses merge-base to show only the worktree's changes since branching,
/// avoiding showing changes main has that the worktree doesn't.
/// Wraps in bash -c so command substitution is evaluated, and
/// exec $SHELL keeps window open after nvim exits.
pub fn build_review_command(main_branch: &str) -> String {
    format!(
        "bash -c 'nvim -c \"DiffviewOpen $(git merge-base {} HEAD)\"; exec $SHELL'",
        main_branch
    )
}
