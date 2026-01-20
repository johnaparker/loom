//! Linear GraphQL API client.

use anyhow::Result;

use crate::error::GwtError;
use super::types::LinearIssue;

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
