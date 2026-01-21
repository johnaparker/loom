//! State management for Claude sessions.

use anyhow::Result;
use std::path::Path;

use super::cache::{read_state, write_state};
use super::time::now_iso8601;
use super::types::{ClaudeEvent, ClaudeSession, ClaudeState};

/// Update the state from a hook event
pub fn update_state_from_event(
    base: &Path,
    project: &str,
    worktree: &str,
    event: ClaudeEvent,
    session_id: &str,
    new_state: ClaudeState,
) -> Result<()> {
    // Read existing session or create new one
    let mut session = read_state(base, project, worktree).unwrap_or_else(|| {
        ClaudeSession::new(session_id.to_string(), ClaudeState::Inactive)
    });

    // Update session ID if changed (preserve events for continuous log)
    session.session_id = session_id.to_string();

    // Update state and add event
    session.state = new_state;
    session.add_event(event);

    write_state(base, project, worktree, &session)
}

/// Get the effective state based on the most recent event
/// This ensures the displayed state matches what the event log shows
pub fn effective_state(session: &ClaudeSession) -> ClaudeState {
    if session.is_stale() {
        return ClaudeState::Inactive;
    }

    // Derive state from the most recent event
    if let Some(event) = session.events.last() {
        match event.event_type.as_str() {
            "SessionEnd" => ClaudeState::Inactive,
            "Stop" => ClaudeState::Idle,
            "Notification" => {
                // Check notification kind
                match event.kind.as_deref() {
                    Some(k) if k.contains("permission") => ClaudeState::WaitingPermission,
                    Some(k) if k.contains("idle") => ClaudeState::Idle,
                    _ => ClaudeState::Working,
                }
            }
            "PermissionRequest" => ClaudeState::WaitingPermission,
            // SessionCleared means the session was reset, Claude is now idle
            "SessionCleared" => ClaudeState::Idle,
            // SessionStart: startup and resume mean Claude is waiting for user input
            // compact means Claude is doing background work
            "SessionStart" => {
                match event.kind.as_deref() {
                    Some("startup") | Some("resume") => ClaudeState::Idle,
                    _ => ClaudeState::Working,
                }
            }
            // UserPromptSubmit, ToolUse all mean working
            _ => ClaudeState::Working,
        }
    } else {
        session.state.clone()
    }
}

/// Check if we're currently inside a subagent context
pub fn is_in_subagent(base: &Path, project: &str, worktree: &str) -> bool {
    read_state(base, project, worktree)
        .map(|s| s.subagent_depth > 0)
        .unwrap_or(false)
}

/// Increment the subagent depth (called when Task tool starts)
pub fn increment_subagent_depth(
    base: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
) -> Result<()> {
    let mut session = read_state(base, project, worktree).unwrap_or_else(|| {
        ClaudeSession::new(session_id.to_string(), ClaudeState::Working)
    });

    session.session_id = session_id.to_string();
    session.subagent_depth = session.subagent_depth.saturating_add(1);
    session.last_updated = now_iso8601();

    write_state(base, project, worktree, &session)
}

/// Decrement the subagent depth (called when Task tool completes)
pub fn decrement_subagent_depth(
    base: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
) -> Result<()> {
    let mut session = read_state(base, project, worktree).unwrap_or_else(|| {
        ClaudeSession::new(session_id.to_string(), ClaudeState::Working)
    });

    session.session_id = session_id.to_string();
    session.subagent_depth = session.subagent_depth.saturating_sub(1);
    session.last_updated = now_iso8601();

    write_state(base, project, worktree, &session)
}

/// Touch the session to update the timestamp without adding an event
/// Used to keep the session fresh during subagent execution
pub fn touch_session(
    base: &Path,
    project: &str,
    worktree: &str,
    session_id: &str,
) -> Result<()> {
    let mut session = read_state(base, project, worktree).unwrap_or_else(|| {
        ClaudeSession::new(session_id.to_string(), ClaudeState::Working)
    });

    session.session_id = session_id.to_string();
    session.last_updated = now_iso8601();

    write_state(base, project, worktree, &session)
}
