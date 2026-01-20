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
use crate::error::GwtError;
use crate::git::WorktreeManager;
use crate::github;
use crate::linear;
use crate::sesh;
use crate::sync;
use crate::tmux;
use crate::tui::{Dashboard, DashboardResult};

pub fn status() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let project_name = manager.project_name()?;
    let config = Config::load(Some(manager.repo_root()))?;
    let main_branch = manager.main_branch_name().unwrap_or_else(|_| "main".to_string());

    let worktrees = manager.list_worktrees_with_stats()?;
    let mut dashboard = Dashboard::new(worktrees, project_name.clone(), manager.repo_root().to_path_buf());
    dashboard.set_main_branch(main_branch);

    // Set up terminal once for the entire session
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Use a closure to ensure cleanup happens on all exit paths
    let result = run_dashboard_loop(&mut dashboard, &mut terminal, &manager, &config, &project_name);

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
) -> Result<()> {
    loop {
        match dashboard.run_with_terminal(terminal)? {
            DashboardResult::SwitchTo(wt) => {
                let session = sesh::session_name(project_name, &wt.info.name);
                tmux::switch_to_session(&session, wt.info.path.to_str().unwrap())?;
                dashboard.show_result(true, format!("Switched to '{}'", wt.info.name));
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
                    &config,
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
                    &config,
                    project_name,
                    &worktree.info.name,
                    &worktree.info.path,
                    branch,
                    delete_branch,
                );

                match result {
                    Ok(()) => {
                        dashboard.show_result(
                            true,
                            format!("Merged '{}' to main", worktree.info.name),
                        );
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to merge: {}", e));
                    }
                }

                // Refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
            DashboardResult::Sync { worktree } => {
                let branch = worktree.info.branch.as_deref().unwrap_or(&worktree.info.name);

                // Check if already up to date
                if worktree.commits_behind == Some(0) {
                    dashboard.show_result(
                        true,
                        format!("'{}' already synced with main", worktree.info.name),
                    );
                    let worktrees = manager.list_worktrees_with_stats()?;
                    dashboard.update_worktrees(worktrees);
                    continue;
                }

                // Check for uncommitted changes first
                if worktree.has_uncommitted_changes() {
                    dashboard.show_result(
                        false,
                        "Cannot sync: uncommitted changes present".to_string(),
                    );
                    let worktrees = manager.list_worktrees_with_stats()?;
                    dashboard.update_worktrees(worktrees);
                    continue;
                }

                // Check for conflicts first
                match manager.check_sync_conflicts(branch) {
                    Ok(Some(conflicts)) => {
                        // Has conflicts - show error and refresh
                        dashboard.show_result(
                            false,
                            format!(
                                "Cannot sync: {} file(s) have conflicts",
                                conflicts.len()
                            ),
                        );
                        let worktrees = manager.list_worktrees_with_stats()?;
                        dashboard.update_worktrees(worktrees);
                        continue;
                    }
                    Ok(None) => {
                        // No conflicts, proceed with sync
                    }
                    Err(e) => {
                        // Error checking conflicts, warn but allow sync attempt
                        eprintln!("Warning: Could not check for conflicts: {}", e);
                    }
                }

                let result = execute_sync(
                    &manager,
                    &worktree.info.path,
                );

                match result {
                    Ok(()) => {
                        dashboard.show_result(
                            true,
                            format!("Synced '{}' with main", worktree.info.name),
                        );
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to sync: {}", e));
                    }
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
                    // Try "Claude" first (normal mode), then "✳" (other modes like multi-pane)
                    let claude_pane = tmux::find_pane_with_title(&session, "Claude")
                        .or_else(|| tmux::find_pane_with_title(&session, "✳"));
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
                if let Ok(Some(issue)) = linear::read_metadata(project_name, &worktree.info.name) {
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
                            let _ = github::write_pr_cache(&project_name, &worktree.info.name, &pr);
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

                let result = execute_create(&manager, &config, project_name, &branch, cat);

                match result {
                    Ok((session_name, worktree_path, issue_id)) => {
                        let path_str = worktree_path.to_str().unwrap();

                        if auto_claude && issue_id.is_some() {
                            // Launch Claude with initial prompt for the Linear issue
                            let issue = issue_id.unwrap();
                            let claude_cmd = format!(
                                "bash -c 'claude \"work on {} /plan\"; exec $SHELL'",
                                issue
                            );
                            tmux::create_session_with_command(&session_name, path_str, &claude_cmd)?;
                            tmux::switch_to_session(&session_name, path_str)?;
                            dashboard.show_result(true, format!("Created '{}' and started Claude", branch));
                        } else {
                            // Normal flow - just switch to the new session
                            tmux::switch_to_session(&session_name, path_str)?;
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
    _config: &Config,
    project_name: &str,
    name: &str,
    path: &std::path::Path,
    branch: Option<&str>,
    delete_branch: bool,
    force: bool,
) -> Result<()> {
    operations::delete_worktree(manager, project_name, name, path, branch, delete_branch, force)?;
    Ok(())
}

/// Execute merge to main using shared operations
fn execute_merge(
    manager: &WorktreeManager,
    _config: &Config,
    project_name: &str,
    name: &str,
    path: &std::path::Path,
    branch: &str,
    delete_branch: bool,
) -> Result<()> {
    operations::merge_worktree_to_main(manager, project_name, name, path, branch, delete_branch)?;
    Ok(())
}

/// Execute sync with main using shared operations
fn execute_sync(
    manager: &WorktreeManager,
    path: &std::path::Path,
) -> Result<()> {
    operations::sync_worktree_with_main(manager, path)
}

/// Execute worktree creation
/// Returns (session_name, worktree_path, issue_id) where issue_id is Some if linked to Linear
fn execute_create(
    manager: &WorktreeManager,
    config: &Config,
    project_name: &str,
    branch: &str,
    category: Category,
) -> Result<(String, std::path::PathBuf, Option<String>)> {
    let worktree_root = config.worktree_root()?;

    // Resolve Linear input (issue ID, branch with issue ID, or regular branch)
    let resolved = linear::resolve_input(
        branch,
        config.linear_prefix(),
        config.linear_api_key(),
    )?;

    // Check if worktree already exists
    if let Some(_existing) = manager.get_worktree(&resolved.worktree_name)? {
        return Err(GwtError::WorktreeAlreadyExists {
            name: resolved.worktree_name,
        }
        .into());
    }

    // Build worktree path: ~/worktrees/{project}/{category}/{name}
    let worktree_path = worktree_root
        .join(project_name)
        .join(category.to_string())
        .join(&resolved.worktree_name);

    // Fetch from origin to ensure we have the latest refs
    let _ = manager.fetch_origin(); // Ignore errors - we can still create from local refs

    // Check if remote branch exists
    let track_remote = manager.remote_branch_exists(&resolved.git_branch);

    // Create the worktree - either tracking remote or creating new
    if track_remote {
        manager.create_worktree_tracking(&resolved.git_branch, &worktree_path)?;
    } else {
        manager.create_worktree(&resolved.git_branch, &worktree_path)?;
    }

    // Write Linear metadata if we have issue info
    if let Some(ref issue) = resolved.issue {
        linear::write_metadata(project_name, &resolved.worktree_name, issue)?;
    }

    // Sync files from main repo
    let patterns = config.sync_patterns();
    if !patterns.is_empty() {
        let synced = sync::sync_files(manager.repo_root(), &worktree_path, &patterns)?;

        // Run direnv allow if .envrc was synced
        if synced.iter().any(|p| p == ".envrc") {
            let _ = sync::run_direnv_allow(&worktree_path);
        }
    }

    // Register with sesh if enabled
    let session_name = sesh::session_name(project_name, &resolved.worktree_name);
    if config.sesh_auto_register() {
        sesh::register_worktree(
            project_name,
            &resolved.worktree_name,
            worktree_path.to_str().unwrap(),
        )?;
    }

    // Extract issue ID for auto-claude feature
    let issue_id = resolved.issue.as_ref().map(|i| i.id.clone());

    Ok((session_name, worktree_path, issue_id))
}
