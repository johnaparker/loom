# gwt - Git Worktree Manager

A Rust CLI/TUI for managing git worktrees with tmux/sesh integration, designed for trunk-based development workflows.

## Features

- Create and manage git worktrees with organized directory structure
- Fuzzy picker TUI for fast worktree switching
- Automatic tmux/sesh session management
- File syncing (.env, .envrc, .claude/) from main repo to worktrees
- Category-based organization (dev, review, demo)

## Installation

```bash
cargo install --path .
```

## Usage

### Create a new worktree

```bash
# Create a new worktree for feature branch
gwt new feature-branch

# Create with specific category
gwt new feature-branch --category review
gwt new feature-branch -c demo
```

Worktrees are created at `~/worktrees/{project}/{category}/{branch-name}`.

### List worktrees

```bash
gwt list
```

### Switch worktrees (TUI)

```bash
gwt switch
```

Opens an interactive fuzzy picker to select and switch to a worktree via tmux.

### Show worktree status

```bash
gwt status
```

Shows the git status of all worktrees including modified/added/deleted file counts.

### Merge and cleanup

```bash
gwt merge feature-branch
```

Merges the branch into main, removes the worktree, deletes the branch, and unregisters from sesh.

### Remove a worktree

```bash
gwt remove feature-branch

# Force removal with uncommitted changes
gwt remove feature-branch --force
```

### Switch to main

```bash
gwt main
```

Switches to the main branch tmux session.

## Configuration

### Global Config: `~/.config/gwt/config.toml`

```toml
worktree_root = "~/worktrees"
default_category = "dev"

[sync]
patterns = [".env", ".envrc", ".claude/"]

[sesh]
auto_register = true
```

### Project Config: `.gwt.toml`

```toml
project_name = "my-project"  # Override detected name

[sync]
patterns = [".env", ".envrc", ".claude/", ".env.local"]
```

## Integrations

### Sesh

When creating a worktree, gwt automatically registers it with sesh at `~/.config/sesh/sesh.toml`. Session names follow the format `{project}/{worktree-name}`.

### Direnv

If `.envrc` is synced to a new worktree, gwt automatically runs `direnv allow`.

### Tmux

`gwt switch` and `gwt main` use tmux to create and switch sessions. If a session doesn't exist, it's created automatically.

## License

MIT
