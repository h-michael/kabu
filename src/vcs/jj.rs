//! jj (Jujutsu) VCS provider implementation.
//!
//! Provides workspace operations using jj workspace commands.

use super::{UnpushedInfo, VcsKind, VcsProvider, WorkspaceInfo, WorkspaceStatus};
use crate::cli::AddArgs;
use crate::error::{Error, Result};

use std::path::{Path, PathBuf};
use std::process::Command;

/// jj VCS provider.
pub(crate) struct JjProvider;

impl VcsProvider for JjProvider {
    fn kind(&self) -> VcsKind {
        // Check if colocated
        if is_colocated() {
            VcsKind::JjColocated
        } else {
            VcsKind::Jj
        }
    }

    fn is_inside_repo(&self) -> bool {
        is_inside_repo()
    }

    fn main_workspace_root(&self) -> Result<PathBuf> {
        main_workspace_root()
    }

    fn main_workspace_path_for(&self, repo_root: &Path) -> Result<PathBuf> {
        main_workspace_path_for(repo_root)
    }

    fn workspace_add(&self, args: &AddArgs, path: &Path) -> Result<()> {
        workspace_add(args, path)
    }

    fn workspace_remove(&self, path: &Path, force: bool) -> Result<()> {
        workspace_remove(path, force)
    }

    fn workspace_remove_checked(&self, path: &Path, force: bool) -> Result<()> {
        workspace_remove_checked(path, force)
    }

    fn list_workspaces(&self) -> Result<Vec<WorkspaceInfo>> {
        list_workspaces()
    }

    fn workspace_status(&self, path: &Path) -> Result<WorkspaceStatus> {
        workspace_status(path)
    }

    fn workspace_unpushed(&self, path: &Path) -> Result<UnpushedInfo> {
        workspace_unpushed(path)
    }

    fn get_upstream(&self, _path: &Path) -> Result<Option<String>> {
        // jj doesn't have a direct concept of upstream branches
        Ok(None)
    }

    fn list_tracked_files(&self, repo_root: &Path) -> Result<Vec<PathBuf>> {
        list_tracked_files(repo_root)
    }

    fn list_branches(&self) -> Result<Vec<String>> {
        list_bookmarks()
    }

    fn list_remote_branches(&self) -> Result<Vec<String>> {
        list_remote_bookmarks()
    }

    fn log_oneline(&self, revset: &str, limit: usize) -> Result<Vec<String>> {
        log_oneline(revset, limit)
    }

    fn validate_branch_name(&self, name: &str) -> Result<Option<String>> {
        validate_bookmark_name(name)
    }

    fn delete_branch(&self, name: &str) -> Result<()> {
        delete_bookmark(name)
    }
}

/// Check if current directory is inside a jj repository.
pub(crate) fn is_inside_repo() -> bool {
    Command::new("jj")
        .args(["root"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if the jj repository is colocated with git.
fn is_colocated() -> bool {
    let output = Command::new("jj").args(["root"]).output().ok();

    if let Some(output) = output
        && output.status.success()
    {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let git_dir = PathBuf::from(&root).join(".git");
        return git_dir.exists();
    }
    false
}

/// Get the main workspace's root directory.
///
/// In jj, this returns the path of the "default" workspace, which is the main
/// repository location. This is different from `jj root` which returns the
/// current workspace's root.
pub(crate) fn main_workspace_root() -> Result<PathBuf> {
    // First, get the current workspace root
    let output = Command::new("jj").args(["root"]).output()?;

    if !output.status.success() {
        return Err(Error::NotInRepo {
            vcs: "jj".to_string(),
        });
    }

    let workspace_root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());

    // Check if .jj/repo is a file (non-default workspace) or directory (default workspace)
    let jj_repo = workspace_root.join(".jj").join("repo");

    if jj_repo.is_file() {
        // Non-default workspace: .jj/repo contains path to the real repo directory
        // Read the file and resolve the path to find the default workspace
        if let Ok(content) = std::fs::read(&jj_repo) {
            let repo_path_str = String::from_utf8_lossy(&content);
            let repo_path = PathBuf::from(repo_path_str.trim());

            // The path in .jj/repo is relative to the .jj directory
            let jj_dir = workspace_root.join(".jj");
            let absolute_repo_path = if repo_path.is_absolute() {
                repo_path
            } else {
                jj_dir.join(&repo_path)
            };

            // The repo directory is typically at <default_workspace>/.jj/repo
            // So the default workspace is the parent of .jj
            if let Ok(canonical) = absolute_repo_path.canonicalize() {
                // canonical is like /path/to/default/.jj/repo
                // We want /path/to/default
                if let Some(jj_parent) = canonical.parent() {
                    // jj_parent is /path/to/default/.jj
                    if let Some(default_ws) = jj_parent.parent() {
                        return Ok(default_ws.to_path_buf());
                    }
                }
            }
        }
    }

    // Default workspace or fallback: .jj/repo is a directory
    Ok(workspace_root)
}

/// Get the main workspace path for a specific repository directory.
///
/// In jj, the "default" workspace is typically the main one.
pub(crate) fn main_workspace_path_for(repo_root: &Path) -> Result<PathBuf> {
    let workspaces = list_workspaces_at(repo_root)?;

    // First, look for the "default" workspace
    for ws in &workspaces {
        if ws.workspace_name.as_deref() == Some("default") {
            return Ok(ws.path.clone());
        }
    }

    // If no default workspace, return the first one (main workspace)
    workspaces
        .into_iter()
        .next()
        .map(|ws| ws.path)
        .ok_or_else(|| Error::NotInRepo {
            vcs: "jj".to_string(),
        })
}

/// Run `jj workspace add` with CLI arguments.
pub(crate) fn workspace_add(args: &AddArgs, path: &Path) -> Result<()> {
    // jj doesn't create parent directories automatically, so we need to do it
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new("jj");
    cmd.arg("workspace").arg("add");

    // Workspace name is derived from the directory name
    let workspace_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");

    cmd.arg("--name").arg(workspace_name);

    // If creating a new branch (bookmark in jj terms), we need to handle it differently
    // jj workspace add doesn't create bookmarks, so we add the workspace first
    // and then create the bookmark if needed

    // If there's a commitish/revision specified, use it
    if let Some(revision) = &args.commitish {
        cmd.arg("-r").arg(revision);
    }

    cmd.arg(path);

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(Error::JjWorkspaceAddFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // If a new branch was requested, create a bookmark
    if let Some(branch_name) = args.new_branch.as_ref().or(args.new_branch_force.as_ref()) {
        create_bookmark_at_workspace(path, branch_name)?;
    }

    Ok(())
}

/// Create a bookmark at the workspace's working copy commit.
fn create_bookmark_at_workspace(workspace_path: &Path, name: &str) -> Result<()> {
    let output = Command::new("jj")
        .args(["bookmark", "create", name, "-r", "@"])
        .current_dir(workspace_path)
        .output()?;

    if !output.status.success() {
        return Err(Error::JjCommandFailed {
            command: format!("jj bookmark create {}", name),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(())
}

/// Delete a bookmark.
pub(crate) fn delete_bookmark(name: &str) -> Result<()> {
    let output = Command::new("jj")
        .args(["bookmark", "delete", name])
        .output()?;

    if !output.status.success() {
        eprintln!(
            "Warning: Failed to delete bookmark '{}': {}",
            name,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

/// Remove a workspace (forget + delete directory).
pub(crate) fn workspace_remove(path: &Path, force: bool) -> Result<()> {
    let output = workspace_forget_inner(path)?;

    if !output.status.success() {
        eprintln!(
            "Warning: Failed to forget workspace: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    // Also delete the directory
    if path.exists() {
        if force {
            std::fs::remove_dir_all(path).ok();
        } else {
            // Try to remove, but don't fail if it doesn't work
            std::fs::remove_dir_all(path).ok();
        }
    }

    Ok(())
}

/// Remove a workspace with error checking.
pub(crate) fn workspace_remove_checked(path: &Path, force: bool) -> Result<()> {
    let output = workspace_forget_inner(path)?;

    if !output.status.success() {
        return Err(Error::JjWorkspaceForgetFailed {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Delete the directory
    // Note: jj workspace forget doesn't delete the directory, so we need to do it manually
    // The force parameter is ignored here as jj doesn't have a similar concept
    let _ = force; // Acknowledge unused parameter
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }

    Ok(())
}

fn workspace_forget_inner(path: &Path) -> Result<std::process::Output> {
    // Get workspace name from path
    let workspace_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // We need to run from the repo root, not the workspace itself
    let repo_root = find_repo_root_from_workspace(path)?;

    let output = Command::new("jj")
        .args(["workspace", "forget", workspace_name])
        .current_dir(&repo_root)
        .output()?;

    Ok(output)
}

/// Find the repository root from a workspace path.
fn find_repo_root_from_workspace(workspace_path: &Path) -> Result<PathBuf> {
    // Try to get root from the workspace
    let output = Command::new("jj")
        .args(["root"])
        .current_dir(workspace_path)
        .output();

    if let Ok(output) = output
        && output.status.success()
    {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(PathBuf::from(root));
    }

    // If workspace doesn't work, try parent directories
    let mut current = workspace_path.to_path_buf();
    while let Some(parent) = current.parent() {
        let jj_dir = parent.join(".jj");
        if jj_dir.is_dir() {
            return Ok(parent.to_path_buf());
        }
        current = parent.to_path_buf();
    }

    Err(Error::NotInRepo {
        vcs: "jj".to_string(),
    })
}

/// List all workspaces.
pub(crate) fn list_workspaces() -> Result<Vec<WorkspaceInfo>> {
    let repo_root = main_workspace_root()?;
    list_workspaces_at(&repo_root)
}

/// List all workspaces at a specific repository root.
fn list_workspaces_at(repo_root: &Path) -> Result<Vec<WorkspaceInfo>> {
    // Use jj workspace list with a custom template for easier parsing
    // Note: Using self.target() to get the working copy commit (jj 0.35+)
    let output = Command::new("jj")
        .args([
            "workspace",
            "list",
            "--template",
            r#"self.name() ++ "\t" ++ self.target().commit_id().short(12) ++ "\n""#,
        ])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInRepo {
            vcs: "jj".to_string(),
        });
    }

    parse_workspace_list(&output.stdout, repo_root)
}

fn parse_workspace_list(bytes: &[u8], repo_root: &Path) -> Result<Vec<WorkspaceInfo>> {
    let text = String::from_utf8_lossy(bytes);
    let mut workspaces = Vec::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }

        // jj may quote workspace names that contain special characters
        let name = parts[0].trim_matches('"').to_string();
        let commit_id = parts[1].to_string();

        // Get workspace path using jj workspace root --name (if available)
        // or search for .jj/repo files that point to the repo
        let path = get_workspace_path_by_name(repo_root, &name)
            .or_else(|| {
                if name == "default" {
                    Some(repo_root.to_path_buf())
                } else {
                    // Search for workspace directory with .jj/repo pointing to this repo
                    find_workspace_path_by_search(repo_root, &name)
                }
            })
            .unwrap_or_else(|| repo_root.join(&name));

        // Get bookmark for this workspace
        let branch = if path.exists() {
            get_workspace_bookmark(&path).unwrap_or(None)
        } else {
            None
        };

        workspaces.push(WorkspaceInfo {
            path,
            head: commit_id,
            branch,
            // In jj, "default" workspace is the main workspace
            is_main: name == "default",
            is_locked: false, // jj doesn't have workspace locking
            workspace_name: Some(name),
        });
    }

    Ok(workspaces)
}

/// Search for a workspace directory by looking for .jj/repo files that point to this repo.
///
/// This is used as a fallback when `jj workspace root --name` is not available (jj < 0.38).
fn find_workspace_path_by_search(repo_root: &Path, workspace_name: &str) -> Option<PathBuf> {
    // Strategy:
    // 1. Check if workspace exists inside the repo root
    // 2. Check sibling directories of the repo root
    // 3. Check parent's siblings (one level up)

    let repo_jj_dir = repo_root.join(".jj").join("repo");

    // Helper to check if a directory is a workspace for this repo with the given name
    let is_matching_workspace = |path: &Path| -> bool {
        // Check if this directory has a .jj/repo file
        let jj_repo_file = path.join(".jj").join("repo");
        if !jj_repo_file.is_file() {
            return false;
        }

        // Check if the directory name matches the workspace name
        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir_name != workspace_name {
            return false;
        }

        // Read the .jj/repo file and check if it points to our repo
        if let Ok(content) = std::fs::read(&jj_repo_file) {
            let repo_path_str = String::from_utf8_lossy(&content);
            let repo_path = PathBuf::from(repo_path_str.trim());

            // Resolve the path relative to the workspace's .jj directory
            let jj_dir = path.join(".jj");
            let absolute_repo_path = if repo_path.is_absolute() {
                repo_path
            } else {
                jj_dir.join(&repo_path)
            };

            // Check if it points to the same repo
            if let Ok(canonical) = absolute_repo_path.canonicalize()
                && let Ok(our_repo) = repo_jj_dir.canonicalize()
            {
                return canonical == our_repo;
            }
        }

        false
    };

    // 1. Check inside repo root
    let inside_repo = repo_root.join(workspace_name);
    if inside_repo.exists() && is_matching_workspace(&inside_repo) {
        return Some(inside_repo);
    }

    // 2. Check sibling directories of repo root
    if let Some(parent) = repo_root.parent() {
        let sibling = parent.join(workspace_name);
        if sibling.exists() && is_matching_workspace(&sibling) {
            return Some(sibling);
        }

        // 3. Check all entries in parent directory for matching workspace
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && is_matching_workspace(&path) {
                    return Some(path);
                }
            }
        }
    }

    // 4. Check one level up (parent's parent's children)
    if let Some(parent) = repo_root.parent()
        && let Some(grandparent) = parent.parent()
        && let Ok(entries) = std::fs::read_dir(grandparent)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let candidate = path.join(workspace_name);
                if candidate.exists() && is_matching_workspace(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Try to get workspace path using jj workspace root --name (jj 0.38+)
fn get_workspace_path_by_name(repo_root: &Path, name: &str) -> Option<PathBuf> {
    let output = Command::new("jj")
        .args(["workspace", "root", "--name", name])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(PathBuf::from(path))
    } else {
        None
    }
}

/// Get details (bookmark, change_id) for a specific workspace.
fn get_workspace_bookmark(workspace_path: &Path) -> Result<Option<String>> {
    // Get any associated bookmarks for the current change
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            "@",
            "--no-graph",
            "-T",
            "bookmarks.join(\",\")",
        ])
        .current_dir(workspace_path)
        .output();

    if let Ok(output) = output
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().next().unwrap_or("").trim();
        if line.is_empty() {
            return Ok(None);
        }
        // Take the first bookmark if multiple
        let bookmark = line.split(',').next().map(|b| b.to_string());
        return Ok(bookmark);
    }

    Ok(None)
}

/// Get the status of a workspace.
pub(crate) fn workspace_status(workspace_path: &Path) -> Result<WorkspaceStatus> {
    // Use jj status to check for changes
    let output = Command::new("jj")
        .args(["status"])
        .current_dir(workspace_path)
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInRepo {
            vcs: "jj".to_string(),
        });
    }

    parse_jj_status(&output.stdout)
}

fn parse_jj_status(bytes: &[u8]) -> Result<WorkspaceStatus> {
    let text = String::from_utf8_lossy(bytes);
    let mut modified_count = 0;
    let mut deleted_count = 0;
    let untracked_count = 0;

    for line in text.lines() {
        let line = line.trim();

        // jj status format: "M file.txt" or "A file.txt" or "D file.txt"
        if line.starts_with("M ") {
            modified_count += 1;
        } else if line.starts_with("D ") {
            deleted_count += 1;
        } else if line.starts_with("A ") {
            // In jj, new files are immediately tracked, so count as modified
            modified_count += 1;
        }
        // jj doesn't have untracked files in the same way git does
    }

    let has_uncommitted_changes = modified_count > 0 || deleted_count > 0 || untracked_count > 0;

    Ok(WorkspaceStatus {
        has_uncommitted_changes,
        modified_count,
        deleted_count,
        untracked_count,
    })
}

/// Check for unpushed changes in a workspace.
///
/// In jj, this checks if there are commits not on any remote tracking branch.
pub(crate) fn workspace_unpushed(workspace_path: &Path) -> Result<UnpushedInfo> {
    // Check if @ has any bookmarks that are ahead of their remote counterparts
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            "heads(::@) ~ heads(::remote_bookmarks())",
            "--no-graph",
            "-T",
            r#"change_id.short() ++ "\n""#,
        ])
        .current_dir(workspace_path)
        .output();

    if let Ok(output) = output
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let count = text.lines().filter(|l| !l.trim().is_empty()).count();
        return Ok(UnpushedInfo {
            has_unpushed: count > 0,
            count,
        });
    }

    Ok(UnpushedInfo {
        has_unpushed: false,
        count: 0,
    })
}

/// List all files tracked by jj in the repository.
pub(crate) fn list_tracked_files(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("jj")
        .args(["file", "list"])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInRepo {
            vcs: "jj".to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .collect())
}

/// List local bookmarks.
pub(crate) fn list_bookmarks() -> Result<Vec<String>> {
    let output = Command::new("jj")
        .args(["bookmark", "list", "--template", r#"name ++ "\n""#])
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInRepo {
            vcs: "jj".to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect())
}

/// List remote bookmarks.
pub(crate) fn list_remote_bookmarks() -> Result<Vec<String>> {
    let output = Command::new("jj")
        .args([
            "bookmark",
            "list",
            "--all-remotes",
            "--template",
            r#"if(remote, name ++ "@" ++ remote ++ "\n")"#,
        ])
        .output()?;

    if !output.status.success() {
        return Err(Error::NotInRepo {
            vcs: "jj".to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect())
}

/// Get recent commits for a revset.
pub(crate) fn log_oneline(revset: &str, limit: usize) -> Result<Vec<String>> {
    let output = Command::new("jj")
        .args([
            "log",
            "-r",
            &format!("{}::@ | @::{}", revset, revset),
            "--limit",
            &limit.to_string(),
            "--no-graph",
            "-T",
            r#"change_id.short(12) ++ " " ++ description.first_line() ++ "\n""#,
        ])
        .output()?;

    if !output.status.success() {
        return Err(Error::JjCommandFailed {
            command: format!("jj log -r {}", revset),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect())
}

/// Validate a bookmark name.
///
/// jj is more permissive with bookmark names, but we do basic validation.
pub(crate) fn validate_bookmark_name(name: &str) -> Result<Option<String>> {
    // jj bookmark names are fairly permissive
    // Just check for obviously invalid names
    if name.is_empty() {
        return Ok(Some("Bookmark name cannot be empty".to_string()));
    }

    if name.contains('\0') {
        return Ok(Some(
            "Bookmark name cannot contain null character".to_string(),
        ));
    }

    if name.starts_with('-') {
        return Ok(Some("Bookmark name cannot start with '-'".to_string()));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_bookmark_name_valid() {
        assert_eq!(validate_bookmark_name("main").unwrap(), None);
        assert_eq!(validate_bookmark_name("feature/test").unwrap(), None);
        assert_eq!(validate_bookmark_name("my-bookmark").unwrap(), None);
    }

    #[test]
    fn test_validate_bookmark_name_empty() {
        assert!(validate_bookmark_name("").unwrap().is_some());
    }

    #[test]
    fn test_validate_bookmark_name_starts_with_dash() {
        assert!(validate_bookmark_name("-invalid").unwrap().is_some());
    }

    #[test]
    fn test_parse_jj_status_empty() {
        let output = b"";
        let result = parse_jj_status(output).unwrap();
        assert!(!result.has_uncommitted_changes);
    }

    #[test]
    fn test_parse_jj_status_modified() {
        let output = b"M file1.txt\nM file2.txt\n";
        let result = parse_jj_status(output).unwrap();
        assert!(result.has_uncommitted_changes);
        assert_eq!(result.modified_count, 2);
    }

    #[test]
    fn test_parse_jj_status_deleted() {
        let output = b"D file1.txt\n";
        let result = parse_jj_status(output).unwrap();
        assert!(result.has_uncommitted_changes);
        assert_eq!(result.deleted_count, 1);
    }

    #[test]
    fn test_parse_workspace_list_empty() {
        let result = parse_workspace_list(b"", Path::new("/repo")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_workspace_list_single() {
        let output = b"default\tabc123456789\n";
        let result = parse_workspace_list(output, Path::new("/repo")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].workspace_name, Some("default".to_string()));
        assert_eq!(result[0].head, "abc123456789");
        assert!(result[0].is_main);
    }

    #[test]
    fn test_parse_workspace_list_multiple() {
        let output = b"default\tabc123456789\nfeature\tdef987654321\n";
        let result = parse_workspace_list(output, Path::new("/repo")).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].is_main);
        assert!(!result[1].is_main);
        assert_eq!(result[1].workspace_name, Some("feature".to_string()));
    }

    #[test]
    fn test_parse_workspace_list_quoted_name() {
        // jj quotes workspace names that contain special characters
        let output = b"default\tabc123456789\n\"my-workspace\"\tdef987654321\n";
        let result = parse_workspace_list(output, Path::new("/repo")).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].workspace_name, Some("default".to_string()));
        // Quotes should be stripped from workspace name
        assert_eq!(result[1].workspace_name, Some("my-workspace".to_string()));
    }
}
