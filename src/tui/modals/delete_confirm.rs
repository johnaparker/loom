use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::{Modal, ModalAction};
use crate::git::WorktreeStats;
use crate::tui::widgets::Checkbox;

/// Modal for confirming worktree deletion
pub struct DeleteConfirmModal {
    worktree: WorktreeStats,
    delete_branch_checkbox: Checkbox,
    /// Checkbox to acknowledge data loss (only shown for dirty worktrees)
    acknowledge_checkbox: Checkbox,
    /// Whether the worktree has uncommitted changes
    is_dirty: bool,
    /// Which checkbox is focused (0 = delete_branch, 1 = acknowledge)
    focused_checkbox: usize,
}

impl DeleteConfirmModal {
    pub fn new(worktree: WorktreeStats) -> Self {
        let has_branch = worktree.info.branch.is_some();
        let is_dirty = worktree.has_uncommitted_changes();
        // Focus acknowledge checkbox first if dirty (it's at index 1 if branch exists, else 0)
        let focused_checkbox = if is_dirty {
            if has_branch { 1 } else { 0 }
        } else {
            0
        };
        Self {
            is_dirty,
            delete_branch_checkbox: Checkbox::new("Also delete branch", true),
            acknowledge_checkbox: Checkbox::new("I understand changes will be lost", false),
            focused_checkbox,
            worktree,
        }
    }

    /// Check if deletion can proceed (dirty worktrees require acknowledgment)
    fn can_delete(&self) -> bool {
        !self.is_dirty || self.acknowledge_checkbox.is_checked()
    }

    /// Get the number of focusable checkboxes
    fn checkbox_count(&self) -> usize {
        let has_branch = self.worktree.info.branch.is_some();
        match (has_branch, self.is_dirty) {
            (true, true) => 2,   // Both checkboxes
            (true, false) => 1,  // Only delete branch
            (false, true) => 1,  // Only acknowledge
            (false, false) => 0, // No checkboxes
        }
    }

    /// Move focus to the next checkbox
    fn focus_next(&mut self) {
        let count = self.checkbox_count();
        if count > 1 {
            self.focused_checkbox = (self.focused_checkbox + 1) % count;
        }
    }

    /// Toggle the currently focused checkbox
    fn toggle_focused(&mut self) {
        let has_branch = self.worktree.info.branch.is_some();
        match (has_branch, self.is_dirty, self.focused_checkbox) {
            (true, _, 0) => self.delete_branch_checkbox.toggle(),
            (true, true, 1) => self.acknowledge_checkbox.toggle(),
            (false, true, _) => self.acknowledge_checkbox.toggle(),
            _ => {}
        }
    }
}

impl Modal for DeleteConfirmModal {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ModalAction> {
        match key.code {
            KeyCode::Esc => Some(ModalAction::Cancel),
            KeyCode::Enter => {
                if self.can_delete() {
                    Some(ModalAction::Delete {
                        delete_branch: self.delete_branch_checkbox.is_checked(),
                        force: self.is_dirty,
                    })
                } else {
                    // Can't delete yet - need to acknowledge
                    None
                }
            }
            KeyCode::Char(' ') => {
                self.toggle_focused();
                None
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
                self.focus_next();
                None
            }
            _ => None,
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(" Delete Worktree ")
            .title_style(Style::default().fg(Color::Red).bold());

        let inner = block.inner(area);
        block.render(area, buf);

        let has_branch = self.worktree.info.branch.is_some();

        // Build layout constraints based on what we need to show
        let mut constraints = vec![
            Constraint::Length(1), // spacer
            Constraint::Length(1), // question
            Constraint::Length(1), // path
        ];

        if self.is_dirty {
            constraints.push(Constraint::Length(1)); // spacer before warning
            constraints.push(Constraint::Length(1)); // warning line
            constraints.push(Constraint::Length(1)); // acknowledge checkbox
        }

        if has_branch {
            constraints.push(Constraint::Length(1)); // spacer
            constraints.push(Constraint::Length(1)); // delete branch checkbox
        }

        constraints.push(Constraint::Length(1)); // spacer
        constraints.push(Constraint::Length(1)); // help

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut chunk_idx = 0;

        // Spacer
        chunk_idx += 1;

        // Question
        let question = Paragraph::new(Line::from(vec![
            Span::raw("Delete '"),
            Span::styled(&self.worktree.info.name, Style::default().fg(Color::Yellow)),
            Span::raw("'?"),
        ]));
        question.render(chunks[chunk_idx], buf);
        chunk_idx += 1;

        // Path
        let path_str = self.worktree.info.path.display().to_string();
        let truncated_path = if path_str.len() > (area.width as usize - 10) {
            format!("...{}", &path_str[path_str.len().saturating_sub(area.width as usize - 13)..])
        } else {
            path_str
        };
        let path = Paragraph::new(Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::DarkGray)),
            Span::styled(truncated_path, Style::default().fg(Color::DarkGray)),
        ]));
        path.render(chunks[chunk_idx], buf);
        chunk_idx += 1;

        // Warning section (only for dirty worktrees)
        if self.is_dirty {
            // Spacer
            chunk_idx += 1;

            // Warning line with change counts
            let warning = Paragraph::new(Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
                Span::styled("Uncommitted changes: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("+{}", self.worktree.uncommitted_added),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(" ", Style::default()),
                Span::styled(
                    format!("-{}", self.worktree.uncommitted_removed),
                    Style::default().fg(Color::Red),
                ),
                Span::styled(" lines", Style::default().fg(Color::Yellow)),
            ]));
            warning.render(chunks[chunk_idx], buf);
            chunk_idx += 1;

            // Acknowledge checkbox
            let is_focused = if has_branch {
                self.focused_checkbox == 1
            } else {
                true
            };
            self.acknowledge_checkbox.render(chunks[chunk_idx], buf, is_focused);
            chunk_idx += 1;
        }

        // Delete branch checkbox (only if branch exists)
        if has_branch {
            // Spacer
            chunk_idx += 1;

            let is_focused = self.focused_checkbox == 0;
            self.delete_branch_checkbox.render(chunks[chunk_idx], buf, is_focused);
            chunk_idx += 1;
        }

        // Spacer
        chunk_idx += 1;

        // Help text
        let help_spans = if self.is_dirty && !self.can_delete() {
            vec![
                Span::styled("Space", Style::default().fg(Color::Cyan)),
                Span::styled(": toggle    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::styled(": next    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
            ]
        } else {
            vec![
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::styled(": delete    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Space", Style::default().fg(Color::Cyan)),
                Span::styled(": toggle    ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
            ]
        };
        let help = Paragraph::new(Line::from(help_spans));
        help.render(chunks[chunk_idx], buf);
    }

    fn preferred_size(&self) -> (u16, u16) {
        let has_branch = self.worktree.info.branch.is_some();

        // Base height: borders (2) + spacer + question + path + spacer + help = 7
        let mut height: u16 = 7;

        // Add space for dirty warning section: spacer + warning + checkbox = 3
        if self.is_dirty {
            height += 3;
        }

        // Add space for delete branch checkbox: spacer + checkbox = 2
        if has_branch {
            height += 2;
        }

        (50, height)
    }
}
