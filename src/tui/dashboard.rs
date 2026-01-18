use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::io::{self, stdout};

use crate::git::WorktreeStats;

/// Result of the dashboard interaction
pub enum DashboardResult {
    SwitchTo(WorktreeStats),
    Quit,
}

/// Interactive dashboard for worktree status
pub struct Dashboard {
    worktrees: Vec<WorktreeStats>,
    filtered_indices: Vec<usize>,
    selected: usize,
    list_state: ListState,
    project_name: String,
    search_mode: bool,
    search_input: String,
    matcher: Matcher,
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
            search_mode: false,
            search_input: String::new(),
            matcher: Matcher::new(NucleoConfig::DEFAULT),
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
        // Handle Ctrl+C as escape
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return Some(DashboardResult::Quit);
        }

        // Search mode handling
        if self.search_mode {
            match code {
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_input.clear();
                    self.filter_worktrees();
                }
                KeyCode::Enter => {
                    self.search_mode = false;
                    if !self.filtered_indices.is_empty() {
                        let actual_idx = self.filtered_indices[self.selected];
                        return Some(DashboardResult::SwitchTo(
                            self.worktrees[actual_idx].clone(),
                        ));
                    }
                }
                KeyCode::Backspace => {
                    self.search_input.pop();
                    self.filter_worktrees();
                }
                KeyCode::Char(c) => {
                    self.search_input.push(c);
                    self.filter_worktrees();
                }
                KeyCode::Up => {
                    self.move_selection(-1);
                }
                KeyCode::Down => {
                    self.move_selection(1);
                }
                _ => {}
            }
            return None;
        }

        // Normal mode handling
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Some(DashboardResult::Quit),
            KeyCode::Char('/') => {
                self.search_mode = true;
                None
            }
            KeyCode::Enter => {
                if !self.filtered_indices.is_empty() {
                    let actual_idx = self.filtered_indices[self.selected];
                    Some(DashboardResult::SwitchTo(
                        self.worktrees[actual_idx].clone(),
                    ))
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
            _ => None,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let current = self.selected as i32;
        let new = (current + delta).clamp(0, self.filtered_indices.len() as i32 - 1) as usize;
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
        let constraints = if self.search_mode {
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

        let (title_area, search_area, main_area, help_area) = if self.search_mode {
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
    }

    fn render_search_bar(&self, f: &mut Frame, area: Rect) {
        let search_block = Block::default()
            .borders(Borders::ALL)
            .title(" / Search (Esc to cancel) ");
        let search_paragraph = Paragraph::new(self.search_input.as_str()).block(search_block);
        f.render_widget(search_paragraph, area);
    }

    fn render_title_bar(&self, f: &mut Frame, area: Rect) {
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

        let title = if self.search_mode && !self.search_input.is_empty() {
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
            .map(|b| format!(" [{}]", b))
            .unwrap_or_else(|| " (detached)".to_string());
        line1_spans.push(Span::styled(branch_text.clone(), Style::default().fg(Color::Cyan)));

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
            line1_spans.push(Span::styled(cat_text, Style::default().fg(cat_color).bold()));
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
                    "  Clean vs main",
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

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Commits "),
        );

        f.render_widget(list, area);
    }

    fn render_help(&self, f: &mut Frame, area: Rect) {
        let help_text = Line::from(vec![
            Span::styled(
                " \u{2191}/\u{2193}/j/k",
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(": navigate   ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::Cyan)),
            Span::styled(": search   ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(": switch   ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::styled(": quit", Style::default().fg(Color::DarkGray)),
        ]);

        let paragraph =
            Paragraph::new(help_text).style(Style::default().bg(Color::Rgb(30, 30, 30)));
        f.render_widget(paragraph, area);
    }
}
