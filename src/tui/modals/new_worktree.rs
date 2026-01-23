use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::{Modal, ModalAction};
use crate::tui::widgets::{Checkbox, Selector, TextInput};

/// Modal for creating a new worktree
pub struct NewWorktreeModal {
    branch_input: TextInput,
    category_selector: Option<Selector<String>>,
    auto_claude_checkbox: Checkbox,
    plan_mode_checkbox: Checkbox,
    error_message: Option<String>,
    focused_checkbox: usize, // 0 = auto_claude, 1 = plan_mode
    /// Default category (used when only 1 category configured)
    default_category: String,
}

impl NewWorktreeModal {
    /// Create a new worktree modal with configured categories.
    /// When only 1 category is configured, the selector is hidden.
    pub fn new(categories: Vec<String>, default_category: &str) -> Self {
        let category_selector = if categories.len() > 1 {
            // Build options: (value, display_label)
            let options: Vec<(String, String)> = categories
                .iter()
                .map(|c| (c.clone(), capitalize_first(c)))
                .collect();
            // Find index of default category
            let default_idx = categories.iter().position(|c| c == default_category).unwrap_or(0);
            Some(Selector::new_with_default("Category", options, default_idx))
        } else {
            None
        };

        Self {
            branch_input: TextInput::new("Name"),
            category_selector,
            auto_claude_checkbox: Checkbox::new("Start with Claude", true),
            plan_mode_checkbox: Checkbox::new("Plan mode", true),
            error_message: None,
            focused_checkbox: 0, // Start with auto_claude focused
            default_category: default_category.to_string(),
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

    fn checkbox_count(&self) -> usize {
        if self.auto_claude_checkbox.is_checked() {
            2
        } else {
            1
        }
    }

    fn focus_next(&mut self) {
        let count = self.checkbox_count();
        if count > 1 {
            self.focused_checkbox = (self.focused_checkbox + 1) % count;
        }
    }

    fn focus_prev(&mut self) {
        let count = self.checkbox_count();
        if count > 1 {
            self.focused_checkbox = (self.focused_checkbox + count - 1) % count;
        }
    }

    fn toggle_focused(&mut self) {
        match self.focused_checkbox {
            0 => {
                self.auto_claude_checkbox.toggle();
                // Reset focus when disabling plan_mode
                if !self.auto_claude_checkbox.is_checked() {
                    self.focused_checkbox = 0;
                }
            }
            1 if self.auto_claude_checkbox.is_checked() => {
                self.plan_mode_checkbox.toggle();
            }
            _ => {}
        }
    }
}

impl Default for NewWorktreeModal {
    fn default() -> Self {
        Self::new(vec!["dev".to_string()], "dev")
    }
}

/// Capitalize the first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
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
                            .as_ref()
                            .and_then(|s| s.selected_value().cloned())
                            .unwrap_or_else(|| self.default_category.clone());
                        let auto_claude = self.auto_claude_checkbox.is_checked();
                        let plan_mode = self.plan_mode_checkbox.is_checked();
                        Some(ModalAction::CreateNew { branch, category, auto_claude, plan_mode })
                    }
                    Err(msg) => {
                        self.error_message = Some(msg);
                        None
                    }
                }
            }
            KeyCode::Tab => {
                // Tab cycles through categories (only if selector exists)
                if let Some(ref mut selector) = self.category_selector {
                    selector.select_next();
                }
                None
            }
            KeyCode::BackTab => {
                // Shift+Tab cycles backwards (only if selector exists)
                if let Some(ref mut selector) = self.category_selector {
                    selector.select_prev();
                }
                None
            }
            KeyCode::Up => {
                self.focus_prev();
                None
            }
            KeyCode::Down => {
                self.focus_next();
                None
            }
            KeyCode::Char(' ') => {
                self.toggle_focused();
                None
            }
            KeyCode::Char(c) => {
                // All other typing goes to branch input
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

        let has_category_selector = self.category_selector.is_some();
        let has_error = self.error_message.is_some();

        // Build constraints dynamically based on what elements we have
        let mut constraints = vec![
            Constraint::Length(1), // spacer
            Constraint::Length(3), // branch input
            Constraint::Length(1), // spacer
        ];

        if has_category_selector {
            constraints.push(Constraint::Length(1)); // category
            constraints.push(Constraint::Length(1)); // spacer
        }

        constraints.push(Constraint::Length(1)); // auto-claude checkbox
        constraints.push(Constraint::Length(1)); // plan mode checkbox
        constraints.push(Constraint::Length(1)); // spacer

        if has_error {
            constraints.push(Constraint::Length(1)); // error
            constraints.push(Constraint::Length(1)); // spacer
        }

        constraints.push(Constraint::Length(1)); // help

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        // Branch input - always at index 1
        self.branch_input.render(chunks[1], buf, true);

        // Calculate indices based on whether category selector is shown
        let (auto_claude_idx, plan_mode_idx) = if has_category_selector {
            // Category selector at index 3, checkboxes at 5, 6
            if let Some(ref selector) = self.category_selector {
                selector.render(chunks[3], buf, true);
            }
            (5, 6)
        } else {
            // No category selector, checkboxes at 3, 4
            (3, 4)
        };

        // Auto-claude checkbox - focused when focused_checkbox == 0
        let auto_claude_focused = self.focused_checkbox == 0;
        self.auto_claude_checkbox
            .render(chunks[auto_claude_idx], buf, auto_claude_focused, true);

        // Plan mode checkbox - indent and gray out when auto_claude is disabled
        let plan_mode_enabled = self.auto_claude_checkbox.is_checked();
        let plan_mode_focused = plan_mode_enabled && self.focused_checkbox == 1;
        let plan_mode_area = Rect {
            x: chunks[plan_mode_idx].x + 2, // indent
            ..chunks[plan_mode_idx]
        };
        self.plan_mode_checkbox
            .render(plan_mode_area, buf, plan_mode_focused, plan_mode_enabled);

        // Calculate help index
        let help_idx = if has_category_selector {
            if has_error {
                // Error at 8, help at 10
                if let Some(ref err) = self.error_message {
                    let error = Paragraph::new(Line::from(vec![Span::styled(
                        err,
                        Style::default().fg(Color::Red),
                    )]));
                    error.render(chunks[8], buf);
                }
                10
            } else {
                8
            }
        } else {
            if has_error {
                // Error at 6, help at 8
                if let Some(ref err) = self.error_message {
                    let error = Paragraph::new(Line::from(vec![Span::styled(
                        err,
                        Style::default().fg(Color::Red),
                    )]));
                    error.render(chunks[6], buf);
                }
                8
            } else {
                6
            }
        };

        // Help text - conditionally show Tab hint only if category selector exists
        let mut help_spans = vec![
            Span::styled("\u{2191}\u{2193}", Style::default().fg(Color::Cyan)),
            Span::styled(": options  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Space", Style::default().fg(Color::Cyan)),
            Span::styled(": toggle  ", Style::default().fg(Color::DarkGray)),
        ];

        if has_category_selector {
            help_spans.extend(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::styled(": category  ", Style::default().fg(Color::DarkGray)),
            ]);
        }

        help_spans.extend(vec![
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(": create  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
        ]);

        let help = Paragraph::new(Line::from(help_spans));
        help.render(chunks[help_idx], buf);
    }

    fn preferred_size(&self) -> (u16, u16) {
        let has_category = self.category_selector.is_some();
        let has_error = self.error_message.is_some();

        let height = match (has_category, has_error) {
            (true, true) => 16,
            (true, false) => 14,
            (false, true) => 14,
            (false, false) => 12,
        };

        (74, height)
    }
}
