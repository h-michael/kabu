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

#[test]
fn test_setup_is_idempotent_for_correct_symlinks() {
    let mut repo = TestRepo::with_config(
        r#"
link:
  - source: note.txt
"#,
    );
    repo.create_file("note.txt", "content\n");

    let worktree_path = repo.worktree_path("wt-idempotent");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-idempotent",
        ])
        .assert()
        .success();
    repo.register_worktree(worktree_path.clone());

    assert!(repo.worktree_symlink_exists("wt-idempotent", "note.txt"));

    // Re-running setup with no on_conflict configured must not treat an
    // already-correct symlink as a conflict (which would otherwise
    // require a decision and fail non-interactively).
    repo.kabu()
        .args(["setup", worktree_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Already linked"));

    assert!(repo.worktree_symlink_exists("wt-idempotent", "note.txt"));
}

#[test]
fn test_setup_dry_run_reports_already_linked() {
    // A dry-run preview must report the same "already linked" state a
    // real run would, so a user can compare the two: an already-correct
    // symlink has no "would change" line of its own to stand in for it.
    let mut repo = TestRepo::with_config(
        r#"
link:
  - source: note.txt
"#,
    );
    repo.create_file("note.txt", "content\n");

    let worktree_path = repo.worktree_path("wt-dry-run-parity");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-dry-run-parity",
        ])
        .assert()
        .success();
    repo.register_worktree(worktree_path.clone());

    repo.kabu()
        .args(["setup", worktree_path.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Already linked"))
        .stdout(predicate::str::contains("Would link").not());
}

#[test]
fn test_setup_dry_run_never_prompts_and_never_mutates() {
    let repo = TestRepo::with_config(
        r#"
link:
  - source: tracked.txt
"#,
    );
    repo.create_file_and_commit("tracked.txt", "original\n", "Add tracked.txt");

    let worktree_path = repo.worktree_path("wt-dry-run-conflict");

    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-dry-run-conflict",
        ])
        .output()
        .expect("Failed to create worktree");

    // The checked-out tracked.txt is a real-file conflict with no
    // on_conflict configured. A --dry-run preview must not require an
    // interactive decision or touch the file, even though a real (non
    // dry-run) run would have to prompt or error.
    repo.kabu()
        .args(["setup", worktree_path.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would prompt"));

    assert_eq!(
        std::fs::read_to_string(worktree_path.join("tracked.txt")).unwrap(),
        "original\n"
    );
}

#[test]
fn test_setup_rejects_main_workspace() {
    let repo = TestRepo::with_config(MINIMAL_CONFIG);

    repo.kabu()
        .arg("setup")
        .assert()
        .failure()
        .stderr(predicate::str::contains("main worktree/workspace"));
}
