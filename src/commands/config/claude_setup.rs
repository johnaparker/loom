//! Claude Code integration setup.
//!
//! Handles enabling Claude Code hooks and sandbox configuration.

use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::process::Command;

use super::ConfigTarget;
use crate::commands::ui::confirm_default_yes;
use crate::config::{ClaudeOverride, GlobalConfig, ProjectConfig};
use crate::error::GroveError;

/// Enable Claude Code integration.
pub fn enable(target: &ConfigTarget) -> Result<()> {
    // Step 1: Check if Claude is installed
    check_claude_installed()?;
    println!("{} Claude Code is installed", "✓".green());

    // Step 2: Configure hooks in Claude settings.json
    configure_hooks(target)?;
    println!(
        "{} Configured grove hooks in {}",
        "✓".green(),
        target.claude_settings_path.display()
    );

    // Step 3: Ask about sandbox mode
    let enable_sandbox = confirm_default_yes("Enable sandbox mode with auto-allow bash?")?;

    // Step 4: Update grove config
    update_grove_config(target, enable_sandbox)?;
    println!(
        "{} Updated grove config at {}",
        "✓".green(),
        target.grove_config_path.display()
    );

    println!();
    println!(
        "{} Claude integration enabled at {} scope",
        "✓".green().bold(),
        target.scope_name()
    );

    if enable_sandbox {
        println!(
            "{}",
            "Sandbox mode enabled - Claude will use restricted permissions".dimmed()
        );
    }

    Ok(())
}

/// Check if Claude Code CLI is installed.
fn check_claude_installed() -> Result<()> {
    let output = Command::new("claude").args(["--version"]).output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err(GroveError::ClaudeNotInstalled.into()),
    }
}

/// Configure grove hooks in Claude settings.json.
fn configure_hooks(target: &ConfigTarget) -> Result<()> {
    // Read existing settings or create new object
    let mut settings: serde_json::Value = if target.claude_settings_path.exists() {
        let content = fs::read_to_string(&target.claude_settings_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Define the hooks we need to add
    let hook_events = [
        ("PreToolUse", "grove hook tool-use"),
        ("PostToolUse", "grove hook tool-result"),
        ("UserPromptSubmit", "grove hook user-prompt"),
        ("Stop", "grove hook stop"),
        ("Notification", "grove hook notification"),
        ("SessionStart", "grove hook session-start"),
        ("SessionEnd", "grove hook session-end"),
    ];

    // Ensure hooks object exists
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }

    let hooks = settings["hooks"]
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid hooks structure in Claude settings"))?;

    for (event, command) in hook_events {
        add_grove_hook(hooks, event, command);
    }

    // Write the updated settings
    if let Some(parent) = target.claude_settings_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&settings)?;
    fs::write(&target.claude_settings_path, content)?;

    Ok(())
}

/// Add a grove hook to an event if not already present.
fn add_grove_hook(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    command: &str,
) {
    let grove_hook = serde_json::json!({
        "type": "command",
        "command": command
    });

    // Get or create the array for this event
    let event_hooks = hooks
        .entry(event.to_string())
        .or_insert_with(|| serde_json::json!([]));

    // If it's an array, check if grove hook already exists
    if let Some(arr) = event_hooks.as_array_mut() {
        let has_grove_hook = arr.iter().any(|h| {
            // Check if this is a matcher with hooks array
            if let Some(inner_hooks) = h.get("hooks").and_then(|h| h.as_array()) {
                inner_hooks.iter().any(|inner| {
                    inner
                        .get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c.starts_with("grove hook"))
                })
            } else {
                // Direct hook object
                h.get("command")
                    .and_then(|c| c.as_str())
                    .is_some_and(|c| c.starts_with("grove hook"))
            }
        });

        if !has_grove_hook {
            // Wrap in matcher format (standard Claude hook format)
            arr.push(serde_json::json!({
                "hooks": [grove_hook]
            }));
        }
    }
}

/// Update grove configuration with Claude settings.
fn update_grove_config(target: &ConfigTarget, enable_sandbox: bool) -> Result<()> {
    if target.is_project_scope {
        update_project_config(target, enable_sandbox)
    } else {
        update_global_config(enable_sandbox)
    }
}

fn update_global_config(enable_sandbox: bool) -> Result<()> {
    let mut config = GlobalConfig::load()?;
    config.claude.enabled = true;
    config.claude.sandbox = enable_sandbox;
    if enable_sandbox {
        config.claude.sandbox_auto_allow_bash = true;
    }
    config.save()?;
    Ok(())
}

fn update_project_config(target: &ConfigTarget, enable_sandbox: bool) -> Result<()> {
    let repo_root = target
        .repo_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Project scope requires a repository root"))?;

    let mut config = ProjectConfig::load_or_default(repo_root)?;
    config.claude = Some(ClaudeOverride {
        enabled: Some(true),
        sandbox: Some(enable_sandbox),
        sandbox_auto_allow_bash: if enable_sandbox { Some(true) } else { None },
    });
    config.save(repo_root)?;

    Ok(())
}
