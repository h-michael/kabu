//! Path selection command implementation.
//!
//! Interactively selects a worktree/workspace and prints its path to stdout.
//! Works without shell integration, useful for scripting: `cd "$(kabu path)"`

use crate::cli::PathArgs;
use crate::error::{Error, Result};
use crate::interactive::run_path_interactive;
use crate::vcs;

pub(crate) fn run(args: PathArgs) -> Result<()> {
    let provider = vcs::get_provider()?;

    if !provider.is_inside_repo() {
        return Err(Error::NotInAnyRepo);
    }

    if args.main {
        let repo_root = provider.main_workspace_root()?;
        let main_path = provider.main_workspace_path_for(&repo_root)?;
        println!("{}", main_path.display());
    } else {
        let workspaces = provider.list_workspaces()?;
        let selected = run_path_interactive(&workspaces)?;
        println!("{}", selected.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_run_not_in_repo() {
        // This test would need to mock git::is_inside_repo() to return false
        // For now, it's a documentation test showing the error case
        // In a real environment, this would need a test setup with mocking
    }
}
