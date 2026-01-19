use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::{Modal, ModalAction};
use crate::tui::widgets::{Selector, TextInput};

/// Modal for creating a new worktree
pub struct NewWorktreeModal {
    branch_input: TextInput,
    category_selector: Selector<String>,
    error_message: Option<String>,
}

impl NewWorktreeModal {
    pub fn new() -> Self {
        Self {
            branch_input: TextInput::new("Name"),
            category_selector: Selector::new(
                "Category",
                vec![
                    ("dev".to_string(), "Dev".to_string()),
                    ("review".to_string(), "Review".to_string()),
                    ("demo".to_string(), "Demo".to_string()),
                ],
            ),
            error_message: None,
        }
    }

    fn validate_branch_name(&self) -> Result<(), String> {
        let name = self.branch_input.value();

        if name.is_empty() {
            return Err("Branch name cannot be empty".to_string());
        }

        // Basic git ref validation
        if name.starts_with('-') {
            return Err("Branch name cannot start with '-'".to_string());
        }

        if name.contains("..") {
            return Err("Branch name cannot contain '..'".to_string());
        }

        if name.ends_with(".lock") {
            return Err("Branch name cannot end with '.lock'".to_string());
        }

        // Check for invalid characters
        for c in name.chars() {
            if c.is_whitespace() || c == '~' || c == '^' || c == ':' || c == '\\' || c == '?' || c == '*' || c == '[' {
                return Err(format!("Branch name cannot contain '{}'", c));
            }
        }

        Ok(())
    }
}

impl Default for NewWorktreeModal {
    fn default() -> Self {
        Self::new()
    }
}

impl Modal for NewWorktreeModal {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ModalAction> {
        match key.code {
            KeyCode::Esc => Some(ModalAction::Cancel),
            KeyCode::Enter => {
                // Validate and submit
                match self.validate_branch_name() {
                    Ok(()) => {
                        let branch = self.branch_input.value().to_string();
                        let category = self
                            .category_selector
                            .selected_value()
                            .cloned()
                            .unwrap_or_else(|| "dev".to_string());
                        Some(ModalAction::CreateNew { branch, category })
                    }
                    Err(msg) => {
                        self.error_message = Some(msg);
                        None
                    }
                }
            }
            KeyCode::Tab => {
                // Tab cycles through categories
                self.category_selector.select_next();
                None
            }
            KeyCode::BackTab => {
                // Shift+Tab cycles backwards
                self.category_selector.select_prev();
                None
            }
            KeyCode::Char(c) => {
                // All typing goes to branch input
                self.branch_input.insert_char(c);
                self.error_message = None;
                None
            }
            KeyCode::Backspace => {
                self.branch_input.delete_char();
                self.error_message = None;
                None
            }
            KeyCode::Left => {
                self.branch_input.move_cursor_left();
                None
            }
            KeyCode::Right => {
                self.branch_input.move_cursor_right();
                None
            }
            _ => None,
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" New Worktree ")
            .title_style(Style::default().fg(Color::Cyan).bold());

        let inner = block.inner(area);
        block.render(area, buf);

        let constraints = if self.error_message.is_some() {
            vec![
                Constraint::Length(1), // spacer
                Constraint::Length(3), // branch input
                Constraint::Length(1), // spacer
                Constraint::Length(1), // category
                Constraint::Length(1), // spacer
                Constraint::Length(1), // error
                Constraint::Length(1), // spacer
                Constraint::Length(1), // help
            ]
        } else {
            vec![
                Constraint::Length(1), // spacer
                Constraint::Length(3), // branch input
                Constraint::Length(1), // spacer
                Constraint::Length(1), // category
                Constraint::Length(1), // spacer
                Constraint::Length(1), // help
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        // Branch input - always focused
        self.branch_input.render(chunks[1], buf, true);

        // Category selector - always shows selection, not focused
        self.category_selector.render(chunks[3], buf, true);

        let help_idx = if self.error_message.is_some() {
            // Error message
            if let Some(ref err) = self.error_message {
                let error = Paragraph::new(Line::from(vec![Span::styled(
                    err,
                    Style::default().fg(Color::Red),
                )]));
                error.render(chunks[5], buf);
            }
            7
        } else {
            5
        };

        // Help text
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::styled(": category    ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(": create    ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
        ]));
        help.render(chunks[help_idx], buf);
    }

    fn preferred_size(&self) -> (u16, u16) {
        if self.error_message.is_some() {
            (48, 13)
        } else {
            (48, 11)
        }
    }
}
