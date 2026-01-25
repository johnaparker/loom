//! Disable subcommand implementation.

use anyhow::Result;
use colored::Colorize;

use super::ConfigTarget;
use crate::cli::{ConfigFeature, ConfigScope};
use crate::config::{GlobalConfig, ProjectConfig};

/// Run the disable command for a feature.
pub fn run(feature: ConfigFeature, scope: ConfigScope) -> Result<()> {
    let target = ConfigTarget::resolve(scope)?;

    match feature {
        ConfigFeature::Claude => disable_claude(&target),
        ConfigFeature::Github => disable_github(&target),
        ConfigFeature::Linear => disable_linear(&target),
    }
}

fn disable_claude(target: &ConfigTarget) -> Result<()> {
    if target.is_project_scope {
        disable_project_feature(target, |config| {
            config.claude = Some(crate::config::ClaudeOverride {
                enabled: Some(false),
                sandbox: None,
                sandbox_auto_allow_bash: None,
            });
        })?;
    } else {
        let mut config = GlobalConfig::load()?;
        config.claude.enabled = false;
        config.save()?;
    }

    println!(
        "{} Claude integration disabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );
    println!(
        "{}",
        "Note: Claude hooks are not removed (may be used by other tools)".dimmed()
    );

    Ok(())
}

fn disable_github(target: &ConfigTarget) -> Result<()> {
    if target.is_project_scope {
        disable_project_feature(target, |config| {
            config.github = Some(crate::config::IntegrationOverride {
                enabled: Some(false),
            });
        })?;
    } else {
        let mut config = GlobalConfig::load()?;
        config.github.enabled = false;
        config.save()?;
    }

    println!(
        "{} GitHub integration disabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );

    Ok(())
}

fn disable_linear(target: &ConfigTarget) -> Result<()> {
    if target.is_project_scope {
        disable_project_feature(target, |config| {
            config.linear = Some(crate::config::IntegrationOverride {
                enabled: Some(false),
            });
        })?;
    } else {
        let mut config = GlobalConfig::load()?;
        config.linear.enabled = false;
        // Also clear API key and team prefix for security
        config.linear.api_key = None;
        config.linear.team_prefix = None;
        config.save()?;
    }

    println!(
        "{} Linear integration disabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );

    if !target.is_project_scope {
        println!("{}", "API key and team prefix have been cleared".dimmed());
    }

    Ok(())
}

/// Helper to update project config with a feature modification.
fn disable_project_feature<F>(target: &ConfigTarget, modify: F) -> Result<()>
where
    F: FnOnce(&mut ProjectConfig),
{
    let repo_root = target
        .repo_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Project scope requires a repository root"))?;

    let mut config = ProjectConfig::load_or_default(repo_root)?;
    modify(&mut config);
    config.save(repo_root)?;

    Ok(())
}
