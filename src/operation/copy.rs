use crate::error::{Error, Result};

use std::path::Path;

/// Copy a file or directory to target, creating parent dirs as needed.
pub(crate) fn copy_file(source: &Path, target: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = target.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    if source.is_dir() {
        copy_dir_recursive(source, target)?;
    } else {
        std::fs::copy(source, target).map_err(|e| Error::CopyFailed {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            cause: e,
        })?;
    }

    Ok(())
}

/// Check whether `target` already holds the same content as `source`, so
/// copying would be a no-op. A symlink at `target` is never considered
/// up to date (its replacement semantics are handled as a conflict, like
/// any other `copy` target). Any I/O error (missing file, permission
/// denied) is treated as "not up to date" so the caller falls back to
/// the normal conflict path rather than failing here.
pub(crate) fn is_up_to_date(source: &Path, target: &Path) -> bool {
    let Ok(target_meta) = std::fs::symlink_metadata(target) else {
        return false;
    };
    if target_meta.file_type().is_symlink() {
        return false;
    }
    match (source.is_dir(), target_meta.is_dir()) {
        (true, true) => dirs_are_identical(source, target),
        (false, false) => files_are_identical(source, target),
        _ => false,
    }
}

fn files_are_identical(a: &Path, b: &Path) -> bool {
    matches!((std::fs::read(a), std::fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

/// Compares two directory trees by relative path and, for files, content.
/// Follows symlinks the same way `copy_dir_recursive` does (via `is_dir`),
/// so the comparison matches what an actual copy would produce.
fn dirs_are_identical(source: &Path, target: &Path) -> bool {
    let (Ok(source_entries), Ok(target_entries)) =
        (std::fs::read_dir(source), std::fs::read_dir(target))
    else {
        return false;
    };

    let (Ok(mut source_entries), Ok(mut target_entries)) = (
        source_entries.collect::<std::io::Result<Vec<_>>>(),
        target_entries.collect::<std::io::Result<Vec<_>>>(),
    ) else {
        return false;
    };

    if source_entries.len() != target_entries.len() {
        return false;
    }

    source_entries.sort_by_key(|e| e.file_name());
    target_entries.sort_by_key(|e| e.file_name());

    source_entries
        .iter()
        .zip(target_entries.iter())
        .all(|(s, t)| s.file_name() == t.file_name() && is_up_to_date(&s.path(), &t.path()))
}

/// Recursively copy a directory
fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            std::fs::copy(&source_path, &target_path).map_err(|e| Error::CopyFailed {
                source: source_path.clone(),
                target: target_path.clone(),
                cause: e,
            })?;
        }
    }

    Ok(())
}

#[cfg(all(test, feature = "impure-test"))]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_copy_file() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.txt");
        let target = temp.path().join("target.txt");

        std::fs::write(&source, "hello").unwrap();

        copy_file(&source, &target).unwrap();

        assert!(target.exists());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn test_copy_file_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.txt");
        let target = temp.path().join("nested/dir/target.txt");

        std::fs::write(&source, "hello").unwrap();

        copy_file(&source, &target).unwrap();

        assert!(target.exists());
    }

    #[test]
    fn test_copy_directory() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source_dir");
        let target_dir = temp.path().join("target_dir");

        std::fs::create_dir(&source_dir).unwrap();
        std::fs::write(source_dir.join("file1.txt"), "content1").unwrap();
        std::fs::create_dir(source_dir.join("subdir")).unwrap();
        std::fs::write(source_dir.join("subdir/file2.txt"), "content2").unwrap();

        copy_file(&source_dir, &target_dir).unwrap();

        assert!(target_dir.join("file1.txt").exists());
        assert!(target_dir.join("subdir/file2.txt").exists());
        assert_eq!(
            std::fs::read_to_string(target_dir.join("file1.txt")).unwrap(),
            "content1"
        );
    }

    #[test]
    fn test_is_up_to_date_identical_files() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let target = temp.path().join("b.txt");
        std::fs::write(&source, "same content").unwrap();
        std::fs::write(&target, "same content").unwrap();

        assert!(is_up_to_date(&source, &target));
    }

    #[test]
    fn test_is_up_to_date_different_file_content() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let target = temp.path().join("b.txt");
        std::fs::write(&source, "content a").unwrap();
        std::fs::write(&target, "content b").unwrap();

        assert!(!is_up_to_date(&source, &target));
    }

    #[test]
    fn test_is_up_to_date_missing_target() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let target = temp.path().join("missing.txt");
        std::fs::write(&source, "content").unwrap();

        assert!(!is_up_to_date(&source, &target));
    }

    #[test]
    fn test_is_up_to_date_identical_directories() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source_dir");
        let target_dir = temp.path().join("target_dir");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::create_dir(&target_dir).unwrap();
        std::fs::write(source_dir.join("a.md"), "rule a").unwrap();
        std::fs::write(target_dir.join("a.md"), "rule a").unwrap();
        std::fs::write(source_dir.join("b.md"), "rule b").unwrap();
        std::fs::write(target_dir.join("b.md"), "rule b").unwrap();

        assert!(is_up_to_date(&source_dir, &target_dir));
    }

    #[test]
    fn test_is_up_to_date_directory_with_extra_file() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source_dir");
        let target_dir = temp.path().join("target_dir");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::create_dir(&target_dir).unwrap();
        std::fs::write(source_dir.join("a.md"), "rule a").unwrap();
        std::fs::write(target_dir.join("a.md"), "rule a").unwrap();
        // Stale file that no longer exists at the source.
        std::fs::write(target_dir.join("stale.md"), "old rule").unwrap();

        assert!(!is_up_to_date(&source_dir, &target_dir));
    }

    #[test]
    fn test_is_up_to_date_symlink_target_is_never_up_to_date() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let elsewhere = temp.path().join("elsewhere.txt");
        let target = temp.path().join("b.txt");
        std::fs::write(&source, "content").unwrap();
        std::fs::write(&elsewhere, "content").unwrap();
        std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

        assert!(!is_up_to_date(&source, &target));
    }
}
