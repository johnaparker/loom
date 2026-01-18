use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

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
                copy_dir_recursive(&source_path, &dest_path)
                    .with_context(|| format!("Failed to copy directory: {}", pattern))?;
            } else {
                fs::copy(&source_path, &dest_path)
                    .with_context(|| format!("Failed to copy file: {}", pattern))?;
            }
            synced.push(pattern.to_string());
        }
    }

    Ok(synced)
}

/// Recursively copy a directory
fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<()> {
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &dest_path)?;
        } else {
            fs::copy(&source_path, &dest_path)?;
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
