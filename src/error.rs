use std::path::PathBuf;

use thiserror::Error;

/// Application errors.
#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("Failed to parse config: {message}")]
    ConfigParse { message: String },

    #[error(
        "Your '.kabu/config.yaml' or '.kabu/config.toml' configuration is invalid.\n\n{message}"
    )]
    ConfigValidation { message: String },

    #[error("Failed to parse global config: {message}")]
    GlobalConfigParse { message: String },

    #[error("Your global configuration is invalid.\n\n{message}")]
    GlobalConfigValidation { message: String },

    #[error("Not inside a git repository. Run this command from within a git repository.")]
    NotInGitRepo,

    #[error("Not inside a {vcs} repository. Run this command from within a repository.")]
    NotInRepo { vcs: String },

    #[error("Not inside a git or jj repository. Run this command from within a repository.")]
    NotInAnyRepo,

    #[error("Path is required. Use -i for interactive mode or provide a path.")]
    PathRequired,

    #[error("git worktree add failed:\n{stderr}")]
    GitWorktreeAddFailed { stderr: String },

    #[error("jj workspace add failed:\n{stderr}")]
    JjWorkspaceAddFailed { stderr: String },

    #[error(
        "A source path in '.kabu/config.yaml' or '.kabu/config.toml' was not found.\n\n  Path: {path}\n  Reason: This file does not exist at the root of your repository.\n  Fix:    Ensure the path is correct and relative to the repository root."
    )]
    SourceNotFound { path: String },

    #[error("Failed to create symlink: {source} -> {target}")]
    SymlinkFailed {
        source: PathBuf,
        target: PathBuf,
        #[source]
        cause: std::io::Error,
    },

    #[error("Failed to copy: {source} -> {target}")]
    CopyFailed {
        source: PathBuf,
        target: PathBuf,
        #[source]
        cause: std::io::Error,
    },

    #[error(
        "Refusing to write outside the worktree.\n\n  Target:   {target}\n  Resolves to: {resolved}\n  Reason:   A path component of the target is a symlink that points outside the worktree.\n  Fix:      Remove or repoint the symlink, or change the config so this path is not written to."
    )]
    TargetEscapesWorktree { target: PathBuf, resolved: PathBuf },

    #[error("Operation aborted by user")]
    Aborted,

    #[error(
        "Interactive prompt required but running in non-interactive mode\n  Use --on-conflict to specify how to handle conflicts."
    )]
    NonInteractive,

    #[error("Selector error: {message}")]
    Selector { message: String },

    #[error("Interactive prompt required for {command}")]
    InteractiveRequired { command: &'static str },

    #[error("kabu cd requires shell integration")]
    CdRequiresShellIntegration,

    #[error("The main worktree/workspace cannot be removed.\n  Path: {}", .path.display())]
    CannotRemoveMainWorktree { path: PathBuf },

    #[error(
        "kabu setup cannot target the main worktree/workspace, since source and target paths would be identical.\n  Path: {}",
        .path.display()
    )]
    CannotSetupMainWorkspace { path: PathBuf },

    #[error("No worktrees/workspaces available to remove")]
    NoWorktreesToRemove,

    #[error("No worktrees/workspaces found")]
    NoWorktreesFound,

    #[error("Current directory is not inside any worktree/workspace")]
    NotInWorktree,

    #[error("Has uncommitted changes: {}\n  Use --force to remove anyway.", .path.display())]
    WorktreeHasUncommittedChanges { path: PathBuf },

    #[error("Has unpushed commits: {}\n  Use --force to remove anyway.", .path.display())]
    WorktreeHasUnpushedCommits { path: PathBuf },

    #[error("Worktree/workspace not found: {path}")]
    WorktreeNotFound { path: PathBuf },

    #[error("git worktree remove failed:\n{stderr}")]
    GitWorktreeRemoveFailed { stderr: String },

    #[error("jj workspace forget failed:\n{stderr}")]
    JjWorkspaceForgetFailed { stderr: String },

    #[error("git command failed: {command}\n{stderr}")]
    GitCommandFailed { command: String, stderr: String },

    #[error("jj command failed: {command}\n{stderr}")]
    JjCommandFailed { command: String, stderr: String },

    #[cfg(windows)]
    #[error(
        "Failed to create symlink: permission denied\n  Enable Developer Mode in Windows Settings or run as administrator."
    )]
    WindowsSymlinkPermission,

    // Empty message: detailed warning is displayed by trust command
    #[error("")]
    HooksNotTrusted,

    // Empty message: silent exit for --check flag (returns exit code 1)
    #[error("")]
    TrustCheckFailed,

    #[error("Hook execution failed: {command}\n  {cause}")]
    HookExecutionFailed { command: String, cause: String },

    #[error("Hook failed: {command}")]
    HookFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("Trust storage directory not found")]
    TrustStorageNotFound,

    #[error("Trust file corrupted: {message}")]
    TrustFileCorrupted { message: String },

    #[error("Trust file serialization failed: {message}")]
    TrustFileSerialization { message: String },

    #[error("Trust verification failed: {message}")]
    TrustVerificationFailed { message: String },

    #[error("Config file not found: {path}")]
    ConfigNotFound { path: PathBuf },

    #[error("Config file already exists: {path}")]
    ConfigAlreadyExists { path: PathBuf },

    #[error("Global config directory not found")]
    GlobalConfigDirNotFound,

    #[error("No hooks defined in .kabu/config.yaml or .kabu/config.toml")]
    NoHooksDefined,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias for this crate.
pub(crate) type Result<T> = std::result::Result<T, Error>;
