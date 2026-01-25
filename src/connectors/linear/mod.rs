//! Linear issue tracking integration.
//!
//! Provides functionality for:
//! - Parsing Linear issue IDs from user input and branch names
//! - Fetching issue metadata from the Linear GraphQL API
//! - Caching issue metadata locally
//! - Resolving user input into worktree and git branch names

mod api;
mod cache;
mod parser;
mod resolve;
mod types;

// Re-export types
pub use types::{LinearIssue, LinearTeam, ResolvedInput};

// Re-export parser functions
pub use parser::{extract_issue_id, is_issue_id};

// Re-export API functions
pub use api::{fetch_teams, get_issue, update_issue_status};

// Re-export resolution functions
pub use resolve::resolve_input;

// Re-export cache functions
pub use cache::{delete_metadata, read_metadata, write_metadata};

/// Convert a Linear web URL to a desktop app URL scheme.
///
/// Transforms `https://linear.app/...` to `linear://...` for opening
/// issues directly in the Linear desktop application.
pub fn to_desktop_url(url: &str) -> String {
    url.replace("https://linear.app/", "linear://")
}
