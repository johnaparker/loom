//! Set workflow subcommand implementation.

use anyhow::Result;
use colored::Colorize;

use super::ConfigTarget;
use crate::cli::{ConfigScope, WorkflowArg};
use crate::config::{GlobalConfig, ProjectConfig, SyncWorkflow};

/// Run the set-workflow command.
pub fn run(workflow: WorkflowArg, scope: ConfigScope) -> Result<()> {
    let target = ConfigTarget::resolve(scope)?;

    let sync_workflow = match workflow {
        WorkflowArg::Push => SyncWorkflow::Push,
        WorkflowArg::Pull => SyncWorkflow::Pull,
    };

    if target.is_project_scope {
        set_project_workflow(&target, sync_workflow)?;
    } else {
        set_global_workflow(sync_workflow)?;
    }

    let workflow_str = match workflow {
        WorkflowArg::Push => "push",
        WorkflowArg::Pull => "pull",
    };

    println!(
        "{} Workflow set to {} at {} scope",
        "✓".green(),
        workflow_str.cyan(),
        target.scope_name().cyan()
    );

    Ok(())
}

fn set_global_workflow(workflow: SyncWorkflow) -> Result<()> {
    let mut config = GlobalConfig::load()?;
    config.workflow = workflow;
    config.save()?;
    Ok(())
}

fn set_project_workflow(target: &ConfigTarget, workflow: SyncWorkflow) -> Result<()> {
    let repo_root = target
        .repo_root
        .as_ref()
        .expect("project scope should have repo_root");

    let mut config = ProjectConfig::load_or_default(repo_root)?;
    config.git.workflow = Some(workflow);
    config.save(repo_root)?;

    Ok(())
}
