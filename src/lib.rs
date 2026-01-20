pub mod cli;
pub mod commands;
pub mod config;
pub mod connectors;
pub mod core;
pub mod error;
pub mod git;
pub mod output;
pub mod sync;
pub mod tui;

// Backward-compatible re-exports from connectors
// These allow existing code to use `crate::linear` instead of `crate::connectors::linear`
pub use connectors::claude;
pub use connectors::github;
pub use connectors::linear;
pub use connectors::sesh;
pub use connectors::tmux;
