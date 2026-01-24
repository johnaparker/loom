//! Rendering logic for the dashboard.
//!
//! # Thread Safety: SYNC ONLY
//!
//! All functions in this module are called from the TUI event loop and **must
//! remain synchronous**. They must not:
//! - Use `.await` or block on futures
//! - Make HTTP requests or perform network I/O
//! - Perform blocking file operations
//!
//! These functions are called on every frame render. Any blocking operation
//! would freeze the entire UI. Data loading happens in background threads
//! (see `data.rs`) and is polled via channels in the event loop.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::claude::{self, ClaudeState};
use crate::git::WorktreeStats;
use crate::github::{CachedPRState, GitHubPR};
use crate::tui::modals::render_modal_overlay;

use super::state::DashboardMode;
use super::{Dashboard, Panel, SPINNER_FRAMES};

impl Dashboard {
    pub(super) fn render(&mut self, f: &mut Frame) {
        let is_search_mode = matches!(self.mode, DashboardMode::Search);
        let show_category_bar = self.show_category_filter();
        let panels = self.enabled_panels();

        // Layout: Title | [Category Filter] | [Search] | Content Area(s) | Help
        // Content area structure depends on panel count
        let constraints = self.build_vertical_constraints(is_search_mode, show_category_bar, panels.len());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(f.area());

        // Determine chunk indices based on which optional elements are shown
        let (title_area, category_area, search_area, content_areas, help_area) =
            self.extract_layout_areas(&chunks, is_search_mode, show_category_bar, panels.len());

        self.render_title_bar(f, title_area);

        if let Some(cat_area) = category_area {
            self.render_category_filter_bar(f, cat_area);
        }

        if let Some(search_area) = search_area {
            self.render_search_bar(f, search_area);
        }

        // Main content - dynamic layout based on enabled panels
        if self.filtered_indices.is_empty() {
            self.render_empty(f, content_areas[0]);
        } else {
            self.render_dynamic_layout(f, &content_areas, &panels);
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
            DashboardMode::ConfirmPrune(modal) => {
                render_modal_overlay(modal, f.area(), f.buffer_mut());
            }
            _ => {}
        }
    }

    /// Build vertical layout constraints based on panel count
    fn build_vertical_constraints(
        &self,
        is_search_mode: bool,
        show_category_bar: bool,
        panel_count: usize,
    ) -> Vec<Constraint> {
        let mut constraints = vec![Constraint::Length(1)]; // Title bar

        if show_category_bar {
            constraints.push(Constraint::Length(1)); // Category filter bar
        }

        if is_search_mode {
            constraints.push(Constraint::Length(3)); // Search bar
        }

        // Content areas: 1 or 2 rows depending on panel count
        match panel_count {
            2 | 3 => {
                // Single content row for 2-3 panels
                constraints.push(Constraint::Min(0));
            }
            _ => {
                // Two content rows for 4-5 panels
                constraints.push(Constraint::Percentage(50)); // Top row
                constraints.push(Constraint::Percentage(50)); // Bottom row
            }
        }

        constraints.push(Constraint::Length(1)); // Help bar
        constraints
    }

    /// Extract layout areas from chunks based on current config
    fn extract_layout_areas(
        &self,
        chunks: &[Rect],
        is_search_mode: bool,
        show_category_bar: bool,
        panel_count: usize,
    ) -> (Rect, Option<Rect>, Option<Rect>, Vec<Rect>, Rect) {
        let mut idx = 0;

        let title_area = chunks[idx];
        idx += 1;

        let category_area = if show_category_bar {
            let area = chunks[idx];
            idx += 1;
            Some(area)
        } else {
            None
        };

        let search_area = if is_search_mode {
            let area = chunks[idx];
            idx += 1;
            Some(area)
        } else {
            None
        };

        // Collect content areas (1 or 2 depending on layout)
        let content_count = if panel_count <= 3 { 1 } else { 2 };
        let content_areas: Vec<Rect> = (0..content_count).map(|i| chunks[idx + i]).collect();
        idx += content_count;

        let help_area = chunks[idx];

        (title_area, category_area, search_area, content_areas, help_area)
    }

    /// Render panels using dynamic layout based on enabled panels
    fn render_dynamic_layout(&mut self, f: &mut Frame, content_areas: &[Rect], panels: &[Panel]) {
        match panels.len() {
            2 => self.render_two_panel_layout(f, content_areas[0], panels),
            3 => self.render_three_panel_layout(f, content_areas[0], panels),
            4 => self.render_four_panel_layout(f, content_areas, panels),
            _ => self.render_five_panel_layout(f, content_areas, panels),
        }
    }

    /// 2 panels: 50/50 horizontal split
    /// +------------------+------------------+
    /// |    Worktrees     |     Commits      |
    /// |      (50%)       |      (50%)       |
    /// +------------------+------------------+
    fn render_two_panel_layout(&mut self, f: &mut Frame, area: Rect, panels: &[Panel]) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        for (i, panel) in panels.iter().enumerate() {
            if i < chunks.len() {
                self.render_panel(f, chunks[i], *panel);
            }
        }
    }

    /// 3 panels: Worktrees left, 2 stacked on right
    /// +------------------+------------------+
    /// |                  |     Panel 2      |
    /// |    Worktrees     +------------------+
    /// |      (50%)       |     Panel 3      |
    /// +------------------+------------------+
    fn render_three_panel_layout(&mut self, f: &mut Frame, area: Rect, panels: &[Panel]) {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Left: Worktrees
        self.render_panel(f, main_chunks[0], panels[0]);

        // Right: 2 stacked panels
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[1]);

        self.render_panel(f, right_chunks[0], panels[1]);
        self.render_panel(f, right_chunks[1], panels[2]);
    }

    /// 4 panels: 2x2 grid
    /// +------------------+------------------+
    /// |    Worktrees     |     Panel 2      |
    /// +------------------+------------------+
    /// |     Panel 3      |     Panel 4      |
    /// +------------------+------------------+
    fn render_four_panel_layout(&mut self, f: &mut Frame, content_areas: &[Rect], panels: &[Panel]) {
        // Top row: panels[0], panels[1]
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(content_areas[0]);

        self.render_panel(f, top_chunks[0], panels[0]);
        self.render_panel(f, top_chunks[1], panels[1]);

        // Bottom row: panels[2], panels[3]
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(content_areas[1]);

        self.render_panel(f, bottom_chunks[0], panels[2]);
        self.render_panel(f, bottom_chunks[1], panels[3]);
    }

    /// 5 panels: Current layout (2+3)
    /// +------------------+------------------+
    /// |    Worktrees     |     Claude       |
    /// +--------+---------+------------------+
    /// | Commits| GitHub  |     Linear       |
    /// +--------+---------+------------------+
    fn render_five_panel_layout(&mut self, f: &mut Frame, content_areas: &[Rect], panels: &[Panel]) {
        // Top row: 50/50 split
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(content_areas[0]);

        self.render_panel(f, top_chunks[0], panels[0]); // Worktrees
        self.render_panel(f, top_chunks[1], panels[1]); // Claude

        // Bottom row: 33/33/34 split
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(content_areas[1]);

        self.render_panel(f, bottom_chunks[0], panels[2]); // Linear
        self.render_panel(f, bottom_chunks[1], panels[3]); // GitHub
        self.render_panel(f, bottom_chunks[2], panels[4]); // Commits
    }

    /// Render a single panel by type
    fn render_panel(&mut self, f: &mut Frame, area: Rect, panel: Panel) {
        match panel {
            Panel::Worktrees => self.render_worktree_list(f, area),
            Panel::Claude => self.render_claude_pane(f, area),
            Panel::Linear => self.render_linear_panel(f, area),
            Panel::GitHub => self.render_github_panel(f, area),
            Panel::Commits => self.render_commit_preview(f, area),
        }
    }

    fn render_search_bar(&self, f: &mut Frame, area: Rect) {
        let search_block = Block::default()
            .borders(Borders::ALL)
            .title(" / Search (Esc to cancel) ");
        let search_paragraph = Paragraph::new(self.search_input.as_str()).block(search_block);
        f.render_widget(search_paragraph, area);
    }

    fn render_category_filter_bar(&self, f: &mut Frame, area: Rect) {
        let categories = &self.config.categories;
        let current_filter = self.category_filter();

        let mut spans = vec![Span::raw(" ")];

        // "All" option
        if current_filter.is_none() {
            spans.push(Span::styled("[All]", Style::default().fg(Color::Cyan).bold()));
        } else {
            spans.push(Span::styled("All", Style::default().fg(Color::DarkGray)));
        }

        // Each category
        for cat in categories {
            spans.push(Span::raw("  "));
            let is_selected = current_filter == Some(cat.as_str());
            let cat_color = Self::category_color(cat);
            if is_selected {
                spans.push(Span::styled(format!("[{}]", cat), Style::default().fg(cat_color).bold()));
            } else {
                spans.push(Span::styled(cat.clone(), Style::default().fg(Color::DarkGray)));
            }
        }

        // Help hint
        spans.push(Span::styled("  (Tab to cycle)", Style::default().fg(Color::DarkGray)));

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(35, 35, 40)));
        f.render_widget(paragraph, area);
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
        let is_search = matches!(self.mode, DashboardMode::Search);
        let title = if is_search && !self.search_input.is_empty() {
            format!(" Worktrees ({} matches) ", self.filtered_indices.len())
        } else {
            " Worktrees ".to_string()
        };

        // Render the block and get inner area
        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        // Store viewport height for scroll calculations in move_selection()
        self.worktree_viewport_height = inner.height;

        if self.filtered_indices.is_empty() {
            return;
        }

        // Calculate item heights and cumulative Y positions
        let item_heights = self.calculate_item_heights();
        let y_positions = Self::cumulative_y_positions(&item_heights);

        // Ensure selected item is visible (adjust scroll if needed)
        // This handles the case where viewport was resized
        self.ensure_selected_visible();

        // Calculate available width for content (subtract highlight symbol and bar)
        let content_width = inner.width.saturating_sub(5) as usize; // "> " + "▌ "

        let animation_frame = self.animation_frame;
        let linear_icon = self.linear_icon();
        let github_icon = self.github_icon();
        let branch_icon = self.branch_icon();

        let scroll = self.worktree_scroll_offset;
        let viewport_height = inner.height;

        // Find first visible item (item where y + height > scroll)
        let first_visible = y_positions
            .iter()
            .enumerate()
            .position(|(i, &y)| y + item_heights[i] > scroll)
            .unwrap_or(0);

        // Render visible items with clipping
        let buf = f.buffer_mut();
        let highlight_style = Style::default().bg(Color::Rgb(40, 44, 52));

        let mut render_y = y_positions[first_visible] as i32 - scroll as i32;

        for (list_idx, &actual_idx) in self.filtered_indices[first_visible..].iter().enumerate() {
            let item_idx = first_visible + list_idx;
            if render_y >= viewport_height as i32 {
                break;
            }

            let wt = &self.worktrees[actual_idx];
            let linear_title = self
                .linear_issues
                .get(&wt.info.name)
                .map(|issue| issue.title.as_str());
            let github_pr = self
                .github_prs
                .get(&wt.info.name)
                .and_then(|state| match state {
                    CachedPRState::Found(pr) => Some(pr),
                    CachedPRState::NotFound => None,
                });
            let claude_state = self
                .claude_states
                .get(&wt.info.name)
                .map(|s| claude::effective_state(s));

            let lines = Self::format_worktree_lines(
                wt,
                content_width,
                linear_title,
                github_pr,
                claude_state,
                animation_frame,
                linear_icon,
                github_icon,
                branch_icon,
            );

            let is_selected = item_idx == self.selected;

            for (line_idx, line) in lines.iter().enumerate() {
                if render_y >= 0 && render_y < viewport_height as i32 {
                    let y = inner.y + render_y as u16;

                    // Apply highlight style to the entire row if selected
                    if is_selected {
                        // Fill the row with highlight background first
                        for x in inner.x..inner.x + inner.width {
                            buf[(x, y)].set_style(highlight_style);
                        }
                    }

                    // Render the selection indicator (only on first line of selected item)
                    let indicator = if is_selected && line_idx == 0 { "> " } else { "  " };
                    buf.set_string(inner.x, y, indicator, Style::default());

                    // Render the line content (after the indicator)
                    let line_x = inner.x + 2;
                    let mut x = line_x;
                    for span in &line.spans {
                        let style = if is_selected {
                            span.style.patch(highlight_style)
                        } else {
                            span.style
                        };
                        let remaining_width = (inner.x + inner.width).saturating_sub(x) as usize;
                        let text: String = span.content.chars().take(remaining_width).collect();
                        buf.set_string(x, y, &text, style);
                        x += text.len() as u16;
                    }
                }
                render_y += 1;
            }
        }
    }

    /// Format worktree item as a vector of lines (used by custom rendering)
    fn format_worktree_lines(
        wt: &WorktreeStats,
        width: usize,
        linear_title: Option<&str>,
        github_pr: Option<&GitHubPR>,
        claude_state: Option<ClaudeState>,
        animation_frame: u8,
        linear_icon: Option<&str>,
        github_icon: Option<&str>,
        branch_icon: Option<&str>,
    ) -> Vec<Line<'static>> {
        // Determine bar color based on Claude state
        let bar_color = match claude_state {
            None => Color::Rgb(60, 60, 60),
            Some(ClaudeState::Inactive) => Color::Rgb(60, 60, 60),
            Some(ClaudeState::Idle) => Color::Green,
            Some(ClaudeState::Working) => Color::Yellow,
            Some(ClaudeState::WaitingPermission) => Color::Red,
        };

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

        // Branch
        let branch_text = wt
            .info
            .branch
            .as_ref()
            .map(|b| match branch_icon {
                Some(icon) => format!("   {} {}", icon, b),
                None => format!("   {}", b),
            })
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

        // Line 2: age  [vs main bracket]  [vs self bracket]
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

        // Build vs main bracket
        let vs_main_bracket = Self::render_vs_main_bracket(wt);
        let has_main_bracket = !vs_main_bracket.is_empty();
        if has_main_bracket {
            line2_spans.push(Span::raw("  "));
            line2_spans.extend(vs_main_bracket);
        }

        // Build vs self bracket
        let vs_self_bracket = Self::render_vs_self_bracket(wt);
        let has_self_bracket = !vs_self_bracket.is_empty();
        if has_self_bracket {
            line2_spans.push(Span::raw("  "));
            line2_spans.extend(vs_self_bracket);
        }

        // Show "Clean" only if both brackets are empty and no uncommitted changes
        let is_clean = !has_main_bracket && !has_self_bracket && !wt.has_uncommitted_changes();

        if is_clean && line2_spans.len() <= 2 {
            // Only age was added
            line2_spans.push(Span::raw("  "));
            line2_spans.push(Span::styled("Clean", Style::default().fg(Color::Green)));
        }

        if !line2_spans.is_empty() {
            lines.push(Line::from(line2_spans));
        }

        // Line 3: Linear issue title (if available)
        if let Some(title) = linear_title {
            // Build prefix with optional icon
            let prefix = linear_icon
                .map(|i| format!("  {} ", i))
                .unwrap_or_else(|| "  ".to_string());
            let prefix_len = prefix.chars().count();
            // Truncate if too long (leave room for prefix and ellipsis)
            let max_len = width.saturating_sub(prefix_len + 3);
            let display_title = if title.len() > max_len {
                format!("{}{}...", prefix, &title[..max_len.saturating_sub(3)])
            } else {
                format!("{}{}", prefix, title)
            };
            lines.push(Line::from(Span::styled(
                display_title,
                Style::default().fg(Color::White).italic(),
            )));
        }

        // Line 4: GitHub PR indicator (if available, not for main branch)
        if !wt.info.is_main {
            if let Some(pr) = github_pr {
                let github_line = Self::format_github_indicator(pr, width, github_icon);
                lines.push(github_line);
            }
        }

        // Line 5: Claude status (only if active session)
        if let Some(ref state) = claude_state {
            let claude_line = match state {
                ClaudeState::Working => {
                    let spinner = SPINNER_FRAMES[(animation_frame as usize) % SPINNER_FRAMES.len()];
                    Some((format!("{} Claude working", spinner), Color::Yellow))
                }
                ClaudeState::Idle => Some(("\u{2713} Claude idle".to_string(), Color::Green)),
                ClaudeState::WaitingPermission => {
                    Some(("! Claude waiting for response".to_string(), Color::Red))
                }
                ClaudeState::Inactive => None, // Don't show line
            };

            if let Some((text, color)) = claude_line {
                lines.push(Line::from(Span::styled(
                    format!("  {}", text),
                    Style::default().fg(color),
                )));
            }
        }

        // Empty line for spacing
        lines.push(Line::from(""));

        // Prepend colored bar to each line
        lines
            .into_iter()
            .map(|line| {
                let mut spans = vec![Span::styled("▌ ", Style::default().fg(bar_color))];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect()
    }

    /// Format a compact GitHub PR indicator line for the worktree list.
    /// Format: `  [icon] OPEN  ✓approved  ✓3/3  @john`
    fn format_github_indicator(
        pr: &GitHubPR,
        width: usize,
        github_icon: Option<&str>,
    ) -> Line<'static> {
        let mut spans = Vec::new();

        // State with color - DRAFT shown instead of OPEN when applicable
        let (display_state, state_color) = if pr.draft {
            ("DRAFT".to_string(), Color::Yellow)
        } else {
            (
                pr.state.clone(),
                match pr.state.as_str() {
                    "OPEN" => Color::Green,
                    "MERGED" => Color::Magenta,
                    "CLOSED" => Color::Red,
                    _ => Color::DarkGray,
                },
            )
        };

        // Indent with optional icon (icon colored to match status)
        spans.push(Span::raw("  "));
        if let Some(icon) = github_icon {
            spans.push(Span::styled(
                format!("{} ", icon),
                Style::default().fg(state_color),
            ));
        }

        // State
        spans.push(Span::styled(display_state, Style::default().fg(state_color)));

        // Review decision with icon
        if let Some(ref decision) = pr.review_decision {
            spans.push(Span::raw("  "));
            let (icon, text, color) = match decision.as_str() {
                "APPROVED" => ("\u{2713}", "approved", Color::Green),
                "CHANGES_REQUESTED" => ("\u{2717}", "changes", Color::Red),
                "REVIEW_REQUIRED" => ("\u{25CB}", "review", Color::Rgb(255, 165, 0)), // Orange
                _ => ("", "", Color::DarkGray),
            };
            if !icon.is_empty() {
                spans.push(Span::styled(
                    format!("{}{}", icon, text),
                    Style::default().fg(color),
                ));
            }
        }

        // Checks status with icon
        if let Some(ref checks) = pr.checks_status {
            spans.push(Span::raw("  "));
            let total = checks.total;
            let passing = checks.passing;

            let (icon, color) = if checks.failing > 0 {
                ("\u{2717}", Color::Red)
            } else if checks.pending > 0 {
                ("\u{25CB}", Color::Yellow)
            } else if passing > 0 {
                ("\u{2713}", Color::Green)
            } else {
                ("-", Color::DarkGray)
            };

            spans.push(Span::styled(
                format!("{}{}/{}", icon, passing, total),
                Style::default().fg(color),
            ));
        }

        // First assignee, or author as fallback (truncated if needed)
        let user = pr
            .assignees
            .first()
            .map(|s| s.as_str())
            .unwrap_or_else(|| if pr.author.is_empty() { "" } else { &pr.author });
        if !user.is_empty() {
            spans.push(Span::raw("  "));
            // Calculate how much space we have left
            let current_len: usize = spans.iter().map(|s| s.content.len()).sum();
            let max_user_len = width.saturating_sub(current_len + 1); // +1 for @

            let display_name = if user.len() > max_user_len {
                if max_user_len > 3 {
                    format!("@{}...", &user[..max_user_len.saturating_sub(3)])
                } else {
                    format!("@{}", &user[..max_user_len])
                }
            } else {
                format!("@{}", user)
            };

            spans.push(Span::styled(display_name, Style::default().fg(Color::Cyan)));
        }

        Line::from(spans)
    }

    /// Render the "vs main" bracket: [↑N ↓N +X -Y vs main]
    /// For main branch: shows [↑N ↓N vs origin/main] comparing to origin/main
    /// For other branches: shows commits ahead/behind main and diff stats
    fn render_vs_main_bracket(wt: &WorktreeStats) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut has_content = false;

        spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));

        // Commits ahead (↑)
        if let Some(ahead) = wt.commits_ahead
            && ahead > 0
        {
            spans.push(Span::styled(
                format!("\u{2191}{}", ahead),
                Style::default().fg(Color::Yellow),
            ));
            has_content = true;
        }

        // Commits behind (↓)
        if let Some(behind) = wt.commits_behind
            && behind > 0
        {
            if has_content {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("\u{2193}{}", behind),
                Style::default().fg(Color::Magenta),
            ));
            has_content = true;
        }

        // Diff vs main (only for non-main branches)
        if !wt.info.is_main {
            if let (Some(added), Some(removed)) = (wt.diff_added, wt.diff_removed) {
                if added > 0 || removed > 0 {
                    if has_content {
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled(
                        format!("+{}", added),
                        Style::default().fg(Color::Green),
                    ));
                    spans.push(Span::styled(
                        format!(" -{}", removed),
                        Style::default().fg(Color::Red),
                    ));
                    has_content = true;
                }
            }
        }

        if has_content {
            // Use different label for main branch vs other branches
            let label = if wt.info.is_main {
                " vs origin/main"
            } else {
                " vs main"
            };
            spans.push(Span::styled(label, Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
            spans
        } else {
            // No content - return empty, don't show empty bracket
            Vec::new()
        }
    }

    /// Render the "vs self" bracket: [↑N ↓N +X -Y vs self]
    /// Shows unpushed commits (tracking_ahead) and uncommitted changes
    /// For main branch: only shows uncommitted changes (tracking is shown in vs origin bracket)
    fn render_vs_self_bracket(wt: &WorktreeStats) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut has_content = false;

        spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));

        // For non-main branches, show tracking info (ahead/behind origin/<branch>)
        // When no tracking branch exists, fall back to commits_ahead (vs main)
        // since those commits would all be pushed when creating the remote branch
        if !wt.info.is_main {
            // Unpushed commits: prefer tracking_ahead, fall back to commits_ahead if no tracking
            let unpushed = wt.tracking_ahead.or(wt.commits_ahead);
            if let Some(ahead) = unpushed
                && ahead > 0
            {
                spans.push(Span::styled(
                    format!("\u{2191}{}", ahead),
                    Style::default().fg(Color::Yellow),
                ));
                has_content = true;
            }

            // Behind tracking branch (only shown if tracking exists)
            if let Some(behind) = wt.tracking_behind
                && behind > 0
            {
                if has_content {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(
                    format!("\u{2193}{}", behind),
                    Style::default().fg(Color::Magenta),
                ));
                has_content = true;
            }
        }

        // Uncommitted changes (for all branches)
        if wt.uncommitted_added > 0 || wt.uncommitted_removed > 0 {
            if has_content {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("+{}", wt.uncommitted_added),
                Style::default().fg(Color::Green),
            ));
            spans.push(Span::styled(
                format!(" -{}", wt.uncommitted_removed),
                Style::default().fg(Color::Red),
            ));
            has_content = true;
        }

        if has_content {
            spans.push(Span::styled(
                " vs self",
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
            spans
        } else {
            // No content - return empty, don't show empty bracket
            Vec::new()
        }
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

    fn render_linear_panel(&self, f: &mut Frame, area: Rect) {
        // Get the selected worktree info
        let worktree = self.get_selected_worktree();
        let worktree_name = worktree.as_ref().map(|wt| wt.info.name.as_str());
        let issue = worktree
            .as_ref()
            .and_then(|wt| self.linear_issues.get(&wt.info.name));
        let is_loading = worktree_name
            .map(|name| self.is_linear_loading(name))
            .unwrap_or(false);

        let is_main = worktree.as_ref().map(|wt| wt.info.is_main).unwrap_or(true);

        let block = Block::default().borders(Borders::ALL).title(" Linear ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content: Text = if is_main {
            Text::styled(
                "Main branch - no Linear issue",
                Style::default().fg(Color::DarkGray).italic(),
            )
        } else if is_loading && issue.is_none() {
            Text::styled("Loading...", Style::default().fg(Color::Yellow).italic())
        } else {
            match issue {
                Some(issue) => {
                    let mut lines = Vec::new();

                    // Issue title in bold white
                    lines.push(Line::from(Span::styled(
                        &issue.title,
                        Style::default().fg(Color::White).bold(),
                    )));

                    // Issue ID with hint to open
                    lines.push(Line::from(vec![
                        Span::styled(&issue.id, Style::default().fg(Color::Cyan)),
                        Span::styled(" (l to open)", Style::default().fg(Color::DarkGray)),
                    ]));

                    // Empty line before description
                    lines.push(Line::from(""));

                    // Description or "No description"
                    if let Some(ref desc) = issue.description {
                        // Word wrap the description
                        for line in desc.lines() {
                            if line.is_empty() {
                                lines.push(Line::from(""));
                            } else {
                                lines.push(Line::from(Span::styled(
                                    line,
                                    Style::default().fg(Color::Gray),
                                )));
                            }
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            "No description",
                            Style::default().fg(Color::DarkGray).italic(),
                        )));
                    }

                    Text::from(lines)
                }
                None => Text::styled(
                    "No Linear issue linked\n\nCreate worktrees with Linear issue IDs\nto see issue details here.",
                    Style::default().fg(Color::DarkGray).italic(),
                ),
            }
        };

        let paragraph = Paragraph::new(content).wrap(Wrap { trim: true });
        f.render_widget(paragraph, inner);
    }

    fn render_github_panel(&self, f: &mut Frame, area: Rect) {
        // Get the selected worktree info
        let worktree = self.get_selected_worktree();
        let worktree_name = worktree.as_ref().map(|wt| wt.info.name.as_str());
        let cached_state = worktree
            .as_ref()
            .and_then(|wt| self.github_prs.get(&wt.info.name));
        let is_loading = worktree_name
            .map(|name| self.is_github_loading(name))
            .unwrap_or(false);

        let is_main = worktree.as_ref().map(|wt| wt.info.is_main).unwrap_or(true);

        let block = Block::default().borders(Borders::ALL).title(" GitHub ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content: Text = if is_main {
            Text::styled(
                "Main branch - no PR",
                Style::default().fg(Color::DarkGray).italic(),
            )
        } else if is_loading && cached_state.is_none() {
            // Only show "Loading..." when there's no cached state at all (first load)
            Text::styled("Loading...", Style::default().fg(Color::Yellow).italic())
        } else {
            match cached_state {
                Some(CachedPRState::Found(pr)) => self.format_github_pr_content(pr, inner.width),
                Some(CachedPRState::NotFound) | None => Text::styled(
                    "No PR found\n\nPress 'g' to create a PR",
                    Style::default().fg(Color::DarkGray).italic(),
                ),
            }
        };

        let paragraph = Paragraph::new(content).wrap(Wrap { trim: true });
        f.render_widget(paragraph, inner);
    }

    fn format_github_pr_content(
        &self,
        pr: &crate::github::GitHubPR,
        inner_width: u16,
    ) -> Text<'static> {
        let mut lines = Vec::new();

        // PR title in bold white
        lines.push(Line::from(Span::styled(
            pr.title.clone(),
            Style::default().fg(Color::White).bold(),
        )));

        // PR number and state with hint to open - DRAFT shown instead of OPEN when applicable
        let (display_state, state_color) = if pr.draft {
            ("DRAFT".to_string(), Color::Yellow)
        } else {
            (
                pr.state.clone(),
                match pr.state.as_str() {
                    "OPEN" => Color::Green,
                    "MERGED" => Color::Magenta,
                    "CLOSED" => Color::Red,
                    _ => Color::DarkGray,
                },
            )
        };
        lines.push(Line::from(vec![
            Span::styled(format!("#{}", pr.number), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!(" {}", display_state),
                Style::default().fg(state_color),
            ),
            Span::styled(" (g to open)", Style::default().fg(Color::DarkGray)),
        ]));

        // Assignees and reviewers
        if !pr.assignees.is_empty() || !pr.reviewers.is_empty() {
            if !pr.assignees.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Assignees: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        pr.assignees
                            .iter()
                            .map(|a| format!("@{}", a))
                            .collect::<Vec<_>>()
                            .join(", "),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
            }
            if !pr.reviewers.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Reviewers: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        pr.reviewers
                            .iter()
                            .map(|r| format!("@{}", r))
                            .collect::<Vec<_>>()
                            .join(", "),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
            }
        }

        // Empty line before review/checks
        lines.push(Line::from(""));

        // Review decision (only show if a known decision exists)
        if let Some(ref decision) = pr.review_decision {
            let review_line = match decision.as_str() {
                "APPROVED" => Some(("✓", "Approved", Color::Green)),
                "CHANGES_REQUESTED" => Some(("✗", "Changes requested", Color::Red)),
                "REVIEW_REQUIRED" => Some(("○", "Review required", Color::Rgb(255, 165, 0))), // Orange
                _ => None,
            };
            if let Some((icon, text, color)) = review_line {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    Span::styled(text, Style::default().fg(color)),
                ]));
            }
        }

        // CI Status
        if let Some(ref checks) = pr.checks_status {
            let status_line = if checks.failing > 0 {
                Line::from(vec![
                    Span::styled("✗ ", Style::default().fg(Color::Red)),
                    Span::styled(
                        format!("{} failing", checks.failing),
                        Style::default().fg(Color::Red),
                    ),
                    Span::styled(
                        format!(", {} passing, {} pending", checks.passing, checks.pending),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            } else if checks.pending > 0 {
                Line::from(vec![
                    Span::styled("○ ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("{} pending", checks.pending),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!(", {} passing", checks.passing),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            } else if checks.passing > 0 {
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(Color::Green)),
                    Span::styled(
                        format!("{} checks passing", checks.passing),
                        Style::default().fg(Color::Green),
                    ),
                ])
            } else {
                Line::from(Span::styled(
                    "No checks",
                    Style::default().fg(Color::DarkGray),
                ))
            };
            lines.push(status_line);

            // Show failing check names
            for name in &checks.failing_names {
                lines.push(Line::from(vec![
                    Span::styled("  ✗ ", Style::default().fg(Color::Red)),
                    Span::styled(name.clone(), Style::default().fg(Color::Red)),
                ]));
            }
        }

        // Recent comments
        if !pr.comments.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Recent comments:",
                Style::default().fg(Color::DarkGray),
            )));

            for comment in pr.comments.iter().take(3) {
                // Truncate body to fit
                let max_len = (inner_width as usize).saturating_sub(4);
                let body_preview = if comment.body.len() > max_len {
                    format!("{}...", &comment.body[..max_len.saturating_sub(3)])
                } else {
                    comment.body.clone()
                };
                // Replace newlines with spaces for preview
                let body_preview = body_preview.replace('\n', " ");

                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  @{}: ", comment.author),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(body_preview, Style::default().fg(Color::Gray)),
                ]));
            }
        }

        Text::from(lines)
    }

    fn render_claude_pane(&self, f: &mut Frame, area: Rect) {
        // Get the selected worktree's Claude session
        let session = self
            .get_selected_worktree()
            .and_then(|wt| self.claude_states.get(&wt.info.name));

        // Build title with state indicator
        let title = match session {
            Some(s) => {
                let effective_state = claude::effective_state(s);
                let (icon, color) = match effective_state {
                    ClaudeState::Working => ("\u{2699}", Color::Yellow), // ⚙
                    ClaudeState::Idle => ("\u{2713}", Color::Green),     // ✓
                    ClaudeState::WaitingPermission => ("!", Color::Red),
                    ClaudeState::Inactive => ("-", Color::DarkGray),
                };
                Line::from(vec![
                    Span::styled(" Claude ", Style::default().fg(Color::Rgb(194, 76, 48))),
                    Span::styled(icon, Style::default().fg(color)),
                    Span::styled(format!(" {} ", effective_state), Style::default().fg(color)),
                ])
            }
            None => Line::from(Span::styled(
                " Claude ",
                Style::default().fg(Color::Rgb(154, 76, 48)),
            )),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(154, 76, 48)))
            .title(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content: Text = match session {
            Some(s) if !s.events.is_empty() => self.format_claude_events(s, inner),
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

    fn format_claude_events(
        &self,
        session: &crate::claude::ClaudeSession,
        inner: Rect,
    ) -> Text<'static> {
        let mut lines = Vec::new();

        // Show recent events (most recent first, fill available height)
        // Track index to know if notification is "handled" (not the most recent)
        // Filter out ToolResult events - they're used for state tracking but pollute the log
        let max_events = inner.height as usize;
        for (idx, event) in session
            .events
            .iter()
            .rev()
            .filter(|e| e.event_type != "ToolResult")
            .take(max_events)
            .enumerate()
        {
            let time_str = claude::relative_time(&event.timestamp);
            let is_most_recent = idx == 0;

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
                        Span::styled("Completed".to_string(), Style::default().fg(Color::Green)),
                    ]
                }
                "Notification" => {
                    let kind = event.kind.as_deref().unwrap_or("notification");
                    // Show in color if most recent, gray if handled (not most recent)
                    let (icon, color) = if !is_most_recent {
                        // Handled - show in gray
                        ("\u{25CF}", Color::DarkGray) // ●
                    } else if kind.contains("permission") {
                        ("!", Color::Red)
                    } else {
                        ("\u{25CF}", Color::Cyan) // ●
                    };
                    let mut spans = vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(format!("{} ", icon), Style::default().fg(color)),
                        Span::styled(kind.to_string(), Style::default().fg(color)),
                    ];
                    // Add message if available
                    if let Some(ref msg) = event.message {
                        let max_msg_len =
                            inner.width.saturating_sub(14 + kind.len() as u16) as usize;
                        let truncated = if msg.len() > max_msg_len {
                            format!("{}...", &msg[..max_msg_len.saturating_sub(3)])
                        } else {
                            msg.clone()
                        };
                        spans.push(Span::styled(
                            format!(": {}", truncated),
                            Style::default().fg(color),
                        ));
                    }
                    spans
                }
                "ToolUse" => {
                    let tool = event.prompt_preview.as_deref().unwrap_or("tool");
                    let mut spans = vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled("\u{2699} ", Style::default().fg(Color::DarkGray)), // ⚙
                        Span::styled(tool.to_string(), Style::default().fg(Color::DarkGray)),
                    ];
                    // Add detail if available (file path, command, etc.)
                    if let Some(ref detail) = event.message {
                        let max_detail_len =
                            inner.width.saturating_sub(14 + tool.len() as u16) as usize;
                        let truncated = if detail.len() > max_detail_len {
                            format!("{}...", &detail[..max_detail_len.saturating_sub(3)])
                        } else {
                            detail.clone()
                        };
                        spans.push(Span::styled(
                            format!(" {}", truncated),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                    spans
                }
                "SessionStart" => {
                    vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled("\u{25B7} ", Style::default().fg(Color::Cyan)), // ▷
                        Span::styled(
                            "Session started".to_string(),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]
                }
                "SessionCleared" => {
                    vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled("\u{21BB} ", Style::default().fg(Color::Yellow)), // ↻
                        Span::styled(
                            "Session cleared".to_string(),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]
                }
                "SessionEnd" => {
                    vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled("\u{25A1} ", Style::default().fg(Color::Red)), // □
                        Span::styled("Session ended".to_string(), Style::default().fg(Color::Red)),
                    ]
                }
                _ => {
                    vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::raw(event.event_type.clone()),
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

    fn render_help(&self, f: &mut Frame, area: Rect) {
        let help_text = match &self.mode {
            DashboardMode::Normal => {
                let selected = self.get_selected_worktree();
                let is_main = selected.as_ref().map(|w| w.info.is_main).unwrap_or(true);
                let has_linear = self.config.linear.enabled && selected
                    .as_ref()
                    .map(|w| self.linear_issues.contains_key(&w.info.name))
                    .unwrap_or(false);

                let mut spans = vec![
                    Span::styled(" \u{2191}/\u{2193}", Style::default().fg(Color::Cyan)),
                    Span::styled(": nav  ", Style::default().fg(Color::DarkGray)),
                ];

                // Show Tab: filter only when category filter is available
                if self.show_category_filter() {
                    spans.extend(vec![
                        Span::styled("Tab", Style::default().fg(Color::Cyan)),
                        Span::styled(": filter  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                spans.extend(vec![
                    Span::styled("/", Style::default().fg(Color::Cyan)),
                    Span::styled(": search  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Enter", Style::default().fg(Color::Cyan)),
                    Span::styled(": switch  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("n", Style::default().fg(Color::Cyan)),
                    Span::styled(": new  ", Style::default().fg(Color::DarkGray)),
                ]);

                // Only show d/x for non-main worktrees, m only in push workflow
                if !is_main {
                    // Only show d: diff if diffview is enabled
                    if self.config.diffview.enabled {
                        spans.extend(vec![
                            Span::styled("d", Style::default().fg(Color::Cyan)),
                            Span::styled(": diff  ", Style::default().fg(Color::DarkGray)),
                        ]);
                    }
                    // Only show merge in push workflow
                    if !self.is_pull_workflow() {
                        spans.extend(vec![
                            Span::styled("m", Style::default().fg(Color::Cyan)),
                            Span::styled(": merge  ", Style::default().fg(Color::DarkGray)),
                        ]);
                    }
                    spans.extend(vec![
                        Span::styled("x", Style::default().fg(Color::Cyan)),
                        Span::styled(": delete  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                // Show s: sync for all worktrees (contextual push/pull with remote)
                spans.extend(vec![
                    Span::styled("s", Style::default().fg(Color::Cyan)),
                    Span::styled(": sync  ", Style::default().fg(Color::DarkGray)),
                ]);

                // Show c: claude only if Claude is enabled
                if self.config.claude.enabled {
                    spans.extend(vec![
                        Span::styled("c", Style::default().fg(Color::Cyan)),
                        Span::styled(": claude  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                // Show l: linear only if worktree has a Linear issue and Linear is enabled
                if has_linear {
                    spans.extend(vec![
                        Span::styled("l", Style::default().fg(Color::Cyan)),
                        Span::styled(": linear  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                // Show g: github only for non-main worktrees and if GitHub is enabled
                if !is_main && self.config.github.enabled {
                    spans.extend(vec![
                        Span::styled("g", Style::default().fg(Color::Cyan)),
                        Span::styled(": github  ", Style::default().fg(Color::DarkGray)),
                    ]);
                }

                // Show P: prune only in pull workflow with GitHub enabled
                if self.is_pull_workflow() && self.config.github.enabled {
                    spans.extend(vec![
                        Span::styled("P", Style::default().fg(Color::Cyan)),
                        Span::styled(": prune  ", Style::default().fg(Color::DarkGray)),
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
