use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use crate::cli::Category;
use crate::config::Config;
use crate::git::WorktreeManager;
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
            } => {
                let result = execute_delete(
                    &manager,
                    &config,
                    project_name,
                    &worktree.info.name,
                    &worktree.info.path,
                    worktree.info.branch.as_deref(),
                    delete_branch,
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
            DashboardResult::CreateNew { branch, category } => {
                let cat = match category.as_str() {
                    "review" => Category::Review,
                    "demo" => Category::Demo,
                    _ => Category::Dev,
                };

                let result = execute_create(&manager, &config, project_name, &branch, cat);

                match result {
                    Ok(_session_name) => {
                        // Successfully created - show result
                        dashboard.show_result(true, format!("Created worktree '{}'", branch));
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

/// Execute worktree deletion
fn execute_delete(
    manager: &WorktreeManager,
    _config: &Config,
    project_name: &str,
    name: &str,
    path: &std::path::Path,
    branch: Option<&str>,
    delete_branch: bool,
) -> Result<()> {
    // Remove the worktree
    manager.remove_worktree(path, false)?;

    // Kill tmux session if it exists
    let session_name = sesh::session_name(project_name, name);
    tmux::kill_session(&session_name);

    // Unregister from sesh
    sesh::unregister_worktree(project_name, name)?;

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

    // Delete branch if requested
    if delete_branch {
        manager.delete_branch(branch, true)?;
    }

    Ok(())
}

/// Execute worktree creation
fn execute_create(
    manager: &WorktreeManager,
    config: &Config,
    project_name: &str,
    branch: &str,
    category: Category,
) -> Result<String> {
    let worktree_root = config.worktree_root()?;

    // Sanitize branch name for filesystem
    let sanitized_name = branch.replace('/', "-");

    // Build worktree path: ~/worktrees/{project}/{category}/{name}
    let worktree_path = worktree_root
        .join(project_name)
        .join(category.to_string())
        .join(&sanitized_name);

    // Fetch from origin to ensure we have the latest refs
    let _ = manager.fetch_origin(); // Ignore errors - we can still create from local refs

    // Create the worktree
    manager.create_worktree(branch, &worktree_path)?;

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
    let session_name = sesh::session_name(project_name, &sanitized_name);
    if config.sesh_auto_register() {
        sesh::register_worktree(
            project_name,
            &sanitized_name,
            worktree_path.to_str().unwrap(),
        )?;
    }

    Ok(session_name)
}
