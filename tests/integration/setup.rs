use crate::common::{MINIMAL_CONFIG, TestRepo};
use predicates::prelude::*;

#[test]
fn test_setup_applies_new_config_entry() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);
    let worktree_path = repo.worktree_path("wt-setup");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-setup",
            "--no-setup",
        ])
        .assert()
        .success();
    repo.register_worktree(worktree_path.clone());

    assert!(!repo.worktree_dir_exists("wt-setup", ".cache"));

    // Config changes after the worktree was created; setup should pick
    // them up without recreating the worktree.
    repo.write_config(
        r#"
mkdir:
  - path: .cache
"#,
    );

    repo.kabu()
        .args(["setup", worktree_path.to_str().unwrap()])
        .assert()
        .success();

    assert!(repo.worktree_dir_exists("wt-setup", ".cache"));
}

#[test]
fn test_setup_defaults_to_current_worktree() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);
    let worktree_path = repo.worktree_path("wt-setup-cwd");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-setup-cwd",
            "--no-setup",
        ])
        .assert()
        .success();
    repo.register_worktree(worktree_path.clone());

    repo.write_config(
        r#"
mkdir:
  - path: .cache
"#,
    );

    repo.kabu()
        .current_dir(&worktree_path)
        .arg("setup")
        .assert()
        .success();

    assert!(repo.worktree_dir_exists("wt-setup-cwd", ".cache"));
}

#[test]
fn test_setup_dry_run_does_not_modify() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);
    let worktree_path = repo.worktree_path("wt-setup-dry-run");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-setup-dry-run",
            "--no-setup",
        ])
        .assert()
        .success();
    repo.register_worktree(worktree_path.clone());

    repo.write_config(
        r#"
mkdir:
  - path: .cache
"#,
    );

    repo.kabu()
        .args(["setup", worktree_path.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[dry-run]"));

    assert!(!repo.worktree_dir_exists("wt-setup-dry-run", ".cache"));
}

#[test]
fn test_setup_rejects_non_worktree_path() {
    let repo = TestRepo::with_config(MINIMAL_CONFIG);
    let not_a_worktree = repo.worktree_path("not-a-worktree");
    std::fs::create_dir_all(&not_a_worktree).unwrap();

    repo.kabu()
        .args(["setup", not_a_worktree.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_setup_on_conflict_flag_overwrites_tracked_file() {
    let mut repo = TestRepo::with_config(
        r#"
link:
  - source: local.env
"#,
    );
    repo.create_file_and_commit("local.env", "original content\n", "Add local.env");

    let worktree_path = repo.worktree_path("wt-setup-overwrite");

    // Created without setup, so the worktree only has the tracked file
    // checked out by git, not kabu's symlink.
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-setup-overwrite",
            "--no-setup",
        ])
        .assert()
        .success();
    repo.register_worktree(worktree_path.clone());

    assert!(!repo.worktree_symlink_exists("wt-setup-overwrite", "local.env"));

    repo.kabu()
        .args([
            "setup",
            worktree_path.to_str().unwrap(),
            "--on-conflict",
            "overwrite",
        ])
        .assert()
        .success();

    assert!(repo.worktree_symlink_exists("wt-setup-overwrite", "local.env"));
}
