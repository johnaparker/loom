//! Terminal setup and teardown utilities.
//!
//! Provides helpers for managing the terminal state when running TUI applications.

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout, Stdout};

/// A RAII guard for terminal alternate screen mode.
///
/// When dropped, this guard will restore the terminal to its normal state.
/// This ensures cleanup happens even if the code panics.
pub struct AlternateScreenGuard {
    _private: (),
}

impl Drop for AlternateScreenGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Run a function with the terminal in alternate screen mode.
///
/// This function:
/// 1. Enables raw mode
/// 2. Enters the alternate screen
/// 3. Creates a terminal
/// 4. Runs the provided function
/// 5. Restores the terminal state (even on error)
///
/// # Example
///
/// ```ignore
/// use gwt::core::with_alternate_screen;
///
/// with_alternate_screen(|terminal| {
///     terminal.draw(|f| {
///         // Draw your UI here
///     })?;
///     Ok(())
/// })?;
/// ```
pub fn with_alternate_screen<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&mut Terminal<CrosstermBackend<Stdout>>) -> Result<T>,
{
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the function - cleanup happens even on error via the guard
    let _guard = AlternateScreenGuard { _private: () };
    let result = f(&mut terminal);

    // Guard drop will handle cleanup
    drop(_guard);

    // Now do explicit cleanup (guard already handled it, but be explicit)
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    result
}

/// Set up the terminal for alternate screen mode.
///
/// Returns a terminal and a guard that will restore the terminal on drop.
/// Use this when you need persistent terminal access across multiple operations.
///
/// # Example
///
/// ```ignore
/// use gwt::core::terminal::setup_terminal;
///
/// let (mut terminal, _guard) = setup_terminal()?;
/// // Use terminal...
/// // Terminal is restored when _guard is dropped
/// ```
pub fn setup_terminal() -> Result<(Terminal<CrosstermBackend<Stdout>>, AlternateScreenGuard)> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    Ok((terminal, AlternateScreenGuard { _private: () }))
}

/// Tear down the terminal from alternate screen mode.
///
/// This is an explicit cleanup function. The `AlternateScreenGuard` will
/// also perform cleanup on drop, but this allows for explicit cleanup
/// with error handling.
pub fn teardown_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
