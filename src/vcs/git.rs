//! Git VCS provider implementation.
//!
//! Provides workspace operations using git worktree commands.

use super::{UnpushedInfo, VcsKind, VcsProvider, WorkspaceInfo, WorkspaceStatus};
use crate::cli::AddArgs;
use crate::error::{Error, Result};

use std::path::{Path, PathBuf};
use std::process::Command;

/// Git VCS provider.
pub(crate) struct GitProvider;

impl VcsProvider for GitProvider {
    fn kind(&self) -> VcsKind {
        VcsKind::Git
    }

    fn is_inside_repo(&self) -> bool {
        is_inside_repo()
    }

    fn repository_root(&self) -> Result<PathBuf> {
        repository_root()
    }

    fn main_workspace_path_for(&self, repo_root: &Path) -> Result<PathBuf> {
        main_worktree_path_for(repo_root)
    }

    fn workspace_add(&self, args: &AddArgs, path: &Path) -> Result<()> {
        worktree_add(args, path)
    }

    fn workspace_remove(&self, path: &Path, force: bool) -> Result<()> {
        worktree_remove(path, force)
    }

    fn workspace_remove_checked(&self, path: &Path, force: bool) -> Result<()> {
        worktree_remove_checked(path, force)
    }

    fn list_workspaces(&self) -> Result<Vec<WorkspaceInfo>> {
        list_worktrees()
    }

    fn workspace_status(&self, path: &Path) -> Result<WorkspaceStatus> {
        worktree_status(path)
    }

    fn workspace_unpushed(&self, path: &Path) -> Result<UnpushedInfo> {
        worktree_unpushed_commits(path)
    }

    fn get_upstream(&self, path: &Path) -> Result<Option<String>> {
        get_upstream_branch(path)
    }

    fn list_tracked_files(&self, repo_root: &Path) -> Result<Vec<PathBuf>> {
        list_tracked_files(repo_root)
    }

    fn list_branches(&self) -> Result<Vec<String>> {
        list_branches()
    }

    fn list_remote_branches(&self) -> Result<Vec<String>> {
        list_remote_branches()
    }

    fn log_oneline(&self, commitish: &str, limit: usize) -> Result<Vec<String>> {
        log_oneline(commitish, limit)
    }

    fn validate_branch_name(&self, name: &str) -> Result<Option<String>> {
        validate_branch_name(name)
    }

    fn delete_branch(&self, name: &str) -> Result<()> {
        delete_branch(name)
    }
}

/// Run `git worktree add` with CLI arguments.
pub(crate) fn worktree_add(args: &AddArgs, path: &std::path::Path) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("worktree").arg("add");

    if args.force {
        cmd.arg("--force");
    }
    if args.detach {
        cmd.arg("--detach");
    }
    if let Some(branch) = &args.new_branch {
        cmd.arg("-b").arg(branch);
    }
    if let Some(branch) = &args.new_branch_force {
        cmd.arg("-B").arg(branch);
    }
    if args.no_checkout {
        cmd.arg("--no-checkout");
    }
    if args.lock {
        cmd.arg("--lock");
    }
    if args.track {
        cmd.arg("--track");
    }
    if args.no_track {
        cmd.arg("--no-track");
    }
    if args.guess_remote {
        cmd.arg("--guess-remote");
    }
    if args.no_guess_remote {
        cmd.arg("--no-guess-remote");
    }
    if args.quiet {
        cmd.arg("--quiet");
    }

    cmd.arg(path);

    if let Some(commitish) = &args.commitish {
        cmd.arg(commitish);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(Error::GitWorktreeAddFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(())
}

/// Get the repository root directory.
pub(crate) fn repository_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

/// Remove a worktree. Used for rollback on failure.
pub(crate) fn worktree_remove(path: &std::path::Path, force: bool) -> Result<()> {
    let output = worktree_remove_inner(path, force)?;

    if !output.status.success() {
        eprintln!(
            "Warning: Failed to remove worktree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

pub(crate) fn worktree_remove_checked(path: &std::path::Path, force: bool) -> Result<()> {
    let output = worktree_remove_inner(path, force)?;

    if !output.status.success() {
        return Err(Error::GitWorktreeRemoveFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(())
}

fn worktree_remove_inner(path: &std::path::Path, force: bool) -> Result<std::process::Output> {
    let mut cmd = Command::new("git");
    cmd.args(["worktree", "remove"]);

    if force {
        cmd.arg("--force");
    }

    cmd.arg(path);

    Ok(cmd.output()?)
}

/// Force-delete a local branch.
pub(crate) fn delete_branch(name: &str) -> Result<()> {
    let output = Command::new("git").args(["branch", "-D", name]).output()?;

    if !output.status.success() {
        eprintln!(
            "Warning: Failed to delete branch '{}': {}",
            name,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

/// Get recent commits for a branch or commitish.
pub(crate) fn log_oneline(commitish: &str, limit: usize) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["log", "--oneline", &format!("-n{limit}"), commitish])
        .output()?;

    if !output.status.success() {
        return Err(Error::GitCommandFailed {
            command: "git log".to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(parse_output_lines(&output.stdout))
}

/// Parse git command output into lines.
fn parse_output_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(String::from)
        .collect()
}

/// List local branch names.
pub(crate) fn list_branches() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    Ok(parse_output_lines(&output.stdout))
}

/// List remote branch names (e.g., "origin/main").
/// Filters out symbolic references like "origin" (which points to origin/HEAD).
pub(crate) fn list_remote_branches() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname:short) %(symref)",
            "refs/remotes/",
        ])
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    Ok(parse_output_lines(&output.stdout)
        .into_iter()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                return None;
            }
            let branch_name = parts[0];
            let is_symref = parts.len() > 1 && !parts[1].is_empty();

            if is_symref {
                None
            } else {
                Some(branch_name.to_string())
            }
        })
        .collect())
}

/// Validate a branch name using git check-ref-format.
/// Returns Ok(None) when valid, Ok(Some(message)) when invalid.
pub(crate) fn validate_branch_name(name: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .output()?;

    if output.status.success() {
        return Ok(None);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if stderr.is_empty() {
        "Invalid branch name".to_string()
    } else {
        stderr
    };

    if output.status.code() == Some(1) {
        return Ok(Some(message));
    }

    Err(Error::GitCommandFailed {
        command: format!("git check-ref-format --branch {name}"),
        stderr: message,
    })
}

/// Check if current directory is inside a git repository.
pub(crate) fn is_inside_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// List all files tracked by git in the specified repository.
/// Returns paths relative to the repository root.
pub(crate) fn list_tracked_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files"])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    Ok(parse_output_lines(&output.stdout)
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

/// Get the main worktree path for a specific repository directory.
pub(crate) fn main_worktree_path_for(repo_root: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    let lines = parse_output_lines(&output.stdout);

    for line in lines {
        if !line.starts_with("worktree ") {
            continue;
        }

        let path_str = line.strip_prefix("worktree ").unwrap_or("").trim();
        if path_str.is_empty() {
            continue;
        }

        let path = PathBuf::from(path_str);

        if !path.exists() {
            continue;
        }

        let git_dir_output = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&path)
            .output();

        let git_dir_output = match git_dir_output {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };

        let git_dir = String::from_utf8_lossy(&git_dir_output.stdout)
            .trim()
            .to_string();
        if git_dir.is_empty() {
            continue;
        }

        let git_dir_path = PathBuf::from(&git_dir);
        if git_dir_path.file_name() == Some(std::ffi::OsStr::new(".git")) {
            return path.canonicalize().map_err(|e| {
                Error::Internal(format!(
                    "Failed to canonicalize primary worktree path '{}': {}",
                    path.display(),
                    e
                ))
            });
        }
    }

    Err(Error::NotInGitRepo)
}

/// List all worktrees with their information.
pub(crate) fn list_worktrees() -> Result<Vec<WorkspaceInfo>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    parse_worktree_list(&output.stdout)
}

fn parse_worktree_list(bytes: &[u8]) -> Result<Vec<WorkspaceInfo>> {
    let text = String::from_utf8_lossy(bytes);
    let mut worktrees = Vec::new();
    let mut current: Option<WorkspaceInfo> = None;
    let mut is_first = true;

    for line in text.lines() {
        if line.starts_with("worktree ") {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            let path = PathBuf::from(line.strip_prefix("worktree ").unwrap_or(""));
            current = Some(WorkspaceInfo {
                path,
                head: String::new(),
                branch: None,
                is_main: is_first,
                is_locked: false,
                workspace_name: None,
            });
            is_first = false;
        } else if line.starts_with("HEAD ") {
            if let Some(ref mut wt) = current {
                wt.head = line.strip_prefix("HEAD ").unwrap_or("").to_string();
            }
        } else if line.starts_with("branch ") {
            if let Some(ref mut wt) = current {
                wt.branch = Some(line.strip_prefix("branch ").unwrap_or("").to_string());
            }
        } else if line == "detached" {
            // HEAD is detached, branch remains None
        } else if line.starts_with("locked")
            && let Some(ref mut wt) = current
        {
            wt.is_locked = true;
        }
    }

    if let Some(wt) = current {
        worktrees.push(wt);
    }

    Ok(worktrees)
}

/// Get the status of a worktree.
pub(crate) fn worktree_status(worktree_path: &std::path::Path) -> Result<WorkspaceStatus> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInGitRepo);
    }

    parse_status_output(&output.stdout)
}

fn parse_status_output(bytes: &[u8]) -> Result<WorkspaceStatus> {
    let text = String::from_utf8_lossy(bytes);
    let mut modified_count = 0;
    let mut deleted_count = 0;
    let mut untracked_count = 0;

    for line in text.lines() {
        if line.len() < 2 {
            continue;
        }
        let index = line.chars().next().unwrap_or(' ');
        let worktree = line.chars().nth(1).unwrap_or(' ');

        if index == '?' && worktree == '?' {
            untracked_count += 1;
        } else if index == 'M' || worktree == 'M' {
            modified_count += 1;
        } else if index == 'D' || worktree == 'D' {
            deleted_count += 1;
        } else if index == 'A' {
            modified_count += 1;
        }
    }

    let has_uncommitted_changes = modified_count > 0 || deleted_count > 0 || untracked_count > 0;

    Ok(WorkspaceStatus {
        has_uncommitted_changes,
        modified_count,
        deleted_count,
        untracked_count,
    })
}

/// Check for unpushed commits in a worktree.
pub(crate) fn worktree_unpushed_commits(worktree_path: &std::path::Path) -> Result<UnpushedInfo> {
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(worktree_path)
        .output();

    if !matches!(upstream, Ok(ref output) if output.status.success()) {
        return check_unpushed_against_remote(worktree_path);
    }

    let output = Command::new("git")
        .args(["log", "--oneline", "@{upstream}..HEAD"])
        .current_dir(worktree_path)
        .output()?;

    if !output.status.success() {
        return Ok(UnpushedInfo {
            has_unpushed: false,
            count: 0,
        });
    }

    parse_log_output(&output.stdout)
}

fn check_unpushed_against_remote(worktree_path: &std::path::Path) -> Result<UnpushedInfo> {
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree_path)
        .output()?;

    if !branch_output.status.success() {
        return Ok(UnpushedInfo {
            has_unpushed: false,
            count: 0,
        });
    }

    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    if branch == "HEAD" {
        return Ok(UnpushedInfo {
            has_unpushed: false,
            count: 0,
        });
    }

    let remote_output = Command::new("git")
        .args(["config", "--get", &format!("branch.{}.remote", branch)])
        .current_dir(worktree_path)
        .output()?;

    if !remote_output.status.success() {
        return Ok(UnpushedInfo {
            has_unpushed: false,
            count: 0,
        });
    }

    let remote = String::from_utf8_lossy(&remote_output.stdout)
        .trim()
        .to_string();

    let remote_ref = format!("{}/{}", remote, branch);
    let check = Command::new("git")
        .args(["rev-parse", "--verify", &remote_ref])
        .current_dir(worktree_path)
        .output();

    if !matches!(check, Ok(ref output) if output.status.success()) {
        return Ok(UnpushedInfo {
            has_unpushed: false,
            count: 0,
        });
    }

    let output = Command::new("git")
        .args(["log", "--oneline", &format!("{}..HEAD", remote_ref)])
        .current_dir(worktree_path)
        .output()?;

    parse_log_output(&output.stdout)
}

/// Get the upstream branch name for the current branch in a worktree.
pub(crate) fn get_upstream_branch(worktree_path: &std::path::Path) -> Result<Option<String>> {
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(worktree_path)
        .output()?;

    if !upstream.status.success() {
        return Ok(None);
    }

    let upstream_name = String::from_utf8_lossy(&upstream.stdout).trim().to_string();
    if upstream_name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(upstream_name))
    }
}

fn parse_log_output(bytes: &[u8]) -> Result<UnpushedInfo> {
    let lines = parse_output_lines(bytes);
    let count = lines.len();
    Ok(UnpushedInfo {
        has_unpushed: count > 0,
        count,
    })
}

#[cfg(all(test, feature = "impure-test"))]
mod impure_tests {
    use super::*;

    #[test]
    fn test_is_inside_repo() {
        let _ = is_inside_repo();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_worktree_list_single() {
        let output = b"worktree /home/user/repo\nHEAD abc1234\nbranch refs/heads/main\n\n";
        let result = parse_worktree_list(output).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/home/user/repo"));
        assert_eq!(result[0].head, "abc1234");
        assert_eq!(result[0].branch, Some("refs/heads/main".to_string()));
        assert!(result[0].is_main);
        assert!(!result[0].is_locked);
    }

    #[test]
    fn test_parse_worktree_list_multiple() {
        let output = b"worktree /home/user/repo\nHEAD abc1234\nbranch refs/heads/main\n\nworktree /home/user/feature\nHEAD def5678\nbranch refs/heads/feature\n\n";
        let result = parse_worktree_list(output).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].is_main);
        assert!(!result[1].is_main);
        assert_eq!(result[1].path, PathBuf::from("/home/user/feature"));
        assert_eq!(result[1].head, "def5678");
        assert_eq!(result[1].branch, Some("refs/heads/feature".to_string()));
    }

    #[test]
    fn test_parse_worktree_list_detached() {
        let output = b"worktree /home/user/repo\nHEAD abc1234\nbranch refs/heads/main\n\nworktree /home/user/detached\nHEAD def5678\ndetached\n\n";
        let result = parse_worktree_list(output).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[1].branch, None);
    }

    #[test]
    fn test_parse_worktree_list_locked() {
        let output = b"worktree /home/user/repo\nHEAD abc1234\nbranch refs/heads/main\n\nworktree /home/user/locked\nHEAD def5678\nbranch refs/heads/feature\nlocked\n\n";
        let result = parse_worktree_list(output).unwrap();

        assert_eq!(result.len(), 2);
        assert!(!result[0].is_locked);
        assert!(result[1].is_locked);
    }

    #[test]
    fn test_parse_status_output_empty() {
        let output = b"";
        let result = parse_status_output(output).unwrap();

        assert!(!result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 0);
        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.untracked_count, 0);
    }

    #[test]
    fn test_parse_status_output_modified() {
        let output = b" M file1.txt\nM  file2.txt\n";
        let result = parse_status_output(output).unwrap();

        assert!(result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 2);
        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.untracked_count, 0);
    }

    #[test]
    fn test_parse_status_output_deleted() {
        let output = b" D file1.txt\nD  file2.txt\n";
        let result = parse_status_output(output).unwrap();

        assert!(result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 0);
        assert_eq!(result.deleted_count, 2);
        assert_eq!(result.untracked_count, 0);
    }

    #[test]
    fn test_parse_status_output_untracked() {
        let output = b"?? file1.txt\n?? file2.txt\n";
        let result = parse_status_output(output).unwrap();

        assert!(result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 0);
        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.untracked_count, 2);
    }

    #[test]
    fn test_parse_status_output_added() {
        let output = b"A  file1.txt\n";
        let result = parse_status_output(output).unwrap();

        assert!(result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 1);
        assert_eq!(result.deleted_count, 0);
        assert_eq!(result.untracked_count, 0);
    }

    #[test]
    fn test_parse_status_output_mixed() {
        let output = b" M file1.txt\nD  file2.txt\n?? file3.txt\nA  file4.txt\n";
        let result = parse_status_output(output).unwrap();

        assert!(result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 2);
        assert_eq!(result.deleted_count, 1);
        assert_eq!(result.untracked_count, 1);
    }

    #[test]
    fn test_parse_log_output_empty() {
        let output = b"";
        let result = parse_log_output(output).unwrap();

        assert!(!result.has_unpushed);
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_parse_log_output_single() {
        let output = b"abc1234 Commit message\n";
        let result = parse_log_output(output).unwrap();

        assert!(result.has_unpushed);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_parse_log_output_multiple() {
        let output = b"abc1234 Commit 1\ndef5678 Commit 2\nghi9012 Commit 3\n";
        let result = parse_log_output(output).unwrap();

        assert!(result.has_unpushed);
        assert_eq!(result.count, 3);
    }
}
