//! Input resolution for Linear integration.
//!
//! Resolves user input (issue IDs, branch names) into worktree and git branch names.

use anyhow::Result;

use super::api::get_issue;
use super::parser::{extract_issue_id, is_issue_id, sanitize_for_filesystem};
use super::types::ResolvedInput;
use crate::error::LoomError;

/// Resolve user input into worktree name, git branch, and optional issue metadata
pub fn resolve_input(
    name: &str,
    prefix: Option<&str>,
    api_key: Option<&str>,
) -> Result<ResolvedInput> {
    match (prefix, api_key) {
        // Case 1: Have prefix and API key, and input looks like an issue ID
        (Some(prefix), Some(key)) if is_issue_id(name, prefix) => {
            let issue = get_issue(key, name)?;
            Ok(ResolvedInput {
                worktree_name: issue.id.clone(),
                git_branch: issue.branch_name.clone(),
                issue: Some(issue),
            })
        }
        // Case 2: Have prefix (no API key), input looks like an issue ID -> error
        (Some(prefix), None) if is_issue_id(name, prefix) => {
            Err(LoomError::LinearApiKeyRequired {
                issue_id: name.to_string(),
            })?
        }
        // Case 3: Have prefix and API key, input contains issue ID in branch name
        // Fetch issue metadata so we can cache it
        (Some(prefix), Some(key)) if extract_issue_id(name, prefix).is_some() => {
            let id = extract_issue_id(name, prefix).unwrap();
            // Try to fetch issue metadata (don't fail if API call fails)
            let issue = get_issue(key, &id).ok();
            Ok(ResolvedInput {
                worktree_name: id,
                git_branch: name.to_string(),
                issue,
            })
        }
        // Case 4: Have prefix but no API key, input contains issue ID in branch name
        (Some(prefix), None) if extract_issue_id(name, prefix).is_some() => {
            let id = extract_issue_id(name, prefix).unwrap();
            Ok(ResolvedInput {
                worktree_name: id,
                git_branch: name.to_string(),
                issue: None,
            })
        }
        // Case 5: Regular branch (no Linear pattern detected)
        _ => Ok(ResolvedInput {
            worktree_name: sanitize_for_filesystem(name),
            git_branch: name.to_string(),
            issue: None,
        }),
    }
}
