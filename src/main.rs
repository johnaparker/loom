use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use gwt::cli::{Cli, Commands};
use gwt::commands;

fn main() {
    if let Err(err) = run() {
        eprintln!("{} {}", "Error:".red().bold(), err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { branch, category } => {
            commands::new(&branch, category)?;
        }
        Commands::List => {
            commands::list()?;
        }
        Commands::Switch { name } => {
            commands::switch(name.as_deref())?;
        }
        Commands::Merge { name, force } => {
            commands::merge(&name, force)?;
        }
        Commands::Remove { name, force } => {
            commands::remove(&name, force)?;
        }
        Commands::Status => {
            commands::status()?;
        }
        Commands::Main => {
            commands::main_cmd()?;
        }
    }

    Ok(())
}
