mod action_result;
mod delete_confirm;
pub mod merge_confirm;
mod new_worktree;
mod prune_confirm;

pub use action_result::ActionResultModal;
pub use delete_confirm::DeleteConfirmModal;
pub use merge_confirm::{MergeConfirmModal, MergeConflictInfo};
pub use new_worktree::NewWorktreeModal;
pub use prune_confirm::{PruneConfirmModal, PruneWorktreeInfo};

use crossterm::event::KeyEvent;
use ratatui::prelude::*;

/// Actions that can result from modal interactions
#[derive(Debug, Clone)]
pub enum ModalAction {
    /// Close the modal without doing anything
    Cancel,
    /// Delete the selected worktree
    Delete { delete_branch: bool, force: bool },
    /// Merge the selected worktree to main
    Merge { delete_branch: bool },
    /// Create a new worktree
    CreateNew {
        branch: String,
        category: String,
        auto_claude: bool,
        plan_mode: bool,
    },
    /// Show an action result (stays open until dismissed)
    ShowResult { success: bool, message: String },
    /// Dismiss result and return to normal mode
    DismissResult,
    /// Prune all merged worktrees
    Prune,
}

/// Trait for modal dialogs
pub trait Modal {
    /// Handle a key event, returning an action if the modal should close
    fn handle_key(&mut self, key: KeyEvent) -> Option<ModalAction>;

    /// Render the modal
    fn render(&mut self, area: Rect, buf: &mut Buffer);

    /// Get the preferred size for this modal (width, height)
    fn preferred_size(&self) -> (u16, u16);
}

/// Helper to render a centered modal overlay
pub fn render_modal_overlay(modal: &mut dyn Modal, frame_area: Rect, buf: &mut Buffer) {
    let (pref_width, pref_height) = modal.preferred_size();

    // Calculate centered position
    let width = pref_width.min(frame_area.width.saturating_sub(4));
    let height = pref_height.min(frame_area.height.saturating_sub(4));

    let x = frame_area.x + (frame_area.width.saturating_sub(width)) / 2;
    let y = frame_area.y + (frame_area.height.saturating_sub(height)) / 2;

    let modal_area = Rect::new(x, y, width, height);

    // Clear the modal area with a background
    for row in modal_area.y..modal_area.y + modal_area.height {
        for col in modal_area.x..modal_area.x + modal_area.width {
            if let Some(cell) = buf.cell_mut((col, row)) {
                cell.set_char(' ');
                cell.set_bg(Color::Rgb(30, 30, 30));
            }
        }
    }

    modal.render(modal_area, buf);
}
