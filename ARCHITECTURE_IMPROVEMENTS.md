# Architecture Improvement Plan

This document tracks architectural improvements identified during a codebase analysis.
The quick wins have been implemented; these are the remaining tasks for future work.

## Completed (Quick Wins)

- [x] **Add reqwest timeout** - `src/connectors/linear/api.rs` now has 5-second timeout
- [x] **Create ui.rs module** - `src/commands/ui.rs` centralizes confirmation prompts
- [x] **Remove duplicate time logic** - `src/commands/hook.rs` now uses `claude::time::now_iso8601()`
- [x] **Extract operations module** - `src/commands/operations.rs` shares cleanup logic

---

## Remaining Tasks

### High Priority

#### 1. Standardize Error Handling
**Severity: High** | **Effort: 2-3 hours**

The codebase mixes `anyhow::Result` with `GwtError` inconsistently, using `.into()` conversions that obscure error types.

**Current pattern (problematic):**
```rust
// src/connectors/linear/api.rs:36-40
return Err(GwtError::LinearApiError {
    message: format!("API returned status {}", response.status()),
}.into());  // .into() converts to Box<dyn Error>
```

**Recommended approach:**
- Use `GwtError` for all user-facing errors in commands and connectors
- Reserve `anyhow::Result` for internal utilities only
- Remove bare `.into()` conversions - use explicit error mapping

**Files to update:**
- `src/connectors/linear/api.rs` - remove `.into()` calls
- `src/connectors/linear/resolve.rs` - standardize error returns
- `src/commands/*.rs` - audit all error handling

---

### Medium Priority

#### 2. Improve Testability with Dependency Injection
**Severity: Medium** | **Effort: 4-6 hours**

Hard-coded dependencies make unit testing difficult.

**Current issues:**
- `WorktreeManager::open()` calls `Repository::discover()` directly (git/worktree.rs:58)
- `Config::load()` reads from filesystem directly (config/mod.rs:19-27)
- No traits/interfaces for dependency injection

**Recommended approach:**
```rust
// Create trait for git operations
pub trait GitOps {
    fn discover_repo(&self, path: &Path) -> Result<Repository>;
    fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>>;
}

impl GitOps for WorktreeManager { ... }
impl GitOps for MockGitOps { ... }  // for tests
```

**Files to create/modify:**
- Create `src/git/traits.rs` with `GitOps` trait
- Update `src/git/worktree.rs` to implement trait
- Add `src/config/traits.rs` for config loading abstraction

---

#### 3. Split Dashboard Module
**Severity: Medium** | **Effort: 3-4 hours**

`src/commands/status.rs` is 537 lines with high coupling to all subsystems.

**Current structure issues:**
- Dashboard orchestration mixed with business logic
- Multiple responsibilities in one file

**Recommended split:**
- Keep `status.rs` as thin orchestrator only
- Extract any remaining business logic to `operations.rs`
- Consider if execute_create logic should move to operations.rs

---

#### 4. Add File Locking for Cache Operations
**Severity: Medium** | **Effort: 1-2 hours**

Potential race conditions when multiple processes update cache files.

**Current issues:**
- `src/commands/hook.rs` updates Claude session cache without locking
- Two simultaneous `gwt new` calls could clobber Linear cache
- No coordination between hook and status command polling

**Recommended approach:**
- Add `fs2` crate for file locking
- Implement `try_lock()` for cache writes
- Document cache consistency guarantees

**Files to modify:**
- `src/connectors/claude/cache.rs`
- `src/connectors/linear/cache.rs`

---

### Low Priority

#### 5. Validate Config at Load Time
**Severity: Low** | **Effort: 1 hour**

Config validation happens at access time, not load time.

**Current issue (config/mod.rs:30-38):**
```rust
pub fn worktree_root(&self) -> Result<PathBuf> {
    let root = &self.global.worktree_root;
    if root.starts_with("~") {
        // Path expansion at access time
    }
}
```

**Recommended approach:**
- Expand paths during `Config::load()`
- Store normalized paths in struct
- Add `Config::validate()` method

---

#### 6. Use Parameter Structs for Complex Functions
**Severity: Low** | **Effort: 1-2 hours**

Functions with 4+ parameters are harder to maintain.

**Example (tui/dashboard/data.rs):**
```rust
pub fn fetch_github_pr_async(
    worktree_name: String,
    branch: String,
    repo_root: PathBuf,
    project_name: String,
    sender: Sender<GitHubPRResult>,
) { ... }
```

**Recommended refactor:**
```rust
pub struct FetchPRRequest {
    worktree_name: String,
    branch: String,
    repo_root: PathBuf,
    project_name: String,
}

pub fn fetch_github_pr_async(req: FetchPRRequest, sender: Sender<GitHubPRResult>) { ... }
```

---

#### 7. Create Facades for Connector Modules
**Severity: Low** | **Effort: 2-3 hours**

Connector modules expose low-level functions alongside high-level ones.

**Current issue (connectors/linear/mod.rs):**
```rust
pub use api::get_issue;           // Low-level API call
pub use cache::{delete_metadata, read_metadata, write_metadata};  // Cache ops
pub use resolve::resolve_input;   // High-level input resolution
```

**Recommended approach:**
- Keep cache operations private
- Create high-level facade functions that handle caching internally
- Only export what external callers need

---

#### 8. Document Async/Sync Boundaries
**Severity: Low** | **Effort: 30 minutes**

CLAUDE.md documents async/sync rules but code doesn't enforce them.

**Recommended approach:**
- Add `#[doc = "SYNC ONLY"]` annotations to TUI functions
- Document `reqwest::blocking` usage rationale
- Add lint rule or pre-commit check if possible

---

#### 9. Consider gitoxide as git2 Replacement
**Severity: Low** | **Effort: Research only**

The codebase mixes git2 library with git CLI commands.

**Current approach:**
- git2 for branch lookup
- git CLI for worktree operations (git2 insufficient)

**Future consideration:**
- gitoxide is more actively maintained
- May have better worktree support
- Worth evaluating when time permits

---

## Implementation Order Suggestion

1. **Error handling standardization** - Highest impact, affects debugging
2. **File locking** - Prevents data corruption, relatively quick
3. **Config validation** - Small change, improves UX
4. **Dashboard split** - If status.rs continues to grow
5. **Testability** - When adding comprehensive test suite
6. **Parameter structs** - When touching affected functions
7. **Connector facades** - When refactoring connectors

---

## Notes

- These improvements can be done incrementally
- Each task is independent and can be tackled in any order
- Consider writing tests before refactoring where applicable
