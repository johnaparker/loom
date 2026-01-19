# gwt - Feature Roadmap

## Current Version (v0.1.0)

### Implemented Features

- [x] `gwt new <branch> [--category dev|review|demo]` - Create new worktree
- [x] `gwt list` - List all worktrees
- [x] `gwt switch` - TUI fuzzy picker for worktree switching
- [x] `gwt merge <name>` - Merge branch to main and cleanup
- [x] `gwt remove <name>` - Remove worktree
- [x] `gwt status` - Show status of all worktrees
- [x] `gwt main` - Switch to main branch session
- [x] Global config at `~/.config/gwt/config.toml`
- [x] Project config at `.gwt.toml`
- [x] Sesh integration (auto-register/unregister sessions)
- [x] File sync (.env, .envrc, .claude/) from main to worktree
- [x] Direnv allow on worktree creation

## Planned Features

### v0.2.0 - Polish

- [ ] Shell completions (bash/zsh/fish via clap)
- [ ] `gwt cd <name>` - Output path for shell scripting (`cd $(gwt cd feature)`)
- [ ] `gwt init` - Initialize gwt config for a project
- [ ] Improved error messages with suggestions
- [ ] Dry-run mode for destructive operations

### v0.3.0 - Maintenance

- [ ] `gwt clean` - Remove worktrees for merged/deleted branches
- [ ] `gwt prune` - Prune stale worktree entries
- [ ] Automatic stale worktree detection
- [ ] Config validation command

### v0.4.0 - Enhanced TUI

- [ ] Full TUI dashboard with worktree status overview
- [ ] Inline worktree creation from TUI
- [ ] Preview pane showing recent commits
- [ ] Mouse support

### Future Ideas

- [ ] Linear integration - Fetch issue info for Linear issue branch names
- [ ] GitHub/GitLab PR status in worktree list
- [ ] Worktree templates (pre-configured files/settings)
- [ ] Multi-repo support (monorepo worktrees)
- [ ] Worktree archival (compress inactive worktrees)

## Architecture Notes

### Directory Convention

```
~/worktrees/{project}/{category}/{name}
```

- **project**: Derived from git repo name (or overridden in .gwt.toml)
- **category**: `dev` (default), `review`, or `demo`
- **name**: Branch name with `/` replaced by `-`

### Session Naming

Sesh sessions use format: `{project}/{name}`

Example: `my-app/feature-auth` for branch `feature/auth` in project `my-app`
