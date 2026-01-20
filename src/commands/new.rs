use anyhow::Result;
use colored::Colorize;

use crate::cli::Category;
use crate::config::Config;
use crate::error::GwtError;
use crate::git::WorktreeManager;
use crate::github::{self, GitHubUrlType};
use crate::linear;
use crate::sesh;
use crate::sync;
use crate::tmux;

/// Result of resolving input for the new command
struct ResolvedNewInput {
    /// Folder name for the worktree
    worktree_name: String,
    /// Git branch name
    git_branch: String,
    /// Whether to track a remote branch (vs creating new)
    track_remote: bool,
    /// Linear issue metadata (if available)
    issue: Option<linear::LinearIssue>,
}

pub fn new(branch: &str, category: Category) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let config = Config::load(Some(manager.repo_root()))?;

    let project_name = config.project_name(&manager.project_name()?);
    let worktree_root = config.worktree_root()?;
    let cache_dir = config.cache_dir()?;

    // Fetch from origin first to ensure we have the latest refs
    if manager.fetch_origin().is_ok() {
        println!("{} Fetched latest from origin", "✓".green());
    }

    // Resolve the input: check GitHub URL, then Linear, then plain branch
    let resolved = resolve_new_input(
        branch,
        &manager,
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
        .join(&project_name)
        .join(category.to_string())
        .join(&resolved.worktree_name);

    println!(
        "{} Creating worktree for branch '{}' at {}",
        "→".blue(),
        resolved.git_branch.green(),
        worktree_path.display()
    );

    // Create the worktree - either tracking remote or creating new
    if resolved.track_remote {
        println!(
            "{} Tracking existing remote branch origin/{}",
            "→".blue(),
            resolved.git_branch.cyan()
        );
        manager.create_worktree_tracking(&resolved.git_branch, &worktree_path)?;
    } else {
        manager.create_worktree(&resolved.git_branch, &worktree_path)?;
    }
    println!("{} Worktree created", "✓".green());

    // Write Linear metadata if we have issue info
    if let Some(ref issue) = resolved.issue {
        linear::write_metadata(&cache_dir, &project_name, &resolved.worktree_name, issue)?;
        println!(
            "{} Cached Linear metadata for {}",
            "✓".green(),
            issue.id.cyan()
        );
    }

    // Sync files from main repo
    let patterns = config.sync_patterns();
    if !patterns.is_empty() {
        let synced = sync::sync_files(manager.repo_root(), &worktree_path, &patterns)?;
        if !synced.is_empty() {
            println!("{} Synced files: {}", "✓".green(), synced.join(", "));
        }

        // Run direnv allow if .envrc was synced
        if synced.iter().any(|p| p == ".envrc") {
            if sync::run_direnv_allow(&worktree_path)? {
                println!("{} Ran direnv allow", "✓".green());
            }
        }
    }

    // Register with sesh if enabled
    let session_name = sesh::session_name(&project_name, &resolved.worktree_name);
    if config.sesh_auto_register() {
        sesh::register_worktree(
            &project_name,
            &resolved.worktree_name,
            worktree_path.to_str().unwrap(),
        )?;
        println!("{} Registered sesh session: {}", "✓".green(), session_name);
    }

    // Auto-switch to the new tmux session
    println!("{} Switching to session '{}'", "→".blue(), session_name.green());
    tmux::switch_to_session(&session_name, worktree_path.to_str().unwrap())?;

    Ok(())
}

/// Resolve input for the new command
/// Priority: GitHub URL > Linear issue ID > plain branch name
fn resolve_new_input(
    input: &str,
    manager: &WorktreeManager,
    linear_prefix: Option<&str>,
    linear_api_key: Option<&str>,
) -> Result<ResolvedNewInput> {
    // 1. Check if input is a GitHub URL
    if github::is_github_url(input) {
        if let Some(url_info) = github::parse_github_url(input) {
            let branch = match url_info.url_type {
                GitHubUrlType::PullRequest(pr_num) => {
                    // Fetch branch name from PR
                    println!(
                        "{} Fetching branch from PR #{}...",
                        "→".blue(),
                        pr_num
                    );
                    github::get_pr_branch(manager.repo_root(), pr_num)?
                }
                GitHubUrlType::Branch(ref b) | GitHubUrlType::Tree(ref b) => b.clone(),
            };

            // For GitHub URLs, check whether the corresponding remote branch exists
            let track_remote = manager.remote_branch_exists(&branch);

            // Generate worktree name from branch (sanitize / to -)
            let worktree_name = sanitize_for_filesystem(&branch);

            return Ok(ResolvedNewInput {
                worktree_name,
                git_branch: branch,
                track_remote,
                issue: None,
            });
        }
    }

    // 2. Try Linear resolution
    let linear_resolved = linear::resolve_input(input, linear_prefix, linear_api_key)?;

    // 3. Check if the resolved branch exists on remote
    let track_remote = manager.remote_branch_exists(&linear_resolved.git_branch);

    Ok(ResolvedNewInput {
        worktree_name: linear_resolved.worktree_name,
        git_branch: linear_resolved.git_branch,
        track_remote,
        issue: linear_resolved.issue,
    })
}

/// Sanitize a string for use as a filesystem path (replace / with -)
fn sanitize_for_filesystem(name: &str) -> String {
    name.replace('/', "-")
}
