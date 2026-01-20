//! Core shared utilities for TUI and CLI.
//!
//! This module contains reusable components that are shared between
//! the terminal UI (TUI) and command-line interface (CLI) parts of gwt.

pub mod fuzzy;
pub mod terminal;

pub use fuzzy::FuzzyMatcher;
pub use terminal::with_alternate_screen;
