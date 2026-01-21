use colored::Colorize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GwtError {
    #[error("Worktree '{name}' not found")]
    WorktreeNotFound { name: String },

    #[error("Worktree '{name}' already exists")]
    WorktreeAlreadyExists { name: String },

    #[error("Cannot merge the main worktree")]
    CannotMergeMain,

    #[error("Worktree has no associated branch (detached HEAD)")]
    NoBranch,

    #[error("No worktree matching '{query}'")]
    NoMatch { query: String },

    #[error("Not a git repository")]
    NotGitRepo,

    #[error("Could not find main or master branch")]
    NoMainBranch,

    #[error("Git command failed: {command}")]
    GitCommandFailed { command: String, stderr: String },

    #[error("Config error: {message}")]
    ConfigError { message: String, path: String },

    #[error("Not in a worktree")]
    NotInWorktree,

    #[error("Worktree has uncommitted changes")]
    UncommittedChanges,

    #[error("Linear issue '{issue_id}' not found")]
    LinearIssueNotFound { issue_id: String },

    #[error("Linear API key required to fetch issue '{issue_id}'")]
    LinearApiKeyRequired { issue_id: String },

    #[error("Linear API error: {message}")]
    LinearApiError { message: String },

    #[error("No Linear issue associated with this worktree")]
    NoLinearIssue,

    #[error("GitHub CLI (gh) not found")]
    GitHubCliNotFound,

    #[error("GitHub CLI not authenticated")]
    GitHubNotAuthenticated,

    #[error("GitHub API error: {message}")]
    GitHubApiError { message: String },

    #[error("No GitHub remote found")]
    NoGitHubRemote,

    #[error("No GitHub PR associated with this worktree")]
    NoGitHubPR,

    #[error("Push rejected by remote")]
    PushRejected,

    #[error("No tracking branch for '{branch}'")]
    NoTrackingBranch { branch: String },
}

impl GwtError {
    pub fn suggestion(&self) -> Option<String> {
        match self {
            GwtError::WorktreeNotFound { .. } => {
                Some("Run 'gwt list' to see available worktrees".to_string())
            }
            GwtError::WorktreeAlreadyExists { name } => {
                Some(format!("Use 'gwt switch {}' to switch to it, or choose a different name", name))
            }
            GwtError::CannotMergeMain => {
                Some("You can only merge feature branches into main".to_string())
            }
            GwtError::NoBranch => Some(
                "The worktree is in detached HEAD state. Check out a branch first.".to_string(),
            ),
            GwtError::NoMatch { .. } => {
                Some("Run 'gwt list' to see available worktrees".to_string())
            }
            GwtError::NotGitRepo => {
                Some("Run this command from within a git repository".to_string())
            }
            GwtError::NoMainBranch => Some(
                "Create a 'main' or 'master' branch, or check that you have fetched from remote"
                    .to_string(),
            ),
            GwtError::GitCommandFailed { stderr, .. } => {
                if !stderr.is_empty() {
                    Some(stderr.trim().to_string())
                } else {
                    None
                }
            }
            GwtError::ConfigError { path, .. } => Some(format!("Check config file at: {}", path)),
            GwtError::NotInWorktree => {
                Some("Run this command from within a worktree, or provide a worktree name".to_string())
            }
            GwtError::UncommittedChanges => {
                Some("Commit or stash your changes before syncing".to_string())
            }
            GwtError::LinearIssueNotFound { .. } => {
                Some("Check the issue ID and try again, or provide the full branch name".to_string())
            }
            GwtError::LinearApiKeyRequired { .. } => {
                Some("Configure linear.api_key in ~/.config/gwt/config.toml, or provide the full branch name".to_string())
            }
            GwtError::LinearApiError { .. } => {
                Some("Check your Linear API key and network connection".to_string())
            }
            GwtError::NoLinearIssue => {
                Some("This worktree was not created from a Linear issue ID".to_string())
            }
            GwtError::GitHubCliNotFound => {
                Some("Install the GitHub CLI: https://cli.github.com/".to_string())
            }
            GwtError::GitHubNotAuthenticated => {
                Some("Run 'gh auth login' to authenticate with GitHub".to_string())
            }
            GwtError::GitHubApiError { .. } => {
                Some("Check your network connection and GitHub authentication".to_string())
            }
            GwtError::NoGitHubRemote => {
                Some("Ensure the repository has a GitHub remote (origin)".to_string())
            }
            GwtError::NoGitHubPR => {
                Some("Create a PR first with 'gh pr create' or push the branch".to_string())
            }
            GwtError::PushRejected => {
                Some("Pull remote changes first with 'P' or 'git pull'".to_string())
            }
            GwtError::NoTrackingBranch { .. } => {
                Some("Push first with 'p' to create a remote tracking branch".to_string())
            }
        }
    }

    pub fn display_with_suggestion(&self) -> String {
        let mut output = format!("{} {}", "Error:".red().bold(), self);
        if let Some(hint) = self.suggestion() {
            output.push_str(&format!("\n{} {}", "Hint:".yellow(), hint));
        }
        output
    }
}
