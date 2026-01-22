use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::{Modal, ModalAction};
use crate::git::WorktreeStats;
use crate::tui::widgets::Checkbox;

/// Merge conflict information
pub struct MergeConflictInfo {
    pub conflicted_files: Vec<String>,
}

/// Modal for confirming merge to main
pub struct MergeConfirmModal {
    worktree: WorktreeStats,
    main_branch: String,
    conflicts: Option<MergeConflictInfo>,
    delete_branch_checkbox: Checkbox,
}

impl MergeConfirmModal {
    pub fn new(
        worktree: WorktreeStats,
        main_branch: String,
        conflicts: Option<MergeConflictInfo>,
    ) -> Self {
        Self {
            worktree,
            main_branch,
            conflicts,
            delete_branch_checkbox: Checkbox::new("Delete branch after merge", true),
        }
    }

    pub fn has_conflicts(&self) -> bool {
        self.conflicts.is_some()
    }
}

impl Modal for MergeConfirmModal {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ModalAction> {
        match key.code {
            KeyCode::Esc => Some(ModalAction::Cancel),
            KeyCode::Enter => {
                if self.conflicts.is_some() {
                    // Can't merge with conflicts - just cancel
                    Some(ModalAction::Cancel)
                } else {
                    Some(ModalAction::Merge {
                        delete_branch: self.delete_branch_checkbox.is_checked(),
                    })
                }
            }
            KeyCode::Char(' ') | KeyCode::Tab => {
                if self.conflicts.is_none() {
                    self.delete_branch_checkbox.toggle();
                }
                None
            }
            _ => None,
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if let Some(ref conflicts) = self.conflicts {
            self.render_conflicts(area, buf, conflicts);
        } else {
            self.render_confirm(area, buf);
        }
    }

    fn preferred_size(&self) -> (u16, u16) {
        if let Some(ref conflicts) = self.conflicts {
            let height = 8 + conflicts.conflicted_files.len().min(5) as u16;
            (40, height)
        } else {
            (40, 11)
        }
    }
}

impl MergeConfirmModal {
    fn render_confirm(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .title(" Merge to Main ")
            .title_style(Style::default().fg(Color::Green).bold());

        let inner = block.inner(area);
        block.render(area, buf);

        let branch_name = self
            .worktree
            .info
            .branch
            .as_deref()
            .unwrap_or(&self.worktree.info.name);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // spacer
                Constraint::Length(1), // merge line
                Constraint::Length(1), // stats line
                Constraint::Length(1), // diff stats
                Constraint::Length(1), // spacer
                Constraint::Length(1), // checkbox
                Constraint::Length(1), // spacer
                Constraint::Length(1), // help
            ])
            .split(inner);

        // Merge description
        let merge_line = Paragraph::new(Line::from(vec![
            Span::raw("Merge '"),
            Span::styled(branch_name, Style::default().fg(Color::Green)),
            Span::raw("' -> "),
            Span::styled(&self.main_branch, Style::default().fg(Color::Yellow)),
            Span::raw("?"),
        ]));
        merge_line.render(chunks[1], buf);

        // Commits ahead
        if let Some(ahead) = self.worktree.commits_ahead
            && ahead > 0
        {
            let stats = Paragraph::new(Line::from(vec![
                Span::styled("Commits: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} ahead", ahead),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
            stats.render(chunks[2], buf);
        }

        // Diff stats
        if let (Some(added), Some(removed)) = (self.worktree.diff_added, self.worktree.diff_removed)
        {
            let diff = Paragraph::new(Line::from(vec![
                Span::styled("Changes: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("+{}", added), Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::styled(format!("-{}", removed), Style::default().fg(Color::Red)),
            ]));
            diff.render(chunks[3], buf);
        }

        // Checkbox
        self.delete_branch_checkbox.render(chunks[5], buf, true, true);

        // Help text
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(": merge    ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
        ]));
        help.render(chunks[7], buf);
    }

    fn render_conflicts(&self, area: Rect, buf: &mut Buffer, conflicts: &MergeConflictInfo) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(" Merge Conflicts ")
            .title_style(Style::default().fg(Color::Red).bold());

        let inner = block.inner(area);
        block.render(area, buf);

        let num_files = conflicts.conflicted_files.len().min(5);
        let mut constraints = vec![
            Constraint::Length(1), // spacer
            Constraint::Length(1), // warning
            Constraint::Length(1), // spacer
        ];
        for _ in 0..num_files {
            constraints.push(Constraint::Length(1)); // file
        }
        if conflicts.conflicted_files.len() > 5 {
            constraints.push(Constraint::Length(1)); // "and X more"
        }
        constraints.push(Constraint::Length(1)); // spacer
        constraints.push(Constraint::Length(1)); // message
        constraints.push(Constraint::Length(1)); // spacer
        constraints.push(Constraint::Length(1)); // help

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        // Warning
        let warning = Paragraph::new(Line::from(vec![Span::styled(
            "⚠ Conflicts detected!",
            Style::default().fg(Color::Yellow).bold(),
        )]));
        warning.render(chunks[1], buf);

        // Conflicted files
        for (i, file) in conflicts.conflicted_files.iter().take(5).enumerate() {
            let file_line = Paragraph::new(Line::from(vec![
                Span::styled("  - ", Style::default().fg(Color::Red)),
                Span::styled(file, Style::default().fg(Color::White)),
            ]));
            file_line.render(chunks[3 + i], buf);
        }

        let mut next_idx = 3 + num_files;

        // "and X more" if needed
        if conflicts.conflicted_files.len() > 5 {
            let more = Paragraph::new(Line::from(vec![Span::styled(
                format!("  ...and {} more", conflicts.conflicted_files.len() - 5),
                Style::default().fg(Color::DarkGray),
            )]));
            more.render(chunks[next_idx], buf);
            next_idx += 1;
        }

        // Message
        let msg = Paragraph::new(Line::from(vec![Span::styled(
            "Resolve conflicts before merging.",
            Style::default().fg(Color::DarkGray),
        )]));
        msg.render(chunks[next_idx + 1], buf);

        // Help text
        let help = Paragraph::new(Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(": back", Style::default().fg(Color::DarkGray)),
        ]));
        help.render(chunks[next_idx + 3], buf);
    }
}
