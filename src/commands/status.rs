use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use super::operations;
use crate::cli::Category;
use crate::config::Config;
use crate::git::WorktreeManager;
use crate::github;
use crate::linear;
use crate::sesh;
use crate::tmux;
use crate::tui::{Dashboard, DashboardResult};

pub fn status() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let project_name = manager.project_name()?;
    let config = Config::load(Some(manager.repo_root()))?;
    let cache_dir = config.cache_dir()?;
    let main_branch = manager.main_branch_name().unwrap_or_else(|_| "main".to_string());

    let linear_api_key = config.linear_api_key().map(|s| s.to_string());
    let linear_prefix = config.linear_prefix().map(|s| s.to_string());

    let worktrees = manager.list_worktrees_with_stats()?;
    let mut dashboard = Dashboard::new(
        worktrees,
        project_name.clone(),
        manager.repo_root().to_path_buf(),
        cache_dir,
        linear_api_key,
        linear_prefix,
        config.is_pull_workflow(),
    );
    dashboard.set_main_branch(main_branch);

    // Set up terminal once for the entire session
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let cache_dir_for_loop = config.cache_dir()?;

    // Use a closure to ensure cleanup happens on all exit paths
    let result = run_dashboard_loop(&mut dashboard, &mut terminal, &manager, &config, &project_name, &cache_dir_for_loop);

    // Tear down terminal
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    result
}

fn run_dashboard_loop(
    dashboard: &mut Dashboard,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    manager: &WorktreeManager,
    config: &Config,
    project_name: &str,
    cache_dir: &std::path::Path,
) -> Result<()> {
    loop {
        match dashboard.run_with_terminal(terminal)? {
            DashboardResult::SwitchTo(wt) => {
                let session = sesh::session_name(project_name, &wt.info.name);

                // Auto-pull in pull workflow if branch is behind tracking
                let pulled = if config.is_pull_workflow()
                    && !wt.info.is_main
                    && let Some(behind) = wt.tracking_behind
                    && behind > 0
                    && !wt.has_uncommitted_changes()
                    && let Some(branch) = &wt.info.branch
                {
                    manager.pull_from_remote(&wt.info.path, branch).ok().map(|_| behind)
                } else {
                    None
                };

                tmux::switch_to_session(&session, wt.info.path.to_str().unwrap())?;

                if let Some(behind) = pulled {
                    dashboard.show_result(
                        true,
                        format!("Pulled {} commit(s), switching to '{}'", behind, wt.info.name),
                    );
                } else {
                    dashboard.show_result(true, format!("Switched to '{}'", wt.info.name));
                }
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Quit => {
                return Ok(());
            }
            DashboardResult::Delete {
                worktree,
                delete_branch,
                force,
            } => {
                let result = execute_delete(
                    &manager,
                    cache_dir,
                    project_name,
                    &worktree.info.name,
                    &worktree.info.path,
                    worktree.info.branch.as_deref(),
                    delete_branch,
                    force,
                );

                match result {
                    Ok(()) => {
                        dashboard.show_result(
                            true,
                            format!("Deleted worktree '{}'", worktree.info.name),
                        );
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to delete: {}", e));
                    }
                }

                // Refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Merge {
                worktree,
                delete_branch,
            } => {
                let branch = worktree.info.branch.as_deref().unwrap_or(&worktree.info.name);

                // Read Linear metadata BEFORE merge (merge deletes the cache)
                // Fallback: if no cache, try to extract issue from branch name
                let linear_issue = linear::read_metadata(cache_dir, project_name, &worktree.info.name)
                    .ok()
                    .flatten()
                    .or_else(|| {
                        let branch = worktree.info.branch.as_deref()?;
                        let prefix = config.linear_prefix()?;
                        let api_key = config.linear_api_key()?;
                        let issue_id = linear::extract_issue_id(branch, prefix)?;
                        linear::get_issue(api_key, &issue_id).ok()
                    });

                // Check for conflicts first
                match manager.check_merge_conflicts(branch) {
                    Ok(Some(conflicts)) => {
                        // Has conflicts - show error and refresh
                        dashboard.show_result(
                            false,
                            format!(
                                "Cannot merge: {} file(s) have conflicts",
                                conflicts.len()
                            ),
                        );
                        let worktrees = manager.list_worktrees_with_stats()?;
                        dashboard.update_worktrees(worktrees);
                        continue;
                    }
                    Ok(None) => {
                        // No conflicts, proceed with merge
                    }
                    Err(e) => {
                        // Error checking conflicts, warn but allow merge attempt
                        eprintln!("Warning: Could not check for conflicts: {}", e);
                    }
                }

                let result = execute_merge(
                    &manager,
                    cache_dir,
                    project_name,
                    &worktree.info.name,
                    &worktree.info.path,
                    branch,
                    delete_branch,
                );

                match result {
                    Ok(()) => {
                        // Update Linear issue to "Done" (non-critical)
                        let linear_updated = if config.linear_auto_update_status() {
                            config.linear_api_key().and_then(|api_key| {
                                linear_issue.as_ref().and_then(|issue| {
                                    linear::update_issue_status(api_key, &issue.id, "completed").ok()
                                })
                            })
                        } else {
                            None
                        };

                        let msg = if linear_updated.is_some() {
                            format!("Merged '{}' to main, Linear updated", worktree.info.name)
                        } else {
                            format!("Merged '{}' to main", worktree.info.name)
                        };
                        dashboard.show_result(true, msg);
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to merge: {}", e));
                    }
                }

                // Refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::SyncWithRemote { worktree } => {
                let branch = worktree.info.branch.as_deref().unwrap_or(&worktree.info.name);

                // Determine sync action based on tracking branch status
                let ahead = worktree.tracking_ahead.unwrap_or(0);
                let behind = worktree.tracking_behind.unwrap_or(0);
                let has_tracking = worktree.tracking_ahead.is_some();

                // Check for uncommitted changes first (applies to all operations)
                if worktree.has_uncommitted_changes() {
                    dashboard.show_result(
                        false,
                        "Cannot sync: uncommitted changes present".to_string(),
                    );
                    let worktrees = manager.list_worktrees_with_stats()?;
                    dashboard.update_worktrees(worktrees);
                    continue;
                }

                // Decide action based on state
                if ahead > 0 && behind > 0 {
                    // Both ahead and behind - needs manual resolution
                    dashboard.show_result(
                        false,
                        format!(
                            "Cannot sync: ↑{} ahead, ↓{} behind. Resolve manually",
                            ahead, behind
                        ),
                    );
                    let worktrees = manager.list_worktrees_with_stats()?;
                    dashboard.update_worktrees(worktrees);
                    continue;
                }

                if !has_tracking {
                    // No tracking branch - push with -u to create it
                    match manager.push_to_remote(&worktree.info.path, branch) {
                        Ok(crate::git::PushResult::CreatedRemoteBranch) => {
                            dashboard.show_result(
                                true,
                                format!("Created remote branch and pushed '{}'", worktree.info.name),
                            );
                        }
                        Ok(crate::git::PushResult::Success) => {
                            dashboard.show_result(
                                true,
                                format!("Pushed '{}' to origin", worktree.info.name),
                            );
                        }
                        Ok(crate::git::PushResult::Rejected { reason: _ }) => {
                            dashboard.show_result(
                                false,
                                "Push rejected. Pull first with 'P'".to_string(),
                            );
                        }
                        Err(e) => {
                            dashboard.show_result(false, format!("Failed to push: {}", e));
                        }
                    }
                } else if behind > 0 {
                    // Behind tracking - pull
                    match manager.pull_from_remote(&worktree.info.path, branch) {
                        Ok(()) => {
                            dashboard.show_result(
                                true,
                                format!("Pulled {} commit(s) for '{}'", behind, worktree.info.name),
                            );
                        }
                        Err(e) => {
                            dashboard.show_result(false, format!("Failed to pull: {}", e));
                        }
                    }
                } else if ahead > 0 {
                    // Ahead of tracking - push
                    match manager.push_to_remote(&worktree.info.path, branch) {
                        Ok(crate::git::PushResult::Success) | Ok(crate::git::PushResult::CreatedRemoteBranch) => {
                            dashboard.show_result(
                                true,
                                format!("Pushed {} commit(s) for '{}'", ahead, worktree.info.name),
                            );
                        }
                        Ok(crate::git::PushResult::Rejected { reason: _ }) => {
                            dashboard.show_result(
                                false,
                                "Push rejected. Pull first with 'P'".to_string(),
                            );
                        }
                        Err(e) => {
                            dashboard.show_result(false, format!("Failed to push: {}", e));
                        }
                    }
                } else {
                    // Already synced
                    dashboard.show_result(
                        true,
                        format!("'{}' already synced with remote", worktree.info.name),
                    );
                }

                // Refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Review { worktree } => {
                let session = sesh::session_name(project_name, &worktree.info.name);
                let path_str = worktree.info.path.to_str().unwrap();
                let main_branch = manager.main_branch_name().unwrap_or_else(|_| "main".to_string());

                // Use merge-base to show only the worktree's changes since branching
                // This avoids showing changes main has that the worktree doesn't
                // Wrap in bash -c so command substitution is evaluated
                // exec $SHELL keeps window open after nvim exits
                let nvim_command = format!(
                    "bash -c 'nvim -c \"DiffviewOpen $(git merge-base {} HEAD)\"; exec $SHELL'",
                    main_branch
                );

                if tmux::session_exists(&session) {
                    // Session exists - create new window with diff command
                    tmux::create_window(&session, "review", path_str, &nvim_command)?;
                } else {
                    // Session doesn't exist - create it with diff as first window
                    tmux::create_session_with_command(&session, path_str, &nvim_command)?;
                }

                // Switch to the session
                tmux::switch_to_session(&session, path_str)?;
                dashboard.show_result(true, format!("Opened review for '{}'", worktree.info.name));
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Claude { worktree } => {
                let session = sesh::session_name(project_name, &worktree.info.name);
                let path_str = worktree.info.path.to_str().unwrap();

                // Run claude, then keep window open with a shell after it exits
                let claude_command = "bash -c 'claude; exec $SHELL'";

                if tmux::session_exists(&session) {
                    // Check if Claude is actively running in ANY pane (by checking pane title)
                    // Claude uses: "Claude" (normal), "✳" (multi-pane), or Braille patterns (spinner/stats)
                    let claude_pane = tmux::find_pane_with_title(&session, "Claude")
                        .or_else(|| tmux::find_pane_with_title(&session, "✳"))
                        .or_else(|| tmux::find_pane_with_braille_title(&session));
                    if let Some(location) = claude_pane {
                        // Claude is running - switch to that specific pane
                        tmux::switch_to_session(&session, path_str)?;
                        tmux::switch_to_pane(&session, &location)?;
                    } else {
                        // No active Claude anywhere - create new window
                        tmux::create_window(&session, "claude", path_str, claude_command)?;
                        tmux::switch_to_session(&session, path_str)?;
                    }
                } else {
                    // Session doesn't exist - create it with claude as first window
                    tmux::create_session_with_command(&session, path_str, claude_command)?;
                    tmux::switch_to_session(&session, path_str)?;
                }

                dashboard.show_result(true, format!("Opened Claude for '{}'", worktree.info.name));
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Linear { worktree } => {
                // Read Linear metadata and open URL
                if let Ok(Some(issue)) = linear::read_metadata(cache_dir, project_name, &worktree.info.name) {
                    if !issue.url.is_empty() {
                        // Convert to desktop app URL scheme (linear:// instead of https://linear.app/)
                        let desktop_url = issue.url.replace("https://linear.app/", "linear://");
                        #[cfg(target_os = "macos")]
                        {
                            std::process::Command::new("open").arg(&desktop_url).spawn()?;
                        }
                        #[cfg(target_os = "linux")]
                        {
                            std::process::Command::new("xdg-open").arg(&desktop_url).spawn()?;
                        }
                        #[cfg(target_os = "windows")]
                        {
                            std::process::Command::new("cmd")
                                .args(["/C", "start", &desktop_url])
                                .spawn()?;
                        }
                        dashboard.show_result(true, format!("Opened {}", issue.id));
                    }
                }
                // Stay in dashboard - don't exit
            }
            DashboardResult::GitHub { worktree } => {
                // Open GitHub PR or create-PR page
                if let Some(branch) = &worktree.info.branch {
                    // Try to get PR info
                    match github::get_pr_for_branch(manager.repo_root(), branch) {
                        Ok(Some(pr)) => {
                            // Cache the PR info and open it
                            let _ = github::write_pr_cache(cache_dir, project_name, &worktree.info.name, &pr);
                            if let Err(e) = github::open_url(&pr.url) {
                                dashboard.show_result(false, format!("Failed to open URL: {}", e));
                            } else {
                                dashboard.show_result(true, format!("Opening PR #{}: {}", pr.number, pr.title));
                            }
                        }
                        Ok(None) => {
                            // No PR exists - open create PR page
                            match github::get_repo_info(manager.repo_root()) {
                                Ok((owner, repo)) => {
                                    let create_url = github::get_create_pr_url(&owner, &repo, branch);
                                    if let Err(e) = github::open_url(&create_url) {
                                        dashboard.show_result(false, format!("Failed to open URL: {}", e));
                                    } else {
                                        dashboard.show_result(true, format!("Opening create PR page for '{}'", branch));
                                    }
                                }
                                Err(e) => {
                                    dashboard.show_result(false, format!("GitHub: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            dashboard.show_result(false, format!("GitHub: {}", e));
                        }
                    }
                }
                // Stay in dashboard - don't exit
            }
            DashboardResult::CreateNew { branch, category, auto_claude } => {
                let cat = match category.as_str() {
                    "review" => Category::Review,
                    "demo" => Category::Demo,
                    _ => Category::Dev,
                };

                let result = operations::create_worktree(&manager, &config, project_name, &branch, cat, cache_dir);

                match result {
                    Ok(create_result) => {
                        let path_str = create_result.worktree_path.to_str().unwrap();

                        if auto_claude && create_result.issue.is_some() {
                            // Launch Claude with initial prompt for the Linear issue
                            let issue_id = &create_result.issue.as_ref().unwrap().id;
                            let claude_cmd = format!(
                                "bash -c 'claude \"work on {} /plan\"; exec $SHELL'",
                                issue_id
                            );
                            tmux::create_session_with_command(&create_result.session_name, path_str, &claude_cmd)?;
                            tmux::switch_to_session(&create_result.session_name, path_str)?;
                            dashboard.show_result(true, format!("Created '{}' and started Claude", branch));
                        } else {
                            // Normal flow - just switch to the new session
                            tmux::switch_to_session(&create_result.session_name, path_str)?;
                            dashboard.show_result(true, format!("Created and switched to '{}'", branch));
                        }
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to create: {}", e));
                    }
                }
                // Refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Refresh => {
                // Just refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
        }
    }
}

/// Execute worktree deletion using shared operations
fn execute_delete(
    manager: &WorktreeManager,
    cache_dir: &std::path::Path,
    project_name: &str,
    name: &str,
    path: &std::path::Path,
    branch: Option<&str>,
    delete_branch: bool,
    force: bool,
) -> Result<()> {
    operations::delete_worktree(manager, cache_dir, project_name, name, path, branch, delete_branch, force)?;
    Ok(())
}

/// Execute merge to main using shared operations
fn execute_merge(
    manager: &WorktreeManager,
    cache_dir: &std::path::Path,
    project_name: &str,
    name: &str,
    path: &std::path::Path,
    branch: &str,
    delete_branch: bool,
) -> Result<()> {
    operations::merge_worktree_to_main(manager, cache_dir, project_name, name, path, branch, delete_branch)?;
    Ok(())
}
