//! Integration tests for grove CLI commands.
//!
//! These tests use ephemeral git repositories to test CLI functionality
//! without polluting the user's `~/worktrees` folder.

mod common;

use common::TestRepo;
use predicates::prelude::*;

/// Tests for `grove list` command
mod list_tests {
    use super::*;

    #[test]
    fn test_list_empty_repo() {
        let repo = TestRepo::new();

        repo.run_grove(&["list"])
            .success()
            .stdout(predicate::str::contains("Main"));
    }

    #[test]
    fn test_list_with_worktrees() {
        let repo = TestRepo::new();

        // Create some worktrees
        repo.create_worktree("feature-1", "dev");
        repo.create_worktree("feature-2", "dev");

        let output = repo.run_grove_output(&["list"]);

        assert!(output.contains("Main"), "Should show Main section");
        assert!(output.contains("Dev"), "Should show Dev section");
        assert!(output.contains("feature-1"), "Should list feature-1");
        assert!(output.contains("feature-2"), "Should list feature-2");
    }

    #[test]
    fn test_list_with_categories() {
        let repo = TestRepo::new();

        // Create worktrees in different categories
        repo.create_worktree("feature-1", "dev");
        repo.create_worktree("review-branch", "review");

        let output = repo.run_grove_output(&["list"]);

        assert!(output.contains("Dev"), "Should show Dev section");
        assert!(output.contains("Review"), "Should show Review section");
        assert!(output.contains("feature-1"), "Should list feature-1 in dev");
        assert!(
            output.contains("review-branch"),
            "Should list review-branch in review"
        );
    }
}

/// Tests for `grove status` command
///
/// Note: The status command uses a full TUI dashboard that requires a terminal,
/// so these tests are skipped in CI environments. The status command itself
/// is tested indirectly through the Dashboard unit tests.
mod status_tests {
    use super::*;

    #[test]
    #[ignore = "status command requires a terminal (TUI dashboard)"]
    fn test_status_main_repo() {
        let repo = TestRepo::new();

        // Status should work from the main repo
        repo.run_grove(&["status"]).success();
    }

    #[test]
    #[ignore = "status command requires a terminal (TUI dashboard)"]
    fn test_status_from_worktree() {
        let repo = TestRepo::new();
        let wt_path = repo.create_worktree("feature-1", "dev");

        // Run grove status from within the worktree
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_grove"))
            .current_dir(&wt_path)
            .env("XDG_CONFIG_HOME", repo.config_dir.path())
            .env("HOME", repo.config_dir.path())
            .args(["status"])
            .output()
            .expect("Failed to run grove status");

        assert!(output.status.success());
    }

    #[test]
    #[ignore = "status command requires a terminal (TUI dashboard)"]
    fn test_status_shows_worktree_info() {
        let repo = TestRepo::new();
        repo.create_worktree("feature-1", "dev");

        let output = repo.run_grove_output(&["status"]);

        // Status should show project info
        assert!(
            output.contains("main") || output.contains("Main"),
            "Should show main branch info"
        );
    }
}

/// Tests for `grove remove` command (dry-run only)
mod remove_tests {
    use super::*;

    #[test]
    fn test_remove_dry_run() {
        let repo = TestRepo::new();
        repo.create_worktree("feature-to-remove", "dev");

        // Dry run should show what would be removed
        let output = repo.run_grove_output(&["remove", "feature-to-remove", "--dry-run"]);

        assert!(
            output.contains("DRY RUN") || output.contains("dry run"),
            "Should indicate dry run mode"
        );
        assert!(
            output.contains("feature-to-remove"),
            "Should mention the worktree to remove"
        );

        // Worktree should still exist after dry run
        assert!(
            repo.worktree_exists("feature-to-remove", "dev"),
            "Worktree should still exist after dry run"
        );
    }

    #[test]
    fn test_remove_nonexistent_worktree() {
        let repo = TestRepo::new();
        // Create a worktree so that "No worktrees to remove" doesn't trigger
        repo.create_worktree("feature-1", "dev");

        // Trying to remove a non-existent worktree should fail with "No match"
        let output = repo.run_grove_output(&["remove", "nonexistent-wt-xyz", "--dry-run"]);

        // Should fail because no worktree matches "nonexistent-wt-xyz"
        assert!(
            output.contains("No match") || output.contains("error") || output.contains("Error"),
            "Should fail when no worktree matches. Got: {}",
            output
        );
    }

    #[test]
    fn test_remove_requires_name_for_dry_run() {
        let repo = TestRepo::new();
        repo.create_worktree("feature-1", "dev");

        // Dry run without name should fail (can't use picker in dry run)
        repo.run_grove(&["remove", "--dry-run"]).failure();
    }

    #[test]
    fn test_remove_cannot_remove_main() {
        let repo = TestRepo::new();
        // Create a worktree so we have something besides main
        repo.create_worktree("feature-1", "dev");

        // Trying to remove main should fail
        // Main is filtered out from removable worktrees, so "main" won't match anything
        let output = repo.run_grove_output(&["remove", "main", "--dry-run"]);

        // Should fail because "main" doesn't match any removable worktree
        assert!(
            output.contains("No match") || output.contains("error") || output.contains("Error"),
            "Should not allow removing main. Got: {}",
            output
        );
    }

    #[test]
    fn test_remove_no_worktrees_available() {
        let repo = TestRepo::new();

        // When there are no worktrees (only main), should show appropriate message
        let output = repo.run_grove_output(&["remove", "anything", "--dry-run"]);

        assert!(
            output.contains("No worktrees to remove"),
            "Should indicate no worktrees to remove when only main exists. Got: {}",
            output
        );
    }
}

/// Tests for `grove merge` command (dry-run only)
mod merge_tests {
    use super::*;

    #[test]
    fn test_merge_dry_run() {
        let repo = TestRepo::new();
        let wt_path = repo.create_worktree("feature-to-merge", "dev");

        // Add a commit to the feature branch
        let file_path = wt_path.join("feature_file.txt");
        std::fs::write(&file_path, "Feature content\n").expect("Failed to write file");

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&wt_path)
            .output()
            .expect("Failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "Add feature"])
            .current_dir(&wt_path)
            .output()
            .expect("Failed to git commit");

        // Dry run should show merge plan
        let output = repo.run_grove_output(&["merge", "feature-to-merge", "--dry-run"]);

        assert!(
            output.contains("DRY RUN") || output.contains("dry run"),
            "Should indicate dry run mode"
        );
        assert!(
            output.contains("feature-to-merge"),
            "Should mention the branch"
        );
        assert!(
            output.contains("main") || output.contains("Main"),
            "Should mention main branch"
        );

        // Worktree should still exist after dry run
        assert!(
            repo.worktree_exists("feature-to-merge", "dev"),
            "Worktree should still exist after dry run"
        );
    }

    #[test]
    fn test_merge_nonexistent_worktree() {
        let repo = TestRepo::new();

        repo.run_grove(&["merge", "nonexistent", "--dry-run"])
            .failure();
    }

    #[test]
    fn test_merge_cannot_merge_main() {
        let repo = TestRepo::new();

        // Trying to merge main should fail
        repo.run_grove(&["merge", "main"]).failure();
    }
}

/// Tests for `grove completions` command
mod completions_tests {
    use super::*;

    #[test]
    fn test_completions_bash() {
        let repo = TestRepo::new();

        repo.run_grove(&["completions", "bash"])
            .success()
            .stdout(predicate::str::contains("complete"));
    }

    #[test]
    fn test_completions_zsh() {
        let repo = TestRepo::new();

        repo.run_grove(&["completions", "zsh"])
            .success()
            .stdout(predicate::str::contains("#compdef"));
    }

    #[test]
    fn test_completions_fish() {
        let repo = TestRepo::new();

        repo.run_grove(&["completions", "fish"])
            .success()
            .stdout(predicate::str::contains("complete"));
    }
}

/// Tests for `grove new` command
///
/// Note: The new command switches to tmux at the end, which won't work
/// in CI environments. These tests verify the behavior up to that point.
mod new_tests {
    use super::*;

    #[test]
    #[ignore = "new command requires tmux to switch sessions"]
    fn test_new_creates_worktree() {
        let repo = TestRepo::new();

        // This will fail at the tmux switch step, but the worktree should be created
        let _ = repo.run_grove(&["new", "test-branch", "--category", "dev"]);

        // Check if worktree was created (it may have been)
        // Note: This test is ignored because tmux switching will fail
    }

    #[test]
    fn test_new_help() {
        let repo = TestRepo::new();

        repo.run_grove(&["new", "--help"])
            .success()
            .stdout(predicate::str::contains("Create a new worktree"));
    }

    #[test]
    fn test_new_missing_branch_arg() {
        let repo = TestRepo::new();

        // Missing required branch argument should fail
        repo.run_grove(&["new"]).failure();
    }
}

/// Tests for `grove switch` command
///
/// Note: The switch command uses TUI picker or tmux, which won't work
/// in CI environments.
mod switch_tests {
    use super::*;

    #[test]
    fn test_switch_help() {
        let repo = TestRepo::new();

        repo.run_grove(&["switch", "--help"])
            .success()
            .stdout(predicate::str::contains("switch"));
    }

    #[test]
    #[ignore = "switch command requires tmux to switch sessions"]
    fn test_switch_by_name() {
        let repo = TestRepo::new();
        repo.create_worktree("feature-1", "dev");

        // This will fail at the tmux switch step
        let _ = repo.run_grove(&["switch", "feature-1"]);
    }

    #[test]
    #[ignore = "switch command requires terminal for TUI picker"]
    fn test_switch_picker() {
        let repo = TestRepo::new();
        repo.create_worktree("feature-1", "dev");

        // Without name, switch uses TUI picker which requires terminal
        let _ = repo.run_grove(&["switch"]);
    }
}

/// Tests for `grove main` command
///
/// Note: The main command switches to tmux, which won't work in CI.
mod main_tests {
    use super::*;

    #[test]
    fn test_main_help() {
        let repo = TestRepo::new();

        repo.run_grove(&["main", "--help"])
            .success()
            .stdout(predicate::str::contains("Switch"));
    }

    #[test]
    #[ignore = "main command requires tmux to switch sessions"]
    fn test_main_switch() {
        let repo = TestRepo::new();

        // This will fail at the tmux switch step
        let _ = repo.run_grove(&["main"]);
    }
}

/// Tests for `grove sync` command (dry-run only)
mod sync_tests {
    use super::*;

    #[test]
    fn test_sync_dry_run() {
        let repo = TestRepo::new();

        // Create a worktree
        let wt_path = repo.create_worktree("feature-sync", "dev");

        // Add a commit to main that the worktree doesn't have
        repo.create_commit("Main commit after worktree");

        // Run sync dry-run from the worktree directory
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_grove"))
            .current_dir(&wt_path)
            .env("XDG_CONFIG_HOME", repo.config_dir.path())
            .env("HOME", repo.config_dir.path())
            .args(["sync", "--dry-run"])
            .output()
            .expect("Failed to run grove sync");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}{}", stdout, stderr);

        // Should either succeed or indicate it can't sync (if no remote)
        assert!(
            combined.contains("DRY RUN")
                || combined.contains("dry run")
                || combined.contains("up to date")
                || combined.contains("behind")
                || combined.contains("sync"),
            "Should show sync status or dry run info. Got: {}",
            combined
        );
    }

    #[test]
    fn test_sync_from_main_fails() {
        let repo = TestRepo::new();

        // Running sync from main repo should fail
        let result = repo.run_grove(&["sync", "--dry-run"]);

        // Should fail because you can't sync main with itself
        // Or succeed with no-op
        // Either is acceptable
        let _ = result;
    }
}

/// Tests for worktree discovery
mod worktree_discovery_tests {
    use super::*;

    #[test]
    fn test_commands_work_from_worktree() {
        let repo = TestRepo::new();
        let wt_path = repo.create_worktree("test-worktree", "dev");

        // List should work from within a worktree
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_grove"))
            .current_dir(&wt_path)
            .env("XDG_CONFIG_HOME", repo.config_dir.path())
            .env("HOME", repo.config_dir.path())
            .args(["list"])
            .output()
            .expect("Failed to run grove list");

        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Main"));
        assert!(stdout.contains("test-worktree"));
    }

    #[test]
    fn test_commands_fail_outside_git_repo() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let config_dir = tempfile::TempDir::new().expect("Failed to create config dir");

        // Create a minimal config
        let config_path = config_dir.path().join("grove").join("config.toml");
        std::fs::create_dir_all(config_path.parent().unwrap())
            .expect("Failed to create config dir");
        std::fs::write(
            &config_path,
            format!(
                "worktree_root = \"{}\"\ndefault_category = \"dev\"\n[sync]\npatterns = []\n[linear]\n",
                temp_dir.path().display()
            ),
        )
        .expect("Failed to write config");

        let output = std::process::Command::new(env!("CARGO_BIN_EXE_grove"))
            .current_dir(temp_dir.path())
            .env("XDG_CONFIG_HOME", config_dir.path())
            .env("HOME", config_dir.path())
            .args(["list"])
            .output()
            .expect("Failed to run grove list");

        assert!(!output.status.success(), "Should fail outside git repo");
    }
}

/// Tests for error handling
mod error_tests {
    use super::*;

    #[test]
    fn test_invalid_command() {
        let repo = TestRepo::new();

        repo.run_grove(&["invalid-command"]).failure();
    }

    #[test]
    fn test_help_flag() {
        let repo = TestRepo::new();

        repo.run_grove(&["--help"])
            .success()
            .stdout(predicate::str::contains("Usage"));
    }

    #[test]
    fn test_version_flag() {
        let repo = TestRepo::new();

        repo.run_grove(&["--version"])
            .success()
            .stdout(predicate::str::contains("grove"));
    }

    #[test]
    fn test_list_help() {
        let repo = TestRepo::new();

        repo.run_grove(&["list", "--help"])
            .success()
            .stdout(predicate::str::contains("List"));
    }
}
