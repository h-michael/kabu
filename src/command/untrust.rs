use crate::cli::UntrustArgs;
use crate::{config, error::Error, error::Result, trust, vcs};

pub(crate) fn run(args: UntrustArgs) -> Result<()> {
    if args.list {
        let entries = trust::list_trusted()?;
        if entries.is_empty() {
            println!("No trusted repositories");
        } else {
            println!("Trusted repositories:");
            for entry in entries {
                println!("  {}", entry.main_worktree_path.display());
                println!("    Trusted at: {}", entry.trusted_at);
            }
        }
        return Ok(());
    }

    let provider = vcs::get_provider()?;
    let repo_root = match args.path {
        Some(p) => provider.main_workspace_path_for(&p.canonicalize()?)?,
        None => provider.main_workspace_root()?,
    };

    let config = config::load(&repo_root)?.ok_or_else(|| Error::ConfigNotFound {
        path: repo_root.clone(),
    })?;

    if trust::untrust(&repo_root, &config)? {
        println!("Untrusted configuration for: {}", repo_root.display());
    } else {
        println!("Configuration was not trusted: {}", repo_root.display());
    }

    Ok(())
}
