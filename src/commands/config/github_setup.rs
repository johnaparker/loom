//! GitHub integration setup.
//!
//! Handles enabling GitHub CLI integration for PR status tracking.

use anyhow::Result;
use colored::Colorize;

use super::ConfigTarget;
use crate::config::{
    GlobalConfig, IntegrationOverride, ProjectConfig, ProjectGitConfig, ProjectSyncConfig,
};
use crate::github::check_gh_cli;

/// Enable GitHub integration.
pub fn enable(target: &ConfigTarget) -> Result<()> {
    // Step 1: Check if gh CLI is installed and authenticated
    check_gh_cli()?;
    println!(
        "{} GitHub CLI (gh) is installed and authenticated",
        "✓".green()
    );

    // Step 2: Update grove config
    update_grove_config(target)?;
    println!(
        "{} Updated grove config at {}",
        "✓".green(),
        target.grove_config_path.display()
    );

    println!();
    println!(
        "{} GitHub integration enabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );
    println!(
        "{}",
        "PR status and checks will be shown in the dashboard".dimmed()
    );

    Ok(())
}

/// Update grove configuration with GitHub settings.
fn update_grove_config(target: &ConfigTarget) -> Result<()> {
    if target.is_project_scope {
        update_project_config(target)
    } else {
        update_global_config()
    }
}

fn update_global_config() -> Result<()> {
    let mut config = GlobalConfig::load()?;
    config.github.enabled = true;
    config.save()?;
    Ok(())
}

fn update_project_config(target: &ConfigTarget) -> Result<()> {
    let repo_root = target
        .repo_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Project scope requires a repository root"))?;

    // Load existing project config or create new one
    let mut config = ProjectConfig::load(None, repo_root)?.unwrap_or_else(|| ProjectConfig {
        project_name: None,
        sync: ProjectSyncConfig::default(),
        git: ProjectGitConfig::default(),
        linear: None,
        github: None,
        diffview: None,
        claude: None,
    });

    config.github = Some(IntegrationOverride {
        enabled: Some(true),
    });

    config.save(repo_root)?;
    Ok(())
}
