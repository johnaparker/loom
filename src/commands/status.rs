use anyhow::Result;
use colored::Colorize;
use std::process::Command;

use crate::git::WorktreeManager;

pub fn status() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;

    let worktrees = manager.list_worktrees()?;

    if worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    println!("{}", "Worktree Status:".bold());
    println!();

    for wt in worktrees {
        let name = if wt.is_main {
            format!("{} (main repo)", wt.name).yellow()
        } else {
            wt.name.green()
        };

        let branch = wt
            .branch
            .as_ref()
            .map(|b| format!("[{}]", b).cyan().to_string())
            .unwrap_or_default();

        println!("{} {}", name, branch);

        // Get git status for this worktree
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&wt.path)
            .output();

        match status_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.lines().collect();

                if lines.is_empty() {
                    println!("  {}", "Clean".green());
                } else {
                    let modified = lines.iter().filter(|l| l.starts_with(" M") || l.starts_with("M ")).count();
                    let added = lines.iter().filter(|l| l.starts_with("A ") || l.starts_with("??")).count();
                    let deleted = lines.iter().filter(|l| l.starts_with(" D") || l.starts_with("D ")).count();

                    let mut parts = Vec::new();
                    if modified > 0 {
                        parts.push(format!("{} modified", modified).yellow().to_string());
                    }
                    if added > 0 {
                        parts.push(format!("{} added", added).green().to_string());
                    }
                    if deleted > 0 {
                        parts.push(format!("{} deleted", deleted).red().to_string());
                    }

                    if parts.is_empty() {
                        parts.push(format!("{} changes", lines.len()).yellow().to_string());
                    }

                    println!("  {}", parts.join(", "));
                }
            }
            Err(_) => {
                println!("  {}", "Could not get status".red());
            }
        }

        // Get ahead/behind status
        if let Some(branch) = &wt.branch {
            let ahead_behind = Command::new("git")
                .args(["rev-list", "--left-right", "--count", &format!("{}...origin/{}", branch, branch)])
                .current_dir(&wt.path)
                .output();

            if let Ok(output) = ahead_behind {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split('\t').collect();
                if parts.len() == 2 {
                    let ahead: i32 = parts[0].parse().unwrap_or(0);
                    let behind: i32 = parts[1].parse().unwrap_or(0);

                    if ahead > 0 || behind > 0 {
                        let mut sync_status = Vec::new();
                        if ahead > 0 {
                            sync_status.push(format!("↑{}", ahead).green().to_string());
                        }
                        if behind > 0 {
                            sync_status.push(format!("↓{}", behind).red().to_string());
                        }
                        println!("  {}", sync_status.join(" "));
                    }
                }
            }
        }

        println!();
    }

    Ok(())
}
