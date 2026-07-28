//! Remove worktree/workspace command implementation.
//!
//! Removes git worktrees or jj workspaces with safety checks for uncommitted changes
//! and unpushed commits. Supports interactive selection and dry-run mode.

use crate::cli::RemoveArgs;
use crate::color::{self, ColorConfig};
use crate::command::trust_check::{TrustHint, load_config_with_trust_check};
use crate::error::{Error, Result};
use crate::hook::{self, HookEnv};
use crate::interactive::{SafetyWarning, run_remove_confirmation, run_remove_selection};
use crate::output::Output;
use crate::prompt;
use crate::vcs::{self, WorkspaceInfo};

use std::path::PathBuf;

pub(crate) fn run(args: RemoveArgs, color: ColorConfig) -> Result<()> {
    let output = Output::new(args.quiet, color);

    let provider = vcs::get_provider()?;

    if !provider.is_inside_repo() {
        return Err(Error::NotInAnyRepo);
    }

    let repo_root = provider.main_workspace_root()?;

    // Get main workspace path for trust operations
    let main_worktree_path = provider.main_workspace_path_for(&repo_root)?;

    let config = load_config_with_trust_check(
        &main_worktree_path,
        &main_worktree_path,
        true,
        TrustHint::None,
    )?;
    color::set_cli_theme(&config.ui.colors);

    let worktrees = provider.list_workspaces()?;

    let targets = if args.interactive {
        select_worktrees_interactively(&worktrees)?
    } else if args.current {
        let current_worktree = find_current_worktree(&worktrees)?;
        let mut paths = vec![current_worktree];
        // Also include any explicitly specified paths
        if !args.paths.is_empty() {
            paths.extend(resolve_worktree_paths(&args.paths, &worktrees)?);
        }
        paths
    } else if args.paths.is_empty() {
        return Err(Error::PathRequired);
    } else {
        resolve_worktree_paths(&args.paths, &worktrees)?
    };

    for path in &targets {
        if is_main_worktree(path, &worktrees) {
            return Err(Error::CannotRemoveMainWorktree { path: path.clone() });
        }
    }

    let warnings = if !args.force {
        collect_safety_warnings(&targets, provider.as_ref())?
    } else {
        vec![]
    };

    if !warnings.is_empty() {
        if args.dry_run {
            for warning in &warnings {
                display_warning(&output, warning);
            }
        } else if prompt::is_interactive() {
            if !run_remove_confirmation(&warnings)? {
                return Err(Error::Aborted);
            }
        } else {
            let first_warning = &warnings[0];
            if first_warning.has_uncommitted {
                return Err(Error::WorktreeHasUncommittedChanges {
                    path: first_warning.path.clone(),
                });
            } else {
                return Err(Error::WorktreeHasUnpushedCommits {
                    path: first_warning.path.clone(),
                });
            }
        }
    }

    for path in &targets {
        // Create hook environment
        let worktree_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let branch = worktrees
            .iter()
            .find(|wt| &wt.path == path)
            .and_then(|wt| wt.branch.as_deref())
            .map(|b| b.strip_prefix("refs/heads/").unwrap_or(b).to_string());

        let hook_shell = {
            #[cfg(windows)]
            {
                args.hook_shell
                    .clone()
                    .or_else(|| config.hooks.hook_shell.clone())
            }
            #[cfg(not(windows))]
            {
                None
            }
        };

        let hook_env = HookEnv {
            worktree_path: path.to_string_lossy().to_string(),
            worktree_name,
            branch,
            repo_root: main_worktree_path.to_string_lossy().to_string(),
            vcs_type: provider.name().to_string(),
            change_id: None,
            commit_id: None,
            hook_shell,
        };

        // Run pre_remove hooks
        if !config.hooks.pre_remove.is_empty() {
            if args.dry_run {
                if !args.quiet {
                    hook::dry_run_hooks("pre_remove", &config.hooks.pre_remove, &output);
                }
            } else {
                hook::run_pre_remove(&config.hooks, &hook_env, path, &output)?;
            }
        }

        if args.dry_run {
            output.dry_run(&format!("Would remove: {}", path.display()));
        } else {
            let use_force = args.force || !warnings.is_empty();
            provider.workspace_remove_checked(path, use_force)?;
            output.remove(path);
        }

        // Run post_remove hooks
        if !config.hooks.post_remove.is_empty() {
            if args.dry_run {
                if !args.quiet {
                    hook::dry_run_hooks("post_remove", &config.hooks.post_remove, &output);
                }
            } else if let Err(e) =
                hook::run_post_remove(&config.hooks, &hook_env, &main_worktree_path, &output)
            {
                // Extract exit code from error if available
                let exit_code = match &e {
                    Error::HookFailed { exit_code, .. } => *exit_code,
                    _ => None,
                };
                output.hook_warning("post_remove", &e.to_string(), exit_code);
                output.hook_note("Worktree was removed but post-cleanup may be incomplete.");
            }
        }
    }

    Ok(())
}

fn select_worktrees_interactively(worktrees: &[WorkspaceInfo]) -> Result<Vec<PathBuf>> {
    // Clear screen before entering interactive mode
    prompt::clear_screen_interactive()?;

    let paths = run_remove_selection(worktrees)?;
    Ok(paths)
}

fn find_current_worktree(worktrees: &[WorkspaceInfo]) -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let current_dir = current_dir
        .canonicalize()
        .unwrap_or_else(|_| current_dir.clone());

    for wt in worktrees {
        if current_dir.starts_with(&wt.path) {
            return Ok(wt.path.clone());
        }
    }

    Err(Error::NotInWorktree)
}

fn resolve_worktree_paths(paths: &[PathBuf], worktrees: &[WorkspaceInfo]) -> Result<Vec<PathBuf>> {
    let mut resolved = Vec::new();

    for path in paths {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(path)
        };

        let canonical = abs_path
            .canonicalize()
            .map_err(|_| Error::WorktreeNotFound { path: path.clone() })?;

        if !worktrees.iter().any(|wt| wt.path == canonical) {
            return Err(Error::WorktreeNotFound { path: path.clone() });
        }

        resolved.push(canonical);
    }

    Ok(resolved)
}

fn is_main_worktree(path: &PathBuf, worktrees: &[WorkspaceInfo]) -> bool {
    worktrees
        .iter()
        .find(|wt| &wt.path == path)
        .map(|wt| wt.is_main)
        .unwrap_or(false)
}

fn collect_safety_warnings(
    targets: &[PathBuf],
    provider: &dyn vcs::VcsProvider,
) -> Result<Vec<SafetyWarning>> {
    let mut warnings = Vec::new();

    for path in targets {
        let status = provider.workspace_status(path)?;
        let unpushed = provider.workspace_unpushed(path)?;

        if status.has_uncommitted_changes || unpushed.has_unpushed {
            warnings.push(SafetyWarning {
                path: path.clone(),
                has_uncommitted: status.has_uncommitted_changes,
                modified_count: status.modified_count,
                deleted_count: status.deleted_count,
                untracked_count: status.untracked_count,
                has_unpushed: unpushed.has_unpushed,
                unpushed_count: unpushed.count,
            });
        }
    }

    Ok(warnings)
}

fn display_warning(output: &Output, warning: &SafetyWarning) {
    if warning.modified_count > 0 {
        output.safety_warning(
            &warning.path,
            &format!("{} modified file(s)", warning.modified_count),
        );
    }
    if warning.deleted_count > 0 {
        output.safety_warning(
            &warning.path,
            &format!("{} deleted file(s)", warning.deleted_count),
        );
    }
    if warning.untracked_count > 0 {
        output.safety_warning(
            &warning.path,
            &format!("{} untracked file(s)", warning.untracked_count),
        );
    }
    if warning.has_unpushed {
        output.safety_warning(
            &warning.path,
            &format!("{} unpushed commit(s)", warning.unpushed_count),
        );
    }
}
