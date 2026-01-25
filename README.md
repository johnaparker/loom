[![CI](https://github.com/johnaparker/grove/actions/workflows/ci.yml/badge.svg)](https://github.com/johnaparker/grove/actions/workflows/ci.yml)
[![GitHub Release](https://img.shields.io/github/v/release/johnaparker/grove?v=1)](https://github.com/johnaparker/grove/releases/latest)

# Grove

Grove is a TUI control center for managing git worktrees in tmux with Claude Code integration.
Monitor multiple Claude agents, optionally track Linear issues and GitHub PRs, all from a single dashboard.

![grove demo](assets/grove.gif)

## What is this?

If you work on multiple features simultaneously using git worktrees, `grove` gives you:

- **Claude agent monitoring** - See which agents are working, waiting for permission, or idle across all your worktrees
- **Worktree lifecycle** - Create, switch, merge, and remove worktrees without leaving the dashboard
- **Project context** - Link worktrees to Linear issues and GitHub PRs for at-a-glance status

The dashboard is the primary interface. CLI commands exist for quick one-off operations.

## Setup

Grove minimally requires:
- git
- tmux

### Installation

```bash
curl -fsSL https://github.com/johnaparker/grove/releases/latest/download/install.sh | bash
```

This installs grove to `~/.local/bin`. Add it to your PATH if not already:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Building from source

```bash
cargo install --path .
```

### Claude Code Integration

Enable Claude agent tracking with:

```bash
grove config enable claude
```

This configures grove hooks in Claude's settings and enables state tracking (working/waiting/idle) per worktree. You'll be prompted about sandbox mode for new worktrees.

### GitHub Integration

Requires the [GitHub CLI](https://cli.github.com/). Enable with:

```bash
grove config enable github
```

This verifies `gh` is installed and authenticated, then enables PR status, check results, and review comments in the dashboard.

### Linear Integration

Enable with:

```bash
grove config enable linear
```

You'll be prompted for your API key (get from Linear → Settings → API) and shown available teams to select your issue prefix. Grove then links worktrees to Linear issues automatically when branch names contain issue IDs (e.g., `user/lin-123-feature`).

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
# Enable Claude Code integration (default: true)
# When disabled, Claude state tracking is inactive
enabled = true

# Create new worktrees with Claude sandbox mode enabled
sandbox = true

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

## Workflow Modes

Grove supports two workflow modes, configured via `workflow` in your config:

### Push workflow (default)

For local-first development where you merge directly to main. New worktrees branch from your local main. When work is complete, merge the branch into main locally and clean up the worktree. This is ideal for solo projects or when you have direct push access to main.

### Pull workflow

For PR-based development where merging happens on GitHub. New worktrees branch from origin/main (auto-fetched). When work is complete, push to origin and open a PR. After the PR is merged on GitHub, use prune to clean up worktrees with merged PRs. Grove auto-pulls when switching to a branch that's behind origin.

## Dashboard

Run `grove status` to open the interactive dashboard. This is the primary interface for managing worktrees.

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`k` | Move selection up |
| `↓`/`j` | Move selection down |
| `Enter` | Switch to selected worktree |
| `/` | Search/filter worktrees |
| `Tab` | Cycle category filter (when >2 categories) |
| `n` | Create new worktree |
| `x` | Delete selected worktree |
| `m` | Merge to main (push workflow only) |
| `s` | Sync with remote |
| `d` | Open diff review in nvim |
| `c` | Open Claude Code session |
| `l` | Open Linear issue |
| `g` | Open GitHub PR |
| `P` | Prune merged PRs (pull workflow only) |
| `q`/`Esc` | Quit |

## CLI Commands

All commands support `--help` for detailed usage.

| Command | Alias | Description |
|---------|-------|-------------|
| `grove status` | `st` | Open interactive dashboard |
| `grove new <branch>` | `n` | Create new worktree |
| `grove switch [name]` | `s` | Switch worktree (fuzzy picker if no name) |
| `grove list` | `l`, `ls` | List all worktrees |
| `grove merge <name>` | `m` | Merge worktree to main (push workflow) |
| `grove remove [name]` | `r`, `rm` | Remove worktree |
| `grove sync [name]` | `sy` | Sync worktree with origin/main |
| `grove prune` | `p` | Delete worktrees with merged PRs (pull workflow) |
| `grove review` | `rev` | Open diff review in nvim |
| `grove main` | `ma` | Switch to main worktree |
| `grove linear` | `li` | Open Linear issue for current worktree |
| `grove github` | `gh` | Open GitHub PR for current worktree |
| `grove config enable <feature>` | `cfg` | Enable an integration (claude, github, linear) |
| `grove config disable <feature>` | | Disable an integration |
| `grove config set-workflow <mode>` | | Set workflow mode (push or pull) |
| `grove config categories` | | Manage worktree categories interactively |
| `grove config sync-patterns` | | Manage sync patterns interactively |
| `grove completions <shell>` | | Generate shell completions |

### Common Flags

- `--dry-run`: Preview changes without executing (merge, remove, sync, prune)
- `-f, --force`: Force operation even with uncommitted changes
- `-c, --category`: Specify worktree category (new)
