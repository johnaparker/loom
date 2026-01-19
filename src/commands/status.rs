use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use crate::cli::Category;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::WorktreeManager;
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
    let mut dashboard = Dashboard::new(worktrees, project_name.clone());
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
                return Ok(());
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
                return Ok(());
            }
            DashboardResult::Claude { worktree } => {
                let session = sesh::session_name(project_name, &worktree.info.name);
                let path_str = worktree.info.path.to_str().unwrap();

                // Run claude, then keep window open with a shell after it exits
                let claude_command = "bash -c 'claude; exec $SHELL'";

                if tmux::session_exists(&session) {
                    // Session exists - create new window running claude
                    tmux::create_window(&session, "claude", path_str, claude_command)?;
                } else {
                    // Session doesn't exist - create it with claude as first window
                    tmux::create_session_with_command(&session, path_str, claude_command)?;
                }

                // Switch to the session
                tmux::switch_to_session(&session, path_str)?;
                return Ok(());
            }
            DashboardResult::CreateNew { branch, category } => {
                let cat = match category.as_str() {
                    "review" => Category::Review,
                    "demo" => Category::Demo,
                    _ => Category::Dev,
                };

                let result = execute_create(&manager, &config, project_name, &branch, cat);

                match result {
                    Ok((session_name, worktree_path)) => {
                        // Successfully created - switch to the new session and exit
                        tmux::switch_to_session(&session_name, worktree_path.to_str().unwrap())?;
                        return Ok(());
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to create: {}", e));
                        // Refresh worktrees and continue
                        let worktrees = manager.list_worktrees_with_stats()?;
                        dashboard.update_worktrees(worktrees);
                    }
                }
            }
            DashboardResult::Refresh => {
                // Just refresh worktrees
                let worktrees = manager.list_worktrees_with_stats()?;
                dashboard.update_worktrees(worktrees);
            }
        }
    }
}

/// Execute worktree deletion
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
    // Remove the worktree (force=true if worktree has uncommitted changes)
    manager.remove_worktree(path, force)?;

    // Kill tmux session if it exists
    let session_name = sesh::session_name(project_name, name);
    tmux::kill_session(&session_name);

    // Unregister from sesh
    sesh::unregister_worktree(project_name, name)?;

    // Clean up Linear cache
    let _ = linear::delete_metadata(project_name, name);

    // Delete branch if requested
    if delete_branch {
        if let Some(branch) = branch {
            // Use force delete since worktree is already gone
            manager.delete_branch(branch, true)?;
        }
    }

    Ok(())
}

/// Execute merge to main
fn execute_merge(
    manager: &WorktreeManager,
    _config: &Config,
    project_name: &str,
    name: &str,
    path: &std::path::Path,
    branch: &str,
    delete_branch: bool,
) -> Result<()> {
    // Merge to main
    manager.merge_to_main(branch)?;

    // Remove the worktree
    manager.remove_worktree(path, false)?;

    // Kill tmux session if it exists
    let session_name = sesh::session_name(project_name, name);
    tmux::kill_session(&session_name);

    // Unregister from sesh
    sesh::unregister_worktree(project_name, name)?;

    // Clean up Linear cache
    let _ = linear::delete_metadata(project_name, name);

    // Delete branch if requested
    if delete_branch {
        manager.delete_branch(branch, true)?;
    }

    Ok(())
}

/// Execute sync with main
fn execute_sync(
    manager: &WorktreeManager,
    path: &std::path::Path,
) -> Result<()> {
    // Fetch from origin first
    let _ = manager.fetch_origin(); // Ignore errors - we can still try sync

    // Get sync source ref
    let source_ref = manager
        .get_sync_source_ref()
        .ok_or_else(|| anyhow::anyhow!("No main branch found to sync from"))?;

    // Perform sync
    manager.sync_branch_with_main(path, &source_ref)?;

    Ok(())
}

/// Execute worktree creation
fn execute_create(
    manager: &WorktreeManager,
    config: &Config,
    project_name: &str,
    branch: &str,
    category: Category,
) -> Result<(String, std::path::PathBuf)> {
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

    // Create the worktree
    manager.create_worktree(&resolved.git_branch, &worktree_path)?;

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

    Ok((session_name, worktree_path))
}
