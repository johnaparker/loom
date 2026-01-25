//! Linear integration setup.
//!
//! Handles enabling Linear issue tracking integration.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use colored::Colorize;

use super::ConfigTarget;
use crate::commands::ui::{confirm, prompt_secret, prompt_string};
use crate::config::{GlobalConfig, IntegrationOverride, ProjectConfig};
use crate::linear::{LinearTeam, fetch_teams};

/// Enable Linear integration.
pub fn enable(target: &ConfigTarget) -> Result<()> {
    // For project scope, just enable the override (API key is in global config)
    if target.is_project_scope {
        return enable_project_scope(target);
    }

    // Step 1: Check if API key is already configured
    let existing_config = GlobalConfig::load()?;
    let api_key = if let Some(key) = &existing_config.linear.api_key {
        println!("{} Using existing Linear API key", "✓".green());
        key.clone()
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

    // Step 4: Update loom config
    update_global_config(&api_key, &team_prefix)?;
    println!(
        "{} Updated loom config at {}",
        "✓".green(),
        target.loom_config_path.display()
    );

    // Step 5: Offer to install Linear skill for Claude Code
    println!();
    install_linear_skill_prompt(target, &team_prefix)?;

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

    let mut config = ProjectConfig::load_or_default(repo_root)?;
    config.linear = Some(IntegrationOverride {
        enabled: Some(true),
    });

    config.save(repo_root)?;

    println!(
        "{} Updated loom config at {}",
        "✓".green(),
        target.loom_config_path.display()
    );

    // Get team prefix from global config for skill installation
    let global_config = GlobalConfig::load()?;
    let team_prefix = global_config
        .linear
        .team_prefix
        .clone()
        .unwrap_or_else(|| "PREFIX".to_string());

    // Offer to install Linear skill for Claude Code
    println!();
    install_linear_skill_prompt(target, &team_prefix)?;

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

/// Prompt user to install the Linear skill for Claude Code.
fn install_linear_skill_prompt(target: &ConfigTarget, team_prefix: &str) -> Result<()> {
    if confirm("Install Linear skill for Claude Code?")? {
        install_linear_skill(target, team_prefix)?;
    }
    Ok(())
}

/// Install the Linear skill for Claude Code.
fn install_linear_skill(target: &ConfigTarget, team_prefix: &str) -> Result<()> {
    let skill_path = get_skill_path(target)?;

    // Create parent directories if needed
    if let Some(parent) = skill_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write skill content
    let content = generate_skill_content(team_prefix);
    fs::write(&skill_path, content)?;

    println!(
        "{} Installed Linear skill at {}",
        "✓".green(),
        skill_path.display()
    );

    Ok(())
}

/// Get the path for the Linear skill file based on scope.
fn get_skill_path(target: &ConfigTarget) -> Result<PathBuf> {
    if target.is_project_scope {
        // Project scope: <repo_root>/.claude/skills/loom-linear/SKILL.md
        let repo_root = target
            .repo_root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Project scope requires a repository root"))?;
        Ok(repo_root.join(".claude/skills/loom-linear/SKILL.md"))
    } else {
        // User scope: ~/.claude/skills/loom-linear/SKILL.md
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        Ok(home.join(".claude/skills/loom-linear/SKILL.md"))
    }
}

/// Generate the Linear skill content with the team prefix interpolated.
fn generate_skill_content(team_prefix: &str) -> String {
    format!(
        r#"---
name: loom-linear
description: When asked about Linear issues, tasks, or projects, or when {prefix}-<number> is mentioned
---

# Linear

Linear is project management software for tracking issues and tasks. Issues move through a Kanban-style board and each has an ID prefixed with "{prefix}-" (e.g., "{prefix}-163").

Use the Linear MCP server tools to interact with Linear.

## Working on an issue

1. Review the issue description using `get_issue`, along with any user input
2. Explore the codebase to understand the context
3. Ask clarifying questions if requirements are unclear or multiple approaches exist
4. Implement the solution

## Other common tasks

- **Find issues**: Use `list_issues` with filters (assignee, project, label, state)
- **Create issues**: Use `create_issue` with title, description, and team
- **Update issues**: Use `update_issue` to change status, assignee, labels, etc.
- **Projects**: Use `list_projects` and `get_project` for project-level context
"#,
        prefix = team_prefix
    )
}
