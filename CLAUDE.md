# CLAUDE.md - Project Guide

## Project Overview

gwt (Git Worktree) is a Rust CLI/TUI tool for managing git worktrees with tmux/sesh integration. It's designed for trunk-based development workflows where developers work on multiple branches simultaneously.

## Project Structure

```
src/
├── main.rs           # Entry point, CLI dispatch
├── lib.rs            # Library exports
├── error.rs          # GwtError enum with user-friendly suggestions
├── output.rs         # Dry-run output helpers
├── cli/
│   └── mod.rs        # Clap CLI definitions (Commands, Category enum)
├── commands/
│   ├── mod.rs        # Command exports
│   ├── new.rs        # gwt new - create worktree + auto-switch
│   ├── list.rs       # gwt list - list worktrees by category
│   ├── switch.rs     # gwt switch [name] - TUI picker or fuzzy match
│   ├── merge.rs      # gwt merge - merge to main (supports --dry-run)
│   ├── remove.rs     # gwt remove [name] - TUI picker or fuzzy match (supports --dry-run)
│   ├── status.rs     # gwt status - show status
│   ├── main_cmd.rs   # gwt main - switch to main
│   └── completions.rs # gwt completions <shell> - generate shell completions
├── config/
│   ├── mod.rs        # Combined config handling
│   ├── global.rs     # ~/.config/gwt/config.toml
│   └── project.rs    # .gwt.toml in repo root
├── git/
│   ├── mod.rs
│   └── worktree.rs   # Git2 + git CLI worktree ops
├── sesh/
│   └── mod.rs        # Sesh.toml integration
├── tmux.rs           # Tmux session management (create, switch, kill)
├── tui/
│   ├── mod.rs
│   └── picker.rs     # Ratatui fuzzy picker (Esc/Ctrl+C to cancel)
└── sync/
    └── mod.rs        # File sync from main to worktree
```

## Key Dependencies

| Dependency | Purpose |
|------------|---------|
| clap | CLI argument parsing with derive macros |
| clap_complete | Shell completion generation |
| git2 | Git repository operations |
| ratatui + crossterm | Terminal UI for fuzzy picker |
| nucleo | Fuzzy matching algorithm |
| serde + toml | Configuration file handling |
| anyhow + thiserror | Error handling (GwtError for user-friendly messages) |
| colored | Colored terminal output |
| dirs | Cross-platform directory paths |

## Coding Conventions

- Use `anyhow::Result` for error handling in commands
- Use `GwtError` (in `src/error.rs`) for user-facing errors with helpful suggestions
- Use `colored` for terminal output formatting
- Commands follow pattern: open repo -> load config -> perform action -> update sesh/tmux
- Git operations use `git2` where possible, fall back to CLI for complex operations
- Commands with optional name arg: no arg = TUI picker, with arg = fuzzy match (nucleo)
- Destructive commands (merge, remove) support `--dry-run` flag to preview actions
- Destructive commands (remove) should confirm with y/N prompt when fuzzy matching
- Use shared `tmux` module for session operations, `sesh` module for sesh.toml
- Use `output.rs` helpers for dry-run output formatting

## Testing Approach

Currently no tests. Future tests should:
- Unit test config parsing
- Integration test worktree operations in temp repos
- Mock tmux/sesh for session management tests

## Common Tasks

### Adding a new command

1. Add variant to `Commands` enum in `src/cli/mod.rs`
2. Create `src/commands/{name}.rs` with `pub fn {name}() -> Result<()>`
3. Export from `src/commands/mod.rs`
4. Add match arm in `src/main.rs`

### Modifying config

1. Update structs in `src/config/global.rs` or `src/config/project.rs`
2. Add accessor methods to `Config` in `src/config/mod.rs`

### Adding a new error type

1. Add variant to `GwtError` enum in `src/error.rs`
2. Implement `suggestion()` match arm with helpful hint
3. Use in commands: `return Err(GwtError::YourError { ... }.into())`

### Shell completions

Generate and install completions:
```bash
gwt completions bash > ~/.local/share/bash-completion/completions/gwt
gwt completions zsh > ~/.zfunc/_gwt
gwt completions fish > ~/.config/fish/completions/gwt.fish
```
