use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nucleo::{Config as NucleoConfig, Matcher, Utf32Str};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::io::{self, stdout};

/// A fuzzy picker TUI component
pub struct Picker {
    items: Vec<(String, String, String)>, // (name, details, path)
    filtered_indices: Vec<usize>,
    input: String,
    list_state: ListState,
    matcher: Matcher,
}

impl Picker {
    /// Create a new picker with items (name, details, path)
    pub fn new(items: Vec<(String, String, String)>) -> Result<Self> {
        let filtered_indices: Vec<usize> = (0..items.len()).collect();
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            items,
            filtered_indices,
            input: String::new(),
            list_state,
            matcher: Matcher::new(NucleoConfig::DEFAULT),
        })
    }

    /// Run the picker and return the selected index (if any)
    pub fn run(&mut self) -> Result<Option<usize>> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;

        result
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<Option<usize>> {
        loop {
            terminal.draw(|f| self.render(f))?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Handle Ctrl+C as escape
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(None);
                }

                match key.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => {
                        if let Some(selected) = self.list_state.selected() {
                            if selected < self.filtered_indices.len() {
                                return Ok(Some(self.filtered_indices[selected]));
                            }
                        }
                        return Ok(None);
                    }
                    KeyCode::Up => {
                        self.move_selection(-1);
                    }
                    KeyCode::Down => {
                        self.move_selection(1);
                    }
                    KeyCode::Char(c) => {
                        self.input.push(c);
                        self.filter();
                    }
                    KeyCode::Backspace => {
                        self.input.pop();
                        self.filter();
                    }
                    _ => {}
                }
            }
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let current = self.list_state.selected().unwrap_or(0) as i32;
        let new = (current + delta).clamp(0, self.filtered_indices.len() as i32 - 1) as usize;
        self.list_state.select(Some(new));
    }

    fn filter(&mut self) {
        if self.input.is_empty() {
            self.filtered_indices = (0..self.items.len()).collect();
        } else {
            let mut scored: Vec<(usize, u16)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, (name, details, _))| {
                    let haystack = format!("{} {}", name, details);
                    let mut haystack_buf = Vec::new();
                    let haystack_str = Utf32Str::new(&haystack, &mut haystack_buf);
                    let mut needle_buf = Vec::new();
                    let needle_str = Utf32Str::new(&self.input, &mut needle_buf);

                    self.matcher
                        .fuzzy_match(haystack_str, needle_str)
                        .map(|score| (i, score))
                })
                .collect();

            // Sort by score descending
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_indices = scored.into_iter().map(|(i, _)| i).collect();
        }

        // Reset selection
        if self.filtered_indices.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(f.area());

        // Input field
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Search (Esc to cancel) ");
        let input_paragraph = Paragraph::new(self.input.as_str()).block(input_block);
        f.render_widget(input_paragraph, chunks[0]);

        // Worktree list
        let items: Vec<ListItem> = self
            .filtered_indices
            .iter()
            .map(|&i| {
                let (name, details, _) = &self.items[i];

                // Split details to style lag indicator (↓N) differently
                let mut spans = vec![
                    Span::styled(name, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                ];

                if let Some(lag_start) = details.find(" ↓") {
                    let (before, lag) = details.split_at(lag_start);
                    spans.push(Span::styled(before, Style::default().fg(Color::Cyan)));
                    spans.push(Span::styled(lag, Style::default().fg(Color::Yellow)));
                } else {
                    spans.push(Span::styled(details, Style::default().fg(Color::Cyan)));
                }

                let line = Line::from(spans);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Worktrees (↑/↓ to navigate, Enter to select) "),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, chunks[1], &mut self.list_state);
    }
}
