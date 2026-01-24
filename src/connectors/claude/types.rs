//! Type definitions for Claude session tracking.

use serde::{Deserialize, Serialize};

use super::time::now_iso8601;

/// Maximum number of events to keep in the cache
pub const MAX_EVENTS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeState {
    Working,
    Idle,
    WaitingPermission,
    Inactive,
}

impl std::fmt::Display for ClaudeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeState::Working => write!(f, "Working"),
            ClaudeState::Idle => write!(f, "Idle"),
            ClaudeState::WaitingPermission => write!(f, "Waiting"),
            ClaudeState::Inactive => write!(f, "Inactive"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSession {
    pub session_id: String,
    pub state: ClaudeState,
    pub last_updated: String,
    pub events: Vec<ClaudeEvent>,
    /// Track nested subagent calls (Task tool depth)
    /// When > 0, we're inside a subagent and should filter events
    #[serde(default)]
    pub subagent_depth: u32,
}

impl ClaudeSession {
    /// Create a new session with the given ID and state
    pub fn new(session_id: String, state: ClaudeState) -> Self {
        Self {
            session_id,
            state,
            last_updated: now_iso8601(),
            events: Vec::new(),
            subagent_depth: 0,
        }
    }

    /// Add an event and update the last_updated timestamp
    pub fn add_event(&mut self, event: ClaudeEvent) {
        self.last_updated = now_iso8601();
        self.events.push(event);

        // Keep only the last MAX_EVENTS
        if self.events.len() > MAX_EVENTS {
            let drain_count = self.events.len() - MAX_EVENTS;
            self.events.drain(0..drain_count);
        }
    }

    /// Check if this session is stale (last_updated > threshold)
    pub fn is_stale(&self) -> bool {
        use super::time::{STALE_THRESHOLD_SECS, parse_iso8601};

        let Ok(last) = parse_iso8601(&self.last_updated) else {
            return true;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        (now - last) > STALE_THRESHOLD_SECS
    }
}
