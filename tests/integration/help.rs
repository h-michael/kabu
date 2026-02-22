use crate::common::TestRepo;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

#[test]
fn test_root_help_has_new_sections() {
    let repo = TestRepo::new();

    repo.kabu()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("QUICK EXAMPLES:"))
        .stdout(predicate::str::contains("WHAT THIS COMMAND DOES:"))
        .stdout(predicate::str::contains("SAFETY NOTES:"))
        .stdout(predicate::str::contains("SEE ALSO:"));
}

#[test]
fn test_subcommand_help_has_new_sections() {
    let repo = TestRepo::new();

    for sub in ["add", "remove", "config", "trust"] {
        repo.kabu()
            .args([sub, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("QUICK EXAMPLES:"))
            .stdout(predicate::str::contains("WHAT THIS COMMAND DOES:"))
            .stdout(predicate::str::contains("SEE ALSO:"));
    }
}

#[test]
fn test_examples_yaml_validate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = root.join("examples");

    let mut entries: Vec<_> = fs::read_dir(&examples_dir)
        .expect("Failed to read examples directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("yaml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let content = fs::read_to_string(&path).expect("Failed to read example");
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();

        let repo = TestRepo::new();
        repo.write_config(&content);

        let assert = repo.kabu().args(["config", "validate"]).assert().success();
        let _ = assert.append_context("example", file_name);
    }
}
