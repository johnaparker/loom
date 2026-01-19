use anyhow::{Context, Result};
use std::io::{self, Read};
use std::path::Path;

use crate::claude::{ClaudeEvent, ClaudeSession, ClaudeState};

/// Handle hook events from Claude Code
/// Reads JSON from stdin, updates cache file
pub fn hook(event: &str) -> Result<()> {
    // Read JSON from stdin
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("Failed to read from stdin")?;

    // Parse JSON (allow empty input for some events)
    let json: serde_json::Value = if input.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&input).context("Failed to parse JSON from stdin")?
    };

    // Extract common fields
    let cwd = json["cwd"].as_str().unwrap_or(".");
    let session_id = json["session_id"].as_str().unwrap_or("unknown");

    // Determine project and worktree from cwd
    let (project, worktree) = resolve_project_worktree(cwd)?;

    // Handle the event based on type
    match event {
        "user-prompt" => handle_user_prompt(&project, &worktree, session_id, &json)?,
        "stop" => handle_stop(&project, &worktree, session_id, &json)?,
        "notification" => handle_notification(&project, &worktree, session_id, &json)?,
        "session-start" => handle_session_start(&project, &worktree, session_id)?,
        "session-end" => handle_session_end(&project, &worktree, session_id)?,
        _ => {
            // Unknown event type, ignore
        }
    }

    Ok(())
}

fn handle_user_prompt(
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Extract prompt preview (first 50 chars)
    let prompt_preview = json["prompt"]
        .as_str()
        .map(|p| {
            let trimmed = p.trim();
            if trimmed.len() > 50 {
                format!("{}...", &trimmed[..47])
            } else {
                trimmed.to_string()
            }
        });

    let event = ClaudeEvent {
        event_type: "UserPromptSubmit".to_string(),
        timestamp: now_iso8601(),
        prompt_preview,
        kind: None,
        message: None,
    };

    crate::claude::update_state_from_event(project, worktree, event, session_id, ClaudeState::Working)
}

fn handle_stop(
    project: &str,
    worktree: &str,
    session_id: &str,
    _json: &serde_json::Value,
) -> Result<()> {
    let event = ClaudeEvent {
        event_type: "Stop".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: None,
        message: None,
    };

    crate::claude::update_state_from_event(project, worktree, event, session_id, ClaudeState::Idle)
}

fn handle_notification(
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Check notification type
    let kind = json["type"].as_str().or_else(|| json["kind"].as_str());
    let message = json["message"].as_str().map(|s| {
        if s.len() > 100 {
            format!("{}...", &s[..97])
        } else {
            s.to_string()
        }
    });

    // Determine state based on notification kind
    let new_state = match kind {
        Some("permission_prompt") | Some("permission") => ClaudeState::WaitingPermission,
        Some("idle_prompt") | Some("idle") => ClaudeState::Idle,
        _ => ClaudeState::Working, // Default to working for unknown notifications
    };

    let event = ClaudeEvent {
        event_type: "Notification".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: kind.map(|s| s.to_string()),
        message,
    };

    crate::claude::update_state_from_event(project, worktree, event, session_id, new_state)
}

fn handle_session_start(project: &str, worktree: &str, session_id: &str) -> Result<()> {
    // Create a fresh session
    let session = ClaudeSession::new(session_id.to_string(), ClaudeState::Working);
    crate::claude::write_state(project, worktree, &session)
}

fn handle_session_end(project: &str, worktree: &str, session_id: &str) -> Result<()> {
    let event = ClaudeEvent {
        event_type: "SessionEnd".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: None,
        message: None,
    };

    crate::claude::update_state_from_event(project, worktree, event, session_id, ClaudeState::Inactive)
}

/// Resolve project name and worktree name from the current working directory
fn resolve_project_worktree(cwd: &str) -> Result<(String, String)> {
    let path = Path::new(cwd);

    // Try to find the git repository root
    let repo = git2::Repository::discover(path)
        .context("Not in a git repository")?;

    // Get the worktree root (commondir for worktrees, workdir for main)
    let worktree_root = repo.workdir()
        .context("Cannot determine worktree directory")?;

    // Worktree name is the basename of the worktree root
    let worktree_name = worktree_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("main")
        .to_string();

    // Project name is derived from the main repo path
    // For worktrees, the common_dir points to the main repo's .git directory
    let project_name = if let Some(common_dir) = repo.path().parent() {
        // Check if this is a worktree (path contains "worktrees")
        if repo.path().to_string_lossy().contains("worktrees") {
            // This is a worktree, repo.path() is .git/worktrees/<name>
            // common_dir (parent) is .git/worktrees
            // Go up two levels to get main repo root
            common_dir
                .parent() // from .git/worktrees to .git
                .and_then(|p| p.parent()) // from .git to repo root
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        } else {
            // This is the main repo
            common_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        }
    } else {
        "unknown".to_string()
    };

    Ok((project_name, worktree_name))
}

/// Get current time as ISO 8601 string
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap();

    let secs = duration.as_secs();

    // Calculate date/time components
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Simplified date calculation (days since 1970-01-01)
    let mut year = 1970i32;
    let mut remaining_days = days as i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [i32; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for days_in_month in days_in_months {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }
    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
