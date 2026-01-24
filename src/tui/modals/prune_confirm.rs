//! Modal for confirming worktree pruning (bulk deletion of merged PRs).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use super::{Modal, ModalAction};
use crate::tui::widgets::Checkbox;

/// Information about a worktree to be pruned
#[derive(Debug, Clone)]
pub struct PruneWorktreeInfo {
    pub name: String,
    pub pr_number: u32,
    pub pr_title: String,
    pub has_uncommitted: bool,
    pub uncommitted_added: u32,
    pub uncommitted_removed: u32,
}

/// Modal for confirming prune operation
pub struct PruneConfirmModal {
    worktrees: Vec<PruneWorktreeInfo>,
    /// Scroll offset for the worktree list
    scroll_offset: usize,
    /// Checkbox to acknowledge data loss (only shown if any dirty worktrees)
    acknowledge_checkbox: Checkbox,
    /// Whether any worktree has uncommitted changes
    has_dirty: bool,
}

impl PruneConfirmModal {
    pub fn new(worktrees: Vec<PruneWorktreeInfo>) -> Self {
        let has_dirty = worktrees.iter().any(|w| w.has_uncommitted);
        Self {
            worktrees,
            scroll_offset: 0,
            acknowledge_checkbox: Checkbox::new("I understand changes will be lost", false),
            has_dirty,
        }
    }

    /// Get the worktrees to be pruned
    pub fn worktrees(&self) -> &[PruneWorktreeInfo] {
        &self.worktrees
    }

    /// Check if deletion can proceed (dirty worktrees require acknowledgment)
    fn can_delete(&self) -> bool {
        !self.has_dirty || self.acknowledge_checkbox.is_checked()
    }

    /// Scroll the list up
    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll the list down
    fn scroll_down(&mut self) {
        let max_offset = self.worktrees.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + 1).min(max_offset);
    }
}

impl Modal for PruneConfirmModal {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ModalAction> {
        match key.code {
            KeyCode::Esc => Some(ModalAction::Cancel),
            KeyCode::Enter => {
                if self.can_delete() {
                    Some(ModalAction::Prune)
                } else {
                    None
                }
            }
            KeyCode::Char(' ') => {
                if self.has_dirty {
                    self.acknowledge_checkbox.toggle();
                }
                None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up();
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_down();
                None
            }
            _ => None,
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.has_dirty {
            Color::Yellow
        } else {
            Color::Red
        };

        let title = format!(" Prune {} Worktree(s) ", self.worktrees.len());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title)
            .title_style(Style::default().fg(border_color).bold());

        let inner = block.inner(area);
        block.render(area, buf);

        // Calculate layout
        let mut constraints = vec![
            Constraint::Length(1), // spacer
            Constraint::Length(1), // header
            Constraint::Length(1), // spacer
        ];

        // Worktree list area (flexible)
        let list_height = if self.has_dirty {
            inner.height.saturating_sub(9) // Leave room for warning + checkbox + help
        } else {
            inner.height.saturating_sub(5) // Just header + help
        };
        constraints.push(Constraint::Length(list_height));

        if self.has_dirty {
            constraints.push(Constraint::Length(1)); // spacer
            constraints.push(Constraint::Length(1)); // warning
            constraints.push(Constraint::Length(1)); // checkbox
        }

        constraints.push(Constraint::Length(1)); // spacer
        constraints.push(Constraint::Length(1)); // help

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut chunk_idx = 1; // Skip first spacer

        // Header
        let dirty_count = self.worktrees.iter().filter(|w| w.has_uncommitted).count();
        let header_text = if dirty_count > 0 {
            format!(
                "The following worktrees will be deleted ({} with uncommitted changes):",
                dirty_count
            )
        } else {
            "The following worktrees will be deleted:".to_string()
        };
        let header = Paragraph::new(header_text).style(Style::default().fg(Color::White));
        header.render(chunks[chunk_idx], buf);
        chunk_idx += 2; // Skip header and spacer

        // Worktree list with scrolling
        let list_area = chunks[chunk_idx];
        let visible_items = list_area.height as usize;
        let total_items = self.worktrees.len();

        // Render visible worktrees
        for (i, wt) in self
            .worktrees
            .iter()
            .skip(self.scroll_offset)
            .take(visible_items)
            .enumerate()
        {
            let y = list_area.y + i as u16;
            if y >= list_area.y + list_area.height {
                break;
            }

            let mut spans = vec![
                Span::raw("  "),
                Span::styled(&wt.name, Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!(" (PR #{})", wt.pr_number),
                    Style::default().fg(Color::Cyan),
                ),
            ];

            if wt.has_uncommitted {
                spans.push(Span::styled(" !", Style::default().fg(Color::Yellow)));
                spans.push(Span::styled(
                    format!("+{}", wt.uncommitted_added),
                    Style::default().fg(Color::Green),
                ));
                spans.push(Span::styled(
                    format!(" -{}", wt.uncommitted_removed),
                    Style::default().fg(Color::Red),
                ));
            }

            let line = Line::from(spans);
            buf.set_line(list_area.x, y, &line, list_area.width);
        }

        // Render scrollbar if needed
        if total_items > visible_items {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            let mut scrollbar_state = ScrollbarState::new(total_items)
                .position(self.scroll_offset)
                .viewport_content_length(visible_items);
            scrollbar.render(list_area, buf, &mut scrollbar_state);
        }

        chunk_idx += 1;

        // Warning section (only for dirty worktrees)
        if self.has_dirty {
            chunk_idx += 1; // spacer

            // Warning line
            let warning = Paragraph::new(Line::from(vec![
                Span::styled("! ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    "Some worktrees have uncommitted changes!",
                    Style::default().fg(Color::Yellow),
                ),
            ]));
            warning.render(chunks[chunk_idx], buf);
            chunk_idx += 1;

            // Acknowledge checkbox
            self.acknowledge_checkbox
                .render(chunks[chunk_idx], buf, true, true);
            chunk_idx += 1;
        }

        chunk_idx += 1; // spacer

        // Help text
        let help_spans = if self.has_dirty && !self.can_delete() {
            vec![
                Span::styled("Space", Style::default().fg(Color::Cyan)),
                Span::styled(": acknowledge    ", Style::default().fg(Color::DarkGray)),
                Span::styled("j/k", Style::default().fg(Color::Cyan)),
                Span::styled(": scroll    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(": prune    ", Style::default().fg(Color::DarkGray)),
                Span::styled("j/k", Style::default().fg(Color::Cyan)),
                Span::styled(": scroll    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
            ]
        };
        let help = Paragraph::new(Line::from(help_spans));
        help.render(chunks[chunk_idx], buf);
    }

    fn preferred_size(&self) -> (u16, u16) {
        // Width: enough for PR titles
        let width = 60u16;

        // Height: header (3) + min list items (5) + help (2) + dirty section (3 if needed)
        let base_height = 10u16;
        let dirty_extra = if self.has_dirty { 3 } else { 0 };
        let list_extra = (self.worktrees.len() as u16).min(8).saturating_sub(5);

        let height = base_height + dirty_extra + list_extra;

        (width, height)
    }
}
