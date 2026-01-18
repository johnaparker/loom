use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::io::{self, stdout};

use crate::git::WorktreeStats;
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
    /// Create a new worktree
    CreateNew { branch: String, category: String },
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
}

impl Dashboard {
    pub fn new(worktrees: Vec<WorktreeStats>, project_name: String) -> Self {
        let filtered_indices: Vec<usize> = (0..worktrees.len()).collect();
        let mut list_state = ListState::default();
        if !worktrees.is_empty() {
            list_state.select(Some(0));
        }

        Self {
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
        self.worktrees = worktrees;
        self.filter_worktrees();

        // Show pending error modal if any
        if let Some((success, message)) = self.pending_result.take() {
            if !success {
                self.mode = DashboardMode::ActionResult(ActionResultModal::error(message));
            }
        }
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
        loop {
            terminal.draw(|f| self.render(f))?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if let Some(result) = self.handle_key(key.code, key.modifiers) {
                    return Ok(result);
                }
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
            self.render_commit_preview(f, main_chunks[1]);
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
            .map(|&i| Self::format_worktree_item(&self.worktrees[i], content_width))
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

    fn format_worktree_item(wt: &WorktreeStats, width: usize) -> ListItem<'static> {
        let mut lines = Vec::new();

        // Line 1: Name, branch (left), Category badge (right)
        let mut line1_spans = Vec::new();

        // Name
        let name_style = if wt.info.is_main {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White).bold()
        };
        line1_spans.push(Span::styled(wt.info.name.clone(), name_style));

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

        // Line 2: Commits ahead and age
        let mut line2_spans = Vec::new();

        if let Some(ahead) = wt.commits_ahead
            && ahead > 0
        {
            line2_spans.push(Span::styled(
                format!("  \u{2191}{} commits", ahead),
                Style::default().fg(Color::Yellow),
            ));
        }

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

        if !line2_spans.is_empty() {
            lines.push(Line::from(line2_spans));
        }

        // Line 3: Diff stats vs main and uncommitted
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
            line3_spans.push(Span::styled("~", Style::default().fg(Color::DarkGray)));
            line3_spans.push(Span::styled(
                format!("+{}", wt.uncommitted_added),
                Style::default().fg(Color::Green),
            ));
            line3_spans.push(Span::styled(
                format!(" -{}", wt.uncommitted_removed),
                Style::default().fg(Color::Red),
            ));
        } else if line3_spans.is_empty() {
            line3_spans.push(Span::styled("  Clean", Style::default().fg(Color::Green)));
        }

        // Commits behind main (purple, after diff stats)
        if let Some(behind) = wt.commits_behind
            && behind > 0
        {
            line3_spans.push(Span::styled(
                format!("    \u{2193}{}", behind),
                Style::default().fg(Color::Magenta),
            ));
        }

        if !line3_spans.is_empty() {
            lines.push(Line::from(line3_spans));
        }

        // Empty line for spacing
        lines.push(Line::from(""));

        ListItem::new(lines)
    }

    fn render_commit_preview(&self, f: &mut Frame, area: Rect) {
        let commits = if self.filtered_indices.is_empty() {
            Vec::new()
        } else {
            let actual_idx = self.filtered_indices[self.selected];
            self.worktrees[actual_idx].recent_commits.clone()
        };

        let items: Vec<ListItem> = commits
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
            .collect();

        let list =
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Commits "));

        f.render_widget(list, area);
    }

    fn render_help(&self, f: &mut Frame, area: Rect) {
        let help_text = match &self.mode {
            DashboardMode::Normal => {
                let selected = self.get_selected_worktree();
                let is_main = selected.as_ref().map(|w| w.info.is_main).unwrap_or(true);

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

                // Only show d/m for non-main worktrees
                if !is_main {
                    spans.extend(vec![
                        Span::styled("d", Style::default().fg(Color::Cyan)),
                        Span::styled(": delete  ", Style::default().fg(Color::DarkGray)),
                        Span::styled("m", Style::default().fg(Color::Cyan)),
                        Span::styled(": merge  ", Style::default().fg(Color::DarkGray)),
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
