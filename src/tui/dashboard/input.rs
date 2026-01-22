//! Input handling for the dashboard.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::tui::modals::{ActionResultModal, DeleteConfirmModal, MergeConfirmModal, Modal, ModalAction, NewWorktreeModal};

use super::state::{DashboardMode, DashboardResult};
use super::Dashboard;

impl Dashboard {
    pub(super) fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<DashboardResult> {
        // Clear status message on any keypress
        self.status_message = None;

        // Handle Ctrl+C as escape in any mode
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return Some(DashboardResult::Quit);
        }

        match &mut self.mode {
            DashboardMode::Normal => self.handle_normal_key(code),
            DashboardMode::Search => self.handle_search_key(code),
            DashboardMode::ConfirmDelete(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
            DashboardMode::ConfirmMerge(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
            DashboardMode::NewWorktree(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
            DashboardMode::ActionResult(modal) => {
                if let Some(action) =
                    modal.handle_key(crossterm::event::KeyEvent::new(code, modifiers))
                {
                    self.handle_modal_action(action)
                } else {
                    None
                }
            }
        }
    }

    fn handle_modal_action(&mut self, action: ModalAction) -> Option<DashboardResult> {
        match action {
            ModalAction::Cancel => {
                self.mode = DashboardMode::Normal;
                None
            }
            ModalAction::Delete {
                delete_branch,
                force,
            } => {
                if let Some(worktree) = self.get_selected_worktree() {
                    self.mode = DashboardMode::Normal;
                    Some(DashboardResult::Delete {
                        worktree,
                        delete_branch,
                        force,
                    })
                } else {
                    self.mode = DashboardMode::Normal;
                    None
                }
            }
            ModalAction::Merge { delete_branch } => {
                if let Some(worktree) = self.get_selected_worktree() {
                    self.mode = DashboardMode::Normal;
                    Some(DashboardResult::Merge {
                        worktree,
                        delete_branch,
                    })
                } else {
                    self.mode = DashboardMode::Normal;
                    None
                }
            }
            ModalAction::CreateNew { branch, category, auto_claude, plan_mode } => {
                self.mode = DashboardMode::Normal;
                Some(DashboardResult::CreateNew { branch, category, auto_claude, plan_mode })
            }
            ModalAction::ShowResult { success, message } => {
                self.mode = if success {
                    DashboardMode::ActionResult(ActionResultModal::success(message))
                } else {
                    DashboardMode::ActionResult(ActionResultModal::error(message))
                };
                None
            }
            ModalAction::DismissResult => {
                self.mode = DashboardMode::Normal;
                Some(DashboardResult::Refresh)
            }
        }
    }

    fn handle_normal_key(&mut self, code: KeyCode) -> Option<DashboardResult> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => Some(DashboardResult::Quit),
            KeyCode::Char('/') => {
                self.mode = DashboardMode::Search;
                None
            }
            KeyCode::Enter => {
                if let Some(worktree) = self.get_selected_worktree() {
                    Some(DashboardResult::SwitchTo(worktree))
                } else {
                    None
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                None
            }
            // Quick actions
            KeyCode::Char('n') => {
                self.mode = DashboardMode::NewWorktree(NewWorktreeModal::new());
                None
            }
            KeyCode::Char('x') => {
                // Delete - only for non-main worktrees
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main {
                        self.mode = DashboardMode::ConfirmDelete(DeleteConfirmModal::new(worktree));
                    }
                }
                None
            }
            KeyCode::Char('m') => {
                // Merge - only available in push workflow, and only for non-main worktrees
                if self.pull_workflow {
                    self.status_message = Some((false, "Merge not available in pull workflow - use GitHub PR".to_string()));
                    return None;
                }
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main && worktree.info.branch.is_some() {
                        // Create modal without conflict info initially
                        // The status command will check for conflicts and update
                        self.mode = DashboardMode::ConfirmMerge(MergeConfirmModal::new(
                            worktree,
                            self.main_branch.clone(),
                            None,
                        ));
                    }
                }
                None
            }
            KeyCode::Char('s') => {
                // Sync with remote - contextual push/pull based on tracking branch status
                if let Some(worktree) = self.get_selected_worktree() {
                    if worktree.info.branch.is_some() {
                        return Some(DashboardResult::SyncWithRemote { worktree });
                    }
                }
                None
            }
            KeyCode::Char('d') => {
                // Diff - open diff view in neovim for non-main worktrees
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main {
                        return Some(DashboardResult::Review { worktree });
                    }
                }
                None
            }
            KeyCode::Char('c') => {
                // Claude - open claude in tmux window for any worktree
                if let Some(worktree) = self.get_selected_worktree() {
                    return Some(DashboardResult::Claude { worktree });
                }
                None
            }
            KeyCode::Char('l') => {
                // Linear - open Linear issue (only if worktree has one)
                if let Some(worktree) = self.get_selected_worktree() {
                    if self.linear_issues.contains_key(&worktree.info.name) {
                        return Some(DashboardResult::Linear { worktree });
                    }
                }
                None
            }
            KeyCode::Char('g') => {
                // GitHub - open PR or create-PR page (only for non-main worktrees)
                if let Some(worktree) = self.get_selected_worktree() {
                    if !worktree.info.is_main && worktree.info.branch.is_some() {
                        return Some(DashboardResult::GitHub { worktree });
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn handle_search_key(&mut self, code: KeyCode) -> Option<DashboardResult> {
        match code {
            KeyCode::Esc => {
                self.mode = DashboardMode::Normal;
                self.search_input.clear();
                self.filter_worktrees();
                None
            }
            KeyCode::Enter => {
                self.mode = DashboardMode::Normal;
                if let Some(worktree) = self.get_selected_worktree() {
                    Some(DashboardResult::SwitchTo(worktree))
                } else {
                    None
                }
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.filter_worktrees();
                None
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.filter_worktrees();
                None
            }
            KeyCode::Up => {
                self.move_selection(-1);
                None
            }
            KeyCode::Down => {
                self.move_selection(1);
                None
            }
            _ => None,
        }
    }
}
