//! Linear integration setup.
//!
//! Handles enabling Linear issue tracking integration.

use anyhow::Result;
use colored::Colorize;

use super::ConfigTarget;
use crate::commands::ui::{prompt_secret, prompt_string};
use crate::config::{
    GlobalConfig, IntegrationOverride, ProjectConfig, ProjectGitConfig, ProjectSyncConfig,
};
use crate::linear::{LinearTeam, fetch_teams};

/// Enable Linear integration.
pub fn enable(target: &ConfigTarget) -> Result<()> {
    // For project scope, just enable the override (API key is in global config)
    if target.is_project_scope {
        return enable_project_scope(target);
    }

    // Step 1: Check if API key is already configured
    let existing_config = GlobalConfig::load()?;
    let api_key = if existing_config.linear.api_key.is_some() {
        println!("{} Using existing Linear API key", "✓".green());
        existing_config.linear.api_key.clone().unwrap()
    } else {
        // Prompt for API key
        println!(
            "{}",
            "Linear API key required (find at https://linear.app/settings/api)".dimmed()
        );
        let key = prompt_secret("Linear API key")?;
        if key.is_empty() {
            return Err(anyhow::anyhow!("API key is required"));
        }
        key
    };

    // Step 2: Validate API key by fetching teams
    println!("{}", "Validating API key...".dimmed());
    let teams = fetch_teams(&api_key)?;

    if teams.is_empty() {
        return Err(anyhow::anyhow!(
            "No teams found. Check your API key permissions."
        ));
    }

    println!("{} API key is valid", "✓".green());

    // Step 3: Show available teams and let user select
    let team_prefix = select_team(&teams)?;
    println!(
        "{} Selected team prefix: {}",
        "✓".green(),
        team_prefix.cyan()
    );

    // Step 4: Update grove config
    update_global_config(&api_key, &team_prefix)?;
    println!(
        "{} Updated grove config at {}",
        "✓".green(),
        target.grove_config_path.display()
    );

    println!();
    println!(
        "{} Linear integration enabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );
    println!(
        "{}",
        format!(
            "Issues with prefix {} will be linked to worktrees",
            team_prefix.cyan()
        )
        .dimmed()
    );

    Ok(())
}

/// Enable Linear integration at project scope (just the override).
fn enable_project_scope(target: &ConfigTarget) -> Result<()> {
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

    config.linear = Some(IntegrationOverride {
        enabled: Some(true),
    });

    config.save(repo_root)?;

    println!(
        "{} Updated grove config at {}",
        "✓".green(),
        target.grove_config_path.display()
    );

    println!();
    println!(
        "{} Linear integration enabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );
    println!(
        "{}",
        "Note: API key and team prefix must be configured at user scope".dimmed()
    );

    Ok(())
}

/// Let user select a team from the available teams.
fn select_team(teams: &[LinearTeam]) -> Result<String> {
    println!();
    println!("Available teams:");
    for (i, team) in teams.iter().enumerate() {
        println!(
            "  {} {} ({})",
            format!("[{}]", i + 1).dimmed(),
            team.name,
            team.key.cyan()
        );
    }
    println!();

    // If only one team, auto-select it
    if teams.len() == 1 {
        println!(
            "{}",
            format!("Auto-selecting only available team: {}", teams[0].key).dimmed()
        );
        return Ok(teams[0].key.clone());
    }

    // Let user select by number or enter custom prefix
    let prompt = format!("Select team (1-{}) or enter prefix", teams.len());
    let input = prompt_string(&prompt, Some(&teams[0].key))?;

    // Try to parse as number
    if let Ok(num) = input.parse::<usize>()
        && num >= 1
        && num <= teams.len()
    {
        return Ok(teams[num - 1].key.clone());
    }

    // Treat as custom prefix
    Ok(input.to_uppercase())
}

/// Update global configuration with Linear settings.
fn update_global_config(api_key: &str, team_prefix: &str) -> Result<()> {
    let mut config = GlobalConfig::load()?;
    config.linear.enabled = true;
    config.linear.api_key = Some(api_key.to_string());
    config.linear.team_prefix = Some(team_prefix.to_string());
    config.linear.auto_update_status = true;
    config.save()?;
    Ok(())
}
