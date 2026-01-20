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
pub use types::{LinearIssue, ResolvedInput};

// Re-export parser functions
pub use parser::{extract_issue_id, is_issue_id};

// Re-export API functions
pub use api::get_issue;

// Re-export resolution functions
pub use resolve::resolve_input;

// Re-export cache functions
pub use cache::{delete_metadata, read_metadata, write_metadata};
