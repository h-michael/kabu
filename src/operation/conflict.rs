use crate::config::{ConflictKind, OnConflict};
use crate::error::Result;

use std::path::Path;

/// Action to take after conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConflictAction {
    Proceed,
    Skip,
    Abort,
}

/// Check if target path already exists.
pub(crate) fn check_conflict(target: &Path) -> bool {
    target.exists()
}

/// Classify an existing conflict target as a symlink or a regular
/// file/directory, so `on_conflict` can treat them differently.
///
/// Uses `symlink_metadata`, which does not follow the final path
/// component, so a symlink is reported as such regardless of whether it
/// points to a valid location.
pub(crate) fn conflict_kind(target: &Path) -> ConflictKind {
    if target.is_symlink() {
        ConflictKind::Symlink
    } else {
        ConflictKind::File
    }
}

/// Resolve conflict by removing or backing up the target.
pub(crate) fn resolve_conflict(target: &Path, mode: OnConflict) -> Result<ConflictAction> {
    match mode {
        OnConflict::Abort => Ok(ConflictAction::Abort),
        OnConflict::Skip => Ok(ConflictAction::Skip),
        OnConflict::Overwrite => {
            // Remove existing file/symlink
            if target.is_symlink() || target.is_file() {
                std::fs::remove_file(target)?;
            } else if target.is_dir() {
                std::fs::remove_dir_all(target)?;
            }
            Ok(ConflictAction::Proceed)
        }
        OnConflict::Backup => {
            // Create backup: file.txt -> file.txt.bak, file -> file.bak
            let backup_path = match target.extension() {
                Some(ext) => target.with_extension(format!("{}.bak", ext.to_string_lossy())),
                None => target.with_extension("bak"),
            };
            std::fs::rename(target, &backup_path)?;
            Ok(ConflictAction::Proceed)
        }
    }
}

#[cfg(all(test, feature = "impure-test"))]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_check_conflict_exists() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("existing.txt");
        std::fs::write(&file, "content").unwrap();

        assert!(check_conflict(&file));
    }

    #[test]
    fn test_check_conflict_not_exists() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("nonexistent.txt");

        assert!(!check_conflict(&file));
    }

    #[test]
    fn test_resolve_conflict_abort() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();

        let action = resolve_conflict(&file, OnConflict::Abort).unwrap();
        assert_eq!(action, ConflictAction::Abort);
        assert!(file.exists()); // File should still exist
    }

    #[test]
    fn test_resolve_conflict_skip() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();

        let action = resolve_conflict(&file, OnConflict::Skip).unwrap();
        assert_eq!(action, ConflictAction::Skip);
        assert!(file.exists()); // File should still exist
    }

    #[test]
    fn test_resolve_conflict_overwrite() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();

        let action = resolve_conflict(&file, OnConflict::Overwrite).unwrap();
        assert_eq!(action, ConflictAction::Proceed);
        assert!(!file.exists()); // File should be removed
    }

    #[test]
    fn test_resolve_conflict_backup() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();

        let action = resolve_conflict(&file, OnConflict::Backup).unwrap();
        assert_eq!(action, ConflictAction::Proceed);
        assert!(!file.exists()); // Original should be moved
        assert!(temp.path().join("file.txt.bak").exists()); // Backup should exist
    }

    #[test]
    fn test_resolve_conflict_backup_no_extension() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("Makefile");
        std::fs::write(&file, "content").unwrap();

        let action = resolve_conflict(&file, OnConflict::Backup).unwrap();
        assert_eq!(action, ConflictAction::Proceed);
        assert!(!file.exists());
        assert!(temp.path().join("Makefile.bak").exists());
    }
}
