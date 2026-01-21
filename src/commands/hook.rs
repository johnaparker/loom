use anyhow::{Context, Result};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::claude::{time::now_iso8601, ClaudeEvent, ClaudeState};
use crate::config::GlobalConfig;

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

    // Get cache directory from config
    let cache_dir = get_cache_dir()?;

    // Determine project and worktree from cwd
    let (project, worktree) = resolve_project_worktree(cwd)?;

    // Handle the event based on type
    match event {
        "user-prompt" => handle_user_prompt(&cache_dir, &project, &worktree, session_id, &json)?,
        "stop" => handle_stop(&cache_dir, &project, &worktree, session_id, &json)?,
        "notification" => handle_notification(&cache_dir, &project, &worktree, session_id, &json)?,
        "session-start" => handle_session_start(&cache_dir, &project, &worktree, session_id, &json)?,
        "session-end" => handle_session_end(&cache_dir, &project, &worktree, session_id, &json)?,
        "tool-use" => handle_tool_use(&cache_dir, &project, &worktree, session_id, &json)?,
        "permission-request" => handle_permission_request(&cache_dir, &project, &worktree, session_id, &json)?,
        _ => {
            // Unknown event type, ignore
        }
    }

    Ok(())
}

/// Get the cache directory from config
fn get_cache_dir() -> Result<PathBuf> {
    // Check environment variable first (fast path)
    if let Ok(env_dir) = std::env::var("GWT_CACHE_DIR") {
        return Ok(PathBuf::from(env_dir));
    }

    // Load global config to get cache_dir
    let config = GlobalConfig::load()?;
    let dir = &config.cache_dir;
    if dir.starts_with("~") {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home.join(dir.strip_prefix("~/").unwrap_or(dir.strip_prefix("~").unwrap_or(dir))))
    } else {
        Ok(PathBuf::from(dir))
    }
}

fn handle_user_prompt(
    cache_dir: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Extract prompt preview (first 200 chars - will be truncated to fit display width)
    let prompt_preview = json["prompt"]
        .as_str()
        .map(|p| {
            let trimmed = p.trim();
            if trimmed.len() > 200 {
                format!("{}...", &trimmed[..197])
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

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, ClaudeState::Working)
}

fn handle_stop(
    cache_dir: &Path,
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

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, ClaudeState::Idle)
}

fn handle_notification(
    cache_dir: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Check notification type from explicit field
    let kind = json["type"].as_str().or_else(|| json["kind"].as_str());
    let message_raw = json["message"].as_str();
    let message = message_raw.map(|s| {
        if s.len() > 200 {
            format!("{}...", &s[..197])
        } else {
            s.to_string()
        }
    });

    // Determine state based on notification kind, or infer from message content
    let (new_state, effective_kind) = match kind {
        Some("permission_prompt") | Some("permission") => {
            (ClaudeState::WaitingPermission, Some("permission".to_string()))
        }
        Some("elicitation_dialog") => {
            // MCP tool elicitation - Claude is waiting for user input
            (ClaudeState::WaitingPermission, Some("elicitation".to_string()))
        }
        Some("idle_prompt") | Some("idle") => {
            (ClaudeState::Idle, Some("idle".to_string()))
        }
        _ => {
            // Fallback: infer notification type from message content
            if let Some(msg) = message_raw {
                if msg.contains("permission") || msg.contains("Permission")
                    || msg.contains("approval") || msg.contains("Approval")
                    || msg.contains("needs your attention")
                    || msg.contains("waiting for")
                {
                    (ClaudeState::WaitingPermission, Some("permission".to_string()))
                } else if msg.contains("waiting for your input") {
                    (ClaudeState::Idle, Some("idle".to_string()))
                } else {
                    (ClaudeState::Working, kind.map(|s| s.to_string()))
                }
            } else {
                (ClaudeState::Working, kind.map(|s| s.to_string()))
            }
        }
    };

    let event = ClaudeEvent {
        event_type: "Notification".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: effective_kind,
        message,
    };

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, new_state)
}

fn handle_session_start(
    cache_dir: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Extract source: startup, resume, clear, compact
    let source = json["source"].as_str();

    // Check if this is a clear operation - combine with previous SessionEnd
    if source == Some("clear") {
        if let Some(mut session) = crate::claude::read_state(cache_dir, project, worktree) {
            // Check if last event was SessionEnd with reason "clear"
            if let Some(last_event) = session.events.last() {
                if last_event.event_type == "SessionEnd"
                    && last_event.kind.as_deref() == Some("clear")
                {
                    // Replace the SessionEnd with a SessionCleared event
                    session.events.pop();
                    session.events.push(ClaudeEvent {
                        event_type: "SessionCleared".to_string(),
                        timestamp: now_iso8601(),
                        prompt_preview: None,
                        kind: None,
                        message: None,
                    });
                    session.session_id = session_id.to_string();
                    session.state = ClaudeState::Working;
                    return crate::claude::write_state(cache_dir, project, worktree, &session);
                }
            }
        }
    }

    // Normal session start
    let event = ClaudeEvent {
        event_type: "SessionStart".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: source.map(|s| s.to_string()),
        message: None,
    };

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, ClaudeState::Working)
}

fn handle_session_end(
    cache_dir: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Extract reason: clear, logout, prompt_input_exit, other
    let reason = json["reason"].as_str().map(|s| s.to_string());

    let event = ClaudeEvent {
        event_type: "SessionEnd".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: reason, // Store reason in kind field
        message: None,
    };

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, ClaudeState::Inactive)
}

fn handle_tool_use(
    cache_dir: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Extract tool name if available
    let tool_name = json["tool_name"]
        .as_str()
        .or_else(|| json["tool"].as_str())
        .map(|s| s.to_string());

    // Extract detailed info from tool_input based on tool type
    // Store up to 200 chars - dashboard will truncate to fit display width
    let tool_input = &json["tool_input"];
    let detail = match tool_name.as_deref() {
        Some("Read") => tool_input["file_path"]
            .as_str()
            .map(|p| shorten_path(p, 200)),
        Some("Edit") => tool_input["file_path"]
            .as_str()
            .map(|p| shorten_path(p, 200)),
        Some("Write") => tool_input["file_path"]
            .as_str()
            .map(|p| shorten_path(p, 200)),
        Some("Bash") => tool_input["command"]
            .as_str()
            .map(|c| truncate_str(c, 200)),
        Some("Grep") => tool_input["pattern"]
            .as_str()
            .map(|p| truncate_str(p, 200)),
        Some("Glob") => tool_input["pattern"]
            .as_str()
            .map(|p| truncate_str(p, 200)),
        Some("Task") => tool_input["description"]
            .as_str()
            .map(|d| truncate_str(d, 200)),
        Some("WebFetch") => tool_input["url"]
            .as_str()
            .map(|u| truncate_str(u, 200)),
        Some("WebSearch") => tool_input["query"]
            .as_str()
            .map(|q| truncate_str(q, 200)),
        _ => None,
    };

    let event = ClaudeEvent {
        event_type: "ToolUse".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: tool_name, // Store tool name in prompt_preview field
        kind: None,
        message: detail, // Store detail in message field
    };

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, ClaudeState::Working)
}

fn handle_permission_request(
    cache_dir: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
    json: &serde_json::Value,
) -> Result<()> {
    // Extract message if available
    let message = json["message"].as_str().map(|s| truncate_str(s, 200));

    let event = ClaudeEvent {
        event_type: "PermissionRequest".to_string(),
        timestamp: now_iso8601(),
        prompt_preview: None,
        kind: Some("permission".to_string()),
        message,
    };

    crate::claude::update_state_from_event(cache_dir, project, worktree, event, session_id, ClaudeState::WaitingPermission)
}

/// Shorten a file path by replacing home dir with ~ and keeping basename visible
fn shorten_path(path: &str, max_len: usize) -> String {
    // Replace home directory with ~
    let shortened = if let Some(home) = dirs::home_dir() {
        if let Some(home_str) = home.to_str() {
            if path.starts_with(home_str) {
                format!("~{}", &path[home_str.len()..])
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    if shortened.len() <= max_len {
        shortened
    } else {
        // Keep the end (filename) visible, truncate the middle
        let keep_end = 30.min(max_len / 2);
        let keep_start = max_len - keep_end - 3; // 3 for "..."
        format!(
            "{}...{}",
            &shortened[..keep_start],
            &shortened[shortened.len() - keep_end..]
        )
    }
}

/// Truncate a string to max length, adding ... if needed
fn truncate_str(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.len() > max_len {
        format!("{}...", &trimmed[..max_len.saturating_sub(3)])
    } else {
        trimmed.to_string()
    }
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

    // Check if this is a worktree (path contains "worktrees")
    let is_worktree = repo.path().to_string_lossy().contains("worktrees");

    // Worktree name: use "main" for main repo, basename for worktrees
    let worktree_name = if is_worktree {
        worktree_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        // Main repo always uses "main" as worktree name to match dashboard
        "main".to_string()
    };

    // Project name is derived from the main repo path
    // For worktrees, the common_dir points to the main repo's .git directory
    let project_name = if let Some(common_dir) = repo.path().parent() {
        if is_worktree {
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
