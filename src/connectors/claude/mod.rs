//! Claude Code session state tracking.
//!
//! Tracks Claude session states (working, idle, waiting permission, inactive)
//! by monitoring hook events and caching the state locally.

mod cache;
mod state;
pub mod time;
mod types;

// Re-export types
pub use types::{ClaudeEvent, ClaudeSession, ClaudeState};

// Re-export cache functions
pub use cache::{read_state, write_state};

// Re-export state functions
pub use state::{effective_state, update_state_from_event};

// Re-export time functions
pub use time::relative_time;
