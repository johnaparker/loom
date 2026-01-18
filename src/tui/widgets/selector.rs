use ratatui::prelude::*;

/// A horizontal selector widget for choosing between options
pub struct Selector<T: Clone + PartialEq> {
    label: String,
    options: Vec<(T, String)>,
    selected: usize,
}

impl<T: Clone + PartialEq> Selector<T> {
    pub fn new(label: impl Into<String>, options: Vec<(T, String)>) -> Self {
        Self {
            label: label.into(),
            options,
            selected: 0,
        }
    }

    pub fn selected_value(&self) -> Option<&T> {
        self.options.get(self.selected).map(|(v, _)| v)
    }

    pub fn select_next(&mut self) {
        if !self.options.is_empty() {
            self.selected = (self.selected + 1) % self.options.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.options.is_empty() {
            self.selected = if self.selected == 0 {
                self.options.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, focused: bool) {
        let mut spans = vec![
            Span::styled(
                format!("{}: ", self.label),
                Style::default().fg(Color::DarkGray),
            ),
        ];

        for (i, (_, label)) in self.options.iter().enumerate() {
            let is_selected = i == self.selected;

            if is_selected {
                spans.push(Span::styled(
                    format!("[{}]", label),
                    Style::default()
                        .fg(if focused { Color::Cyan } else { Color::White })
                        .bold(),
                ));
            } else {
                spans.push(Span::styled(
                    label.clone(),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            if i < self.options.len() - 1 {
                spans.push(Span::raw("  "));
            }
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}
