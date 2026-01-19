use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use gwt::cli::{Cli, Commands};
use gwt::commands;
use gwt::error::GwtError;

fn main() {
    if let Err(err) = run() {
        // Check if the error is a GwtError for enhanced display
        if let Some(gwt_err) = err.downcast_ref::<GwtError>() {
            eprintln!("{}", gwt_err.display_with_suggestion());
        } else {
            eprintln!("{} {}", "Error:".red().bold(), err);
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Status) {
        Commands::New { branch, category } => {
            commands::new(&branch, category)?;
        }
        Commands::List => {
            commands::list()?;
        }
        Commands::Switch { name } => {
            commands::switch(name.as_deref())?;
        }
        Commands::Merge { name, force, dry_run } => {
            commands::merge(&name, force, dry_run)?;
        }
        Commands::Remove { name, force, dry_run } => {
            commands::remove(name.as_deref(), force, dry_run)?;
        }
        Commands::Status => {
            commands::status()?;
        }
        Commands::Sync { name, dry_run } => {
            commands::sync(name.as_deref(), dry_run)?;
        }
        Commands::Review => {
            commands::review()?;
        }
        Commands::Main => {
            commands::main_cmd()?;
        }
        Commands::Linear => {
            commands::linear_cmd()?;
        }
        Commands::GitHub => {
            commands::github_cmd()?;
        }
        Commands::Completions { shell } => {
            commands::completions(shell)?;
        }
        Commands::Hook { event } => {
            commands::hook(&event)?;
        }
    }

    Ok(())
}
