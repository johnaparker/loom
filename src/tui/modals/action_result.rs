use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::{Modal, ModalAction};

/// Modal for displaying action results (success or error)
pub struct ActionResultModal {
    success: bool,
    message: String,
}

impl ActionResultModal {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
        }
    }
}

impl Modal for ActionResultModal {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ModalAction> {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => Some(ModalAction::DismissResult),
            _ => None,
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let (title, border_color, icon) = if self.success {
            (" Success ", Color::Green, "✓")
        } else {
            (" Error ", Color::Red, "✗")
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title)
            .title_style(Style::default().fg(border_color).bold());

        let inner = block.inner(area);
        block.render(area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // spacer
                Constraint::Min(1),    // message
                Constraint::Length(1), // spacer
                Constraint::Length(1), // help
            ])
            .split(inner);

        // Message with icon
        let message_line = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", icon),
                Style::default().fg(border_color).bold(),
            ),
            Span::styled(&self.message, Style::default().fg(Color::White)),
        ]))
        .wrap(ratatui::widgets::Wrap { trim: true });
        message_line.render(chunks[1], buf);

        // Help text
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(" or ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(": dismiss", Style::default().fg(Color::DarkGray)),
        ]));
        help.render(chunks[3], buf);
    }

    fn preferred_size(&self) -> (u16, u16) {
        // Calculate height based on message length
        let lines = (self.message.len() as u16 / 35).max(1) + 1;
        (40, 5 + lines)
    }
}
