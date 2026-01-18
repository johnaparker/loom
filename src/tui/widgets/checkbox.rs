use ratatui::prelude::*;

/// A checkbox widget
pub struct Checkbox {
    label: String,
    checked: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
        }
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn toggle(&mut self) {
        self.checked = !self.checked;
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, focused: bool) {
        let checkbox = if self.checked { "[x]" } else { "[ ]" };

        let style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        };

        let line = Line::from(vec![
            Span::styled(checkbox, style.bold()),
            Span::raw(" "),
            Span::styled(&self.label, style),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}
