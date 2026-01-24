use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Check if a file is tracked in git
fn is_git_tracked(repo_root: &Path, relative_path: &str) -> bool {
    Command::new("git")
        .args(["ls-files", "--error-unmatch", relative_path])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Sync files from source to destination based on patterns
pub fn sync_files(source: &Path, dest: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let mut synced = Vec::new();

    for pattern in patterns {
        let pattern = pattern.trim_end_matches('/');
        let source_path = source.join(pattern);

        if source_path.exists() {
            let dest_path = dest.join(pattern);

            // Create parent directory if needed
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            if source_path.is_dir() {
                copy_dir_recursive(&source_path, &dest_path, dest, pattern)
                    .with_context(|| format!("Failed to copy directory: {}", pattern))?;
            } else {
                // Skip git-tracked files - git already placed the correct version
                if !is_git_tracked(dest, pattern) {
                    fs::copy(&source_path, &dest_path)
                        .with_context(|| format!("Failed to copy file: {}", pattern))?;
                }
            }
            synced.push(pattern.to_string());
        }
    }

    Ok(synced)
}

/// Recursively copy a directory, skipping git-tracked files
fn copy_dir_recursive(source: &Path, dest: &Path, repo_root: &Path, relative_base: &str) -> Result<()> {
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let file_name = entry.file_name().to_string_lossy().to_string();
        let relative_path = format!("{}/{}", relative_base, file_name);

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &dest_path, repo_root, &relative_path)?;
        } else {
            // Skip git-tracked files - git already placed the correct version
            if !is_git_tracked(repo_root, &relative_path) {
                fs::copy(&source_path, &dest_path)?;
            }
        }
    }

    Ok(())
}

/// Run direnv allow if .envrc exists in the directory
pub fn run_direnv_allow(path: &Path) -> Result<bool> {
    let envrc = path.join(".envrc");
    if !envrc.exists() {
        return Ok(false);
    }

    let output = Command::new("direnv")
        .args(["allow"])
        .current_dir(path)
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(true)
            } else {
                // direnv might not be installed, that's ok
                Ok(false)
            }
        }
        Err(_) => {
            // direnv not found, that's ok
            Ok(false)
        }
    }
}

/// Write Claude sandbox settings to .claude/settings.local.json
///
/// IMPORTANT: This MERGES with existing settings, not overwrites.
/// Only the "sandbox" key is modified; all other keys are preserved.
pub fn write_claude_sandbox_settings(worktree_path: &Path, auto_allow_bash: bool) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    let settings_path = claude_dir.join("settings.local.json");

    // Create .claude/ directory if needed
    fs::create_dir_all(&claude_dir)
        .with_context(|| format!("Failed to create .claude directory at {:?}", claude_dir))?;

    // Read existing settings if present
    let mut settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read {:?}", settings_path))?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure settings is an object
    if !settings.is_object() {
        settings = json!({});
    }

    // Merge sandbox settings
    settings["sandbox"] = json!({
        "enabled": true,
        "autoAllowBashIfSandboxed": auto_allow_bash
    });

    // Write back with pretty formatting
    let content = serde_json::to_string_pretty(&settings)
        .with_context(|| "Failed to serialize settings")?;
    fs::write(&settings_path, content)
        .with_context(|| format!("Failed to write {:?}", settings_path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_claude_sandbox_settings_creates_dir() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        write_claude_sandbox_settings(worktree_path, true).unwrap();

        let settings_path = worktree_path.join(".claude/settings.local.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(settings["sandbox"]["enabled"], true);
        assert_eq!(settings["sandbox"]["autoAllowBashIfSandboxed"], true);
    }

    #[test]
    fn test_write_claude_sandbox_settings_auto_allow_false() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        write_claude_sandbox_settings(worktree_path, false).unwrap();

        let settings_path = worktree_path.join(".claude/settings.local.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(settings["sandbox"]["enabled"], true);
        assert_eq!(settings["sandbox"]["autoAllowBashIfSandboxed"], false);
    }

    #[test]
    fn test_write_claude_sandbox_settings_merges_existing() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();
        let claude_dir = worktree_path.join(".claude");
        let settings_path = claude_dir.join("settings.local.json");

        // Create existing settings
        fs::create_dir_all(&claude_dir).unwrap();
        let existing = json!({
            "permissions": {
                "allow": ["Bash(npm run:*)"]
            },
            "env": {
                "CI": "true"
            }
        });
        fs::write(&settings_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // Write sandbox settings
        write_claude_sandbox_settings(worktree_path, true).unwrap();

        // Verify merge
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        // Sandbox settings should be added
        assert_eq!(settings["sandbox"]["enabled"], true);
        assert_eq!(settings["sandbox"]["autoAllowBashIfSandboxed"], true);

        // Existing settings should be preserved
        assert_eq!(settings["permissions"]["allow"][0], "Bash(npm run:*)");
        assert_eq!(settings["env"]["CI"], "true");
    }

    #[test]
    fn test_write_claude_sandbox_settings_overwrites_sandbox() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();
        let claude_dir = worktree_path.join(".claude");
        let settings_path = claude_dir.join("settings.local.json");

        // Create existing settings with old sandbox config
        fs::create_dir_all(&claude_dir).unwrap();
        let existing = json!({
            "sandbox": {
                "enabled": false,
                "autoAllowBashIfSandboxed": false
            },
            "other": "preserved"
        });
        fs::write(&settings_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // Write new sandbox settings
        write_claude_sandbox_settings(worktree_path, true).unwrap();

        // Verify sandbox was overwritten
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(settings["sandbox"]["enabled"], true);
        assert_eq!(settings["sandbox"]["autoAllowBashIfSandboxed"], true);
        // Other settings should be preserved
        assert_eq!(settings["other"], "preserved");
    }
}
