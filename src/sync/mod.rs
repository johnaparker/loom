use anyhow::{Context, Result};
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
