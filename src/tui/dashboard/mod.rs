//! Interactive dashboard for worktree status.
//!
//! Provides a terminal UI for viewing and managing worktrees.

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
use crate::core::FuzzyMatcher;
use crate::git::WorktreeStats;
use crate::github::GitHubPR;
use crate::linear::LinearIssue;

pub use state::{DashboardMode, DashboardResult};
use data::{GitHubPRResult, LinearIssueResult, fetch_github_pr_async, fetch_linear_issue_async, load_claude_states, load_github_prs_from_cache, load_linear_issues};

/// Braille spinner frames for smooth rotation animation
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Interactive dashboard for worktree status
pub struct Dashboard {
    worktrees: Vec<WorktreeStats>,
    filtered_indices: Vec<usize>,
    selected: usize,
    list_state: ListState,
    project_name: String,
    repo_root: PathBuf,
    cache_dir: PathBuf,
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
    /// Cached GitHub PRs by worktree name
    github_prs: HashMap<String, GitHubPR>,
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
    /// Linear API key from config
    linear_api_key: Option<String>,
    /// Linear team prefix from config
    linear_prefix: Option<String>,
}

impl Dashboard {
    pub fn new(
        worktrees: Vec<WorktreeStats>,
        project_name: String,
        repo_root: PathBuf,
        cache_dir: PathBuf,
        linear_api_key: Option<String>,
        linear_prefix: Option<String>,
    ) -> Self {
        let filtered_indices: Vec<usize> = (0..worktrees.len()).collect();
        let mut list_state = ListState::default();
        if !worktrees.is_empty() {
            list_state.select(Some(0));
        }

        // Load Linear issues for all worktrees (from cache)
        let linear_issues = load_linear_issues(&cache_dir, &project_name, &worktrees);

        // Load GitHub PRs from cache only (API fetch happens lazily when GitHub panel is viewed)
        let github_prs = load_github_prs_from_cache(&cache_dir, &project_name, &worktrees);

        // Create channel for async GitHub PR fetches
        let (github_pr_sender, github_pr_receiver) = mpsc::channel();

        // Create channel for async Linear issue fetches
        let (linear_issue_sender, linear_issue_receiver) = mpsc::channel();

        // Load initial Claude states
        let (claude_states, has_active_claude) = load_claude_states(&cache_dir, &project_name, &worktrees);

        let mut dashboard = Self {
            worktrees,
            filtered_indices,
            selected: 0,
            list_state,
            project_name,
            repo_root,
            cache_dir,
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
            linear_api_key,
            linear_prefix,
        };

        // GitHub panel is always visible, so fetch PR data for initial selection
        dashboard.fetch_github_pr_for_selected();

        // Linear panel is always visible, so fetch issue data for initial selection
        dashboard.fetch_linear_issue_for_selected();

        dashboard
    }

    /// Spawn async fetch of GitHub PR for the selected worktree (non-blocking)
    fn fetch_github_pr_for_selected(&mut self) {
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
            self.cache_dir.clone(),
            worktree_name,
            branch,
            self.repo_root.clone(),
            self.project_name.clone(),
            self.github_pr_sender.clone(),
        );
    }

    /// Check for completed async GitHub PR fetches and update state
    pub fn poll_github_results(&mut self) {
        // Non-blocking receive of all pending results
        while let Ok((worktree_name, pr)) = self.github_pr_receiver.try_recv() {
            self.github_loading.remove(&worktree_name);
            if let Some(pr) = pr {
                self.github_prs.insert(worktree_name, pr);
            } else {
                self.github_prs.remove(&worktree_name);
            }
        }
    }

    /// Check if a worktree's GitHub PR is currently loading
    fn is_github_loading(&self, worktree_name: &str) -> bool {
        self.github_loading.contains(worktree_name)
    }

    /// Spawn async fetch of Linear issue for the selected worktree (non-blocking)
    fn fetch_linear_issue_for_selected(&mut self) {
        // Skip if no API key configured (cache-only mode)
        let (Some(api_key), Some(prefix)) = (&self.linear_api_key, &self.linear_prefix) else {
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
            self.cache_dir.clone(),
            worktree_name,
            issue_id,
            api_key.clone(),
            self.project_name.clone(),
            self.linear_issue_sender.clone(),
        );
    }

    /// Check for completed async Linear issue fetches and update state
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

    /// Refresh Claude session states for all worktrees
    fn refresh_claude_states(&mut self) {
        let (states, has_active) = load_claude_states(&self.cache_dir, &self.project_name, &self.worktrees);
        self.claude_states = states;
        self.has_active_claude = has_active;
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

        // Refresh Linear issues
        self.linear_issues = load_linear_issues(&self.cache_dir, &self.project_name, &worktrees);

        // Refresh GitHub PRs from cache (API fetch happens lazily)
        self.github_prs = load_github_prs_from_cache(&self.cache_dir, &self.project_name, &worktrees);

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

    fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<DashboardResult> {
        // Intervals for polling
        const ANIMATION_INTERVAL: Duration = Duration::from_millis(150);
        const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

        let last_refresh = Instant::now();

        loop {
            // Check for completed async fetches
            self.poll_github_results();
            self.poll_linear_results();

            terminal.draw(|f| self.render(f))?;

            // Use short interval when animating or loading async data
            let has_pending_github = !self.github_loading.is_empty();
            let has_pending_linear = !self.linear_loading.is_empty();
            let poll_timeout = if self.has_active_claude || has_pending_github || has_pending_linear {
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

                // Check if it's also time for a full refresh
                if last_refresh.elapsed() >= REFRESH_INTERVAL {
                    return Ok(DashboardResult::Refresh);
                }
            } else {
                // Full refresh interval elapsed
                return Ok(DashboardResult::Refresh);
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
