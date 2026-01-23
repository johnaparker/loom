//! Interactive dashboard for worktree status.
//!
//! Provides a terminal UI for viewing and managing worktrees.
//!
//! # Threading Model
//!
//! The dashboard uses a **synchronous event loop** with **async background tasks**:
//!
//! - **Event loop** (`run_loop`): Runs synchronously, polling for keyboard events
//!   and checking for completed background tasks. Never blocks on I/O.
//! - **Background tasks**: External I/O (API calls, git operations, cache loading)
//!   runs in spawned threads via `std::thread::spawn`. Results are sent back
//!   through `mpsc` channels.
//! - **Polling**: The event loop uses `try_recv()` to check channels without blocking.
//!
//! ## SYNC ONLY Functions
//!
//! Functions in `mod.rs`, `render.rs`, and `input.rs` are called from the event loop
//! and **must remain synchronous**. They must not:
//! - Use `.await` (blocks the event loop)
//! - Make HTTP requests directly (use background tasks in `data.rs`)
//! - Perform expensive file I/O (use `load_all_caches_async` instead)
//!
//! ## Background Tasks
//!
//! Functions in `data.rs` spawn background threads for I/O. Pattern:
//! ```ignore
//! // In data.rs - spawns background thread
//! pub fn fetch_data_async(sender: Sender<Result>) {
//!     thread::spawn(move || {
//!         let result = blocking_operation();
//!         let _ = sender.send(result);
//!     });
//! }
//!
//! // In mod.rs - non-blocking poll
//! fn poll_data_results(&mut self) {
//!     while let Ok(result) = self.receiver.try_recv() {
//!         // Process result
//!     }
//! }
//! ```
//!
//! This architecture keeps the UI responsive while performing network and disk I/O.

mod data;
mod input;
mod render;
mod state;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::claude::ClaudeSession;
use crate::config::ResolvedConfig;
use crate::core::FuzzyMatcher;
use crate::git::WorktreeStats;
use crate::github::CachedPRState;
use crate::linear::LinearIssue;

pub use state::{DashboardMode, DashboardResult};
use data::{
    CacheLoadResult, GitFetchResult, GitHubPRResult, LinearIssueResult, WorktreeStatsResult,
    fetch_github_pr_async, fetch_linear_issue_async, fetch_origin_async,
    load_all_caches_async, load_claude_states, load_github_prs_from_cache, load_linear_issues,
    load_worktree_stats_async,
};

/// Braille spinner frames for smooth rotation animation
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Interval between periodic git fetch operations
const GIT_FETCH_INTERVAL: Duration = Duration::from_secs(15);

/// Interactive dashboard for worktree status
pub struct Dashboard {
    worktrees: Vec<WorktreeStats>,
    filtered_indices: Vec<usize>,
    selected: usize,
    list_state: ListState,
    project_name: String,
    repo_root: PathBuf,
    /// Resolved configuration with all levels merged
    config: ResolvedConfig,
    mode: DashboardMode,
    search_input: String,
    matcher: FuzzyMatcher,
    main_branch: String,
    /// Pending action result to show after refresh
    pending_result: Option<(bool, String)>,
    /// Status message to show in title bar (auto-clears on keypress)
    /// Tuple of (is_success, message)
    status_message: Option<(bool, String)>,
    /// Cached Linear issues by worktree name
    linear_issues: HashMap<String, LinearIssue>,
    /// Cached GitHub PR states by worktree name
    github_prs: HashMap<String, CachedPRState>,
    /// Cached Claude session states by worktree name
    claude_states: HashMap<String, ClaudeSession>,
    /// Animation frame counter (wraps around 0-255)
    animation_frame: u8,
    /// Whether any worktree has an active animation state
    has_active_claude: bool,
    /// Channel for receiving async GitHub PR results
    github_pr_receiver: Receiver<GitHubPRResult>,
    /// Sender for spawning async GitHub PR fetches
    github_pr_sender: Sender<GitHubPRResult>,
    /// Worktrees currently loading GitHub PR data
    github_loading: std::collections::HashSet<String>,
    /// Channel for receiving async Linear issue results
    linear_issue_receiver: Receiver<LinearIssueResult>,
    /// Sender for spawning async Linear issue fetches
    linear_issue_sender: Sender<LinearIssueResult>,
    /// Worktrees currently loading Linear issue data
    linear_loading: std::collections::HashSet<String>,
    /// Channel for receiving git fetch completion
    git_fetch_receiver: Receiver<GitFetchResult>,
    /// Sender for spawning git fetch
    git_fetch_sender: Sender<GitFetchResult>,
    /// Whether a git fetch is currently in progress
    git_fetch_in_progress: bool,
    /// Last fetch time (for periodic fetching)
    last_fetch_time: Option<Instant>,
    /// Channel for receiving async worktree stats results
    worktree_stats_receiver: Receiver<WorktreeStatsResult>,
    /// Sender for spawning async worktree stats loads
    worktree_stats_sender: Sender<WorktreeStatsResult>,
    /// Whether worktree stats are currently loading
    worktree_stats_loading: bool,
    /// Channel for receiving async cache load results
    cache_receiver: Receiver<CacheLoadResult>,
    /// Sender for spawning async cache loads
    cache_sender: Sender<CacheLoadResult>,
    /// Whether caches are currently loading
    cache_loading: bool,
}

impl Dashboard {
    pub fn new(
        worktrees: Vec<WorktreeStats>,
        project_name: String,
        repo_root: PathBuf,
        config: ResolvedConfig,
    ) -> Self {
        let filtered_indices: Vec<usize> = (0..worktrees.len()).collect();
        let mut list_state = ListState::default();
        if !worktrees.is_empty() {
            list_state.select(Some(0));
        }

        let cache_dir = &config.cache_dir;

        // Load Linear issues for all worktrees (from cache) - only if enabled
        let linear_issues = if config.linear.enabled {
            load_linear_issues(cache_dir, &project_name, &worktrees)
        } else {
            HashMap::new()
        };

        // Load GitHub PRs from cache only (API fetch happens lazily when GitHub panel is viewed) - only if enabled
        let github_prs = if config.github.enabled {
            load_github_prs_from_cache(cache_dir, &project_name, &worktrees)
        } else {
            HashMap::new()
        };

        // Create channel for async GitHub PR fetches
        let (github_pr_sender, github_pr_receiver) = mpsc::channel();

        // Create channel for async Linear issue fetches
        let (linear_issue_sender, linear_issue_receiver) = mpsc::channel();

        // Create channel for async git fetch
        let (git_fetch_sender, git_fetch_receiver) = mpsc::channel();

        // Create channel for async worktree stats
        let (worktree_stats_sender, worktree_stats_receiver) = mpsc::channel();

        // Create channel for async cache loading
        let (cache_sender, cache_receiver) = mpsc::channel();

        // Load initial Claude states
        let (claude_states, has_active_claude) = load_claude_states(cache_dir, &project_name, &worktrees);

        let mut dashboard = Self {
            worktrees,
            filtered_indices,
            selected: 0,
            list_state,
            project_name,
            repo_root,
            config,
            mode: DashboardMode::Normal,
            search_input: String::new(),
            matcher: FuzzyMatcher::new(),
            main_branch: "main".to_string(),
            pending_result: None,
            status_message: None,
            linear_issues,
            github_prs,
            claude_states,
            animation_frame: 0,
            has_active_claude,
            github_pr_receiver,
            github_pr_sender,
            github_loading: std::collections::HashSet::new(),
            linear_issue_receiver,
            linear_issue_sender,
            linear_loading: std::collections::HashSet::new(),
            git_fetch_receiver,
            git_fetch_sender,
            git_fetch_in_progress: false,
            last_fetch_time: None,
            worktree_stats_receiver,
            worktree_stats_sender,
            worktree_stats_loading: false,
            cache_receiver,
            cache_sender,
            cache_loading: false,
        };

        // GitHub panel is always visible, so fetch PR data for initial selection - only if enabled
        if dashboard.config.github.enabled {
            dashboard.fetch_github_pr_for_selected();
        }

        // Linear panel is always visible, so fetch issue data for initial selection - only if enabled
        if dashboard.config.linear.enabled {
            dashboard.fetch_linear_issue_for_selected();
        }

        // Start initial git fetch to get fresh remote refs (only for pull-based workflows)
        if dashboard.is_pull_workflow() {
            dashboard.start_git_fetch();
        }

        dashboard
    }

    /// Spawn async fetch of GitHub PR for the selected worktree (non-blocking)
    fn fetch_github_pr_for_selected(&mut self) {
        // Skip if GitHub integration is disabled
        if !self.config.github.enabled {
            return;
        }

        let Some(worktree) = self.get_selected_worktree() else {
            return;
        };

        // Skip main worktree
        if worktree.info.is_main {
            return;
        }

        let worktree_name = worktree.info.name.clone();
        let branch = match &worktree.info.branch {
            Some(b) => b.clone(),
            None => return,
        };

        // Skip if already loading this worktree
        if self.github_loading.contains(&worktree_name) {
            return;
        }

        // Mark as loading
        self.github_loading.insert(worktree_name.clone());

        // Clone data needed for the thread
        fetch_github_pr_async(
            self.config.cache_dir.clone(),
            worktree_name,
            branch,
            self.repo_root.clone(),
            self.project_name.clone(),
            self.github_pr_sender.clone(),
        );
    }

    /// Check for completed async GitHub PR fetches and update state.
    ///
    /// # Thread Safety: SYNC ONLY
    ///
    /// Uses non-blocking `try_recv()` to poll the channel. Safe to call from
    /// the event loop - never blocks.
    pub fn poll_github_results(&mut self) {
        // Non-blocking receive of all pending results
        while let Ok((worktree_name, state)) = self.github_pr_receiver.try_recv() {
            self.github_loading.remove(&worktree_name);
            // Always insert the state (Found or NotFound)
            self.github_prs.insert(worktree_name, state);
        }
    }

    /// Check if a worktree's GitHub PR is currently loading
    fn is_github_loading(&self, worktree_name: &str) -> bool {
        self.github_loading.contains(worktree_name)
    }

    /// Spawn async fetch of Linear issue for the selected worktree (non-blocking)
    fn fetch_linear_issue_for_selected(&mut self) {
        // Skip if Linear integration is disabled or no API key configured
        if !self.config.linear.enabled {
            return;
        }
        let (Some(api_key), Some(prefix)) = (&self.config.linear.api_key, &self.config.linear.team_prefix) else {
            return;
        };

        let Some(worktree) = self.get_selected_worktree() else {
            return;
        };

        // Skip main worktree
        if worktree.info.is_main {
            return;
        }

        let worktree_name = worktree.info.name.clone();
        let branch = match &worktree.info.branch {
            Some(b) => b.clone(),
            None => return,
        };

        // Extract issue ID from branch name
        let Some(issue_id) = crate::linear::extract_issue_id(&branch, prefix) else {
            return;
        };

        // Skip if already loading this worktree
        if self.linear_loading.contains(&worktree_name) {
            return;
        }

        // Mark as loading
        self.linear_loading.insert(worktree_name.clone());

        // Clone data needed for the thread
        fetch_linear_issue_async(
            self.config.cache_dir.clone(),
            worktree_name,
            issue_id,
            api_key.clone(),
            self.project_name.clone(),
            self.linear_issue_sender.clone(),
        );
    }

    /// Check for completed async Linear issue fetches and update state.
    ///
    /// # Thread Safety: SYNC ONLY
    ///
    /// Uses non-blocking `try_recv()` to poll the channel. Safe to call from
    /// the event loop - never blocks.
    fn poll_linear_results(&mut self) {
        // Non-blocking receive of all pending results
        while let Ok((worktree_name, issue)) = self.linear_issue_receiver.try_recv() {
            self.linear_loading.remove(&worktree_name);
            if let Some(issue) = issue {
                self.linear_issues.insert(worktree_name, issue);
            }
        }
    }

    /// Check if a worktree's Linear issue is currently loading
    pub fn is_linear_loading(&self, worktree_name: &str) -> bool {
        self.linear_loading.contains(worktree_name)
    }

    /// Start an async git fetch (if not already in progress)
    fn start_git_fetch(&mut self) {
        if self.git_fetch_in_progress {
            return;
        }
        self.git_fetch_in_progress = true;
        fetch_origin_async(self.repo_root.clone(), self.git_fetch_sender.clone());
    }

    /// Check for completed git fetch and return true if fetch completed (triggers refresh).
    ///
    /// # Thread Safety: SYNC ONLY
    ///
    /// Uses non-blocking `try_recv()` to poll the channel. Safe to call from
    /// the event loop - never blocks.
    fn poll_git_fetch_results(&mut self) -> bool {
        if let Ok(_result) = self.git_fetch_receiver.try_recv() {
            self.git_fetch_in_progress = false;
            self.last_fetch_time = Some(Instant::now());
            // Silently ignore errors - fetch failures shouldn't disrupt the UI
            return true;
        }
        false
    }

    /// Check if a periodic git fetch is due
    fn should_fetch(&self) -> bool {
        self.last_fetch_time
            .map(|t| t.elapsed() >= GIT_FETCH_INTERVAL)
            .unwrap_or(true)
    }

    /// Refresh Claude session states for all worktrees
    fn refresh_claude_states(&mut self) {
        let (states, has_active) = load_claude_states(&self.config.cache_dir, &self.project_name, &self.worktrees);
        self.claude_states = states;
        self.has_active_claude = has_active;
    }

    /// Start async worktree stats refresh (if not already loading)
    fn start_stats_refresh(&mut self) {
        if self.worktree_stats_loading {
            return;
        }
        self.worktree_stats_loading = true;
        load_worktree_stats_async(self.repo_root.clone(), self.worktree_stats_sender.clone());
    }

    /// Check for completed worktree stats load and return true if completed.
    ///
    /// # Thread Safety: SYNC ONLY
    ///
    /// Uses non-blocking `try_recv()` to poll the channel. Safe to call from
    /// the event loop - never blocks.
    fn poll_stats_results(&mut self) -> bool {
        if let Ok(result) = self.worktree_stats_receiver.try_recv() {
            self.worktree_stats_loading = false;
            if let Ok(worktrees) = result {
                // Update worktrees list and start cache refresh
                self.update_worktrees_internal(worktrees);
                self.start_cache_refresh();
            }
            return true;
        }
        false
    }

    /// Trigger async stats refresh (public method for external use)
    pub fn trigger_stats_refresh(&mut self) {
        self.start_stats_refresh();
    }

    /// Start async cache refresh for all caches
    fn start_cache_refresh(&mut self) {
        if self.cache_loading {
            return;
        }
        self.cache_loading = true;
        load_all_caches_async(
            self.config.cache_dir.clone(),
            self.project_name.clone(),
            self.worktrees.clone(),
            self.cache_sender.clone(),
        );
    }

    /// Check for completed cache load and apply results.
    ///
    /// # Thread Safety: SYNC ONLY
    ///
    /// Uses non-blocking `try_recv()` to poll the channel. Safe to call from
    /// the event loop - never blocks.
    fn poll_cache_results(&mut self) -> bool {
        if let Ok((linear_issues, github_prs, claude_states, has_active_claude)) =
            self.cache_receiver.try_recv()
        {
            self.cache_loading = false;
            self.linear_issues = linear_issues;
            self.github_prs = github_prs;
            self.claude_states = claude_states;
            self.has_active_claude = has_active_claude;

            // Show pending error modal if any
            if let Some((success, message)) = self.pending_result.take() {
                if !success {
                    self.mode = DashboardMode::ActionResult(
                        crate::tui::modals::ActionResultModal::error(message),
                    );
                }
            }
            return true;
        }
        false
    }

    /// Update worktrees list without loading caches (internal use)
    fn update_worktrees_internal(&mut self, worktrees: Vec<WorktreeStats>) {
        // Remember currently selected worktree name
        let selected_name = self.get_selected_worktree().map(|w| w.info.name.clone());

        self.worktrees = worktrees;
        self.filter_worktrees();

        // Try to restore selection to the previously selected worktree
        if let Some(name) = selected_name {
            if let Some(pos) = self
                .filtered_indices
                .iter()
                .position(|&i| self.worktrees[i].info.name == name)
            {
                self.selected = pos;
                self.list_state.select(Some(pos));
            }
        }
    }

    /// Set the main branch name (for merge modal)
    pub fn set_main_branch(&mut self, name: String) {
        self.main_branch = name;
    }

    /// Show a result message (called after returning from action)
    /// Success messages show as a banner in the title bar (auto-clears on keypress)
    /// Error messages show as a modal requiring dismissal
    pub fn show_result(&mut self, success: bool, message: String) {
        if success {
            // Success: show as banner in title bar
            self.status_message = Some((true, message));
        } else {
            // Error: show as modal
            self.pending_result = Some((false, message));
        }
    }

    /// Update worktrees (for refresh)
    pub fn update_worktrees(&mut self, worktrees: Vec<WorktreeStats>) {
        // Remember currently selected worktree name
        let selected_name = self.get_selected_worktree().map(|w| w.info.name.clone());

        // Refresh Linear issues (only if enabled)
        if self.config.linear.enabled {
            self.linear_issues = load_linear_issues(&self.config.cache_dir, &self.project_name, &worktrees);
        }

        // Refresh GitHub PRs from cache (API fetch happens lazily) - only if enabled
        if self.config.github.enabled {
            self.github_prs = load_github_prs_from_cache(&self.config.cache_dir, &self.project_name, &worktrees);
        }

        self.worktrees = worktrees;
        self.filter_worktrees();

        // Try to restore selection to the previously selected worktree
        if let Some(name) = selected_name {
            if let Some(pos) = self
                .filtered_indices
                .iter()
                .position(|&i| self.worktrees[i].info.name == name)
            {
                self.selected = pos;
                self.list_state.select(Some(pos));
            }
        }

        // Show pending error modal if any
        if let Some((success, message)) = self.pending_result.take() {
            if !success {
                self.mode = DashboardMode::ActionResult(crate::tui::modals::ActionResultModal::error(message));
            }
        }

        // Refresh Claude states for all worktrees
        self.refresh_claude_states();
    }

    /// Set conflict info for merge modal
    pub fn set_merge_conflicts(&mut self, conflicts: Option<Vec<String>>) {
        // Get worktree data first to avoid borrow issues
        let worktree = self.get_selected_worktree();
        let main_branch = self.main_branch.clone();

        if let DashboardMode::ConfirmMerge(ref mut modal) = self.mode {
            if let Some(wt) = worktree {
                let conflict_info = conflicts.map(|files| crate::tui::modals::MergeConflictInfo {
                    conflicted_files: files,
                });
                *modal = crate::tui::modals::MergeConfirmModal::new(wt, main_branch, conflict_info);
            }
        }
    }

    /// Get color for a category
    fn category_color(category: &str) -> Color {
        match category.to_lowercase().as_str() {
            "main" => Color::LightYellow,
            "dev" | "development" | "feature" | "features" => Color::Green,
            "demo" | "demos" => Color::Magenta,
            "review" | "reviews" | "pr" => Color::Cyan,
            "bugfix" | "fix" | "hotfix" => Color::Red,
            "test" | "testing" => Color::Yellow,
            "experiment" | "experiments" | "spike" => Color::Blue,
            _ => Color::DarkGray,
        }
    }

    /// Run the dashboard with its own terminal setup/teardown (standalone mode)
    pub fn run(&mut self) -> Result<DashboardResult> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;

        result
    }

    /// Run the dashboard with an externally managed terminal (for persistent alternate screen)
    pub fn run_with_terminal(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<DashboardResult> {
        self.run_loop(terminal)
    }

    /// Main event loop for the dashboard.
    ///
    /// # Thread Safety: SYNC ONLY
    ///
    /// This is the core event loop. It must remain synchronous - all operations
    /// here must complete quickly without blocking:
    ///
    /// - Polls channels for completed background tasks (non-blocking `try_recv`)
    /// - Renders the UI (pure computation, no I/O)
    /// - Waits for keyboard events with timeout (`event::poll`)
    /// - Handles input (triggers actions, may spawn new background tasks)
    ///
    /// **Never add `.await`, HTTP calls, or blocking file I/O here.**
    /// Use the async functions in `data.rs` to spawn background tasks instead.
    fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<DashboardResult> {
        // Intervals for polling
        const ANIMATION_INTERVAL: Duration = Duration::from_millis(150);
        const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

        let mut last_refresh = Instant::now();

        loop {
            // Check for completed async fetches
            self.poll_github_results();
            self.poll_linear_results();

            // Check for completed async worktree stats and cache loads
            self.poll_stats_results();
            self.poll_cache_results();

            // Check for completed git fetch and trigger async stats refresh
            if self.poll_git_fetch_results() {
                self.start_stats_refresh();
            }

            // Start periodic git fetch if due (only for pull-based workflows)
            if self.is_pull_workflow() && self.should_fetch() && !self.git_fetch_in_progress {
                self.start_git_fetch();
            }

            terminal.draw(|f| self.render(f))?;

            // Use short interval when animating or loading async data
            let has_pending_github = !self.github_loading.is_empty();
            let has_pending_linear = !self.linear_loading.is_empty();
            let has_pending_git_fetch = self.git_fetch_in_progress;
            let has_pending_stats = self.worktree_stats_loading;
            let has_pending_cache = self.cache_loading;
            let poll_timeout = if self.has_active_claude
                || has_pending_github
                || has_pending_linear
                || has_pending_git_fetch
                || has_pending_stats
                || has_pending_cache
            {
                ANIMATION_INTERVAL
            } else {
                // Calculate remaining time until next refresh
                let elapsed = last_refresh.elapsed();
                if elapsed >= REFRESH_INTERVAL {
                    Duration::ZERO
                } else {
                    REFRESH_INTERVAL - elapsed
                }
            };

            // Poll for events with timeout
            if event::poll(poll_timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if let Some(result) = self.handle_key(key.code, key.modifiers) {
                        return Ok(result);
                    }
                }
            } else if self.has_active_claude {
                // Animation tick - increment frame counter
                self.animation_frame = self.animation_frame.wrapping_add(1);

                // Check if it's also time for a full refresh (async)
                if last_refresh.elapsed() >= REFRESH_INTERVAL {
                    self.start_stats_refresh();
                    last_refresh = Instant::now();
                }
            } else {
                // Full refresh interval elapsed - trigger async refresh
                self.start_stats_refresh();
                last_refresh = Instant::now();
            }
        }
    }

    pub(crate) fn get_selected_worktree(&self) -> Option<WorktreeStats> {
        if self.filtered_indices.is_empty() {
            None
        } else {
            let actual_idx = self.filtered_indices[self.selected];
            Some(self.worktrees[actual_idx].clone())
        }
    }

    /// Get the Linear icon for display
    pub(crate) fn linear_icon(&self) -> Option<&str> {
        self.config.icons.linear.as_deref()
    }

    /// Get the GitHub icon for display
    pub(crate) fn github_icon(&self) -> Option<&str> {
        self.config.icons.github.as_deref()
    }

    /// Get the branch icon for display
    pub(crate) fn branch_icon(&self) -> Option<&str> {
        self.config.icons.branch.as_deref()
    }

    /// Check if pull workflow is enabled
    pub(crate) fn is_pull_workflow(&self) -> bool {
        self.config.workflow == crate::config::SyncWorkflow::Pull
    }

    /// Get the resolved config
    pub(crate) fn config(&self) -> &ResolvedConfig {
        &self.config
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let len = self.filtered_indices.len() as i32;
        let current = self.selected as i32;
        let new = (current + delta).rem_euclid(len) as usize;
        self.selected = new;
        self.list_state.select(Some(new));

        // Both panels are always visible, so fetch data on selection change
        self.fetch_github_pr_for_selected();
        self.fetch_linear_issue_for_selected();
    }

    fn filter_worktrees(&mut self) {
        self.filtered_indices = self.matcher.filter(&self.worktrees, &self.search_input, |wt| {
            format!(
                "{} {} {}",
                wt.info.name,
                wt.info.branch.as_deref().unwrap_or(""),
                wt.info.category.as_deref().unwrap_or("")
            )
        });

        // Reset selection
        if self.filtered_indices.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            self.selected = 0;
            self.list_state.select(Some(0));
        }
    }
}
