//! External service connectors for gwt.
//!
//! This module contains integrations with external services:
//! - `linear` - Linear issue tracking integration
//! - `github` - GitHub PR and repository integration
//! - `claude` - Claude Code session state tracking
//! - `tmux` - Terminal multiplexer session management
//! - `sesh` - Sesh session configuration integration
//!
//! Each connector is organized as a submodule with its own types, API functions,
//! and caching logic. The `cache` module provides shared caching utilities.

pub mod cache;
pub mod claude;
pub mod github;
pub mod linear;
pub mod sesh;
pub mod tmux;
