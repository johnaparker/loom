use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::error::GwtError;

/// Linear issue metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssue {
    pub id: String,
    pub branch_name: String,
    pub title: String,
    pub url: String,
    pub description: Option<String>,
}

/// Result of resolving user input for Linear integration
#[derive(Debug, Clone)]
pub struct ResolvedInput {
    /// Folder name for the worktree (e.g., "ABC-123")
    pub worktree_name: String,
    /// Git branch name (e.g., "john/joh-209-feature")
    pub git_branch: String,
    /// Linear issue metadata (if available from API)
    pub issue: Option<LinearIssue>,
}

/// Check if a string matches the Linear issue ID pattern: {PREFIX}-{NUMBER}
pub fn is_issue_id(name: &str, prefix: &str) -> bool {
    let pattern = format!("{}-", prefix.to_uppercase());
    let name_upper = name.to_uppercase();
    if !name_upper.starts_with(&pattern) {
        return false;
    }
    let rest = &name_upper[pattern.len()..];
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// Extract Linear issue ID from a branch name (e.g., "user/abc-123-feature" -> "ABC-123")
pub fn extract_issue_id(branch_name: &str, prefix: &str) -> Option<String> {
    let lower_prefix = prefix.to_lowercase();
    let branch_lower = branch_name.to_lowercase();

    // Look for pattern: {prefix}-{number} anywhere in the branch name
    let pattern_start = branch_lower.find(&format!("{}-", lower_prefix))?;

    // Extract from the pattern start
    let rest = &branch_name[pattern_start..];
    let prefix_len = prefix.len() + 1; // prefix + "-"

    // Find where the number ends
    let number_part: String = rest[prefix_len..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();

    if number_part.is_empty() {
        return None;
    }

    Some(format!("{}-{}", prefix.to_uppercase(), number_part))
}

/// Fetch Linear issue details from the API
pub fn get_issue(api_key: &str, issue_id: &str) -> Result<LinearIssue> {
    let client = reqwest::blocking::Client::new();

    let query = r#"
        query Issue($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                url
                branchName
                description
            }
        }
    "#;

    let response = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "query": query,
            "variables": { "id": issue_id }
        }))
        .send()?;

    if !response.status().is_success() {
        return Err(GwtError::LinearApiError {
            message: format!("API returned status {}", response.status()),
        }
        .into());
    }

    let body: serde_json::Value = response.json()?;

    // Check for GraphQL errors
    if let Some(errors) = body.get("errors") {
        if errors.as_array().and_then(|arr| arr.first()).is_some() {
            return Err(GwtError::LinearIssueNotFound {
                issue_id: issue_id.to_string(),
            }
            .into());
        }
    }

    let issue_data = body
        .get("data")
        .and_then(|d| d.get("issue"))
        .ok_or_else(|| GwtError::LinearIssueNotFound {
            issue_id: issue_id.to_string(),
        })?;

    if issue_data.is_null() {
        return Err(GwtError::LinearIssueNotFound {
            issue_id: issue_id.to_string(),
        }
        .into());
    }

    let branch_name = issue_data
        .get("branchName")
        .and_then(|b| b.as_str())
        .ok_or_else(|| GwtError::LinearApiError {
            message: "Issue has no branch name".to_string(),
        })?;

    Ok(LinearIssue {
        id: issue_data
            .get("identifier")
            .and_then(|i| i.as_str())
            .unwrap_or(issue_id)
            .to_string(),
        branch_name: branch_name.to_string(),
        title: issue_data
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        url: issue_data
            .get("url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string(),
        description: issue_data
            .get("description")
            .and_then(|d| d.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    })
}

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
            Err(GwtError::LinearApiKeyRequired {
                issue_id: name.to_string(),
            }
            .into())
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

/// Sanitize a string for use as a filesystem path (replace / with -)
fn sanitize_for_filesystem(name: &str) -> String {
    name.replace('/', "-")
}

/// Get the cache directory for a worktree's Linear metadata
/// Returns ~/.cache/gwt/<project>/<worktree_name>/
fn cache_dir(project_name: &str, worktree_name: &str) -> Result<std::path::PathBuf> {
    let cache_base = dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .ok_or_else(|| anyhow::anyhow!("Could not find cache directory"))?;

    Ok(cache_base.join("gwt").join(project_name).join(worktree_name))
}

/// Write Linear issue metadata to cache directory
pub fn write_metadata(project_name: &str, worktree_name: &str, issue: &LinearIssue) -> Result<()> {
    let cache_path = cache_dir(project_name, worktree_name)?;
    fs::create_dir_all(&cache_path)?;

    let metadata = serde_json::json!({
        "id": issue.id,
        "title": issue.title,
        "url": issue.url,
        "branch": issue.branch_name,
        "description": issue.description
    });
    fs::write(
        cache_path.join("linear.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;
    Ok(())
}

/// Read Linear issue metadata from cache directory
pub fn read_metadata(project_name: &str, worktree_name: &str) -> Result<Option<LinearIssue>> {
    let cache_path = cache_dir(project_name, worktree_name)?;
    let file_path = cache_path.join("linear.json");

    if !file_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&file_path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    Ok(Some(LinearIssue {
        id: value.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        url: value.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        branch_name: value.get("branch").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        // Handle missing description for old caches
        description: value
            .get("description")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    }))
}

/// Delete Linear issue metadata from cache directory
pub fn delete_metadata(project_name: &str, worktree_name: &str) -> Result<()> {
    let cache_path = cache_dir(project_name, worktree_name)?;
    if cache_path.exists() {
        fs::remove_dir_all(&cache_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_issue_id() {
        assert!(is_issue_id("ABC-209", "ABC"));
        assert!(is_issue_id("abc-209", "ABC"));
        assert!(is_issue_id("ABC-1", "ABC"));
        assert!(!is_issue_id("ABC-", "ABC"));
        assert!(!is_issue_id("ABC209", "ABC"));
        assert!(!is_issue_id("feature-branch", "ABC"));
        assert!(!is_issue_id("XYZ-123", "ABC"));
    }

    #[test]
    fn test_extract_issue_id() {
        assert_eq!(
            extract_issue_id("user/abc-209-feature", "ABC"),
            Some("ABC-209".to_string())
        );
        assert_eq!(
            extract_issue_id("abc-123-some-feature", "ABC"),
            Some("ABC-123".to_string())
        );
        assert_eq!(extract_issue_id("feature-branch", "ABC"), None);
        assert_eq!(extract_issue_id("xyz-123-feature", "ABC"), None);
    }
}
