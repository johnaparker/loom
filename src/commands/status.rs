use anyhow::Result;

use crate::git::WorktreeManager;
use crate::sesh;
use crate::tmux;
use crate::tui::{Dashboard, DashboardResult};

pub fn status() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let manager = WorktreeManager::open(&current_dir)?;
    let project_name = manager.project_name()?;
    let worktrees = manager.list_worktrees_with_stats()?;

    let mut dashboard = Dashboard::new(worktrees, project_name.clone());
    match dashboard.run()? {
        DashboardResult::SwitchTo(wt) => {
            let session = sesh::session_name(&project_name, &wt.info.name);
            tmux::switch_to_session(&session, wt.info.path.to_str().unwrap())?;
        }
        DashboardResult::Quit => {}
    }

    Ok(())
}
