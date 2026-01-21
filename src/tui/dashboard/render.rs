//! Rendering logic for the dashboard.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::claude::{self, ClaudeState};
use crate::git::WorktreeStats;
use crate::github::GitHubPR;
use crate::tui::modals::render_modal_overlay;

use super::state::DashboardMode;
use super::{Dashboard, SPINNER_FRAMES};

impl Dashboard {
    pub(super) fn render(&mut self, f: &mut Frame) {
        let is_search_mode = matches!(self.mode, DashboardMode::Search);

        // Layout: Title | [Search] | Top row (50%) | Bottom row (50%) | Help
        let constraints = if is_search_mode {
            vec![
                Constraint::Length(1),    // Title bar
                Constraint::Length(3),    // Search bar
                Constraint::Percentage(50), // Top row (Worktrees + Claude)
                Constraint::Percentage(50), // Bottom row (Commits + GitHub + Linear)
                Constraint::Length(1),    // Help bar
            ]
        } else {
            vec![
                Constraint::Length(1),    // Title bar
                Constraint::Percentage(50), // Top row (Worktrees + Claude)
                Constraint::Percentage(50), // Bottom row (Commits + GitHub + Linear)
                Constraint::Length(1),    // Help bar
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints.clone())
            .split(f.area());

        let (title_area, top_row_area, bottom_row_area, help_area) = if is_search_mode {
            (chunks[0], chunks[2], chunks[3], chunks[4])
        } else {
            (chunks[0], chunks[1], chunks[2], chunks[3])
        };

        self.render_title_bar(f, title_area);

        if is_search_mode {
            self.render_search_bar(f, chunks[1]);
        }

        // Main content
        if self.filtered_indices.is_empty() {
            self.render_empty(f, top_row_area);
        } else {
            // Top row: Worktrees (50%) | Claude (50%)
            let top_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(top_row_area);

            self.render_worktree_list(f, top_chunks[0]);
            self.render_claude_pane(f, top_chunks[1]);

            // Bottom row: Commits (33%) | GitHub (33%) | Linear (34%)
            let bottom_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                ])
                .split(bottom_row_area);

            self.render_commit_preview(f, bottom_chunks[0]);
            self.render_github_panel(f, bottom_chunks[1]);
            self.render_linear_panel(f, bottom_chunks[2]);
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
        // Calculate available width for content (subtract borders, highlight symbol, and bar)
        let content_width = area.width.saturating_sub(7) as usize; // 2 borders + "> " + "▌ "

        let animation_frame = self.animation_frame;
        let items: Vec<ListItem> = self
            .filtered_indices
            .iter()
            .map(|&i| {
                let wt = &self.worktrees[i];
                let linear_title = self
                    .linear_issues
                    .get(&wt.info.name)
                    .map(|issue| issue.title.as_str());
                let github_pr = self.github_prs.get(&wt.info.name);
                let claude_state = self
                    .claude_states
                    .get(&wt.info.name)
                    .map(|s| claude::effective_state(s));
                Self::format_worktree_item(
                    wt,
                    content_width,
                    linear_title,
                    github_pr,
                    claude_state,
                    animation_frame,
                )
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
        github_pr: Option<&GitHubPR>,
        claude_state: Option<ClaudeState>,
        animation_frame: u8,
    ) -> ListItem<'static> {
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
            .map(|b| format!("   {}", b))
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
        let is_clean = !has_main_bracket
            && !has_self_bracket
            && !wt.has_uncommitted_changes();

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
            // Truncate if too long (leave room for indent and ellipsis)
            let max_len = width.saturating_sub(6);
            let display_title = if title.len() > max_len {
                format!("  {}...", &title[..max_len.saturating_sub(3)])
            } else {
                format!("  {}", title)
            };
            lines.push(Line::from(Span::styled(
                display_title,
                Style::default().fg(Color::White).italic(),
            )));
        }

        // Line 4: GitHub PR indicator (if available, not for main branch)
        if !wt.info.is_main {
            if let Some(pr) = github_pr {
                let github_line = Self::format_github_indicator(pr, width);
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
        let lines: Vec<Line> = lines
            .into_iter()
            .map(|line| {
                let mut spans = vec![Span::styled("▌ ", Style::default().fg(bar_color))];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect();

        ListItem::new(lines)
    }

    /// Format a compact GitHub PR indicator line for the worktree list.
    /// Format: `  #123 OPEN  ✓approved  ✓3/3  @john`
    fn format_github_indicator(pr: &GitHubPR, width: usize) -> Line<'static> {
        let mut spans = Vec::new();

        // Indent
        spans.push(Span::raw("  "));

        // PR number (cyan)
        spans.push(Span::styled(
            format!("#{}", pr.number),
            Style::default().fg(Color::Cyan),
        ));

        spans.push(Span::raw(" "));

        // State with color (OPEN/MERGED/CLOSED)
        let state_color = match pr.state.as_str() {
            "OPEN" => Color::Green,
            "MERGED" => Color::Magenta,
            "CLOSED" => Color::Red,
            _ => Color::DarkGray,
        };
        spans.push(Span::styled(
            pr.state.clone(),
            Style::default().fg(state_color),
        ));

        // Draft indicator (if applicable)
        if pr.draft {
            spans.push(Span::styled(
                " (draft)",
                Style::default().fg(Color::DarkGray),
            ));
        }

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
        let user = pr.assignees.first().map(|s| s.as_str()).unwrap_or_else(|| {
            if pr.author.is_empty() { "" } else { &pr.author }
        });
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

            spans.push(Span::styled(
                display_name,
                Style::default().fg(Color::Cyan),
            ));
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
            spans.push(Span::styled(" vs self", Style::default().fg(Color::DarkGray)));
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

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Commits "));

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

        let is_main = worktree
            .as_ref()
            .map(|wt| wt.info.is_main)
            .unwrap_or(true);

        let block = Block::default().borders(Borders::ALL).title(" Linear ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content: Text = if is_main {
            Text::styled(
                "Main branch - no Linear issue",
                Style::default().fg(Color::DarkGray).italic(),
            )
        } else if is_loading && issue.is_none() {
            Text::styled(
                "Loading...",
                Style::default().fg(Color::Yellow).italic(),
            )
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
        let pr = worktree
            .as_ref()
            .and_then(|wt| self.github_prs.get(&wt.info.name));
        let is_loading = worktree_name
            .map(|name| self.is_github_loading(name))
            .unwrap_or(false);

        let is_main = worktree
            .as_ref()
            .map(|wt| wt.info.is_main)
            .unwrap_or(true);

        let block = Block::default().borders(Borders::ALL).title(" GitHub ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content: Text = if is_main {
            Text::styled(
                "Main branch - no PR",
                Style::default().fg(Color::DarkGray).italic(),
            )
        } else if is_loading && pr.is_none() {
            Text::styled(
                "Loading...",
                Style::default().fg(Color::Yellow).italic(),
            )
        } else {
            match pr {
                Some(pr) => self.format_github_pr_content(pr, inner.width),
                None => Text::styled(
                    "No PR found\n\nPress 'g' to create a PR",
                    Style::default().fg(Color::DarkGray).italic(),
                ),
            }
        };

        let paragraph = Paragraph::new(content).wrap(Wrap { trim: true });
        f.render_widget(paragraph, inner);
    }

    fn format_github_pr_content(&self, pr: &crate::github::GitHubPR, inner_width: u16) -> Text<'static> {
        let mut lines = Vec::new();

        // PR title in bold white
        lines.push(Line::from(Span::styled(
            pr.title.clone(),
            Style::default().fg(Color::White).bold(),
        )));

        // PR number and state with hint to open
        let state_color = match pr.state.as_str() {
            "OPEN" => Color::Green,
            "MERGED" => Color::Magenta,
            "CLOSED" => Color::Red,
            _ => Color::DarkGray,
        };
        let draft_text = if pr.draft { " (draft)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!("#{}", pr.number), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!(" {}{}", pr.state, draft_text),
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
                        pr.assignees.iter().map(|a| format!("@{}", a)).collect::<Vec<_>>().join(", "),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
            }
            if !pr.reviewers.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Reviewers: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        pr.reviewers.iter().map(|r| format!("@{}", r)).collect::<Vec<_>>().join(", "),
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
                        format!(
                            ", {} passing, {} pending",
                            checks.passing, checks.pending
                        ),
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
            Some(s) if !s.events.is_empty() => {
                self.format_claude_events(s, inner)
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

    fn format_claude_events(&self, session: &crate::claude::ClaudeSession, inner: Rect) -> Text<'static> {
        let mut lines = Vec::new();

        // Show recent events (most recent first, fill available height)
        // Track index to know if notification is "handled" (not the most recent)
        let max_events = inner.height as usize;
        for (idx, event) in session.events.iter().rev().take(max_events).enumerate() {
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
                        Span::styled("Session started".to_string(), Style::default().fg(Color::Cyan)),
                    ]
                }
                "SessionCleared" => {
                    vec![
                        Span::styled(
                            format!("{:>8} ", time_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled("\u{21BB} ", Style::default().fg(Color::Yellow)), // ↻
                        Span::styled("Session cleared".to_string(), Style::default().fg(Color::Yellow)),
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
                let has_linear = selected
                    .as_ref()
                    .map(|w| self.linear_issues.contains_key(&w.info.name))
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

                // Show s: sync for all worktrees (contextual push/pull with remote)
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

                // Show g: github only for non-main worktrees
                if !is_main {
                    spans.extend(vec![
                        Span::styled("g", Style::default().fg(Color::Cyan)),
                        Span::styled(": github  ", Style::default().fg(Color::DarkGray)),
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
