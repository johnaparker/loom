//! Parsing utilities for Linear issue IDs and branch names.

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

/// Sanitize a string for use as a filesystem path (replace / with -)
pub fn sanitize_for_filesystem(name: &str) -> String {
    name.replace('/', "-")
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
