//! Core shared utilities for TUI and CLI.
//!
//! This module contains reusable components that are shared between
//! the terminal UI (TUI) and command-line interface (CLI) parts of loom.

pub mod fuzzy;
pub mod terminal;

pub use fuzzy::FuzzyMatcher;
pub use terminal::with_alternate_screen;

/// Capitalize the first letter of a string.
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
