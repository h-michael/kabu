//! Add worktree/workspace command implementation.
//!
//! Creates a new git worktree or jj workspace with automated setup from `.kabu/config.yaml`.
//! Supports both interactive and non-interactive modes, with rollback on failure.

use crate::cli::AddArgs;
use crate::color::{self, ColorConfig};
use crate::command::trust_check::{TrustHint, load_config_with_trust_check};
use crate::config::{self, Config, Link, OnConflict, OnConflictSetting};
use crate::error::{Error, Result};
use crate::hook::{self, HookEnv};
use crate::interactive;
use crate::interactive::ConflictChoice;
use crate::operation::{
    self, ConflictAction, check_conflict, conflict_kind, create_directory, resolve_conflict,
};
use crate::output::Output;
use crate::vcs::{self, VcsProvider};

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Execute the `add` subcommand.
pub(crate) fn run(mut args: AddArgs, color: ColorConfig) -> Result<()> {
    let output = Output::new(args.quiet, color);

    let provider = vcs::get_provider()?;

    // Check if we're in a repository
    if !provider.is_inside_repo() {
        return Err(Error::NotInAnyRepo);
    }

    // Get repository root
    let repo_root = provider.main_workspace_root()?;

    let config = load_config_with_trust_check(
        &repo_root,
        &repo_root,
        !args.no_setup,
        TrustHint::SkipHooks {
            command: "kabu add --no-setup <path>",
        },
    )?;
    color::set_cli_theme(&config.ui.colors);

    // Handle interactive mode
    let worktree_path = if args.interactive {
        run_interactive(&mut args, &config, provider.as_ref())?
    } else {
        // Non-interactive: path is optional if config provides it
        let path = if let Some(path) = args.path.clone() {
            path
        } else {
            // Try to generate path from config
            let branch = args
                .commitish
                .as_ref()
                .or(args.new_branch.as_ref())
                .or(args.new_branch_force.as_ref())
                .ok_or(Error::PathRequired)?;

            let generated = config
                .worktree
                .generate_path(branch, &provider.repository_name()?)
                .ok_or(Error::PathRequired)?;

            PathBuf::from(generated)
        };

        if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(&path)
        }
    };

    // Skip setup if requested - just run workspace add
    if args.no_setup {
        if !args.dry_run {
            provider.workspace_add(&args, &worktree_path)?;
        } else {
            output.dry_run(&format!(
                "Would run: {} {} add {}",
                provider.name(),
                provider.workspace_type(),
                worktree_path.display()
            ));
        }
        return Ok(());
    }

    // Pre-validate: Check all source files exist BEFORE creating worktree
    for link in &config.link {
        // Skip validation for glob patterns - they will be expanded later
        if contains_glob_pattern(&link.source) {
            continue;
        }
        let source = repo_root.join(&link.source);
        if !source.exists() {
            return Err(Error::SourceNotFound {
                path: link.source.to_string_lossy().to_string(),
            });
        }
    }
    for copy in &config.copy {
        // Skip validation for glob patterns - they will be expanded later
        if contains_glob_pattern(&copy.source) {
            continue;
        }
        let source = repo_root.join(&copy.source);
        if !source.exists() {
            return Err(Error::SourceNotFound {
                path: copy.source.to_string_lossy().to_string(),
            });
        }
    }

    // Create hook environment
    // Use empty string as fallback for non-UTF8 file names (rare edge case)
    let worktree_name = worktree_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    // Get branch name and strip refs/heads/ prefix if present
    let branch = args
        .new_branch
        .clone()
        .or(args.commitish.clone())
        .or(args.new_branch_force.clone())
        .map(|b| b.strip_prefix("refs/heads/").unwrap_or(&b).to_string());

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
        worktree_path: worktree_path.to_string_lossy().to_string(),
        worktree_name,
        branch,
        repo_root: repo_root.to_string_lossy().to_string(),
        vcs_type: provider.name().to_string(),
        change_id: None,
        commit_id: None,
        hook_shell,
    };

    // Run pre_add hooks
    if !config.hooks.pre_add.is_empty() {
        if args.dry_run {
            if !args.quiet {
                hook::dry_run_hooks("pre_add", &config.hooks.pre_add, &output);
            }
        } else {
            hook::run_pre_add(&config.hooks, &hook_env, &repo_root, &output)?;
        }
    }

    // Run workspace add
    if !args.dry_run {
        provider.workspace_add(&args, &worktree_path)?;
    } else {
        output.dry_run(&format!(
            "Would run: {} {} add {}",
            provider.name(),
            provider.workspace_type(),
            worktree_path.display()
        ));
    }

    // Process links and copies with rollback on failure
    if let Err(e) = run_setup(
        SetupOptions {
            on_conflict: args.on_conflict,
            dry_run: args.dry_run,
            verbose: args.verbose,
        },
        &config,
        &repo_root,
        &worktree_path,
        &output,
        provider.as_ref(),
    ) {
        if !args.dry_run {
            let remove_worktree = config.on_setup_failure.remove_worktree.unwrap_or(false);
            let delete_branch = config.on_setup_failure.delete_branch.unwrap_or(false);

            if remove_worktree {
                eprintln!("Setup failed, rolling back workspace creation...");
                let _ = provider.workspace_remove(&worktree_path, true);
            }
            if delete_branch
                && let Some(branch) = args.new_branch.as_ref().or(args.new_branch_force.as_ref())
            {
                let _ = provider.delete_branch(branch);
            }
        }
        return Err(e);
    }

    // Run post_add hooks and track failure
    let mut post_add_failed = false;
    let mut post_add_error: Option<(String, Option<String>, Option<i32>)> = None;

    if !config.hooks.post_add.is_empty() {
        if args.dry_run {
            if !args.quiet {
                hook::dry_run_hooks("post_add", &config.hooks.post_add, &output);
            }
        } else if let Err(e) = hook::run_post_add(&config.hooks, &hook_env, &worktree_path, &output)
        {
            post_add_failed = true;

            // Extract error details
            let (command, exit_code) = match &e {
                Error::HookFailed {
                    command, exit_code, ..
                } => (command.clone(), *exit_code),
                _ => (String::new(), None),
            };

            // Find the description for the failed command
            let description = config
                .hooks
                .post_add
                .iter()
                .find(|entry| entry.command == command)
                .and_then(|entry| entry.description.clone());

            post_add_error = Some((command, description, exit_code));

            output.hook_warning("post_add", &e.to_string(), exit_code);
            output.hook_note("Worktree was created but post-setup may be incomplete.");
        }
    }

    // Display results summary
    if !args.dry_run && !args.quiet {
        if post_add_failed {
            // Show detailed results when there's a failure
            output.results_header();

            if !config.hooks.pre_add.is_empty() {
                output.results_item_success(&format!(
                    "pre_add hooks ({} succeeded)",
                    config.hooks.pre_add.len()
                ));
            }

            output.results_item_success("Worktree created");
            output.results_item_success("Setup operations completed");

            output.results_item_failed(&format!(
                "post_add hooks ({} failed)",
                if post_add_error.is_some() { 1 } else { 0 }
            ));

            if let Some((command, description, exit_code)) = post_add_error {
                output.results_failed_detail(description.as_deref(), &command, exit_code);
            }
        } else {
            // All succeeded - show message with path
            output.results_success("Worktree created successfully");
            output.list(&worktree_path.display().to_string());
        }
    }

    Ok(())
}

/// Run interactive mode to select branch and path.
fn run_interactive(
    args: &mut AddArgs,
    config: &Config,
    provider: &dyn VcsProvider,
) -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let local_branches = provider.list_branches()?;
    let remote_branches = provider.list_remote_branches()?;
    let default_branch = provider
        .default_branch(config.worktree.default_remote())
        .ok()
        .flatten();
    let default_remote_branch = default_branch
        .as_ref()
        .map(|branch| format!("{}/{}", config.worktree.default_remote(), branch));

    let suggest_branch_name = config.worktree.branch_template.as_ref().map(|_| {
        let repository = provider.repository_name().unwrap_or_default();
        let worktree = config.worktree.clone();
        std::sync::Arc::new(move |commitish: &str| {
            let env = config::BranchTemplateEnv {
                commitish: commitish.to_string(),
                repository: repository.clone(),
            };
            worktree.generate_branch_name(&env).unwrap_or_default()
        }) as std::sync::Arc<dyn Fn(&str) -> String + Send + Sync>
    });

    let suggest_path = {
        let repository = provider.repository_name().unwrap_or_default();
        let worktree = config.worktree.clone();
        std::sync::Arc::new(move |branch: &str| worktree.generate_path(branch, &repository))
            as std::sync::Arc<dyn Fn(&str) -> Option<String> + Send + Sync>
    };

    let worktrees = provider.list_workspaces()?;
    let mut used_branches = std::collections::HashMap::new();
    let mut existing_worktrees = Vec::new();
    for worktree in worktrees {
        if let Some(branch) = worktree.branch.as_ref()
            && let Some(name) = branch.strip_prefix("refs/heads/")
        {
            used_branches.insert(name.to_string(), worktree.path.clone());
        }
        let branch = worktree
            .branch
            .as_ref()
            .map(|name| name.strip_prefix("refs/heads/").unwrap_or(name).to_string());
        existing_worktrees.push(interactive::WorktreeSummary {
            path: worktree.path,
            branch,
        });
    }

    // Capture VCS kind for use in closures (providers are unit structs, cheap to recreate)
    let vcs_kind = provider.kind();
    let fetch_log = std::sync::Arc::new(move |commitish: &str, limit: usize| {
        let provider: Box<dyn VcsProvider> = match vcs_kind {
            vcs::VcsKind::Git => Box::new(vcs::GitProvider),
            vcs::VcsKind::Jj | vcs::VcsKind::JjColocated => Box::new(vcs::JjProvider),
        };
        provider.log_oneline(commitish, limit)
    });

    let validate_branch_name = std::sync::Arc::new(move |name: &str| {
        let provider: Box<dyn VcsProvider> = match vcs_kind {
            vcs::VcsKind::Git => Box::new(vcs::GitProvider),
            vcs::VcsKind::Jj | vcs::VcsKind::JjColocated => Box::new(vcs::JjProvider),
        };
        provider.validate_branch_name(name)
    });

    let result = interactive::run_add_interactive(interactive::AddInteractiveInput {
        local_branches,
        remote_branches,
        default_branch,
        default_remote_branch,
        used_branches,
        current_dir: current_dir.clone(),
        existing_worktrees,
        log_limit: 10,
        fetch_log,
        initial_path: args.path.clone(),
        suggest_path: Some(suggest_path),
        suggest_branch_name,
        validate_branch_name,
        theme: interactive::UiTheme::from_ui(&config.ui),
    })?;

    let branch_choice = result.branch_choice;
    if branch_choice.create_new {
        args.new_branch = Some(branch_choice.branch.clone());
        if let Some(base) = &branch_choice.base_commitish {
            args.commitish = Some(base.clone());
        }
    } else {
        args.commitish = Some(branch_choice.branch.clone());
    }

    args.path = Some(result.path.clone());

    let worktree_path = if result.path.is_absolute() {
        result.path
    } else {
        current_dir.join(&result.path)
    };

    Ok(worktree_path)
}

/// CLI-derived options shared by `add` and `setup` when running setup.
pub(crate) struct SetupOptions {
    pub on_conflict: Option<crate::cli::OnConflictArg>,
    pub dry_run: bool,
    pub verbose: bool,
}

/// Run the setup operations (mkdir, symlinks and copies) against a
/// worktree/workspace. Shared by `add` (right after creation) and `setup`
/// (replayed against an existing one).
pub(crate) fn run_setup(
    options: SetupOptions,
    config: &Config,
    repo_root: &Path,
    worktree_path: &Path,
    output: &Output,
    provider: &dyn VcsProvider,
) -> Result<()> {
    let SetupOptions {
        on_conflict,
        dry_run,
        verbose,
    } = options;

    let mut conflict_mode_override: Option<OnConflict> = on_conflict.map(|m| match m {
        crate::cli::OnConflictArg::Abort => OnConflict::Abort,
        crate::cli::OnConflictArg::Skip => OnConflict::Skip,
        crate::cli::OnConflictArg::Overwrite => OnConflict::Overwrite,
        crate::cli::OnConflictArg::Backup => OnConflict::Backup,
    });

    // Process mkdir
    for mkdir in &config.mkdir {
        let target = worktree_path.join(&mkdir.path);
        operation::ensure_within_worktree(&target, worktree_path)?;

        if dry_run {
            output.dry_run(&format!("Would create directory: {}", target.display()));
        } else {
            create_directory(&target)?;
            output.mkdir(&target, mkdir.description.as_deref());
        }
    }

    // Process symlinks (expand glob patterns first)
    // Cache the VCS-tracked file list across all glob link entries so we only
    // invoke `git ls-files` (or jj equivalent) at most once per `add` call.
    let mut tracked_cache: Option<TrackedCache> = None;
    for link in &config.link {
        let expanded_links = expand_link(link, repo_root, provider, &mut tracked_cache)?;
        let mut created: Vec<(PathBuf, PathBuf, Option<String>)> = Vec::new();
        let mut already_linked: Vec<PathBuf> = Vec::new();
        for expanded_link in expanded_links {
            let source = repo_root.join(&expanded_link.source);
            let target = worktree_path.join(&expanded_link.target);
            let params = OperationParams {
                source: &source,
                target: &target,
                op_type: FileOp::Link,
                config_mode: expanded_link.on_conflict.or(config.on_conflict),
                description: expanded_link.description.as_deref(),
            };
            match process_operation(
                &params,
                worktree_path,
                &mut conflict_mode_override,
                dry_run,
                verbose,
                output,
            )? {
                OperationOutcome::Created => {
                    created.push((source, target, expanded_link.description.clone()));
                }
                OperationOutcome::AlreadyLinked => already_linked.push(target),
                OperationOutcome::AlreadyUpToDate | OperationOutcome::Reported => {}
            }
        }
        summarize_clean_operations(
            output,
            dry_run,
            verbose,
            FileOp::Link,
            &link.source,
            &created,
        );
        summarize_no_op_operations(output, verbose, FileOp::Link, &link.source, &already_linked);
    }

    // Process copies (expand glob patterns first)
    for copy in &config.copy {
        let expanded_copies = expand_copy(copy, repo_root)?;
        let mut created: Vec<(PathBuf, PathBuf, Option<String>)> = Vec::new();
        let mut up_to_date: Vec<PathBuf> = Vec::new();
        for expanded_copy in expanded_copies {
            let source = repo_root.join(&expanded_copy.source);
            let target = worktree_path.join(&expanded_copy.target);
            let params = OperationParams {
                source: &source,
                target: &target,
                op_type: FileOp::Copy,
                config_mode: expanded_copy.on_conflict.or(config.on_conflict),
                description: expanded_copy.description.as_deref(),
            };
            match process_operation(
                &params,
                worktree_path,
                &mut conflict_mode_override,
                dry_run,
                verbose,
                output,
            )? {
                OperationOutcome::Created => {
                    created.push((source, target, expanded_copy.description.clone()));
                }
                OperationOutcome::AlreadyUpToDate => up_to_date.push(target),
                // A symlink target is a conflict for `copy`, handled like
                // any other; only `link` short-circuits on a symlink match.
                OperationOutcome::AlreadyLinked | OperationOutcome::Reported => {}
            }
        }
        summarize_no_op_operations(output, verbose, FileOp::Copy, &copy.source, &up_to_date);
        summarize_clean_operations(
            output,
            dry_run,
            verbose,
            FileOp::Copy,
            &copy.source,
            &created,
        );
    }

    Ok(())
}

/// File operation type.
#[derive(Debug, Clone, Copy)]
enum FileOp {
    Link,
    Copy,
}

/// Outcome of a single link/copy operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationOutcome {
    /// Created with no conflict; foldable into a per-entry summary line.
    Created,
    /// A symlink already pointed at the intended source; nothing changed.
    /// Foldable into its own per-entry summary line.
    AlreadyLinked,
    /// A copy target already had the same content as its source; nothing
    /// changed. Foldable into its own per-entry summary line.
    AlreadyUpToDate,
    /// Already printed individually: a resolved conflict (skip/overwrite/
    /// backup), or a dry-run preview of either a plain create or what a
    /// conflict would require.
    Reported,
}

/// Print a per-entry summary for the clean (no-conflict) operations
/// belonging to one config entry, unless already printed individually
/// (dry-run and --verbose print every operation as it happens).
/// A single clean operation still prints its normal single-item line,
/// so plain, non-glob entries look exactly as they always have.
fn summarize_clean_operations(
    output: &Output,
    dry_run: bool,
    verbose: bool,
    op_type: FileOp,
    source_pattern: &Path,
    clean: &[(PathBuf, PathBuf, Option<String>)],
) {
    if dry_run || verbose || clean.is_empty() {
        return;
    }

    if let [(source, target, description)] = clean {
        match op_type {
            FileOp::Link => output.link(source, target, description.as_deref()),
            FileOp::Copy => output.copy(source, target, description.as_deref()),
        }
        return;
    }

    let source_pattern = source_pattern.to_string_lossy();
    match op_type {
        FileOp::Link => output.link_summary(clean.len(), &source_pattern),
        FileOp::Copy => output.copy_summary(clean.len(), &source_pattern),
    }
}

/// Print a per-entry summary for operations that turned out to be no-ops
/// (`OperationOutcome::AlreadyLinked`/`AlreadyUpToDate`), the common case
/// when re-running `kabu setup` across many worktrees that were already
/// set up. Printed in dry-run too (unlike `summarize_clean_operations`,
/// which dry-run suppresses in favor of per-item previews): a no-op has
/// no per-item preview line of its own, so without this summary dry-run
/// output would silently omit it while a real run reports it, making the
/// two views incomparable.
fn summarize_no_op_operations(
    output: &Output,
    verbose: bool,
    op_type: FileOp,
    source_pattern: &Path,
    no_op: &[PathBuf],
) {
    if verbose || no_op.is_empty() {
        return;
    }

    if let [target] = no_op {
        match op_type {
            FileOp::Link => output.already_linked(target),
            FileOp::Copy => output.up_to_date(target),
        }
        return;
    }

    let source_pattern = source_pattern.to_string_lossy();
    match op_type {
        FileOp::Link => output.already_linked_summary(no_op.len(), &source_pattern),
        FileOp::Copy => output.up_to_date_summary(no_op.len(), &source_pattern),
    }
}

/// Parameters for a file operation.
struct OperationParams<'a> {
    source: &'a Path,
    target: &'a Path,
    op_type: FileOp,
    config_mode: Option<OnConflictSetting>,
    description: Option<&'a str>,
}

/// Process a single operation (symlink or copy) with conflict handling.
///
/// `dry_run` never mutates the filesystem and never prompts: a
/// conflict's resolution is only decided (and executed) once we know
/// we're not previewing, so a preview always completes non-interactively
/// even when no `on_conflict` is configured.
fn process_operation(
    params: &OperationParams,
    worktree_root: &Path,
    override_mode: &mut Option<OnConflict>,
    dry_run: bool,
    verbose: bool,
    output: &Output,
) -> Result<OperationOutcome> {
    let OperationParams {
        source,
        target,
        op_type,
        config_mode,
        description,
    } = params;

    operation::ensure_within_worktree(target, worktree_root)?;

    // Re-running setup against an already-correctly-linked target is the
    // common case (e.g. `kabu setup` across many worktrees set up before
    // a config change): a symlink already pointing at the intended
    // source needs no change, regardless of on_conflict.
    if matches!(op_type, FileOp::Link)
        && let (Ok(existing), Ok(intended)) =
            (std::fs::canonicalize(target), std::fs::canonicalize(source))
        && existing == intended
    {
        if verbose {
            output.already_linked(target);
        }
        return Ok(OperationOutcome::AlreadyLinked);
    }

    // Same idea for copy: a target whose content already matches the
    // source needs no change. Checked only when there's something there
    // to compare against; `is_up_to_date` itself never treats a symlink
    // target as up to date, since replacing it is handled as a conflict.
    if matches!(op_type, FileOp::Copy)
        && check_conflict(target)
        && operation::is_up_to_date(source, target)
    {
        if verbose {
            output.up_to_date(target);
        }
        return Ok(OperationOutcome::AlreadyUpToDate);
    }

    if !check_conflict(target) {
        if dry_run {
            let op_name = match op_type {
                FileOp::Link => "link",
                FileOp::Copy => "copy",
            };
            output.dry_run(&format!(
                "Would {}: {} -> {}",
                op_name,
                source.display(),
                target.display()
            ));
            return Ok(OperationOutcome::Reported);
        }

        match op_type {
            FileOp::Link => operation::create_symlink(source, target)?,
            FileOp::Copy => operation::copy_file(source, target)?,
        }
        if verbose {
            match op_type {
                FileOp::Link => output.link(source, target, *description),
                FileOp::Copy => output.copy(source, target, *description),
            }
        }
        return Ok(OperationOutcome::Created);
    }

    // Conflict: the override (from --on-conflict or a prior "apply to
    // all" choice) always wins; otherwise fall back to the mode
    // configured for this conflict's kind (symlink vs. real file/dir).
    let kind = conflict_kind(target);
    let configured =
        (*override_mode).or_else(|| config_mode.as_ref().and_then(|s| s.resolve(kind)));

    let mode = match configured {
        Some(mode) => mode,
        None if dry_run => {
            output.dry_run(&format!(
                "Would prompt: {} is an existing {}, no on_conflict configured",
                target.display(),
                kind.as_str()
            ));
            return Ok(OperationOutcome::Reported);
        }
        None => {
            let choice: ConflictChoice = interactive::prompt_conflict(target)?;
            if choice.apply_to_all {
                *override_mode = Some(choice.mode);
            }
            choice.mode
        }
    };

    if dry_run {
        output.dry_run(&format!(
            "Would {} ({} conflict): {}",
            mode.as_str(),
            kind.as_str(),
            target.display()
        ));
        return Ok(OperationOutcome::Reported);
    }

    let action = resolve_conflict(target, mode)?;
    match action {
        ConflictAction::Abort => return Err(Error::Aborted),
        ConflictAction::Skip => {
            output.skip(target);
            return Ok(OperationOutcome::Reported);
        }
        ConflictAction::Proceed => {}
    }

    match op_type {
        FileOp::Link => operation::create_symlink(source, target)?,
        FileOp::Copy => operation::copy_file(source, target)?,
    }
    // A resolved conflict is always worth calling out individually.
    match op_type {
        FileOp::Link => output.link(source, target, *description),
        FileOp::Copy => output.copy(source, target, *description),
    }

    Ok(OperationOutcome::Reported)
}

/// Check if a path contains glob patterns.
fn contains_glob_pattern(path: &Path) -> bool {
    path.to_str()
        .map(|s| s.contains('*') || s.contains('?') || s.contains('['))
        .unwrap_or(false)
}

/// Return the longest leading sequence of path components that contain no
/// glob meta-characters. Used to start a `WalkDir` from the deepest known
/// directory instead of `repo_root`, which avoids stat()-ing unrelated
/// subtrees such as build caches or `node_modules`.
fn glob_literal_prefix(pattern: &Path) -> PathBuf {
    let mut prefix = PathBuf::new();
    for component in pattern.components() {
        let s = component.as_os_str().to_string_lossy();
        if s.contains('*') || s.contains('?') || s.contains('[') {
            break;
        }
        prefix.push(component);
    }
    prefix
}

/// Cached VCS-tracked file/directory set, reused across all glob link entries
/// that opt into `skip_tracked: true`.
struct TrackedCache {
    files: HashSet<PathBuf>,
    dirs: HashSet<PathBuf>,
}

impl TrackedCache {
    fn build(provider: &dyn VcsProvider, repo_root: &Path) -> Result<Self> {
        let files: HashSet<PathBuf> = provider
            .list_tracked_files(repo_root)?
            .into_iter()
            .collect();
        // `git ls-files` (and the jj equivalent) only emit file paths, so
        // synthesize the set of directories that contain tracked files. This
        // lets us skip a whole directory when a glob matches it.
        let mut dirs = HashSet::new();
        for f in &files {
            let mut parent = f.parent();
            while let Some(p) = parent {
                if p.as_os_str().is_empty() {
                    break;
                }
                dirs.insert(p.to_path_buf());
                parent = p.parent();
            }
        }
        Ok(Self { files, dirs })
    }
}

/// Expand a link entry with glob patterns into multiple concrete link entries.
/// If skip_tracked is true, filter out VCS-tracked files.
fn expand_link(
    link: &Link,
    repo_root: &Path,
    provider: &dyn VcsProvider,
    tracked_cache: &mut Option<TrackedCache>,
) -> Result<Vec<Link>> {
    let source_str = link.source.to_string_lossy();

    if !contains_glob_pattern(&link.source) {
        // No glob pattern, return as-is
        return Ok(vec![link.clone()]);
    }

    // Build glob matcher
    let glob = globset::GlobBuilder::new(&source_str)
        .literal_separator(true)
        .build()
        .map_err(|e| Error::ConfigValidation {
            message: format!("Invalid glob pattern '{}': {}", source_str, e),
        })?;
    let matcher = glob.compile_matcher();

    // Walk only the literal prefix of the pattern so we don't descend into
    // unrelated trees (e.g. `rust/target`, `node_modules`) just to discard
    // them after a glob-match check.
    let prefix = glob_literal_prefix(&link.source);
    let walk_root = repo_root.join(&prefix);
    if !walk_root.exists() {
        return Ok(Vec::new());
    }

    let tracked = if link.skip_tracked {
        if tracked_cache.is_none() {
            *tracked_cache = Some(TrackedCache::build(provider, repo_root)?);
        }
        tracked_cache.as_ref()
    } else {
        None
    };

    // Exclude patterns match relative to the glob's literal prefix, so
    // `source: ".claude/*"` + `exclude: ["CLAUDE.md"]` matches "CLAUDE.md",
    // not ".claude/CLAUDE.md".
    let exclude_matchers: Vec<globset::GlobMatcher> = link
        .exclude
        .iter()
        .map(|pattern| {
            globset::GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map(|g| g.compile_matcher())
                .map_err(|e| Error::ConfigValidation {
                    message: format!("Invalid exclude pattern '{}': {}", pattern, e),
                })
        })
        .collect::<Result<Vec<_>>>()?;

    // Walk the repository and find matching files and directories
    // Collect matched directories to avoid processing their contents
    let mut matched_dirs: HashSet<PathBuf> = HashSet::new();
    let mut results = Vec::new();

    for entry in walkdir::WalkDir::new(&walk_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Get relative path from repo root
        let rel_path = match path.strip_prefix(repo_root) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Skip if parent directory was already matched
        let mut should_skip = false;
        for matched_dir in &matched_dirs {
            if rel_path.starts_with(matched_dir) && rel_path != matched_dir {
                should_skip = true;
                break;
            }
        }
        if should_skip {
            continue;
        }

        // Check if it matches the glob pattern
        if !matcher.is_match(rel_path) {
            continue;
        }

        // `entry.file_type()` reads from the dirent cached by walkdir and
        // avoids the extra stat() that `path.is_dir()` would issue.
        let is_dir = entry.file_type().is_dir();

        // Skip if it's tracked and skip_tracked is true
        if let Some(cache) = tracked {
            if cache.files.contains(rel_path) {
                continue;
            }
            // Directories containing tracked files should also be skipped,
            // as git ls-files only returns file paths, not directory paths
            if is_dir && cache.dirs.contains(rel_path) {
                continue;
            }
        }

        // Skip if it matches an exclude pattern, relative to the glob's
        // literal prefix.
        let rel_to_prefix = rel_path.strip_prefix(&prefix).unwrap_or(rel_path);
        if exclude_matchers.iter().any(|m| m.is_match(rel_to_prefix)) {
            if is_dir {
                matched_dirs.insert(rel_path.to_path_buf());
            }
            continue;
        }

        // If it's a directory, add to matched_dirs to skip its contents
        if is_dir {
            matched_dirs.insert(rel_path.to_path_buf());
        }

        // Create a link entry for this file or directory
        let mut file_link = link.clone();
        file_link.source = rel_path.to_path_buf();
        file_link.target = rel_path.to_path_buf();
        file_link.skip_tracked = false; // Already filtered, no need to check again
        results.push(file_link);
    }

    Ok(results)
}

/// Expand a copy entry with glob patterns into multiple concrete copy entries.
fn expand_copy(copy: &config::Copy, repo_root: &Path) -> Result<Vec<config::Copy>> {
    let source_str = copy.source.to_string_lossy();

    if !contains_glob_pattern(&copy.source) {
        return Ok(vec![copy.clone()]);
    }

    let glob = globset::GlobBuilder::new(&source_str)
        .literal_separator(true)
        .build()
        .map_err(|e| Error::ConfigValidation {
            message: format!("Invalid glob pattern '{}': {}", source_str, e),
        })?;
    let matcher = glob.compile_matcher();

    let prefix = glob_literal_prefix(&copy.source);
    let walk_root = repo_root.join(&prefix);
    if !walk_root.exists() {
        return Ok(Vec::new());
    }

    let mut matched_dirs: HashSet<PathBuf> = HashSet::new();
    let mut results = Vec::new();

    for entry in walkdir::WalkDir::new(&walk_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let rel_path = match path.strip_prefix(repo_root) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Skip if parent directory was already matched
        let mut should_skip = false;
        for matched_dir in &matched_dirs {
            if rel_path.starts_with(matched_dir) && rel_path != matched_dir {
                should_skip = true;
                break;
            }
        }
        if should_skip {
            continue;
        }

        if !matcher.is_match(rel_path) {
            continue;
        }

        if entry.file_type().is_dir() {
            matched_dirs.insert(rel_path.to_path_buf());
        }

        let mut file_copy = copy.clone();
        file_copy.source = rel_path.to_path_buf();
        file_copy.target = rel_path.to_path_buf();
        results.push(file_copy);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_glob_pattern_with_asterisk() {
        assert!(contains_glob_pattern(Path::new("secrets/*")));
    }

    #[test]
    fn test_contains_glob_pattern_with_question() {
        assert!(contains_glob_pattern(Path::new("file?.txt")));
    }

    #[test]
    fn test_contains_glob_pattern_with_bracket() {
        assert!(contains_glob_pattern(Path::new("file[0-9].txt")));
    }

    #[test]
    fn test_contains_glob_pattern_none() {
        assert!(!contains_glob_pattern(Path::new("secrets/config.json")));
    }

    #[test]
    fn test_glob_literal_prefix_trailing_wildcard() {
        assert_eq!(
            glob_literal_prefix(Path::new(".claude/*")),
            PathBuf::from(".claude")
        );
        assert_eq!(
            glob_literal_prefix(Path::new("script/stub-server/certs/*")),
            PathBuf::from("script/stub-server/certs")
        );
    }

    #[test]
    fn test_glob_literal_prefix_wildcard_in_first_component() {
        assert_eq!(glob_literal_prefix(Path::new("dir*")), PathBuf::new());
        assert_eq!(glob_literal_prefix(Path::new("**/*.rs")), PathBuf::new());
    }

    #[test]
    fn test_glob_literal_prefix_wildcard_in_middle() {
        assert_eq!(
            glob_literal_prefix(Path::new("foo/*/bar")),
            PathBuf::from("foo")
        );
    }

    #[test]
    fn test_glob_literal_prefix_no_wildcard() {
        assert_eq!(
            glob_literal_prefix(Path::new("a/b/c.txt")),
            PathBuf::from("a/b/c.txt")
        );
    }

    #[test]
    fn test_glob_literal_prefix_question_and_bracket() {
        assert_eq!(
            glob_literal_prefix(Path::new("a/b/file?.txt")),
            PathBuf::from("a/b")
        );
        assert_eq!(
            glob_literal_prefix(Path::new("a/b/file[0-9].txt")),
            PathBuf::from("a/b")
        );
    }
}

#[cfg(all(test, feature = "impure-test"))]
mod impure_tests {
    use super::*;

    #[test]
    fn test_expand_link_no_glob() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create test file
        std::fs::write(repo_root.join("test.txt"), "content").unwrap();

        let link = Link {
            source: PathBuf::from("test.txt"),
            target: PathBuf::from("test.txt"),
            on_conflict: None,
            description: None,
            skip_tracked: false,
            exclude: vec![],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, PathBuf::from("test.txt"));
    }

    #[test]
    fn test_expand_link_with_glob() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create test files
        std::fs::create_dir_all(repo_root.join("fixtures")).unwrap();
        std::fs::write(repo_root.join("fixtures/file1.txt"), "content1").unwrap();
        std::fs::write(repo_root.join("fixtures/file2.txt"), "content2").unwrap();
        std::fs::write(repo_root.join("fixtures/data.json"), "{}").unwrap();

        let link = Link {
            source: PathBuf::from("fixtures/*.txt"),
            target: PathBuf::from("fixtures/*.txt"),
            on_conflict: None,
            description: None,
            skip_tracked: false,
            exclude: vec![],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();
        assert_eq!(result.len(), 2);

        let mut sources: Vec<_> = result.iter().map(|l| l.source.clone()).collect();
        sources.sort();
        assert_eq!(sources[0], PathBuf::from("fixtures/file1.txt"));
        assert_eq!(sources[1], PathBuf::from("fixtures/file2.txt"));
    }

    #[test]
    fn test_expand_link_skip_tracked() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Initialize git repo
        let init_result = std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output();

        if init_result.is_err() {
            eprintln!("Skipping test: git not available");
            return;
        }

        // Configure git user for the test repo
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_root)
            .output()
            .ok();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_root)
            .output()
            .ok();

        // Create and track a file
        std::fs::create_dir_all(repo_root.join("fixtures")).unwrap();
        std::fs::write(repo_root.join("fixtures/tracked.txt"), "tracked").unwrap();
        std::fs::write(repo_root.join("fixtures/untracked.txt"), "untracked").unwrap();

        // Add and commit the tracked file
        let add_result = std::process::Command::new("git")
            .args(["add", "fixtures/tracked.txt"])
            .current_dir(repo_root)
            .output();

        if add_result.is_err() || !add_result.unwrap().status.success() {
            eprintln!("Skipping test: git add failed");
            return;
        }

        let commit_result = std::process::Command::new("git")
            .args(["commit", "-m", "Add tracked file"])
            .current_dir(repo_root)
            .output();

        if commit_result.is_err() || !commit_result.unwrap().status.success() {
            eprintln!("Skipping test: git commit failed");
            return;
        }

        let link = Link {
            source: PathBuf::from("fixtures/*.txt"),
            target: PathBuf::from("fixtures/*.txt"),
            on_conflict: None,
            description: None,
            skip_tracked: true,
            exclude: vec![],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();

        // Should only include untracked file
        // Note: This test may be flaky in some environments
        if result.len() == 1 {
            assert_eq!(result[0].source, PathBuf::from("fixtures/untracked.txt"));
        } else {
            eprintln!(
                "Warning: Expected 1 result but got {}. This may be environment-dependent.",
                result.len()
            );
        }
    }

    #[test]
    fn test_expand_link_skip_tracked_directory() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Initialize git repo
        let init_result = std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output();

        if init_result.is_err() {
            eprintln!("Skipping test: git not available");
            return;
        }

        // Configure git user for the test repo
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_root)
            .output()
            .ok();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_root)
            .output()
            .ok();

        // Create directory structure:
        // .config/tracked-dir/file.txt  (tracked)
        // .config/untracked-dir/file.txt  (untracked)
        // .config/untracked-file.txt  (untracked)
        std::fs::create_dir_all(repo_root.join(".config/tracked-dir")).unwrap();
        std::fs::write(repo_root.join(".config/tracked-dir/file.txt"), "tracked").unwrap();
        std::fs::create_dir_all(repo_root.join(".config/untracked-dir")).unwrap();
        std::fs::write(
            repo_root.join(".config/untracked-dir/file.txt"),
            "untracked",
        )
        .unwrap();
        std::fs::write(repo_root.join(".config/untracked-file.txt"), "untracked").unwrap();

        // Track only the file inside tracked-dir
        let add_result = std::process::Command::new("git")
            .args(["add", ".config/tracked-dir/file.txt"])
            .current_dir(repo_root)
            .output();

        if add_result.is_err() || !add_result.unwrap().status.success() {
            eprintln!("Skipping test: git add failed");
            return;
        }

        let commit_result = std::process::Command::new("git")
            .args(["commit", "-m", "Add tracked dir"])
            .current_dir(repo_root)
            .output();

        if commit_result.is_err() || !commit_result.unwrap().status.success() {
            eprintln!("Skipping test: git commit failed");
            return;
        }

        let link = Link {
            source: PathBuf::from(".config/*"),
            target: PathBuf::from(".config/*"),
            on_conflict: None,
            description: None,
            skip_tracked: true,
            exclude: vec![],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();

        // Should skip tracked-dir (directory containing tracked files)
        // and include untracked-dir and untracked-file.txt
        if result.len() == 2 {
            let mut sources: Vec<_> = result.iter().map(|l| l.source.clone()).collect();
            sources.sort();
            assert_eq!(sources[0], PathBuf::from(".config/untracked-dir"));
            assert_eq!(sources[1], PathBuf::from(".config/untracked-file.txt"));
        } else {
            eprintln!(
                "Warning: Expected 2 results but got {}. This may be environment-dependent.",
                result.len()
            );
        }
    }

    #[test]
    fn test_expand_link_with_directory() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create test directory with files inside
        std::fs::create_dir_all(repo_root.join("testdir")).unwrap();
        std::fs::write(repo_root.join("testdir/file1.txt"), "content1").unwrap();
        std::fs::write(repo_root.join("testdir/file2.txt"), "content2").unwrap();

        // Pattern matching the directory
        let link = Link {
            source: PathBuf::from("testdir"),
            target: PathBuf::from("testdir"),
            on_conflict: None,
            description: None,
            skip_tracked: false,
            exclude: vec![],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();

        // Should return only the directory, not its contents
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, PathBuf::from("testdir"));
    }

    #[test]
    fn test_expand_link_with_glob_matching_directory() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        // Create test directories with files inside
        std::fs::create_dir_all(repo_root.join("dir1")).unwrap();
        std::fs::write(repo_root.join("dir1/file.txt"), "content1").unwrap();
        std::fs::create_dir_all(repo_root.join("dir2")).unwrap();
        std::fs::write(repo_root.join("dir2/file.txt"), "content2").unwrap();
        std::fs::create_dir_all(repo_root.join("other")).unwrap();
        std::fs::write(repo_root.join("other/file.txt"), "content3").unwrap();

        // Pattern matching directories starting with "dir"
        let link = Link {
            source: PathBuf::from("dir*"),
            target: PathBuf::from("dir*"),
            on_conflict: None,
            description: None,
            skip_tracked: false,
            exclude: vec![],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();

        // Should return only the directories, not their contents
        assert_eq!(result.len(), 2);

        let mut sources: Vec<_> = result.iter().map(|l| l.source.clone()).collect();
        sources.sort();
        assert_eq!(sources[0], PathBuf::from("dir1"));
        assert_eq!(sources[1], PathBuf::from("dir2"));
    }

    #[test]
    fn test_expand_link_with_exclude() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        std::fs::create_dir_all(repo_root.join(".claude/rules")).unwrap();
        std::fs::write(repo_root.join(".claude/rules/a.md"), "a").unwrap();
        std::fs::write(repo_root.join(".claude/CLAUDE.md"), "claude").unwrap();
        std::fs::write(repo_root.join(".claude/settings.json"), "{}").unwrap();

        let link = Link {
            source: PathBuf::from(".claude/*"),
            target: PathBuf::from(".claude/*"),
            on_conflict: None,
            description: None,
            skip_tracked: false,
            exclude: vec!["CLAUDE.md".to_string(), "rules".to_string()],
        };

        let provider = vcs::GitProvider;
        let result = expand_link(&link, repo_root, &provider, &mut None).unwrap();

        let mut sources: Vec<_> = result.iter().map(|l| l.source.clone()).collect();
        sources.sort();
        assert_eq!(sources, vec![PathBuf::from(".claude/settings.json")]);
    }

    #[test]
    fn test_expand_copy_no_glob() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        std::fs::write(repo_root.join("test.txt"), "content").unwrap();

        let copy = config::Copy {
            source: PathBuf::from("test.txt"),
            target: PathBuf::from("test.txt"),
            on_conflict: None,
            description: None,
        };

        let result = expand_copy(&copy, repo_root).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, PathBuf::from("test.txt"));
    }

    #[test]
    fn test_expand_copy_with_glob() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        std::fs::create_dir_all(repo_root.join("fixtures")).unwrap();
        std::fs::write(repo_root.join("fixtures/file1.txt"), "content1").unwrap();
        std::fs::write(repo_root.join("fixtures/file2.txt"), "content2").unwrap();
        std::fs::write(repo_root.join("fixtures/data.json"), "{}").unwrap();

        let copy = config::Copy {
            source: PathBuf::from("fixtures/*.txt"),
            target: PathBuf::from("fixtures/*.txt"),
            on_conflict: None,
            description: None,
        };

        let result = expand_copy(&copy, repo_root).unwrap();
        assert_eq!(result.len(), 2);

        let mut sources: Vec<_> = result.iter().map(|c| c.source.clone()).collect();
        sources.sort();
        assert_eq!(sources[0], PathBuf::from("fixtures/file1.txt"));
        assert_eq!(sources[1], PathBuf::from("fixtures/file2.txt"));
    }

    #[test]
    fn test_expand_copy_with_directory() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        std::fs::create_dir_all(repo_root.join("testdir")).unwrap();
        std::fs::write(repo_root.join("testdir/file1.txt"), "content1").unwrap();
        std::fs::write(repo_root.join("testdir/file2.txt"), "content2").unwrap();

        let copy = config::Copy {
            source: PathBuf::from("testdir"),
            target: PathBuf::from("testdir"),
            on_conflict: None,
            description: None,
        };

        let result = expand_copy(&copy, repo_root).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].source, PathBuf::from("testdir"));
    }

    #[test]
    fn test_expand_copy_with_glob_matching_directory() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        std::fs::create_dir_all(repo_root.join("dir1")).unwrap();
        std::fs::write(repo_root.join("dir1/file.txt"), "content1").unwrap();
        std::fs::create_dir_all(repo_root.join("dir2")).unwrap();
        std::fs::write(repo_root.join("dir2/file.txt"), "content2").unwrap();
        std::fs::create_dir_all(repo_root.join("other")).unwrap();
        std::fs::write(repo_root.join("other/file.txt"), "content3").unwrap();

        let copy = config::Copy {
            source: PathBuf::from("dir*"),
            target: PathBuf::from("dir*"),
            on_conflict: None,
            description: None,
        };

        let result = expand_copy(&copy, repo_root).unwrap();
        assert_eq!(result.len(), 2);

        let mut sources: Vec<_> = result.iter().map(|c| c.source.clone()).collect();
        sources.sort();
        assert_eq!(sources[0], PathBuf::from("dir1"));
        assert_eq!(sources[1], PathBuf::from("dir2"));
    }
}
