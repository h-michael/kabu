use crate::error::{Error, Result};

use std::path::{Path, PathBuf};

/// Refuse to touch `target` if a symlinked path component (the target
/// itself or any existing ancestor) would resolve outside `worktree_root`.
///
/// `target` may not exist yet, so this walks up to the deepest existing
/// ancestor, canonicalizes it (resolving every symlink on the way), and
/// re-appends the non-existent suffix. A dangling symlink at `target`
/// itself is not resolved by this check; the standard library's own
/// "already exists" error on the raw operation catches that case instead.
pub(crate) fn ensure_within_worktree(target: &Path, worktree_root: &Path) -> Result<()> {
    // In `--dry-run`, the worktree itself may not exist yet (VCS add is
    // skipped). Nothing has been written, so there is nothing to protect.
    if !worktree_root.exists() {
        return Ok(());
    }
    let canonical_root = std::fs::canonicalize(worktree_root)?;

    let mut ancestor = target.to_path_buf();
    let mut suffix = PathBuf::new();
    while !ancestor.exists() {
        let Some(file_name) = ancestor.file_name() else {
            // Reached the filesystem root without finding an existing
            // ancestor; nothing to canonicalize against.
            return Ok(());
        };
        suffix = Path::new(file_name).join(&suffix);
        ancestor.pop();
    }

    let canonical_ancestor = std::fs::canonicalize(&ancestor)?;
    let resolved = canonical_ancestor.join(&suffix);

    if !resolved.starts_with(&canonical_root) {
        return Err(Error::TargetEscapesWorktree {
            target: target.to_path_buf(),
            resolved,
        });
    }

    Ok(())
}

#[cfg(all(test, feature = "impure-test"))]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_ensure_within_worktree_plain_path() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path().join("worktree");
        std::fs::create_dir(&worktree).unwrap();

        let target = worktree.join("some/new/dir");
        ensure_within_worktree(&target, &worktree).unwrap();
    }

    #[test]
    fn test_ensure_within_worktree_existing_target_is_symlink() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path().join("worktree");
        let main_repo = temp.path().join("main");
        std::fs::create_dir(&worktree).unwrap();
        std::fs::create_dir_all(main_repo.join(".claude/rules")).unwrap();

        let target = worktree.join(".claude/rules");
        std::fs::create_dir(worktree.join(".claude")).unwrap();
        std::os::unix::fs::symlink(main_repo.join(".claude/rules"), &target).unwrap();

        let err = ensure_within_worktree(&target, &worktree).unwrap_err();
        assert!(matches!(err, Error::TargetEscapesWorktree { .. }));
    }

    #[test]
    fn test_ensure_within_worktree_parent_is_symlink() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path().join("worktree");
        let main_repo = temp.path().join("main");
        std::fs::create_dir(&worktree).unwrap();
        std::fs::create_dir_all(main_repo.join(".claude/rules")).unwrap();

        // .claude itself is a symlink into the main checkout; rules
        // is a real, pre-existing subdirectory reached through it.
        std::os::unix::fs::symlink(main_repo.join(".claude"), worktree.join(".claude")).unwrap();

        let target = worktree.join(".claude/rules");
        let err = ensure_within_worktree(&target, &worktree).unwrap_err();
        assert!(matches!(err, Error::TargetEscapesWorktree { .. }));
    }

    #[test]
    fn test_ensure_within_worktree_parent_is_symlink_new_leaf() {
        let temp = TempDir::new().unwrap();
        let worktree = temp.path().join("worktree");
        let main_repo = temp.path().join("main");
        std::fs::create_dir(&worktree).unwrap();
        std::fs::create_dir_all(&main_repo).unwrap();

        std::os::unix::fs::symlink(&main_repo, worktree.join(".claude")).unwrap();

        // "newdir" doesn't exist anywhere yet, but its parent (.claude)
        // is a symlink pointing outside the worktree.
        let target = worktree.join(".claude/newdir");
        let err = ensure_within_worktree(&target, &worktree).unwrap_err();
        assert!(matches!(err, Error::TargetEscapesWorktree { .. }));
    }
}
