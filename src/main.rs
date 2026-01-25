use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use grove::cli::{Cli, Commands};
use grove::commands;
use grove::error::GroveError;

fn main() {
    if let Err(err) = run() {
        // Check if the error is a GroveError for enhanced display
        if let Some(grove_err) = err.downcast_ref::<GroveError>() {
            eprintln!("{}", grove_err.display_with_suggestion());
        } else {
            eprintln!("{} {}", "Error:".red().bold(), err);
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::New { branch, category }) => {
            commands::new(&branch, category)?;
        }
        Some(Commands::List) => {
            commands::list()?;
        }
        Some(Commands::Switch { name }) => {
            commands::switch(name.as_deref())?;
        }
        Some(Commands::Merge {
            name,
            force,
            dry_run,
        }) => {
            commands::merge(&name, force, dry_run)?;
        }
        Some(Commands::Remove {
            name,
            force,
            dry_run,
        }) => {
            commands::remove(name.as_deref(), force, dry_run)?;
        }
        Some(Commands::Sync { name, dry_run }) => {
            commands::sync(name.as_deref(), dry_run)?;
        }
        Some(Commands::Review) => {
            commands::review()?;
        }
        Some(Commands::Main) => {
            commands::main_cmd()?;
        }
        Some(Commands::Linear) => {
            commands::linear_cmd()?;
        }
        Some(Commands::GitHub) => {
            commands::github_cmd()?;
        }
        Some(Commands::Completions { shell }) => {
            commands::completions(shell)?;
        }
        Some(Commands::Hook { event }) => {
            commands::hook(&event)?;
        }
        Some(Commands::Prune { force, dry_run }) => {
            commands::prune(force, dry_run)?;
        }
        Some(Commands::Config { command }) => {
            commands::config(command)?;
        }
        None => {
            commands::status()?;
        }
    }

    Ok(())
}
