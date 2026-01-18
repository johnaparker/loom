use colored::Colorize;

pub fn dry_run_header() {
    println!();
    println!("{}", "=== DRY RUN MODE ===".yellow().bold());
    println!("The following actions would be performed:");
    println!();
}

pub fn dry_run_action(desc: &str) {
    println!("  {} {}", "->".cyan(), desc);
}

pub fn dry_run_warning(msg: &str) {
    println!("  {} {}", "!".yellow(), msg);
}

pub fn dry_run_footer() {
    println!();
    println!("{}", "No changes were made.".dimmed());
    println!(
        "Remove {} to execute these actions.",
        "--dry-run".cyan()
    );
}
