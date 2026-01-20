//! State management for Claude sessions.

use anyhow::Result;

use super::cache::{read_state, write_state};
use super::types::{ClaudeEvent, ClaudeSession, ClaudeState};

/// Update the state from a hook event
pub fn update_state_from_event(
    project: &str,
    worktree: &str,
    event: ClaudeEvent,
    session_id: &str,
    new_state: ClaudeState,
) -> Result<()> {
    // Read existing session or create new one
    let mut session = read_state(project, worktree).unwrap_or_else(|| {
        ClaudeSession::new(session_id.to_string(), ClaudeState::Inactive)
    });

    // Update session ID if changed (preserve events for continuous log)
    session.session_id = session_id.to_string();

    // Update state and add event
    session.state = new_state;
    session.add_event(event);

    write_state(project, worktree, &session)
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
