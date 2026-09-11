//! Setup command implementation.
//!
//! Re-runs mkdir/link/copy from `.kabu` config against an existing
//! worktree/workspace, so config changes (new `exclude` entries, a
//! changed `on_conflict`, and so on) can be applied without recreating
//! the worktree. Does not run hooks or touch VCS state.

use crate::cli::SetupArgs;
use crate::color::{self, ColorConfig};
use crate::command::add::{SetupOptions, run_setup};
use crate::command::remove::{find_current_worktree, is_main_worktree, resolve_worktree_paths};
use crate::command::trust_check::{TrustHint, load_config_with_trust_check};
use crate::error::{Error, Result};
use crate::output::Output;
use crate::vcs;

/// Execute the `setup` subcommand.
pub(crate) fn run(args: SetupArgs, color: ColorConfig) -> Result<()> {
    let output = Output::new(args.quiet, color);

    let provider = vcs::get_provider()?;

    if !provider.is_inside_repo() {
        return Err(Error::NotInAnyRepo);
    }

    let repo_root = provider.main_workspace_root()?;

    // `setup` never runs hooks, so it doesn't need trust enforcement --
    // only mkdir/link/copy, the same file operations `add` performs
    // without a trust check.
    let config = load_config_with_trust_check(&repo_root, &repo_root, false, TrustHint::None)?;
    color::set_cli_theme(&config.ui.colors);

    let worktrees = provider.list_workspaces()?;

    let target = match &args.path {
        Some(path) => {
            let mut resolved = resolve_worktree_paths(std::slice::from_ref(path), &worktrees)?;
            resolved
                .pop()
                .ok_or_else(|| Error::WorktreeNotFound { path: path.clone() })?
        }
        None => find_current_worktree(&worktrees)?,
    };

    if is_main_worktree(&target, &worktrees) {
        return Err(Error::CannotSetupMainWorkspace { path: target });
    }

    run_setup(
        SetupOptions {
            on_conflict: args.on_conflict,
            dry_run: args.dry_run,
            verbose: args.verbose,
        },
        &config,
        &repo_root,
        &target,
        &output,
        provider.as_ref(),
    )?;

    if !args.dry_run {
        output.results_success(&format!("Setup complete: {}", target.display()));
    }

    Ok(())
}
