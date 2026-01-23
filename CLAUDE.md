# CLAUDE.md - Project Guide

## Project Overview

gwt (Git Worktree) is a **TUI-first** tool for managing git worktrees with deep Claude Code integration. Its primary purpose is to give developers a **centralized control center** for:

1. **Managing multiple Claude agents** - Track working/waiting/idle states across worktrees, capture logs, surface permission requests
2. **Worktree lifecycle** - Create, switch, merge, and destroy worktrees from a single dashboard
3. **External integrations** - Link worktrees to Linear issues and GitHub PRs for project context

### Design Philosophy

- **TUI-first**: The dashboard (`gwt status`) is the primary interface. CLI commands exist for one-off operations and automated testing (TUI cannot be tested).
- **Human in control**: Surface agent activity so developers can monitor, interrupt, or redirect multiple Claude sessions
- **Trunk-based workflow**: All feature work happens in worktrees; main branch stays clean for merging

## Core Features

### Claude Code Integration (Primary)
The killer feature. Via Claude Code hooks (`gwt hook`):
- **State tracking**: Working → WaitingPermission → Idle state machine per worktree
- **Event capture**: Tool calls, permission requests, session start/stop
- **Dashboard alerts**: Visual indicators for agents needing attention
- **Log access**: Quick jump to Claude session from any worktree

### Worktree Management
- Create worktrees linked to Linear issues (`gwt new JOH-123`)
- Merge to main with cleanup (`gwt merge`)
- Sync files from main (.env, .envrc, .claude/)

### External Integrations
| Connector | Purpose |
|-----------|---------|
| Linear | Link worktrees to issues, show status in dashboard |
| GitHub | Track PR state, checks, reviews in dashboard |
| tmux | Session management, quick switching |
| sesh | Worktree registration for session picker |

### Local Tool Integration
- **tmux**: Create/switch sessions per worktree
- **direnv**: Auto-allow .envrc in new worktrees
- **nvim diffview**: Review changes vs main (`gwt review` / `r` key)

## Project Structure

```
src/
├── main.rs               # Entry point, CLI dispatch
├── lib.rs                # Library exports + backward-compat re-exports
├── error.rs              # GwtError enum with user-friendly suggestions
├── output.rs             # Dry-run output helpers
├── cli/
│   └── mod.rs            # Clap CLI definitions (Commands, Category enum)
├── commands/
│   ├── mod.rs            # Command exports
│   ├── new.rs            # gwt new - create worktree + auto-switch
│   ├── list.rs           # gwt list - list worktrees by category
│   ├── switch.rs         # gwt switch [name] - TUI picker or fuzzy match
│   ├── merge.rs          # gwt merge - merge to main (supports --dry-run)
│   ├── remove.rs         # gwt remove [name] - TUI picker or fuzzy match (supports --dry-run)
│   ├── status.rs         # gwt status - interactive dashboard
│   ├── main_cmd.rs       # gwt main - switch to main
│   ├── hook.rs           # Claude Code hook integration
│   └── completions.rs    # gwt completions <shell> - generate shell completions
├── config/
│   ├── mod.rs            # Combined config handling + ResolvedConfig
│   ├── global.rs         # ~/.config/gwt/config.toml
│   ├── project.rs        # .gwt.toml in repo root
│   └── worktree.rs       # .gwt.toml in worktree directory (overrides)
├── core/                 # Shared TUI/CLI utilities
│   ├── mod.rs
│   ├── fuzzy.rs          # FuzzyMatcher - unified fuzzy matching (nucleo)
│   └── terminal.rs       # Terminal setup/teardown helpers
├── connectors/           # External service integrations
│   ├── mod.rs
│   ├── cache.rs          # Shared cache utilities (~/.cache/gwt/)
│   ├── linear/           # Linear issue tracking
│   │   ├── mod.rs        # Re-exports
│   │   ├── types.rs      # LinearIssue, ResolvedInput
│   │   ├── parser.rs     # Issue ID parsing (JOH-123)
│   │   ├── api.rs        # GraphQL API calls
│   │   ├── resolve.rs    # Input resolution (URL, ID, branch)
│   │   └── cache.rs      # Metadata caching
│   ├── github/           # GitHub PR integration
│   │   ├── mod.rs        # Re-exports, check_gh_cli()
│   │   ├── types.rs      # GitHubPR, ChecksStatus, PRComment
│   │   ├── url.rs        # URL parsing/generation
│   │   ├── cli.rs        # gh CLI wrapper functions
│   │   └── cache.rs      # PR cache operations
│   ├── claude/           # Claude session tracking
│   │   ├── mod.rs        # Re-exports
│   │   ├── types.rs      # ClaudeState, ClaudeEvent, ClaudeSession
│   │   ├── time.rs       # ISO8601 time utilities
│   │   ├── state.rs      # State machine logic
│   │   └── cache.rs      # Session state caching
│   ├── tmux/
│   │   └── mod.rs        # Tmux session management
│   └── sesh/
│       └── mod.rs        # Sesh.toml integration
├── git/
│   ├── mod.rs
│   └── worktree.rs       # Git2 + git CLI worktree ops
├── tui/
│   ├── mod.rs
│   ├── picker.rs         # Ratatui fuzzy picker
│   ├── dashboard/        # Interactive worktree dashboard
│   │   ├── mod.rs        # Dashboard struct, run loop
│   │   ├── state.rs      # DashboardMode, DashboardResult enums
│   │   ├── data.rs       # Data loading (Linear, GitHub, Claude)
│   │   ├── input.rs      # Keyboard input handling
│   │   └── render.rs     # UI rendering logic
│   ├── modals/           # Modal dialogs (delete, merge, new)
│   └── widgets/          # Reusable UI components
└── sync/
    └── mod.rs            # File sync from main to worktree
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
- Use `output.rs` helpers for dry-run output formatting

### Module Organization

- **`core/`**: Shared utilities used by both CLI commands and TUI
  - `FuzzyMatcher` for all fuzzy matching (replaces duplicate implementations)
  - `with_alternate_screen()` for terminal setup/teardown
- **`connectors/`**: External service integrations (Linear, GitHub, Claude, tmux, sesh)
  - Each connector has its own subdirectory with types, API, and cache modules
  - Shared cache utilities in `connectors/cache.rs` (platform cache dir via `dirs` crate: `~/Library/Caches/gwt/` on macOS)
  - Import via `crate::linear`, `crate::github`, etc. (re-exported in lib.rs)
- **`tui/dashboard/`**: Split into focused modules (state, data, input, render)

### TUI/CLI Code Sharing

When adding features that have both TUI and CLI interfaces:

1. **Business logic in `core/` or `connectors/`** - Never put domain logic in `commands/` or `tui/`. Commands and TUI should only orchestrate.
2. **Data types are shared** - Define types once (usually in connector's `types.rs`), use everywhere
3. **CLI commands call the same functions as TUI** - If `gwt remove` and the dashboard delete modal both remove worktrees, they should call the same underlying function
4. **TUI-specific code stays in `tui/`** - Rendering, input handling, and state management are TUI concerns
5. **CLI-specific code stays in `commands/`** - Argument parsing, output formatting, and prompts are CLI concerns

This ensures the codebase scales without duplication and bugs are fixed in one place.

### Async Boundaries

The codebase uses async for external I/O but keeps the TUI event loop synchronous:

- **Async**: API calls to Linear, GitHub; worktree stats loading; cache file I/O
- **Sync**: TUI event loop, simple git2 operations, config loading
- **Pattern**: Dashboard spawns async tasks for data loading, polls for completion in the render loop
- **Rule**: Never `.await` in the TUI event loop - it blocks rendering and input handling
- **Worktree refresh**: Use `trigger_stats_refresh()` to reload worktree data asynchronously; this chains into cache loading automatically

When adding new connectors or API calls, follow the existing pattern in `tui/dashboard/data.rs`.

**Documentation convention:** Modules in `src/tui/` have doc comments marked with "Thread Safety: SYNC ONLY" or "Thread Safety: Background Thread Functions" to clarify which functions can block. A pre-commit hook enforces no `.await` in TUI code.

### Anti-patterns

Avoid these common mistakes:

| Don't | Do Instead |
|-------|------------|
| Block TUI event loop with sync API calls | Spawn async task, poll for completion |
| Duplicate worktree lookup/iteration logic | Use functions in `git/worktree.rs` |
| Put connector-specific rendering in `dashboard/render.rs` | Create a widget in `tui/widgets/` |
| Parse Linear/GitHub URLs manually | Use `linear::parser` or `github::url` modules |
| Store state in global/static variables | Pass state through function parameters or store in Dashboard struct |
| Add new cache files without using `connectors/cache.rs` helpers | Use `cache::gwt_cache_dir()` and follow existing patterns |
| Mix CLI output formatting with business logic | Return data, let `commands/` handle formatting |

## Testing Approach

- **CLI commands**: Testable via integration tests in temp repos
- **TUI**: Manual testing only (ratatui doesn't support automated testing)
- **Connectors**: Unit tests for parsing, caching; mock external APIs

Current coverage: Unit tests for fuzzy matching, URL parsing, cache operations.

## Common Tasks

### Adding a new command

1. Add variant to `Commands` enum in `src/cli/mod.rs`
2. Create `src/commands/{name}.rs` with `pub fn {name}() -> Result<()>`
3. Export from `src/commands/mod.rs`
4. Add match arm in `src/main.rs`

### Modifying config

gwt uses a three-level config hierarchy: **global** → **project** → **worktree**. Each level can override the previous.

1. Update structs in `src/config/global.rs`, `src/config/project.rs`, or `src/config/worktree.rs`
2. Add accessor methods to `Config` in `src/config/mod.rs`
3. For TUI/dashboard code, update `ResolvedConfig` and its resolution logic in `Config::resolve()`

**Config levels:**
- `global.rs`: User-wide settings (`~/.config/gwt/config.toml`)
- `project.rs`: Per-repo settings (`.gwt.toml` in repo root)
- `worktree.rs`: Per-worktree overrides (`.gwt.toml` in worktree directory)

**Integration enable flags:** All integrations (Linear, GitHub, Diffview) are disabled by default. Use `enabled = true` in config to enable them. The TUI uses `ResolvedConfig` which merges all three levels.

### Adding a new error type

1. Add variant to `GwtError` enum in `src/error.rs`
2. Implement `suggestion()` match arm with helpful hint
3. Use in commands: `return Err(GwtError::YourError { ... }.into())`

### Adding a new connector

1. Create `src/connectors/{name}/` directory
2. Add `types.rs` for data structures (derive `Serialize`, `Deserialize`)
3. Add `cache.rs` using helpers from `connectors/cache.rs`
4. Add `mod.rs` with re-exports
5. Export from `src/connectors/mod.rs`
6. Add backward-compat re-export in `src/lib.rs`: `pub use connectors::{name};`

### Cache consistency for read-modify-write

When multiple processes may update the same cache file concurrently (e.g., Claude hooks), use `with_lock_modify()` instead of separate read/write calls:

```rust
use crate::connectors::cache::with_lock_modify;

// BAD: Race condition - another process may write between read and write
let data = read_json(...)?;
let modified = modify(data);
write_json(..., &modified)?;

// GOOD: Atomic read-modify-write with file locking
with_lock_modify::<MyType, _>(base, project, worktree, "file.json", |current| {
    let mut data = current.unwrap_or_default();
    data.field += 1;
    data
})?;
```

**Locking behavior:**
- Uses a separate `.lock` file (e.g., `claude.json.lock`)
- Exclusive locks with exponential backoff (10-160ms, 5 attempts)
- Total timeout ~310ms; proceeds unlocked on timeout (hooks must not crash)
- `.lock` files may be left behind (harmless)

For Claude-specific state, use `claude::modify_state()` which wraps `with_lock_modify()`.

### Adding a TUI feature

**New dashboard panel:**
1. Add data types to appropriate connector's `types.rs`
2. Add async data loading function in `tui/dashboard/data.rs`
3. Create widget in `tui/widgets/{name}.rs` for rendering
4. Add panel state to `Dashboard` struct in `tui/dashboard/mod.rs`
5. Wire up rendering in `tui/dashboard/render.rs` (call your widget)
6. Add keybinding in `tui/dashboard/input.rs` if interactive

**New modal dialog:**
1. Create `tui/modals/{name}.rs` with modal struct
2. Implement `Widget` trait for rendering
3. Add modal variant to `DashboardMode` enum in `tui/dashboard/state.rs`
4. Handle modal input in `tui/dashboard/input.rs`
5. Trigger modal from keybinding or action

**New reusable widget:**
1. Create `tui/widgets/{name}.rs`
2. Implement ratatui's `Widget` or `StatefulWidget` trait
3. Export from `tui/widgets/mod.rs`
4. Keep widgets pure: take data in, render out, no side effects

### Adding a periodic async task

For background tasks that run periodically (e.g., git fetch, API polling):

1. **Define result type in `data.rs`**:
   ```rust
   pub type MyTaskResult = Result<Data, String>;
   ```

2. **Add async function in `data.rs`**:
   ```rust
   pub fn run_task_async(params: Params, sender: Sender<MyTaskResult>) {
       thread::spawn(move || {
           let result = do_work(&params);
           let _ = sender.send(result);
       });
   }
   ```

3. **Add fields to Dashboard struct (`mod.rs`)**:
   ```rust
   task_receiver: Receiver<MyTaskResult>,
   task_sender: Sender<MyTaskResult>,
   task_in_progress: bool,
   last_task_time: Option<Instant>,
   ```

4. **Add interval constant**:
   ```rust
   const TASK_INTERVAL: Duration = Duration::from_secs(15);
   ```

5. **Add methods to Dashboard**:
   - `start_task()` - spawns async task if not already in progress
   - `poll_task_results()` - checks for completion via `try_recv()`
   - `should_run_task()` - checks if interval has elapsed

6. **Integrate into event loop**:
   - Poll for results at start of loop (before `terminal.draw()`)
   - When task completes, call `self.start_stats_refresh()` to trigger async worktree reload
   - Check `should_run_task()` and start if due
   - Include `task_in_progress` in poll timeout calculation

**Key principles:**
- Never block the event loop - always use channels and `try_recv()`
- Silently ignore errors for non-critical tasks (e.g., git fetch)
- Call `start_stats_refresh()` internally or `trigger_stats_refresh()` from external code (e.g., `status.rs`) to refresh worktree data without blocking
- Consider workflow modes - some tasks only make sense for certain workflows (e.g., git fetch only needed for pull-based workflows)

### Linear project management

When creating Linear issues for this project, use:
- **Team**: `john.parker.personal`
- **Project**: `Git Worktree Tool`

The project name can be used directly - no need to look up UUIDs.

### Running alignment review

Run `/align` to perform an AI-powered review of your branch's changes vs main:

```bash
# In Claude Code
/align
```

The command checks:
- **Test coverage**: Are tests needed? Do existing tests need updates?
- **CLAUDE.md compliance**: Does code follow documented conventions?
- **Architecture**: Is design modular, scalable, well-integrated?
- **Code cleanup**: Duplication, dead code, unnecessary complexity?

Uses subagents with fresh context for unbiased review. After presenting findings, offers to automatically fix issues.
