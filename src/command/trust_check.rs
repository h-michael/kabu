use crate::color;
use crate::color::ColorScheme;
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::hook;
use crate::trust;

use std::path::Path;

pub(crate) enum TrustHint {
    None,
    /// Used when the trust check runs before the worktree/workspace is
    /// created (currently only `add`), so the abort message can say so
    /// instead of leaving callers to check with `kabu list`.
    SkipHooks {
        command: &'static str,
    },
}

pub(crate) fn load_config_with_trust_check(
    repo_root: &Path,
    main_worktree_path: &Path,
    enforce_hooks: bool,
    hint: TrustHint,
) -> Result<Config> {
    let global_config = config::load_global()?;
    let initial_repo_config = config::load(repo_root)?.unwrap_or_default();
    let initial_config =
        config::merge_with_global(initial_repo_config.clone(), global_config.as_ref());
    color::set_cli_theme(&initial_config.ui.colors);

    if enforce_hooks
        && initial_repo_config.hooks.has_hooks()
        && !trust::is_trusted(main_worktree_path, &initial_repo_config)?
    {
        hook::display_hooks_for_review(&initial_repo_config.hooks);

        eprintln!();
        eprintln!("{}", ColorScheme::error("Configuration is not trusted."));
        eprintln!("The config file contains hooks that can execute arbitrary commands.");
        eprintln!("For security, you must explicitly review and trust the configuration.");
        if let TrustHint::SkipHooks { .. } = &hint {
            eprintln!("No worktree/workspace was created.");
        }
        eprintln!();
        eprintln!("To trust this configuration, run:");
        eprintln!("  kabu trust");

        if let TrustHint::SkipHooks { command } = hint {
            eprintln!();
            eprintln!("Or skip hooks:");
            eprintln!("  {command}");
        }

        return Err(Error::HooksNotTrusted);
    }

    // TOCTOU protection: reload config immediately before use
    let repo_config = config::load(repo_root)?.unwrap_or_default();
    let config = config::merge_with_global(repo_config.clone(), global_config.as_ref());
    color::set_cli_theme(&config.ui.colors);
    if enforce_hooks
        && repo_config.hooks.has_hooks()
        && !trust::is_trusted(main_worktree_path, &repo_config)?
    {
        eprintln!(
            "{}",
            ColorScheme::error("Config file was modified after trust check.")
        );
        eprintln!("For security, configuration must be re-trusted after any changes.");
        if let TrustHint::SkipHooks { .. } = &hint {
            eprintln!("No worktree/workspace was created.");
        }
        eprintln!("Run: kabu trust");
        return Err(Error::HooksNotTrusted);
    }

    Ok(config)
}
