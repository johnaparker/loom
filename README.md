# gwt - Git Worktree Manager

A TUI control center for managing git worktrees with Claude Code integration. Monitor multiple Claude agents, track Linear issues and GitHub PRs, all from a single dashboard.

## What is this?

If you work on multiple features simultaneously using git worktrees, `gwt` gives you:

- **Claude agent monitoring** - See which agents are working, waiting for permission, or idle across all your worktrees
- **Worktree lifecycle** - Create, switch, merge, and remove worktrees without leaving the dashboard
- **Project context** - Link worktrees to Linear issues and GitHub PRs for at-a-glance status

The dashboard (`gwt status`) is the primary interface. CLI commands exist for quick one-off operations.

## Setup

### Installation

```bash
cargo install --path .
```

### Claude Code Integration

Add the hook to your Claude Code settings (`~/.claude/settings.json`):

```json
{
  "hooks": {
    "PreToolUse": [{"hooks": [{"type": "command", "command": "gwt hook tool-use"}]}],
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "gwt hook user-prompt"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "gwt hook stop"}]}],
    "Notification": [{"hooks": [{"type": "command", "command": "gwt hook notification"}]}],
    "SessionStart": [{"hooks": [{"type": "command", "command": "gwt hook session-start"}]}],
    "SessionEnd": [{"hooks": [{"type": "command", "command": "gwt hook session-end"}]}]
  }
}
```

This enables gwt to track Claude agent state (working/waiting/idle) per worktree.

### Connectors

gwt integrates with external services via connectors:

| Connector | Purpose | Setup |
|-----------|---------|-------|
| **Claude** | Track agent state across worktrees | Add hook above |
| **Linear** | Link worktrees to issues, show status | Set `LINEAR_API_KEY` env var |
| **GitHub** | Track PR state, checks, reviews | Install [gh CLI](https://cli.github.com/) and authenticate |
| **tmux** | Session management, quick switching | Just have tmux installed |
| **sesh** | Worktree registration for session picker | Optional, auto-detected |

### Configuration

gwt uses a three-level config hierarchy: **global** → **project** → **worktree**. Each level can override the previous.

**Global config:** `~/.config/gwt/config.toml`

```toml
# Root directory for all worktrees (default: ~/.worktrees)
worktree_root = "~/.worktrees"

# Default category for new worktrees: dev, review, demo, etc.
default_category = "dev"

# Cache directory for connector data (default: ~/.cache/gwt)
cache_dir = "~/.cache/gwt"

# Workflow mode: "push" (local-first) or "pull" (PR-based)
# - push: auto-push after merge, push main before new if ahead
# - pull: auto-fetch before new, auto-pull when behind tracking
workflow = "push"

[sync]
# Files/dirs to sync from main when creating worktrees
patterns = [".env", ".envrc", ".claude/"]

[sesh]
# Register worktrees with sesh session picker
auto_register = true

[linear]
# Enable Linear integration (default: false)
# When disabled, Linear panels are hidden and 'l' key is inactive
enabled = true

# Linear API key (get from Linear settings → API)
api_key = "lin_api_..."

# Team prefix for auto-detecting issues from branch names
# e.g., "JOH" matches branches like "john/joh-123-feature"
team_prefix = "JOH"

# Auto-update Linear issue status on gwt new/merge
auto_update_status = true

[github]
# Enable GitHub integration (default: false)
# When disabled, GitHub panels are hidden and 'g' key is inactive
# Requires gh CLI to be installed and authenticated
enabled = true

[diffview]
# Enable nvim DiffView integration (default: false)
# When disabled, 'd' key for diff review is inactive
enabled = true

# Command to open neovim (default: "nvim")
command = "nvim"

[icons]
# Custom icons for TUI dashboard (optional, uses defaults if omitted)
linear = ""   # Icon before Linear issue titles
github = ""   # Icon before GitHub PR info
branch = ""   # Icon before branch names
```

**Project config:** `.gwt.toml` in repo root

Override global settings for a specific repository:

```toml
# Project display name (shown in dashboard title)
project_name = "my-project"

[sync]
# Additional patterns to sync (merged with global patterns)
patterns = [".env.local", "config/secrets.yml"]

[git]
# Override workflow for this project
workflow = "pull"

[linear]
# Disable Linear for this project even if globally enabled
enabled = false

[github]
# Enable GitHub for this project
enabled = true

[diffview]
# Disable diffview for this project
enabled = false
```

**Worktree config:** `.gwt.toml` in worktree directory

Override settings for a specific worktree:

```toml
[sync]
# Additional patterns for this worktree
patterns = ["extra-config.toml"]

# Exclude patterns from syncing to this worktree
exclude_patterns = [".claude/"]

[linear]
# Override Linear for just this worktree
enabled = true

[github]
enabled = false
```

**Note:** All integrations (Linear, GitHub, Diffview) are **disabled by default**. You must explicitly set `enabled = true` to use them.

## Usage

### Dashboard (primary interface)

```bash
gwt status
```

Interactive dashboard showing all worktrees with Claude state, Linear issues, and GitHub PRs. Keyboard shortcuts for common actions.

### Quick commands

```bash
# Create worktree linked to Linear issue
gwt new JOH-123

# Switch worktrees (fuzzy picker)
gwt switch

# Merge to main and cleanup
gwt merge

# Remove worktree
gwt remove

# Jump to main branch
gwt main
```

### Worktree organization

Worktrees are created at `~/worktrees/{project}/{category}/{branch-name}`:

```bash
gwt new feature-auth              # dev category (default)
gwt new bugfix-123 -c review      # review category
gwt new demo-client -c demo       # demo category
```

## How it works

```
┌─────────────────────────────────────────────────────────────┐
│  gwt status (dashboard)                                      │
├─────────────────────────────────────────────────────────────┤
│  Worktree          │ Claude    │ Linear      │ GitHub       │
│  ────────────────────────────────────────────────────────── │
│  dev/joh-255       │ Working   │ In Progress │ PR #42 ✓     │
│  dev/joh-256       │ Waiting   │ Todo        │ -            │
│  review/joh-240    │ Idle      │ Done        │ PR #41 merged│
└─────────────────────────────────────────────────────────────┘
```

The dashboard polls for updates and highlights worktrees needing attention (e.g., Claude waiting for permission).
