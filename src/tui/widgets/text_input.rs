use ratatui::{
    prelude::*,
    widgets::{Block, Paragraph},
};

/// A text input widget with cursor
pub struct TextInput {
    value: String,
    cursor_position: usize,
    label: String,
}

impl TextInput {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            value: String::new(),
            cursor_position: 0,
            label: label.into(),
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor_position = self.value.len();
    }

    pub fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.value.remove(self.cursor_position);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.value.len() {
            self.cursor_position += 1;
        }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor_position = 0;
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, focused: bool) {
        let style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::bordered()
            .title(format!(" {} ", self.label))
            .border_style(style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Render text with cursor
        let display_text = if focused {
            let (before, after) = self.value.split_at(self.cursor_position);
            Line::from(vec![
                Span::raw(before),
                Span::styled("█", Style::default().fg(Color::Cyan)),
                Span::raw(after),
            ])
        } else {
            Line::from(self.value.as_str())
        };

        Paragraph::new(display_text)
            .style(Style::default().fg(Color::White))
            .render(inner, buf);
    }
}
