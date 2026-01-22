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

    /// Render the checkbox.
    ///
    /// - `focused`: If true, renders in cyan; otherwise white.
    /// - `enabled`: If false, renders grayed out (disabled state).
    pub fn render(&self, area: Rect, buf: &mut Buffer, focused: bool, enabled: bool) {
        let checkbox = if self.checked { "[x]" } else { "[ ]" };

        let (style, use_bold) = if !enabled {
            (Style::default().fg(Color::DarkGray), false)
        } else if focused {
            (Style::default().fg(Color::Cyan), true)
        } else {
            (Style::default().fg(Color::White), true)
        };

        let checkbox_style = if use_bold { style.bold() } else { style };

        let line = Line::from(vec![
            Span::styled(checkbox, checkbox_style),
            Span::raw(" "),
            Span::styled(&self.label, style),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}
