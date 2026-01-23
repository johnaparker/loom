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

### Configuration

**Global config:** `~/.config/gwt/config.toml`

```toml
worktree_root = "~/worktrees"
default_category = "dev"

[sync]
patterns = [".env", ".envrc", ".claude/"]
```

**Project config:** `.gwt.toml` in repo root

```toml
project_name = "my-project"

[sync]
patterns = [".env", ".envrc", ".claude/", ".env.local"]
```

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
