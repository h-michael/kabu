use crate::common::{
    BASIC_CONFIG, CONFIG_WITH_CONFLICT_OVERWRITE, CONFIG_WITH_GLOB,
    CONFIG_WITH_GLOB_IGNORE_TRACKED, MINIMAL_CONFIG, TestRepo,
};
use predicates::prelude::*;

#[test]
fn test_basic_worktree_add() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);
    let worktree_path = repo.worktree_path("feature-branch");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "feature-branch",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Verify worktree was created
    assert!(worktree_path.exists());
    assert!(worktree_path.join("README.md").exists());
}

#[test]
fn test_add_with_mkdir_link_copy() {
    let mut repo = TestRepo::with_config(BASIC_CONFIG);

    // Create source files for link and copy
    repo.create_file_and_commit("local.env", "export FOO=bar\n", "Add local.env");
    repo.create_file_and_commit(
        "config.template",
        "# Config template\n",
        "Add config template",
    );

    let worktree_path = repo.worktree_path("wt-ops");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "wt-ops",
            "--on-conflict",
            "overwrite",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Verify mkdir
    assert!(repo.worktree_dir_exists("wt-ops", ".cache"));
    assert!(repo.worktree_dir_exists("wt-ops", "tmp"));

    // Verify link
    assert!(repo.worktree_symlink_exists("wt-ops", "local.env"));

    // Verify copy
    assert!(repo.worktree_file_exists("wt-ops", "config.local"));
    let content = repo.read_worktree_file("wt-ops", "config.local");
    assert_eq!(content, "# Config template\n");
}

#[test]
fn test_add_dry_run() {
    let repo = TestRepo::with_config(BASIC_CONFIG);

    // Create source files
    repo.create_file_and_commit("local.env", "export FOO=bar\n", "Add local.env");
    repo.create_file_and_commit("config.template", "# Config\n", "Add config");

    let worktree_path = repo.worktree_path("dry-run-test");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "dry-run-test",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[dry-run]"));

    // Worktree should not be created
    assert!(!worktree_path.exists());
}

#[test]
fn test_add_no_setup() {
    let mut repo = TestRepo::with_config(BASIC_CONFIG);

    // Create source files
    repo.create_file_and_commit("local.env", "export FOO=bar\n", "Add local.env");
    repo.create_file_and_commit("config.template", "# Config\n", "Add config");

    let worktree_path = repo.worktree_path("no-setup-test");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "no-setup-test",
            "--no-setup",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Worktree should exist but operations should not be applied
    assert!(worktree_path.exists());
    assert!(!repo.worktree_dir_exists("no-setup-test", ".cache"));
    assert!(!repo.worktree_symlink_exists("no-setup-test", "local.env"));
    assert!(!repo.worktree_file_exists("no-setup-test", "config.local"));
}

#[test]
fn test_add_with_detach_flag() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);
    let worktree_path = repo.worktree_path("detached-head");

    repo.kabu()
        .args(["add", worktree_path.to_str().unwrap(), "--detach"])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());
    assert!(worktree_path.exists());
}

#[test]
fn test_add_with_branch_b_flag() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);
    let worktree_path = repo.worktree_path("new-branch");

    repo.kabu()
        .args(["add", worktree_path.to_str().unwrap(), "-b", "new-branch"])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());
    assert!(worktree_path.exists());
}

#[test]
fn test_add_with_branch_uppercase_b_flag() {
    let mut repo = TestRepo::with_config(MINIMAL_CONFIG);

    // First create a branch in a worktree
    let first_path = repo.worktree_path("first-wt");
    repo.kabu()
        .args(["add", first_path.to_str().unwrap(), "-b", "test-branch"])
        .assert()
        .success();
    repo.register_worktree(first_path.clone());

    // Remove the first worktree (branch still exists)
    repo.kabu()
        .args(["remove", first_path.to_str().unwrap()])
        .assert()
        .success();
    repo.clear_registered_worktrees();

    // Now use -B to reset the existing branch and create a new worktree
    let second_path = repo.worktree_path("second-wt");
    repo.kabu()
        .args(["add", second_path.to_str().unwrap(), "-B", "test-branch"])
        .assert()
        .success();

    repo.register_worktree(second_path.clone());
    assert!(second_path.exists());
}

#[test]
fn test_add_missing_source_file() {
    let repo = TestRepo::with_config(BASIC_CONFIG);
    // Note: source files (local.env, config.template) are NOT created

    let worktree_path = repo.worktree_path("missing-source");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "missing-source",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found").or(predicate::str::contains("does not exist")),
        );
}

#[test]
fn test_add_on_conflict_skip() {
    use crate::common::CONFIG_WITH_CONFLICT_SKIP;

    let mut repo = TestRepo::with_config(CONFIG_WITH_CONFLICT_SKIP);
    repo.create_file_and_commit("local.env", "original content\n", "Add local.env");

    let worktree_path = repo.worktree_path("conflict-skip");

    // First add without kabu to create a conflict
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "conflict-skip",
        ])
        .output()
        .expect("Failed to create worktree");
    repo.register_worktree(worktree_path.clone());

    // Create conflicting file
    std::fs::write(worktree_path.join("local.env"), "conflicting content\n")
        .expect("Failed to write file");

    // Remove worktree
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to remove worktree");
    repo.clear_registered_worktrees();

    // Now add with kabu - should skip the conflict
    // Use -B to reset existing branch
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-B",
            "conflict-skip",
            "--on-conflict",
            "skip",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped").or(predicate::str::contains("skip")));

    repo.register_worktree(worktree_path);
}

#[test]
fn test_add_on_conflict_backup() {
    use crate::common::CONFIG_WITH_CONFLICT_BACKUP;

    let mut repo = TestRepo::with_config(CONFIG_WITH_CONFLICT_BACKUP);
    repo.create_file_and_commit("local.env", "original content\n", "Add local.env");

    let worktree_path = repo.worktree_path("conflict-backup");

    // Create worktree first
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "conflict-backup",
        ])
        .output()
        .expect("Failed to create worktree");
    repo.register_worktree(worktree_path.clone());

    // Create conflicting file
    std::fs::write(worktree_path.join("local.env"), "conflicting content\n")
        .expect("Failed to write file");

    // Remove and re-add
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to remove worktree");
    repo.clear_registered_worktrees();

    // Use -B to reset existing branch
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-B",
            "conflict-backup",
            "--on-conflict",
            "backup",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Check backup file exists
    assert!(worktree_path.join("local.env.bak").exists());
}

#[test]
fn test_add_with_glob_pattern() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_GLOB);

    // Create fixtures directory with txt files
    repo.create_file_and_commit("fixtures/test1.txt", "content1\n", "Add fixtures/test1.txt");
    repo.create_file_and_commit("fixtures/test2.txt", "content2\n", "Add fixtures/test2.txt");
    repo.create_file_and_commit("fixtures/data.json", "ignored\n", "Add fixtures/data.json");

    let worktree_path = repo.worktree_path("glob-test");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "glob-test",
            "--on-conflict",
            "overwrite",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Verify only .txt files are linked (glob pattern: fixtures/*.txt)
    assert!(repo.worktree_symlink_exists("glob-test", "fixtures/test1.txt"));
    assert!(repo.worktree_symlink_exists("glob-test", "fixtures/test2.txt"));
    // data.json should NOT be linked (doesn't match *.txt pattern)
    assert!(!repo.worktree_symlink_exists("glob-test", "fixtures/data.json"));
}

#[test]
fn test_add_with_glob_skip_tracked() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_GLOB_IGNORE_TRACKED);

    // Create tracked file
    repo.create_file_and_commit("fixtures/tracked.txt", "tracked\n", "Add tracked fixture");

    // Create untracked file (do not commit)
    std::fs::create_dir_all(repo.path().join("fixtures")).expect("Failed to create fixtures dir");
    std::fs::write(repo.path().join("fixtures/untracked.txt"), "untracked\n")
        .expect("Failed to write untracked file");

    let worktree_path = repo.worktree_path("ignore-tracked-test");

    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "ignore-tracked-test",
            "--on-conflict",
            "overwrite",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Only untracked file should be linked (skip_tracked: true)
    assert!(repo.worktree_symlink_exists("ignore-tracked-test", "fixtures/untracked.txt"));
    // tracked.txt should NOT be linked (it's git-tracked)
    assert!(!repo.worktree_symlink_exists("ignore-tracked-test", "fixtures/tracked.txt"));
}

#[test]
fn test_add_on_conflict_overwrite() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_CONFLICT_OVERWRITE);
    repo.create_file_and_commit("local.env", "original content\n", "Add local.env");

    let worktree_path = repo.worktree_path("conflict-overwrite");

    // Create worktree first
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "conflict-overwrite",
        ])
        .output()
        .expect("Failed to create worktree");
    repo.register_worktree(worktree_path.clone());

    // Create conflicting file with different content
    std::fs::write(worktree_path.join("local.env"), "conflicting content\n")
        .expect("Failed to write file");

    // Remove and re-add
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to remove worktree");
    repo.clear_registered_worktrees();

    // Use -B to reset existing branch
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-B",
            "conflict-overwrite",
            "--on-conflict",
            "overwrite",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // The symlink should exist (overwritten)
    assert!(repo.worktree_symlink_exists("conflict-overwrite", "local.env"));
    // No backup file should exist
    assert!(!worktree_path.join("local.env.bak").exists());
}
