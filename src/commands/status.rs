use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use super::operations::{self, MergeResult};
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
        config.linear_icon().map(|s| s.to_string()),
        config.github_icon().map(|s| s.to_string()),
        config.branch_icon().map(|s| s.to_string()),
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
                dashboard.trigger_stats_refresh();
            }
            DashboardResult::Quit => {
                return Ok(());
            }
            DashboardResult::Delete {
                worktree,
                delete_branch,
                force,
            } => {
                let result = operations::delete_worktree(
                    manager,
                    cache_dir,
                    project_name,
                    &worktree.info.name,
                    &worktree.info.path,
                    worktree.info.branch.as_deref(),
                    delete_branch,
                    force,
                );

                match result {
                    Ok(_) => {
                        dashboard.show_result(
                            true,
                            format!("Deleted worktree '{}'", worktree.info.name),
                        );
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to delete: {}", e));
                    }
                }
                dashboard.trigger_stats_refresh();
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

                let result = operations::merge_worktree_to_main(
                    manager,
                    cache_dir,
                    project_name,
                    &worktree.info.name,
                    &worktree.info.path,
                    branch,
                    delete_branch,
                    linear_issue.as_ref(),
                    config.linear_api_key(),
                    config.linear_auto_update_status(),
                );

                match result {
                    Ok(MergeResult::Success(cleanup_result)) => {
                        let msg = if cleanup_result.linear_updated {
                            format!("Merged '{}' to main, Linear updated", worktree.info.name)
                        } else {
                            format!("Merged '{}' to main", worktree.info.name)
                        };
                        dashboard.show_result(true, msg);
                    }
                    Ok(MergeResult::Conflicts { files }) => {
                        dashboard.show_result(
                            false,
                            format!("Cannot merge: {} file(s) have conflicts", files.len()),
                        );
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to merge: {}", e));
                    }
                }
                dashboard.trigger_stats_refresh();
            }
            DashboardResult::SyncWithRemote { worktree } => {
                let result = operations::sync_with_remote(manager, &worktree);
                dashboard.show_result(result.is_success(), result.message());
                dashboard.trigger_stats_refresh();
            }
            DashboardResult::Review { worktree } => {
                let session = sesh::session_name(project_name, &worktree.info.name);
                let path_str = worktree.info.path.to_str().unwrap();
                let main_branch = manager.main_branch_name().unwrap_or_else(|_| "main".to_string());
                let nvim_command = tmux::build_review_command(&main_branch);

                if tmux::session_exists(&session) {
                    tmux::create_window(&session, "review", path_str, &nvim_command)?;
                } else {
                    tmux::create_session_with_command(&session, path_str, &nvim_command)?;
                }

                tmux::switch_to_session(&session, path_str)?;
                dashboard.show_result(true, format!("Opened review for '{}'", worktree.info.name));
                dashboard.trigger_stats_refresh();
            }
            DashboardResult::Claude { worktree } => {
                let session = sesh::session_name(project_name, &worktree.info.name);
                let path_str = worktree.info.path.to_str().unwrap();
                let claude_command = "bash -c 'claude; exec $SHELL'";

                if tmux::session_exists(&session) {
                    if let Some(location) = tmux::find_active_claude_pane(&session) {
                        tmux::switch_to_session(&session, path_str)?;
                        tmux::switch_to_pane(&session, &location)?;
                    } else {
                        tmux::create_window(&session, "claude", path_str, claude_command)?;
                        tmux::switch_to_session(&session, path_str)?;
                    }
                } else {
                    tmux::create_session_with_command(&session, path_str, claude_command)?;
                    tmux::switch_to_session(&session, path_str)?;
                }

                dashboard.show_result(true, format!("Opened Claude for '{}'", worktree.info.name));
                dashboard.trigger_stats_refresh();
            }
            DashboardResult::Linear { worktree } => {
                if let Ok(Some(issue)) = linear::read_metadata(cache_dir, project_name, &worktree.info.name) {
                    if !issue.url.is_empty() {
                        let desktop_url = linear::to_desktop_url(&issue.url);
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
            }
            DashboardResult::GitHub { worktree } => {
                if let Some(branch) = &worktree.info.branch {
                    match github::get_pr_for_branch(manager.repo_root(), branch) {
                        Ok(Some(pr)) => {
                            let _ = github::write_pr_cache(cache_dir, project_name, &worktree.info.name, &github::CachedPRState::Found(pr.clone()));
                            if let Err(e) = github::open_url(&pr.url) {
                                dashboard.show_result(false, format!("Failed to open URL: {}", e));
                            } else {
                                dashboard.show_result(true, format!("Opening PR #{}: {}", pr.number, pr.title));
                            }
                        }
                        Ok(None) => {
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
            }
            DashboardResult::CreateNew { branch, category, auto_claude, plan_mode } => {
                let cat = match category.as_str() {
                    "review" => Category::Review,
                    "demo" => Category::Demo,
                    _ => Category::Dev,
                };

                match operations::create_worktree(manager, config, project_name, &branch, cat, cache_dir) {
                    Ok(create_result) => {
                        let path_str = create_result.worktree_path.to_str().unwrap();

                        if auto_claude {
                            let issue_id = create_result.issue.as_ref().map(|i| i.id.as_str());
                            let claude_cmd = tmux::build_claude_command(plan_mode, issue_id);
                            tmux::create_session_with_command(&create_result.session_name, path_str, &claude_cmd)?;
                            tmux::switch_to_session(&create_result.session_name, path_str)?;
                            dashboard.show_result(true, format!("Created '{}' and started Claude", branch));
                        } else {
                            tmux::switch_to_session(&create_result.session_name, path_str)?;
                            dashboard.show_result(true, format!("Created and switched to '{}'", branch));
                        }
                    }
                    Err(e) => {
                        dashboard.show_result(false, format!("Failed to create: {}", e));
                    }
                }
                dashboard.trigger_stats_refresh();
            }
            DashboardResult::Refresh => {
                dashboard.trigger_stats_refresh();
            }
        }
    }
}
