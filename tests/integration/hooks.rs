use crate::common::{
    CONFIG_WITH_FAILING_POST_HOOK, CONFIG_WITH_FAILING_PRE_HOOK, CONFIG_WITH_HOOKS, TestRepo,
};
use predicates::prelude::*;

#[test]
fn test_hooks_require_trust() {
    let repo = TestRepo::with_config(CONFIG_WITH_HOOKS);
    let worktree_path = repo.worktree_path("untrusted-hooks");

    // Without trust, should fail to execute hooks
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "untrusted-hooks",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("trust").or(predicate::str::contains("untrusted")));

    // Worktree should not be created
    assert!(!worktree_path.exists());
}

#[test]
fn test_pre_add_hook_execution() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_HOOKS);
    let worktree_path = repo.worktree_path("pre-add-test");

    // Trust the config first
    repo.trust_config();

    // Add worktree
    repo.kabu()
        .args(["add", worktree_path.to_str().unwrap(), "-b", "pre-add-test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre_add").or(predicate::str::contains("Pre-add")));

    repo.register_worktree(worktree_path);
}

#[test]
fn test_post_add_hook_execution() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_HOOKS);
    let worktree_path = repo.worktree_path("post-add-test");

    // Trust the config first
    repo.trust_config();

    // Add worktree
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "post-add-test",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("post_add").or(predicate::str::contains("Post-add")));

    repo.register_worktree(worktree_path);
}

#[test]
fn test_pre_add_hook_failure_aborts() {
    let repo = TestRepo::with_config(CONFIG_WITH_FAILING_PRE_HOOK);
    let worktree_path = repo.worktree_path("failing-pre-hook");

    // Trust the config
    repo.trust_config();

    // pre_add hook failure should abort worktree creation
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "failing-pre-hook",
        ])
        .assert()
        .failure();

    // Worktree should not be created
    assert!(!worktree_path.exists());
}

#[test]
fn test_post_add_hook_failure_continues() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_FAILING_POST_HOOK);
    let worktree_path = repo.worktree_path("failing-post-hook");

    // Trust the config
    repo.trust_config();

    // post_add hook failure should warn but still succeed
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "failing-post-hook",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning").or(predicate::str::contains("failed")));

    repo.register_worktree(worktree_path.clone());

    // Worktree should still be created
    assert!(worktree_path.exists());

    // mkdir operation should have been applied
    assert!(repo.worktree_dir_exists("failing-post-hook", ".cache"));
}

#[test]
fn test_hook_template_variable_expansion() {
    // Test that hooks with template variables execute without errors
    let config = r#"
hooks:
  post_add:
    - command: "echo worktree_name={{worktree_name}}"
      description: "Echo worktree name"
"#;
    let mut repo = TestRepo::with_config(config);
    let worktree_path = repo.worktree_path("template-test");

    // Trust the config
    repo.trust_config();

    // Add worktree - hooks should execute with template expansion
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "template-test",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path);
}

#[test]
fn test_hook_env_variables() {
    let marker_file = "env_marker.txt";
    let config = format!(
        r#"
hooks:
  post_add:
    - command: "echo $KABU_WORKTREE_NAME > {marker_file}"
      description: "Write env var to file"
"#
    );
    let mut repo = TestRepo::with_config(&config);
    let worktree_path = repo.worktree_path("env-var-test");

    repo.trust_config();

    repo.kabu()
        .args(["add", worktree_path.to_str().unwrap(), "-b", "env-var-test"])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // post_add hook runs in worktree directory, so the file is created there
    let content = std::fs::read_to_string(worktree_path.join(marker_file)).unwrap();
    assert_eq!(content.trim(), "env-var-test");
}

#[test]
fn test_pre_remove_and_post_remove_hooks() {
    let mut repo = TestRepo::with_config(CONFIG_WITH_HOOKS);
    let worktree_path = repo.worktree_path("remove-hooks-test");

    // Trust the config
    repo.trust_config();

    // Create worktree
    repo.kabu()
        .args([
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "remove-hooks-test",
        ])
        .assert()
        .success();

    repo.register_worktree(worktree_path.clone());

    // Remove worktree - should execute pre_remove and post_remove hooks
    repo.kabu()
        .args(["remove", worktree_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("pre_remove")
                .or(predicate::str::contains("Pre-remove"))
                .or(predicate::str::contains("post_remove"))
                .or(predicate::str::contains("Post-remove")),
        );

    repo.clear_registered_worktrees();
}
