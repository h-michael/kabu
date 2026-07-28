use crate::cli::TrustArgs;
use crate::color::{self, ColorConfig, ColorScheme};
use crate::config::ConfigSnapshot;
use crate::{config, error::Error, error::Result, prompt, trust, vcs};

pub(crate) fn run(args: TrustArgs, color_config: ColorConfig) -> Result<()> {
    let provider = vcs::get_provider()?;

    if args.check {
        let repo_root = match args.path {
            Some(p) => p.canonicalize()?,
            None => {
                if !provider.is_inside_repo() {
                    return Ok(());
                }
                provider.main_workspace_root()?
            }
        };

        let config = match config::load(&repo_root)? {
            Some(config) => config,
            None => return Ok(()),
        };

        if !config.hooks.has_hooks() {
            return Ok(());
        }

        let main_worktree_path = provider.main_workspace_path_for(&repo_root)?;
        let is_trusted = trust::is_trusted(&main_worktree_path, &config)?;
        if is_trusted {
            return Ok(());
        }

        return Err(Error::TrustCheckFailed);
    }

    let repo_root = match args.path {
        Some(p) => p.canonicalize()?,
        None => provider.main_workspace_root()?,
    };

    let main_worktree_path = provider.main_workspace_path_for(&repo_root)?;

    let config = config::load(&repo_root)?.ok_or_else(|| Error::ConfigNotFound {
        path: repo_root.clone(),
    })?;

    color::set_cli_theme(&config.ui.colors);

    if !config.hooks.has_hooks() {
        return Err(Error::NoHooksDefined);
    }

    if args.show {
        let use_color = color_config.is_enabled();

        println!("Hooks in {}:", repo_root.display());
        if !config.hooks.pre_add.is_empty() {
            println!();
            if use_color {
                println!("{}", ColorScheme::hook_type("pre_add:"));
            } else {
                println!("pre_add:");
            }
            for entry in &config.hooks.pre_add {
                println!("  {}", entry.command);
                if let Some(desc) = &entry.description {
                    if use_color {
                        println!(
                            "  {} {}",
                            ColorScheme::hook_arrow("->"),
                            ColorScheme::hook_description(desc)
                        );
                    } else {
                        println!("  -> {}", desc);
                    }
                } else if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::dimmed("no description")
                    );
                } else {
                    println!("  -> no description");
                }
            }
        }
        if !config.hooks.post_add.is_empty() {
            println!();
            if use_color {
                println!("{}", ColorScheme::hook_type("post_add:"));
            } else {
                println!("post_add:");
            }
            for entry in &config.hooks.post_add {
                println!("  {}", entry.command);
                if let Some(desc) = &entry.description {
                    if use_color {
                        println!(
                            "  {} {}",
                            ColorScheme::hook_arrow("->"),
                            ColorScheme::hook_description(desc)
                        );
                    } else {
                        println!("  -> {}", desc);
                    }
                } else if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::dimmed("no description")
                    );
                } else {
                    println!("  -> no description");
                }
            }
        }
        if !config.hooks.pre_remove.is_empty() {
            println!();
            if use_color {
                println!("{}", ColorScheme::hook_type("pre_remove:"));
            } else {
                println!("pre_remove:");
            }
            for entry in &config.hooks.pre_remove {
                println!("  {}", entry.command);
                if let Some(desc) = &entry.description {
                    if use_color {
                        println!(
                            "  {} {}",
                            ColorScheme::hook_arrow("->"),
                            ColorScheme::hook_description(desc)
                        );
                    } else {
                        println!("  -> {}", desc);
                    }
                } else if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::dimmed("no description")
                    );
                } else {
                    println!("  -> no description");
                }
            }
        }
        if !config.hooks.post_remove.is_empty() {
            println!();
            if use_color {
                println!("{}", ColorScheme::hook_type("post_remove:"));
            } else {
                println!("post_remove:");
            }
            for entry in &config.hooks.post_remove {
                println!("  {}", entry.command);
                if let Some(desc) = &entry.description {
                    if use_color {
                        println!(
                            "  {} {}",
                            ColorScheme::hook_arrow("->"),
                            ColorScheme::hook_description(desc)
                        );
                    } else {
                        println!("  -> {}", desc);
                    }
                } else if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::dimmed("no description")
                    );
                } else {
                    println!("  -> no description");
                }
            }
        }

        let is_trusted = trust::is_trusted(&main_worktree_path, &config)?;
        println!(
            "\nTrust status: {}",
            if is_trusted { "trusted" } else { "not trusted" }
        );
        return Ok(());
    }

    // Display hooks
    let use_color = color_config.is_enabled();

    if use_color {
        println!(
            "{}",
            ColorScheme::warning("WARNING: Review these commands before trusting")
        );
    } else {
        println!("WARNING: Review these commands before trusting");
    }
    println!();
    println!("Repository: {}", repo_root.display());

    if !config.hooks.pre_add.is_empty() {
        if use_color {
            println!("{}", ColorScheme::hook_type("pre_add:"));
        } else {
            println!("pre_add:");
        }
        for entry in &config.hooks.pre_add {
            println!("  {}", entry.command);
            if let Some(desc) = &entry.description {
                if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::hook_description(desc)
                    );
                } else {
                    println!("  -> {}", desc);
                }
            } else if use_color {
                println!(
                    "  {} {}",
                    ColorScheme::hook_arrow("->"),
                    ColorScheme::dimmed("no description")
                );
            } else {
                println!("  -> no description");
            }
        }
        println!();
    }
    if !config.hooks.post_add.is_empty() {
        if use_color {
            println!("{}", ColorScheme::hook_type("post_add:"));
        } else {
            println!("post_add:");
        }
        for entry in &config.hooks.post_add {
            println!("  {}", entry.command);
            if let Some(desc) = &entry.description {
                if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::hook_description(desc)
                    );
                } else {
                    println!("  -> {}", desc);
                }
            } else if use_color {
                println!(
                    "  {} {}",
                    ColorScheme::hook_arrow("->"),
                    ColorScheme::dimmed("no description")
                );
            } else {
                println!("  -> no description");
            }
        }
        println!();
    }
    if !config.hooks.pre_remove.is_empty() {
        if use_color {
            println!("{}", ColorScheme::hook_type("pre_remove:"));
        } else {
            println!("pre_remove:");
        }
        for entry in &config.hooks.pre_remove {
            println!("  {}", entry.command);
            if let Some(desc) = &entry.description {
                if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::hook_description(desc)
                    );
                } else {
                    println!("  -> {}", desc);
                }
            } else if use_color {
                println!(
                    "  {} {}",
                    ColorScheme::hook_arrow("->"),
                    ColorScheme::dimmed("no description")
                );
            } else {
                println!("  -> no description");
            }
        }
        println!();
    }
    if !config.hooks.post_remove.is_empty() {
        if use_color {
            println!("{}", ColorScheme::hook_type("post_remove:"));
        } else {
            println!("post_remove:");
        }
        for entry in &config.hooks.post_remove {
            println!("  {}", entry.command);
            if let Some(desc) = &entry.description {
                if use_color {
                    println!(
                        "  {} {}",
                        ColorScheme::hook_arrow("->"),
                        ColorScheme::hook_description(desc)
                    );
                } else {
                    println!("  -> {}", desc);
                }
            } else if use_color {
                println!(
                    "  {} {}",
                    ColorScheme::hook_arrow("->"),
                    ColorScheme::dimmed("no description")
                );
            } else {
                println!("  -> no description");
            }
        }
        println!();
    }

    // Check if configuration has changed and display diff if so
    let use_color = color_config.is_enabled();
    if let Ok(Some(trust_entry)) = trust::read_trust_entry(&main_worktree_path) {
        let old_snapshot = &trust_entry.config_snapshot;
        let new_config = &config;

        // Compare snapshots to detect changes
        let new_snapshot = ConfigSnapshot::from_config(new_config);
        if old_snapshot == &new_snapshot {
            let is_trusted = trust::is_trusted(&main_worktree_path, &config)?;
            if is_trusted {
                // Configuration hasn't changed, already trusted
                println!(
                    "Configuration is already trusted for: {}",
                    repo_root.display()
                );
                return Ok(());
            }
            // Snapshot matches but trust hash doesn't; fall through to re-trust.
        }

        // Configuration has changed, show diff
        display_config_diff(old_snapshot, new_config, use_color);
    }

    if args.yes {
        trust::trust(&main_worktree_path, &config)?;
        println!("\n✓ Configuration trusted for: {}", repo_root.display());
        println!("These hooks will now run automatically on kabu add/remove commands.");
        return Ok(());
    }

    // Prompt for confirmation
    if prompt::is_interactive() {
        if prompt::prompt_trust_hooks(&repo_root)? {
            trust::trust(&main_worktree_path, &config)?;
            println!("\n✓ Configuration trusted for: {}", repo_root.display());
            println!("These hooks will now run automatically on kabu add/remove commands.");
        } else {
            println!("\nConfiguration was not trusted.");
            return Err(Error::Aborted);
        }
    } else {
        // Non-interactive: cannot prompt
        eprintln!(
            "\n{}",
            ColorScheme::error("Cannot prompt for confirmation in non-interactive mode.")
        );
        eprintln!("Run this command in an interactive terminal to trust configuration.");
        eprintln!("Or pass --yes to trust without prompting.");
        return Err(Error::NonInteractive);
    }

    Ok(())
}

fn diff_prefix(use_color: bool, added: bool) -> String {
    match (use_color, added) {
        (true, true) => ColorScheme::diff_added("+"),
        (true, false) => ColorScheme::diff_removed("-"),
        (false, true) => "+".to_string(),
        (false, false) => "-".to_string(),
    }
}

fn order_changed_marker(use_color: bool) -> String {
    if use_color {
        ColorScheme::dimmed("(order changed)")
    } else {
        "(order changed)".to_string()
    }
}

fn diff_list<T: Clone + PartialEq>(old: &[T], new: &[T]) -> (Vec<T>, Vec<T>, bool) {
    let mut new_remaining = new.to_vec();
    let mut removed = Vec::new();

    for old_item in old {
        if let Some(pos) = new_remaining.iter().position(|item| item == old_item) {
            new_remaining.remove(pos);
        } else {
            removed.push(old_item.clone());
        }
    }

    let added = new_remaining;
    let order_changed = removed.is_empty() && added.is_empty() && old != new;

    (removed, added, order_changed)
}

fn format_on_conflict(conflict: &config::OnConflict) -> String {
    format!("{:?}", conflict).to_lowercase()
}

/// Display configuration diff between old snapshot and new config.
///
/// Shows which items were removed and which were added in each section
/// (mkdir, link, copy, hooks) to help users understand what changed.
fn display_config_diff(old: &ConfigSnapshot, new: &config::Config, use_color: bool) {
    let new_snapshot = ConfigSnapshot::from_config(new);

    println!(
        "\n{}",
        if use_color {
            ColorScheme::warning("Configuration has changed:")
        } else {
            "Configuration has changed:".to_string()
        }
    );
    println!("────────────────────────────────────────────────────────");

    // Compare mkdir operations
    if old.mkdir != new_snapshot.mkdir {
        println!();
        if use_color {
            println!("{}", ColorScheme::operation("mkdir:"));
        } else {
            println!("mkdir:");
        }

        let (removed, added, order_changed) = diff_list(&old.mkdir, &new_snapshot.mkdir);
        let removed_prefix = diff_prefix(use_color, false);
        let added_prefix = diff_prefix(use_color, true);

        for item in removed {
            println!("    {} path: \"{}\"", removed_prefix, item.path);
            if let Some(desc) = &item.description {
                println!("    {} description: {}", removed_prefix, desc);
            }
        }

        for item in added {
            println!("    {} path: \"{}\"", added_prefix, item.path);
            if let Some(desc) = &item.description {
                println!("    {} description: {}", added_prefix, desc);
            }
        }

        if order_changed {
            println!("    {}", order_changed_marker(use_color));
        }
    }

    // Compare link operations
    if old.link != new_snapshot.link {
        println!();
        if use_color {
            println!("{}", ColorScheme::operation("link:"));
        } else {
            println!("link:");
        }

        let (removed, added, order_changed) = diff_list(&old.link, &new_snapshot.link);
        let removed_prefix = diff_prefix(use_color, false);
        let added_prefix = diff_prefix(use_color, true);

        for item in removed {
            println!("    {} source: \"{}\"", removed_prefix, item.source);
            println!("    {} target: \"{}\"", removed_prefix, item.target);
            if let Some(conflict) = &item.on_conflict {
                println!(
                    "    {} on_conflict: {}",
                    removed_prefix,
                    format_on_conflict(conflict)
                );
            }
            if let Some(desc) = &item.description {
                println!("    {} description: {}", removed_prefix, desc);
            }
            println!("    {} skip_tracked: {}", removed_prefix, item.skip_tracked);
        }

        for item in added {
            println!("    {} source: \"{}\"", added_prefix, item.source);
            println!("    {} target: \"{}\"", added_prefix, item.target);
            if let Some(conflict) = &item.on_conflict {
                println!(
                    "    {} on_conflict: {}",
                    added_prefix,
                    format_on_conflict(conflict)
                );
            }
            if let Some(desc) = &item.description {
                println!("    {} description: {}", added_prefix, desc);
            }
            println!("    {} skip_tracked: {}", added_prefix, item.skip_tracked);
        }

        if order_changed {
            println!("    {}", order_changed_marker(use_color));
        }
    }

    // Compare copy operations
    if old.copy != new_snapshot.copy {
        println!();
        if use_color {
            println!("{}", ColorScheme::operation("copy:"));
        } else {
            println!("copy:");
        }

        let (removed, added, order_changed) = diff_list(&old.copy, &new_snapshot.copy);
        let removed_prefix = diff_prefix(use_color, false);
        let added_prefix = diff_prefix(use_color, true);

        for item in removed {
            println!("    {} source: \"{}\"", removed_prefix, item.source);
            println!("    {} target: \"{}\"", removed_prefix, item.target);
            if let Some(conflict) = &item.on_conflict {
                println!(
                    "    {} on_conflict: {}",
                    removed_prefix,
                    format_on_conflict(conflict)
                );
            }
            if let Some(desc) = &item.description {
                println!("    {} description: {}", removed_prefix, desc);
            }
        }

        for item in added {
            println!("    {} source: \"{}\"", added_prefix, item.source);
            println!("    {} target: \"{}\"", added_prefix, item.target);
            if let Some(conflict) = &item.on_conflict {
                println!(
                    "    {} on_conflict: {}",
                    added_prefix,
                    format_on_conflict(conflict)
                );
            }
            if let Some(desc) = &item.description {
                println!("    {} description: {}", added_prefix, desc);
            }
        }

        if order_changed {
            println!("    {}", order_changed_marker(use_color));
        }
    }

    // Compare hooks
    if old.hooks != new_snapshot.hooks {
        println!();
        if use_color {
            println!("{}", ColorScheme::operation("hooks:"));
        } else {
            println!("hooks:");
        }

        // Pre-add hooks
        if old.hooks.pre_add != new_snapshot.hooks.pre_add {
            if use_color {
                println!("  {}", ColorScheme::hook_type("pre_add:"));
            } else {
                println!("  pre_add:");
            }

            let (removed, added, order_changed) =
                diff_list(&old.hooks.pre_add, &new_snapshot.hooks.pre_add);
            let removed_prefix = diff_prefix(use_color, false);
            let added_prefix = diff_prefix(use_color, true);

            for item in removed {
                println!("    {} command: {}", removed_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", removed_prefix, desc);
                }
            }

            for item in added {
                println!("    {} command: {}", added_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", added_prefix, desc);
                }
            }

            if order_changed {
                println!("    {}", order_changed_marker(use_color));
            }
        }

        // Post-add hooks
        if old.hooks.post_add != new_snapshot.hooks.post_add {
            if use_color {
                println!("  {}", ColorScheme::hook_type("post_add:"));
            } else {
                println!("  post_add:");
            }

            let (removed, added, order_changed) =
                diff_list(&old.hooks.post_add, &new_snapshot.hooks.post_add);
            let removed_prefix = diff_prefix(use_color, false);
            let added_prefix = diff_prefix(use_color, true);

            for item in removed {
                println!("    {} command: {}", removed_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", removed_prefix, desc);
                }
            }

            for item in added {
                println!("    {} command: {}", added_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", added_prefix, desc);
                }
            }

            if order_changed {
                println!("    {}", order_changed_marker(use_color));
            }
        }

        // Pre-remove hooks
        if old.hooks.pre_remove != new_snapshot.hooks.pre_remove {
            if use_color {
                println!("  {}", ColorScheme::hook_type("pre_remove:"));
            } else {
                println!("  pre_remove:");
            }

            let (removed, added, order_changed) =
                diff_list(&old.hooks.pre_remove, &new_snapshot.hooks.pre_remove);
            let removed_prefix = diff_prefix(use_color, false);
            let added_prefix = diff_prefix(use_color, true);

            for item in removed {
                println!("    {} command: {}", removed_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", removed_prefix, desc);
                }
            }

            for item in added {
                println!("    {} command: {}", added_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", added_prefix, desc);
                }
            }

            if order_changed {
                println!("    {}", order_changed_marker(use_color));
            }
        }

        // Post-remove hooks
        if old.hooks.post_remove != new_snapshot.hooks.post_remove {
            if use_color {
                println!("  {}", ColorScheme::hook_type("post_remove:"));
            } else {
                println!("  post_remove:");
            }

            let (removed, added, order_changed) =
                diff_list(&old.hooks.post_remove, &new_snapshot.hooks.post_remove);
            let removed_prefix = diff_prefix(use_color, false);
            let added_prefix = diff_prefix(use_color, true);

            for item in removed {
                println!("    {} command: {}", removed_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", removed_prefix, desc);
                }
            }

            for item in added {
                println!("    {} command: {}", added_prefix, item.command);
                if let Some(desc) = &item.description {
                    println!("    {} description: {}", added_prefix, desc);
                }
            }

            if order_changed {
                println!("    {}", order_changed_marker(use_color));
            }
        }
    }

    println!("────────────────────────────────────────────────────────");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_prefix_added_with_color() {
        let result = diff_prefix(true, true);
        // Should contain "+" (with color codes)
        assert!(result.contains('+'));
    }

    #[test]
    fn test_diff_prefix_removed_with_color() {
        let result = diff_prefix(true, false);
        // Should contain "-" (with color codes)
        assert!(result.contains('-'));
    }

    #[test]
    fn test_diff_prefix_added_no_color() {
        let result = diff_prefix(false, true);
        assert_eq!(result, "+");
    }

    #[test]
    fn test_diff_prefix_removed_no_color() {
        let result = diff_prefix(false, false);
        assert_eq!(result, "-");
    }

    #[test]
    fn test_order_changed_marker_with_color() {
        let result = order_changed_marker(true);
        assert!(result.contains("order changed"));
    }

    #[test]
    fn test_order_changed_marker_no_color() {
        let result = order_changed_marker(false);
        assert_eq!(result, "(order changed)");
    }

    #[test]
    fn test_diff_list_no_changes() {
        let old = vec![1, 2, 3];
        let new = vec![1, 2, 3];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert!(removed.is_empty());
        assert!(added.is_empty());
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_item_removed() {
        let old = vec![1, 2, 3];
        let new = vec![1, 3];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert_eq!(removed, vec![2]);
        assert!(added.is_empty());
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_item_added() {
        let old = vec![1, 3];
        let new = vec![1, 2, 3];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert!(removed.is_empty());
        assert_eq!(added, vec![2]);
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_item_replaced() {
        let old = vec![1, 2, 3];
        let new = vec![1, 4, 3];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert_eq!(removed, vec![2]);
        assert_eq!(added, vec![4]);
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_order_changed() {
        let old = vec![1, 2, 3];
        let new = vec![3, 2, 1];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert!(removed.is_empty());
        assert!(added.is_empty());
        assert!(order_changed);
    }

    #[test]
    fn test_diff_list_empty_old() {
        let old: Vec<i32> = vec![];
        let new = vec![1, 2, 3];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert!(removed.is_empty());
        assert_eq!(added, vec![1, 2, 3]);
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_empty_new() {
        let old = vec![1, 2, 3];
        let new: Vec<i32> = vec![];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert_eq!(removed, vec![1, 2, 3]);
        assert!(added.is_empty());
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_both_empty() {
        let old: Vec<i32> = vec![];
        let new: Vec<i32> = vec![];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert!(removed.is_empty());
        assert!(added.is_empty());
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_with_strings() {
        let old = vec!["a".to_string(), "b".to_string()];
        let new = vec!["b".to_string(), "c".to_string()];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert_eq!(removed, vec!["a".to_string()]);
        assert_eq!(added, vec!["c".to_string()]);
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_duplicates_in_old() {
        let old = vec![1, 1, 2];
        let new = vec![1, 2];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert_eq!(removed, vec![1]); // One duplicate removed
        assert!(added.is_empty());
        assert!(!order_changed);
    }

    #[test]
    fn test_diff_list_duplicates_in_new() {
        let old = vec![1, 2];
        let new = vec![1, 1, 2];

        let (removed, added, order_changed) = diff_list(&old, &new);

        assert!(removed.is_empty());
        assert_eq!(added, vec![1]); // One duplicate added
        assert!(!order_changed);
    }

    #[test]
    fn test_format_on_conflict_abort() {
        let result = format_on_conflict(&config::OnConflict::Abort);
        assert_eq!(result, "abort");
    }

    #[test]
    fn test_format_on_conflict_skip() {
        let result = format_on_conflict(&config::OnConflict::Skip);
        assert_eq!(result, "skip");
    }

    #[test]
    fn test_format_on_conflict_overwrite() {
        let result = format_on_conflict(&config::OnConflict::Overwrite);
        assert_eq!(result, "overwrite");
    }

    #[test]
    fn test_format_on_conflict_backup() {
        let result = format_on_conflict(&config::OnConflict::Backup);
        assert_eq!(result, "backup");
    }
}
