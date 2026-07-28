//! VCS abstraction layer for supporting multiple version control systems.
//!
//! This module provides a unified interface for workspace operations across
//! different VCS backends (Git worktrees, jj workspaces).

mod detect;
mod git;
mod jj;

pub(crate) use detect::{VcsType, detect_vcs};
pub(crate) use git::GitProvider;
pub(crate) use jj::JjProvider;

use crate::cli::AddArgs;
use crate::error::Result;
use std::path::{Path, PathBuf};

/// Type of VCS detected in a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VcsKind {
    /// Pure Git repository (no jj)
    Git,
    /// Pure jj repository (non-colocated)
    Jj,
    /// Colocated jj repository (both .git and .jj exist)
    JjColocated,
}

impl VcsKind {
    /// Returns the display name for this VCS.
    pub fn name(&self) -> &'static str {
        match self {
            VcsKind::Git => "git",
            VcsKind::Jj | VcsKind::JjColocated => "jj",
        }
    }

    /// Returns the workspace type name for this VCS.
    pub fn workspace_type(&self) -> &'static str {
        match self {
            VcsKind::Git => "worktree",
            VcsKind::Jj | VcsKind::JjColocated => "workspace",
        }
    }
}

/// Information about a workspace/worktree.
#[derive(Debug, Clone)]
pub(crate) struct WorkspaceInfo {
    /// Full path to the workspace.
    pub path: PathBuf,
    /// VCS-specific identifier (commit hash for git, commit/change ID for jj).
    pub head: String,
    /// Branch name (git) or bookmark name (jj). None if detached.
    pub branch: Option<String>,
    /// Whether this is the main/primary workspace.
    pub is_main: bool,
    /// Whether the workspace is locked (git only, always false for jj).
    pub is_locked: bool,
    /// Workspace name (jj only, None for git).
    pub workspace_name: Option<String>,
}

/// Working copy status information.
#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceStatus {
    pub has_uncommitted_changes: bool,
    pub modified_count: usize,
    pub deleted_count: usize,
    pub untracked_count: usize,
}

/// Unpushed commits information.
#[derive(Debug, Clone, Default)]
pub(crate) struct UnpushedInfo {
    pub has_unpushed: bool,
    pub count: usize,
}

/// Trait for VCS providers.
///
/// Implementations provide workspace operations for different VCS backends.
pub(crate) trait VcsProvider {
    /// Get the VCS kind.
    fn kind(&self) -> VcsKind;

    /// Get the VCS name for display.
    fn name(&self) -> &'static str {
        self.kind().name()
    }

    /// Get the workspace type name (worktree/workspace).
    fn workspace_type(&self) -> &'static str {
        self.kind().workspace_type()
    }

    /// Check if currently inside a repository of this VCS type.
    fn is_inside_repo(&self) -> bool;

    /// Get the main worktree/workspace's root directory.
    fn main_workspace_root(&self) -> Result<PathBuf>;

    /// Get the repository name (directory name of repo root).
    fn repository_name(&self) -> Result<String> {
        let root = self.main_workspace_root()?;
        root.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| crate::error::Error::NotInRepo {
                vcs: self.name().to_string(),
            })
    }

    /// Get the main workspace path for a specific repository directory.
    fn main_workspace_path_for(&self, repo_root: &Path) -> Result<PathBuf>;

    /// Add a new workspace.
    fn workspace_add(&self, args: &AddArgs, path: &Path) -> Result<()>;

    /// Remove a workspace.
    ///
    /// For git, this calls `git worktree remove`.
    /// For jj, this calls `jj workspace forget` and optionally deletes the directory.
    fn workspace_remove(&self, path: &Path, force: bool) -> Result<()>;

    /// Remove a workspace with error checking (returns error on failure).
    fn workspace_remove_checked(&self, path: &Path, force: bool) -> Result<()>;

    /// List all workspaces.
    fn list_workspaces(&self) -> Result<Vec<WorkspaceInfo>>;

    /// Get the status of a workspace.
    fn workspace_status(&self, path: &Path) -> Result<WorkspaceStatus>;

    /// Get unpushed commits/changes for a workspace.
    fn workspace_unpushed(&self, path: &Path) -> Result<UnpushedInfo>;

    /// Get the upstream branch name for a workspace.
    fn get_upstream(&self, path: &Path) -> Result<Option<String>>;

    /// List all files tracked by the VCS in the repository.
    fn list_tracked_files(&self, repo_root: &Path) -> Result<Vec<PathBuf>>;

    /// List local branches (git) or bookmarks (jj).
    fn list_branches(&self) -> Result<Vec<String>>;

    /// List remote branches (git only, returns empty for jj).
    fn list_remote_branches(&self) -> Result<Vec<String>>;

    /// Get recent commits/changes for a branch or revision.
    fn log_oneline(&self, commitish: &str, limit: usize) -> Result<Vec<String>>;

    /// Validate a branch/bookmark name.
    fn validate_branch_name(&self, name: &str) -> Result<Option<String>>;

    /// Force-delete a branch (git) or bookmark (jj).
    fn delete_branch(&self, name: &str) -> Result<()>;
}

/// Get the appropriate VCS provider for the current directory.
pub(crate) fn get_provider() -> Result<Box<dyn VcsProvider>> {
    match detect_vcs()? {
        VcsType::Git => Ok(Box::new(GitProvider)),
        VcsType::Jj | VcsType::JjColocated => Ok(Box::new(JjProvider)),
    }
}
