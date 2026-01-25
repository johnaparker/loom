[![CI](https://github.com/johnaparker/grove/actions/workflows/ci.yml/badge.svg)](https://github.com/johnaparker/grove/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/johnaparker/grove)

# grove - Git Worktree Manager

A TUI control center for managing git worktrees in tmux with Claude Code integration.
Monitor multiple Claude agents, optionally track Linear issues and GitHub PRs, all from a single dashboard.

## What is this?

If you work on multiple features simultaneously using git worktrees, `grove` gives you:

- **Claude agent monitoring** - See which agents are working, waiting for permission, or idle across all your worktrees
- **Worktree lifecycle** - Create, switch, merge, and remove worktrees without leaving the dashboard
- **Project context** - Link worktrees to Linear issues and GitHub PRs for at-a-glance status

The dashboard is the primary interface. CLI commands exist for quick one-off operations.

## Setup

Grove requires:
- git
- tmux

With these, you can create and manage worktrees, switching between dedicated tmux sessions for each.

Optional integrations:
- **Claude Code**: Track live agent status across all worktrees via hooks
- **Linear**: Display issue details and auto-assign context to new Claude sessions
- **GitHub**: Show PR status, checks, and review comments

### Installation

```bash
cargo install --path .
```

### Claude Code Integration

Add grove hooks to your Claude Code settings (`~/.claude/settings.json`):

```json
{
  "hooks": {
    "PreToolUse": [{"hooks": [{"type": "command", "command": "grove hook tool-use"}]}],
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "grove hook user-prompt"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "grove hook stop"}]}],
    "Notification": [{"hooks": [{"type": "command", "command": "grove hook notification"}]}],
    "SessionStart": [{"hooks": [{"type": "command", "command": "grove hook session-start"}]}],
    "SessionEnd": [{"hooks": [{"type": "command", "command": "grove hook session-end"}]}]
  }
}
```

This enables grove to track Claude agent state (working/waiting/idle) per worktree.

### GitHub Integration

Install the [GitHub CLI](https://cli.github.com/) and authenticate:

```bash
gh auth login
```

Then enable GitHub in your grove config:

```toml
[github]
enabled = true
```

Grove uses `gh` to display PR status, check results, and review comments in the dashboard.

### Linear Integration

Get an API key from Linear (Settings → API → Personal API keys), then add it to your grove config:

```toml
[linear]
enabled = true
api_key = "lin_api_..."
team_prefix = "LIN"  # Your team's issue prefix
```

Grove links worktrees to Linear issues automatically when branch names contain issue IDs (e.g., `user/lin-123-feature`).

### Configuration

grove uses a three-level config hierarchy: **global** → **project** → **worktree**. Each level can override the previous.

**Global config:** `~/.config/grove/config.toml`

```toml
# Root directory for all worktrees (default: ~/.worktrees)
worktree_root = "~/.worktrees"

# Worktree categories (max 4, default: ["dev"])
# When >2 categories, Tab cycles through filter in dashboard
categories = ["dev", "review", "demo"]

# Default category for new worktrees (used when -c not specified)
default_category = "dev"

# Cache directory for connector data (default: ~/.cache/grove)
cache_dir = "~/.cache/grove"

# Workflow mode: "push" (local-first) or "pull" (PR-based)
# - push: merge directly to main, new worktrees branch off main
# - pull: auto-fetch from origin, new worktrees branch off origin/main
workflow = "push"

[sync]
# Files/dirs to sync from main when creating worktrees
patterns = [".env", ".envrc", ".claude/"]

[claude]
# Enable Claude Code integration (default: false)
# When disabled, Claude state tracking is inactive
enabled = true

# Create new worktrees with Claude sandbox mode enabled
sandbox = false

# Auto-approve sandboxed bash commands (only applies when sandbox = true)
sandbox_auto_allow_bash = true

[linear]
# Enable Linear integration (default: false)
# When disabled, Linear panels are hidden and 'l' key is inactive
enabled = true

# Linear API key (get from Linear settings → API)
api_key = "lin_api_..."

# Team prefix for auto-detecting issues from branch names
# e.g., "JOH" matches branches like "john/joh-123-feature"
team_prefix = "JOH"

# Auto-update Linear issue status on grove new/merge
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
linear = ""   # Icon before Linear issue titles
github = ""   # Icon before GitHub PR info
branch = ""   # Icon before branch names
```

**Note:** All integrations (Linear, GitHub, Diffview) are **disabled by default**. You must explicitly set `enabled = true` to use them.
