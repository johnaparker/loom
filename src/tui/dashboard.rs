use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::collections::HashMap;
use std::io::{self, stdout};
use std::time::Duration;

use crate::claude::{self, ClaudeSession, ClaudeState};
use crate::git::WorktreeStats;
use crate::linear;
use crate::tui::modals::{
    ActionResultModal, DeleteConfirmModal, MergeConfirmModal, Modal, ModalAction, NewWorktreeModal,
    render_modal_overlay,
};

/// Result of the dashboard interaction
#[derive(Debug, Clone)]
pub enum DashboardResult {
    /// Switch to a worktree
    SwitchTo(WorktreeStats),
    /// Quit the dashboard
    Quit,
    /// Delete a worktree
    Delete {
        worktree: WorktreeStats,
        delete_branch: bool,
        force: bool,
    },
    /// Merge a worktree to main
    Merge {
        worktree: WorktreeStats,
        delete_branch: bool,
    },
    /// Sync a worktree with main
    Sync { worktree: WorktreeStats },
    /// Review a worktree's changes vs main
    Review { worktree: WorktreeStats },
    /// Open Claude in the worktree
    Claude { worktree: WorktreeStats },
    /// Create a new worktree
    CreateNew { branch: String, category: String },
    /// Open Linear issue for a worktree
    Linear { worktree: WorktreeStats },
    /// Refresh the dashboard (after an action)
    Refresh,
}

/// Dashboard mode
enum DashboardMode {
    /// Normal navigation mode
    Normal,
    /// Search/filter mode
    Search,
    /// Showing delete confirmation modal
    ConfirmDelete(DeleteConfirmModal),
    /// Showing merge confirmation modal
    ConfirmMerge(MergeConfirmModal),
    /// Showing new worktree modal
    NewWorktree(NewWorktreeModal),
    /// Showing action result
    ActionResult(ActionResultModal),
}

/// Interactive dashboard for worktree status
pub struct Dashboard {
    worktrees: Vec<WorktreeStats>,
    filtered_indices: Vec<usize>,
    selected: usize,
    list_state: ListState,
    project_name: String,
    mode: DashboardMode,
    search_input: String,
    matcher: Matcher,
    main_branch: String,
    /// Pending action result to show after refresh
    pending_result: Option<(bool, String)>,
    /// Status message to show in title bar (auto-clears on keypress)
    /// Tuple of (is_success, message)
    status_message: Option<(bool, String)>,
    /// Cached Linear issue titles by worktree name
    linear_titles: HashMap<String, String>,
    /// Cached Claude session states by worktree name
    claude_states: HashMap<String, ClaudeSession>,
}

impl Dashboard {
    pub fn new(worktrees: Vec<WorktreeStats>, project_name: String) -> Self {
        let filtered_indices: Vec<usize> = (0..worktrees.len()).collect();
        let mut list_state = ListState::default();
        if !worktrees.is_empty() {
            list_state.select(Some(0));
        }

        // Load Linear titles for all worktrees
        let linear_titles = Self::load_linear_titles(&project_name, &worktrees);

        let mut dashboard = Self {
            worktrees,
            filtered_indices,
            selected: 0,
            list_state,
            project_name,
            mode: DashboardMode::Normal,
            search_input: String::new(),
            matcher: Matcher::new(NucleoConfig::DEFAULT),
            main_branch: "main".to_string(),
            pending_result: None,
            status_message: None,
            linear_titles,
            claude_states: HashMap::new(),
        };

        // Load initial Claude states
        dashboard.refresh_claude_states();

        dashboard
    }

    /// Load Linear issue titles from cache for all worktrees
    fn load_linear_titles(
        project_name: &str,
        worktrees: &[WorktreeStats],
    ) -> HashMap<String, String> {
        let mut titles = HashMap::new();
        for wt in worktrees {
            if let Ok(Some(issue)) = linear::read_metadata(project_name, &wt.info.name) {
                if !issue.title.is_empty() {
                    titles.insert(wt.info.name.clone(), issue.title);
                }
            }
        }
        titles
    }

    /// Refresh Claude session states for all worktrees
    fn refresh_claude_states(&mut self) {
        self.claude_states.clear();
        for wt in &self.worktrees {
            if let Some(session) = claude::read_state(&self.project_name, &wt.info.name) {
                // Only include non-stale sessions (or explicitly inactive ones)
                if !session.is_stale() || session.state == ClaudeState::Inactive {
                    self.claude_states.insert(wt.info.name.clone(), session);
                }
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

        // Refresh Linear titles
        self.linear_titles = Self::load_linear_titles(&self.project_name, &worktrees);

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
                self.mode = DashboardMode::ActionResult(ActionResultModal::error(message));
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
                *modal = MergeConfirmModal::new(wt, main_branch, conflict_info);
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
        // Auto-refresh interval
        const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

        loop {
            terminal.draw(|f| self.render(f))?;

            // Poll for events with timeout - returns true if event is available
            if event::poll(REFRESH_INTERVAL)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    if let Some(result) = self.handle_key(key.code, key.modifiers) {
                        return Ok(result);
                    }
                }
            } else {
                // Timeout - trigger refresh to update worktree stats
                return Ok(DashboardResult::Refresh);
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<DashboardResult> {
        // Clear status message on any keypress
        self.status_message = None;

        // Handle Ctrl+C as escape in any mode
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return Some(DashboardResult::Quit);
        }

        match &mut self.mode {
            DashboardMode::Normal => self.handle_normal_key(code),
            DashboardMode::Search => self.handle_search_key(code),
            DashboardMode::ConfirmDelete(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
            DashboardMode::ConfirmMerge(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
            DashboardMode::NewWorktree(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
            DashboardMode::ActionResult(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
        }
    }

    fn handle_modal_action(&mut self, action: ModalAction) -> Option<DashboardResult> {
        match action {
            ModalAction::Cancel => {
                self.mode = DashboardMode::Normal;
                None
            }
            ModalAction::Delete {
                delete_branch,
                force,
            } => {
                if let Some(worktree) = self.get_selected_worktree() {
                    self.mode = DashboardMode::Normal;
                    Some(DashboardResult::Delete {
                        worktree,
                        delete_branch,
                        force,
                    })
                } else {
                    self.mode = DashboardMode::Normal;
                    None
                }
            }
            ModalAction::Merge { delete_branch } => {
                if let Some(worktree) = self.get_selected_worktree() {
                    self.mode = DashboardMode::Normal;
                    Some(DashboardResult::Merge {
                        worktree,
                        delete_branch,
                    })
                } else {
                    self.mode = DashboardMode::Normal;
                    None
                }
            }
            ModalAction::CreateNew { branch, category } => {
                self.mode = DashboardMode::Normal;
                Some(DashboardResult::CreateNew { branch, category })
            }
            ModalAction::ShowResult { success, message } => {
                self.mode = if success {
                    DashboardMode::ActionResult(ActionResultModal::success(message))
                } else {
                    DashboardMode::ActionResult(ActionResultModal::error(message))
                };
                None
            }
            ModalAction::DismissResult => {
                self.mode = DashboardMode::Normal;
                Some(DashboardResult::Refresh)
            }
        }
    }

    fn get_selected_worktree(&self) -> Option<WorktreeStats> {
        if self.filtered_indices.is_empty() {
            None
        } else {
            let actual_idx = self.filtered_indices[self.selected];
            Some(self.worktrees[actual_idx].clone())
        }
    }

    fn handle_normal_key(&mut self, code: KeyCode) -> Option<DashboardResult> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Some(DashboardResult::Quit),
            KeyCode::Char('/') => {
                self.mode = DashboardMode::Search;
                None
            }
            KeyCode::Enter => {
                if let Some(worktree) = self.get_selected_worktree() {
                    Some(DashboardResult::SwitchTo(worktree))
                } else {
                    None
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                None
            }
            // Quick actions
            KeyCode::Char('n') => {
                self.mode = DashboardMode::NewWorktree(NewWorktreeModal::new());
                None
            }
            KeyCode::Char('d') => {
                // Delete - only for non-main worktrees
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main {
                        self.mode = DashboardMode::ConfirmDelete(DeleteConfirmModal::new(worktree));
                    }
                }
                None
            }
            KeyCode::Char('m') => {
                // Merge - only for non-main worktrees
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main && worktree.info.branch.is_some() {
                        // Create modal without conflict info initially
                        // The status command will check for conflicts and update
                        self.mode = DashboardMode::ConfirmMerge(MergeConfirmModal::new(
                            worktree,
                            self.main_branch.clone(),
                            None,
                        ));
                    }
                }
                None
            }
            KeyCode::Char('s') => {
                // Sync - for all worktrees with branches (no confirmation needed)
                if let Some(worktree) = self.get_selected_worktree() {
                    if worktree.info.branch.is_some() {
                        return Some(DashboardResult::Sync { worktree });
                    }
                }
                None
            }
            KeyCode::Char('r') => {
                // Review - open diff view in neovim for non-main worktrees
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main {
                        return Some(DashboardResult::Review { worktree });
                    }
                }
                None
            }
            KeyCode::Char('c') => {
                // Claude - open claude in tmux window for any worktree
                if let Some(worktree) = self.get_selected_worktree() {
                    return Some(DashboardResult::Claude { worktree });
                }
                None
            }
            KeyCode::Char('l') => {
                // Linear - open Linear issue (only if worktree has one)
                if let Some(worktree) = self.get_selected_worktree() {
                    if self.linear_titles.contains_key(&worktree.info.name) {
                        return Some(DashboardResult::Linear { worktree });
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn handle_search_key(&mut self, code: KeyCode) -> Option<DashboardResult> {
        match code {
            KeyCode::Esc => {
                self.mode = DashboardMode::Normal;
                self.search_input.clear();
                self.filter_worktrees();
                None
            }
            KeyCode::Enter => {
                self.mode = DashboardMode::Normal;
                if let Some(worktree) = self.get_selected_worktree() {
                    Some(DashboardResult::SwitchTo(worktree))
                } else {
                    None
                }
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.filter_worktrees();
                None
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.filter_worktrees();
                None
            }
            KeyCode::Up => {
                self.move_selection(-1);
                None
            }
            KeyCode::Down => {
                self.move_selection(1);
                None
            }
            _ => None,
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
    }

    fn filter_worktrees(&mut self) {
        if self.search_input.is_empty() {
            self.filtered_indices = (0..self.worktrees.len()).collect();
        } else {
            let mut scored: Vec<(usize, u16)> = self
                .worktrees
                .iter()
                .enumerate()
                .filter_map(|(i, wt)| {
                    let haystack = format!(
                        "{} {} {}",
                        wt.info.name,
                        wt.info.branch.as_deref().unwrap_or(""),
                        wt.info.category.as_deref().unwrap_or("")
                    );
                    let mut haystack_buf = Vec::new();
                    let haystack_str = Utf32Str::new(&haystack, &mut haystack_buf);
                    let mut needle_buf = Vec::new();
                    let needle_str = Utf32Str::new(&self.search_input, &mut needle_buf);

                    self.matcher
                        .fuzzy_match(haystack_str, needle_str)
                        .map(|score| (i, score))
                })
                .collect();

            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_indices = scored.into_iter().map(|(i, _)| i).collect();
        }

        // Reset selection
        if self.filtered_indices.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            self.selected = 0;
            self.list_state.select(Some(0));
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let is_search_mode = matches!(self.mode, DashboardMode::Search);

        let constraints = if is_search_mode {
            vec![
                Constraint::Length(1), // Title bar
                Constraint::Length(3), // Search bar
                Constraint::Min(1),    // Main content
                Constraint::Length(1), // Help bar
            ]
        } else {
            vec![
                Constraint::Length(1), // Title bar
                Constraint::Min(1),    // Main content
                Constraint::Length(1), // Help bar
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(f.area());

        let (title_area, search_area, main_area, help_area) = if is_search_mode {
            (chunks[0], Some(chunks[1]), chunks[2], chunks[3])
        } else {
            (chunks[0], None, chunks[1], chunks[2])
        };

        self.render_title_bar(f, title_area);

        if let Some(area) = search_area {
            self.render_search_bar(f, area);
        }

        if self.filtered_indices.is_empty() {
            self.render_empty(f, main_area);
        } else {
            // Split main area into left (60%) and right (40%)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(main_area);

            self.render_worktree_list(f, main_chunks[0]);

            // Split right panel: commits (top 50%) and claude pane (bottom 50%)
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(main_chunks[1]);

            self.render_commit_preview(f, right_chunks[0]);
            self.render_claude_pane(f, right_chunks[1]);
        }

        self.render_help(f, help_area);

        // Render modal overlay if in modal mode
        match &mut self.mode {
            DashboardMode::ConfirmDelete(modal) => {
                render_modal_overlay(modal, f.area(), f.buffer_mut());
            }
            DashboardMode::ConfirmMerge(modal) => {
                render_modal_overlay(modal, f.area(), f.buffer_mut());
            }
            DashboardMode::NewWorktree(modal) => {
                render_modal_overlay(modal, f.area(), f.buffer_mut());
            }
            DashboardMode::ActionResult(modal) => {
                render_modal_overlay(modal, f.area(), f.buffer_mut());
            }
            _ => {}
        }
    }

    fn render_search_bar(&self, f: &mut Frame, area: Rect) {
        let search_block = Block::default()
            .borders(Borders::ALL)
            .title(" / Search (Esc to cancel) ");
        let search_paragraph = Paragraph::new(self.search_input.as_str()).block(search_block);
        f.render_widget(search_paragraph, area);
    }

    fn render_title_bar(&self, f: &mut Frame, area: Rect) {
        // If we have a status message, show it as a banner
        if let Some((is_success, ref message)) = self.status_message {
            let icon = if is_success { "\u{2713}" } else { "\u{2717}" }; // ✓ or ✗
            let color = if is_success { Color::Green } else { Color::Red };
            let status_text = format!(" {} {} ", icon, message);
            let padding = area.width.saturating_sub(status_text.len() as u16);

            let line = Line::from(vec![
                Span::styled(status_text, Style::default().fg(color).bold()),
                Span::raw(" ".repeat(padding as usize)),
            ]);

            let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(40, 44, 52)));
            f.render_widget(paragraph, area);
            return;
        }

        let title = format!(" gwt status - {} ", self.project_name);
        let quit_hint = "(q to quit)";
        let padding = area
            .width
            .saturating_sub(title.len() as u16 + quit_hint.len() as u16);

        let line = Line::from(vec![
            Span::styled(title, Style::default().fg(Color::White).bold()),
            Span::raw(" ".repeat(padding as usize)),
            Span::styled(quit_hint, Style::default().fg(Color::DarkGray)),
        ]);

        let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(40, 44, 52)));
        f.render_widget(paragraph, area);
    }

    fn render_empty(&self, f: &mut Frame, area: Rect) {
        let message = Paragraph::new("No worktrees found")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" Worktrees "));
        f.render_widget(message, area);
    }

    fn render_worktree_list(&mut self, f: &mut Frame, area: Rect) {
        // Calculate available width for content (subtract borders and highlight symbol)
        let content_width = area.width.saturating_sub(5) as usize; // 2 borders + "> " symbol

        let items: Vec<ListItem> = self
            .filtered_indices
            .iter()
            .map(|&i| {
                let wt = &self.worktrees[i];
                let linear_title = self.linear_titles.get(&wt.info.name).map(|s| s.as_str());
                let claude_state = self.claude_states.get(&wt.info.name)
                    .map(|s| claude::effective_state(s));
                Self::format_worktree_item(wt, content_width, linear_title, claude_state)
            })
            .collect();

        let is_search = matches!(self.mode, DashboardMode::Search);
        let title = if is_search && !self.search_input.is_empty() {
            format!(" Worktrees ({} matches) ", self.filtered_indices.len())
        } else {
            " Worktrees ".to_string()
        };

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::Rgb(40, 44, 52)))
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn format_worktree_item(
        wt: &WorktreeStats,
        width: usize,
        linear_title: Option<&str>,
        claude_state: Option<ClaudeState>,
    ) -> ListItem<'static> {
        let mut lines = Vec::new();

        // Line 1: Name, Claude state indicator, branch (left), Category badge (right)
        let mut line1_spans = Vec::new();

        // Name
        let name_style = if wt.info.is_main {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White).bold()
        };
        line1_spans.push(Span::styled(wt.info.name.clone(), name_style));

        // Claude state indicator (after name)
        if let Some(state) = claude_state {
            let (icon, color) = match state {
                ClaudeState::Working => (" \u{2699}", Color::Yellow),         // ⚙
                ClaudeState::Idle => (" \u{2713}", Color::Green),              // ✓
                ClaudeState::WaitingPermission => (" !", Color::Red),
                ClaudeState::Inactive => ("", Color::DarkGray),  // No indicator for inactive
            };
            if !icon.is_empty() {
                line1_spans.push(Span::styled(icon, Style::default().fg(color)));
            }
        }

        // Branch
        let branch_text = wt
            .info
            .branch
            .as_ref()
            .map(|b| format!("   {}", b))
            .unwrap_or_else(|| " (detached)".to_string());
        line1_spans.push(Span::styled(
            branch_text.clone(),
            Style::default().fg(Color::Cyan),
        ));

        // Category badge - right aligned (use "main" for main worktree)
        let category = if wt.info.is_main {
            Some("main".to_string())
        } else {
            wt.info.category.clone()
        };

        if let Some(ref cat) = category {
            let cat_color = Self::category_color(cat);
            let cat_text = format!("[{}]", cat);

            // Calculate left content length
            let left_len = wt.info.name.len() + branch_text.len();
            let padding = width.saturating_sub(left_len + cat_text.len() + 1);

            if padding > 0 {
                line1_spans.push(Span::raw(" ".repeat(padding)));
            } else {
                line1_spans.push(Span::raw(" "));
            }
            line1_spans.push(Span::styled(
                cat_text,
                Style::default().fg(cat_color).bold(),
            ));
        }

        lines.push(Line::from(line1_spans));

        // Line 1.5: Linear issue title (if available)
        if let Some(title) = linear_title {
            // Truncate if too long (leave room for indent, bullet and ellipsis)
            let max_len = width.saturating_sub(6);
            let display_title = if title.len() > max_len {
                format!("    {}...", &title[..max_len.saturating_sub(3)])
            } else {
                format!("    {}", title)
            };
            lines.push(Line::from(Span::styled(
                display_title,
                Style::default().fg(Color::White).italic(),
            )));
        }

        // Line 2: Age (left), commits ahead (↑), commits behind (↓)
        let mut line2_spans = Vec::new();

        // Age - always far left
        if let Some(days) = wt.age_days {
            let age_text = if days == 0 {
                "today".to_string()
            } else if days == 1 {
                "1d ago".to_string()
            } else {
                format!("{}d ago", days)
            };
            line2_spans.push(Span::raw("  "));
            line2_spans.push(Span::styled(age_text, Style::default().fg(Color::DarkGray)));
        }

        // Commits ahead (↑)
        if let Some(ahead) = wt.commits_ahead
            && ahead > 0
        {
            line2_spans.push(Span::raw("  "));
            line2_spans.push(Span::styled(
                format!(" \u{2191}{}", ahead),
                Style::default().fg(Color::Yellow),
            ));
        }

        // Commits behind (↓)
        if let Some(behind) = wt.commits_behind
            && behind > 0
        {
            line2_spans.push(Span::raw("  "));
            line2_spans.push(Span::styled(
                format!(" \u{2193}{}", behind),
                Style::default().fg(Color::Magenta),
            ));
        }

        if !line2_spans.is_empty() {
            lines.push(Line::from(line2_spans));
        }

        // Line 3: Git changes only (+/- vs main and uncommitted)
        let mut line3_spans = Vec::new();

        // Diff vs main (skip for main worktree)
        if !wt.info.is_main
            && let (Some(added), Some(removed)) = (wt.diff_added, wt.diff_removed)
        {
            if added > 0 || removed > 0 {
                line3_spans.push(Span::styled(
                    format!("  +{}", added),
                    Style::default().fg(Color::Green),
                ));
                line3_spans.push(Span::styled(
                    format!(" -{}", removed),
                    Style::default().fg(Color::Red),
                ));
                line3_spans.push(Span::styled(
                    " vs main",
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                line3_spans.push(Span::styled(
                    "  no commits",
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }

        // Uncommitted changes (use green/red with ~ prefix)
        if wt.uncommitted_added > 0 || wt.uncommitted_removed > 0 {
            let padding = if line3_spans.is_empty() { "  " } else { "   " };
            line3_spans.push(Span::raw(padding));
            line3_spans.push(Span::styled(
                format!("+{}", wt.uncommitted_added),
                Style::default().fg(Color::Green),
            ));
            line3_spans.push(Span::styled(
                format!(" -{}", wt.uncommitted_removed),
                Style::default().fg(Color::Red),
            ));
            line3_spans.push(Span::styled(
                " uncommitted",
                Style::default().fg(Color::DarkGray),
            ));
        } else if line3_spans.is_empty() {
            line3_spans.push(Span::styled("  Clean", Style::default().fg(Color::Green)));
        }

        if !line3_spans.is_empty() {
            lines.push(Line::from(line3_spans));
        }

        // Empty line for spacing
        lines.push(Line::from(""));

        ListItem::new(lines)
    }

    fn render_commit_preview(&self, f: &mut Frame, area: Rect) {
        let (commits, is_main) = if self.filtered_indices.is_empty() {
            (Vec::new(), true)
        } else {
            let actual_idx = self.filtered_indices[self.selected];
            let worktree = &self.worktrees[actual_idx];
            (worktree.recent_commits.clone(), worktree.info.is_main)
        };

        let items: Vec<ListItem> = if commits.is_empty() {
            let message = if is_main {
                "No commits yet"
            } else {
                "No commits ahead of main"
            };
            vec![ListItem::new(Line::from(Span::styled(
                message,
                Style::default().fg(Color::DarkGray).italic(),
            )))]
        } else {
            commits
                .iter()
                .map(|commit| {
                    let lines = vec![
                        Line::from(vec![
                            Span::styled(&commit.hash_short, Style::default().fg(Color::Yellow)),
                            Span::raw(" "),
                            Span::styled(&commit.message, Style::default().fg(Color::White)),
                        ]),
                        Line::from(vec![Span::styled(
                            format!("  {}, {}", commit.author, commit.relative_time),
                            Style::default().fg(Color::DarkGray),
                        )]),
                    ];
                    ListItem::new(lines)
                })
                .collect()
        };

        let list =
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Commits "));

        f.render_widget(list, area);
    }

    fn render_claude_pane(&self, f: &mut Frame, area: Rect) {
        // Get the selected worktree's Claude session
        let session = self.get_selected_worktree()
            .and_then(|wt| self.claude_states.get(&wt.info.name));

        // Build title with state indicator
        let title = match session {
            Some(s) => {
                let effective_state = claude::effective_state(s);
                let (icon, color) = match effective_state {
                    ClaudeState::Working => ("\u{2699}", Color::Yellow),    // ⚙
                    ClaudeState::Idle => ("\u{2713}", Color::Green),         // ✓
                    ClaudeState::WaitingPermission => ("!", Color::Red),
                    ClaudeState::Inactive => ("-", Color::DarkGray),
                };
                Line::from(vec![
                    Span::raw(" Claude "),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::styled(format!(" {} ", effective_state), Style::default().fg(color)),
                ])
            }
            None => Line::from(" Claude "),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content: Text = match session {
            Some(s) if !s.events.is_empty() => {
                let mut lines = Vec::new();

                // Show recent events (most recent first, limit to what fits)
                for event in s.events.iter().rev().take(10) {
                    let time_str = claude::relative_time(&event.timestamp);

                    let event_line = match event.event_type.as_str() {
                        "UserPromptSubmit" => {
                            let preview = event.prompt_preview.as_deref().unwrap_or("");
                            vec![
                                Span::styled(
                                    format!("{:>8} ", time_str),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled("\u{25B6} ", Style::default().fg(Color::Yellow)), // ▶
                                Span::styled(
                                    truncate_str(preview, inner.width.saturating_sub(14) as usize),
                                    Style::default().fg(Color::White),
                                ),
                            ]
                        }
                        "Stop" => {
                            vec![
                                Span::styled(
                                    format!("{:>8} ", time_str),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled("\u{25A0} ", Style::default().fg(Color::Green)), // ■
                                Span::styled("Completed", Style::default().fg(Color::Green)),
                            ]
                        }
                        "Notification" => {
                            let kind = event.kind.as_deref().unwrap_or("notification");
                            let (icon, color) = if kind.contains("permission") {
                                ("!", Color::Red)
                            } else {
                                ("\u{25CF}", Color::Cyan) // ●
                            };
                            vec![
                                Span::styled(
                                    format!("{:>8} ", time_str),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                                Span::styled(kind, Style::default().fg(color)),
                            ]
                        }
                        "SessionStart" => {
                            vec![
                                Span::styled(
                                    format!("{:>8} ", time_str),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled("\u{25B7} ", Style::default().fg(Color::Cyan)), // ▷
                                Span::styled("Session started", Style::default().fg(Color::Cyan)),
                            ]
                        }
                        "SessionEnd" => {
                            vec![
                                Span::styled(
                                    format!("{:>8} ", time_str),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled("\u{25A1} ", Style::default().fg(Color::Red)), // □
                                Span::styled("Session ended", Style::default().fg(Color::Red)),
                            ]
                        }
                        _ => {
                            vec![
                                Span::styled(
                                    format!("{:>8} ", time_str),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::raw(&event.event_type),
                            ]
                        }
                    };

                    lines.push(Line::from(event_line));
                }

                if lines.is_empty() {
                    Text::styled(
                        "No events recorded",
                        Style::default().fg(Color::DarkGray).italic(),
                    )
                } else {
                    Text::from(lines)
                }
            }
            Some(_) => Text::styled(
                "Session active, no events yet",
                Style::default().fg(Color::DarkGray).italic(),
            ),
            None => Text::styled(
                "No Claude session\nPress 'c' to open Claude",
                Style::default().fg(Color::DarkGray).italic(),
            ),
        };

        let paragraph = Paragraph::new(content).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    fn render_help(&self, f: &mut Frame, area: Rect) {
        let help_text = match &self.mode {
            DashboardMode::Normal => {
                let selected = self.get_selected_worktree();
                let is_main = selected.as_ref().map(|w| w.info.is_main).unwrap_or(true);
                let has_linear = selected
                    .as_ref()
                    .map(|w| self.linear_titles.contains_key(&w.info.name))
                    .unwrap_or(false);

                let mut spans = vec![
                    Span::styled(" \u{2191}/\u{2193}", Style::default().fg(Color::Cyan)),
                    Span::styled(": nav  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("/", Style::default().fg(Color::Cyan)),
                    Span::styled(": search  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Enter", Style::default().fg(Color::Cyan)),
                    Span::styled(": switch  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("n", Style::default().fg(Color::Cyan)),
                    Span::styled(": new  ", Style::default().fg(Color::DarkGray)),
                ];

                // Only show d/m/r for non-main worktrees
                if !is_main {
                    spans.extend(vec![
                        Span::styled("d", Style::default().fg(Color::Cyan)),
                        Span::styled(": delete  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("m", Style::default().fg(Color::Cyan)),
                        Span::styled(": merge  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("r", Style::default().fg(Color::Cyan)),
                        Span::styled(": review  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                // Show s: sync for all worktrees (including main)
                spans.extend(vec![
                    Span::styled("s", Style::default().fg(Color::Cyan)),
                    Span::styled(": sync  ", Style::default().fg(Color::DarkGray)),
                ]);

                // Show c: claude for all worktrees
                spans.extend(vec![
                    Span::styled("c", Style::default().fg(Color::Cyan)),
                    Span::styled(": claude  ", Style::default().fg(Color::DarkGray)),
                ]);

                // Show l: linear only if worktree has a Linear issue
                if has_linear {
                    spans.extend(vec![
                        Span::styled("l", Style::default().fg(Color::Cyan)),
                        Span::styled(": linear  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                spans.extend(vec![
                    Span::styled("q", Style::default().fg(Color::Cyan)),
                    Span::styled(": quit", Style::default().fg(Color::DarkGray)),
                ]);

                Line::from(spans)
            }
            DashboardMode::Search => Line::from(vec![
                Span::styled(" \u{2191}/\u{2193}", Style::default().fg(Color::Cyan)),
                Span::styled(": navigate   ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(": select   ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
            ]),
            _ => Line::from(""),
        };

        let paragraph =
            Paragraph::new(help_text).style(Style::default().bg(Color::Rgb(30, 30, 30)));
        f.render_widget(paragraph, area);
    }
}

/// Truncate a string to a maximum length, adding "..." if truncated
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        ".".repeat(max_len)
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
