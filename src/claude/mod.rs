use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of events to keep in the cache
const MAX_EVENTS: usize = 100;

/// Staleness threshold in seconds (5 minutes)
const STALE_THRESHOLD_SECS: i64 = 300;

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
}

impl ClaudeSession {
    /// Create a new session with the given ID and state
    pub fn new(session_id: String, state: ClaudeState) -> Self {
        Self {
            session_id,
            state,
            last_updated: now_iso8601(),
            events: Vec::new(),
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

    /// Check if this session is stale (last_updated > 5 minutes ago)
    pub fn is_stale(&self) -> bool {
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

/// Get the cache directory for a project/worktree
pub fn cache_dir(project: &str, worktree: &str) -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("gwt")
        .join(project)
        .join(worktree)
}

/// Get the path to the claude.json cache file
pub fn cache_file(project: &str, worktree: &str) -> PathBuf {
    cache_dir(project, worktree).join("claude.json")
}

/// Read the Claude session state from cache
pub fn read_state(project: &str, worktree: &str) -> Option<ClaudeSession> {
    let path = cache_file(project, worktree);
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write the Claude session state to cache
pub fn write_state(project: &str, worktree: &str, session: &ClaudeSession) -> Result<()> {
    let dir = cache_dir(project, worktree);
    fs::create_dir_all(&dir)?;

    let path = cache_file(project, worktree);
    let content = serde_json::to_string_pretty(session)?;
    fs::write(&path, content)?;

    Ok(())
}

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
            // UserPromptSubmit, ToolUse, SessionStart, SessionCleared all mean working
            _ => ClaudeState::Working,
        }
    } else {
        session.state.clone()
    }
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
    // This is a rough approximation
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

/// Parse ISO 8601 timestamp to Unix epoch seconds
fn parse_iso8601(s: &str) -> Result<i64, ()> {
    // Parse format: "2026-01-18T22:30:00Z"
    if s.len() < 19 {
        return Err(());
    }

    let year: i32 = s[0..4].parse().map_err(|_| ())?;
    let month: i32 = s[5..7].parse().map_err(|_| ())?;
    let day: i32 = s[8..10].parse().map_err(|_| ())?;
    let hour: i32 = s[11..13].parse().map_err(|_| ())?;
    let minute: i32 = s[14..16].parse().map_err(|_| ())?;
    let second: i32 = s[17..19].parse().map_err(|_| ())?;

    // Days from year
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Days from months
    let days_in_months: [i32; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for m in 1..month {
        days += days_in_months[(m - 1) as usize] as i64;
    }

    // Days of month
    days += (day - 1) as i64;

    // Convert to seconds
    let secs = days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;

    Ok(secs)
}

/// Format a timestamp as relative time (e.g., "2m ago", "1h ago")
pub fn relative_time(timestamp: &str) -> String {
    let Ok(event_secs) = parse_iso8601(timestamp) else {
        return timestamp.to_string();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let diff = now - event_secs;

    if diff < 0 {
        return "now".to_string();
    } else if diff < 60 {
        return format!("{}s ago", diff);
    } else if diff < 3600 {
        return format!("{}m ago", diff / 60);
    } else if diff < 86400 {
        return format!("{}h ago", diff / 3600);
    } else {
        return format!("{}d ago", diff / 86400);
    }
}
