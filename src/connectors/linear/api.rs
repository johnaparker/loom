//! Linear GraphQL API client.

use std::time::Duration;

use anyhow::Result;

use crate::error::GwtError;
use super::types::LinearIssue;

/// API timeout for Linear requests (5 seconds)
const API_TIMEOUT: Duration = Duration::from_secs(5);

/// Fetch Linear issue details from the API
pub fn get_issue(api_key: &str, issue_id: &str) -> Result<LinearIssue> {
    let client = reqwest::blocking::Client::builder()
        .timeout(API_TIMEOUT)
        .build()?;

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
        Err(GwtError::LinearApiError {
            message: format!("API returned status {}", response.status()),
        })?;
    }

    let body: serde_json::Value = response.json()?;

    // Check for GraphQL errors
    if let Some(errors) = body.get("errors") {
        if errors.as_array().and_then(|arr| arr.first()).is_some() {
            Err(GwtError::LinearIssueNotFound {
                issue_id: issue_id.to_string(),
            })?;
        }
    }

    let issue_data = body
        .get("data")
        .and_then(|d| d.get("issue"))
        .ok_or_else(|| GwtError::LinearIssueNotFound {
            issue_id: issue_id.to_string(),
        })?;

    if issue_data.is_null() {
        Err(GwtError::LinearIssueNotFound {
            issue_id: issue_id.to_string(),
        })?;
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

/// Update a Linear issue's status by state type
///
/// state_type values:
/// - "started" - marks issue as "In Progress" (prefers state named "In Progress")
/// - "completed" - marks issue as "Done" (prefers state named "Done")
pub fn update_issue_status(api_key: &str, issue_id: &str, state_type: &str) -> Result<()> {
    // Map state types to preferred state names
    let preferred_name = match state_type {
        "started" => Some("In Progress"),
        "completed" => Some("Done"),
        _ => None,
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(API_TIMEOUT)
        .build()?;

    // First, query the issue to get its team ID and UUID
    let issue_query = r#"
        query Issue($id: String!) {
            issue(id: $id) {
                id
                team {
                    id
                    states {
                        nodes {
                            id
                            name
                            type
                        }
                    }
                }
            }
        }
    "#;

    let response = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "query": issue_query,
            "variables": { "id": issue_id }
        }))
        .send()?;

    if !response.status().is_success() {
        Err(GwtError::LinearApiError {
            message: format!("API returned status {}", response.status()),
        })?;
    }

    let body: serde_json::Value = response.json()?;

    // Check for GraphQL errors
    if let Some(errors) = body.get("errors") {
        if let Some(first_error) = errors.as_array().and_then(|arr| arr.first()) {
            let msg = first_error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            Err(GwtError::LinearApiError {
                message: msg.to_string(),
            })?;
        }
    }

    let issue_data = body
        .get("data")
        .and_then(|d| d.get("issue"))
        .ok_or_else(|| GwtError::LinearIssueNotFound {
            issue_id: issue_id.to_string(),
        })?;

    if issue_data.is_null() {
        Err(GwtError::LinearIssueNotFound {
            issue_id: issue_id.to_string(),
        })?;
    }

    // Get the issue's UUID (not the identifier like JOH-123)
    let issue_uuid = issue_data
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| GwtError::LinearApiError {
            message: "Issue has no UUID".to_string(),
        })?;

    // Find the target state by type
    let states = issue_data
        .get("team")
        .and_then(|t| t.get("states"))
        .and_then(|s| s.get("nodes"))
        .and_then(|n| n.as_array())
        .ok_or_else(|| GwtError::LinearApiError {
            message: "Could not get team workflow states".to_string(),
        })?;

    // First try to find by preferred name (e.g., "In Progress" for started)
    // Fall back to any state with matching type
    let target_state = preferred_name
        .and_then(|name| {
            states.iter().find(|state| {
                state
                    .get("name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        })
        .or_else(|| {
            states.iter().find(|state| {
                state
                    .get("type")
                    .and_then(|t| t.as_str())
                    .is_some_and(|t| t == state_type)
            })
        })
        .ok_or_else(|| GwtError::LinearApiError {
            message: format!("No workflow state with type '{}' found", state_type),
        })?;

    let state_id = target_state
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| GwtError::LinearApiError {
            message: "State has no ID".to_string(),
        })?;

    // Now update the issue status
    let mutation = r#"
        mutation IssueUpdate($id: String!, $stateId: String!) {
            issueUpdate(id: $id, input: { stateId: $stateId }) {
                success
            }
        }
    "#;

    let update_response = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "query": mutation,
            "variables": {
                "id": issue_uuid,
                "stateId": state_id
            }
        }))
        .send()?;

    if !update_response.status().is_success() {
        Err(GwtError::LinearApiError {
            message: format!("API returned status {}", update_response.status()),
        })?;
    }

    let update_body: serde_json::Value = update_response.json()?;

    // Check for GraphQL errors
    if let Some(errors) = update_body.get("errors") {
        if let Some(first_error) = errors.as_array().and_then(|arr| arr.first()) {
            let msg = first_error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            Err(GwtError::LinearApiError {
                message: msg.to_string(),
            })?;
        }
    }

    // Check success field
    let success = update_body
        .get("data")
        .and_then(|d| d.get("issueUpdate"))
        .and_then(|u| u.get("success"))
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    if !success {
        Err(GwtError::LinearApiError {
            message: "Issue update failed".to_string(),
        })?;
    }

    Ok(())
}
