use crate::error::{Error, Result};

use std::path::Path;

#[cfg(windows)]
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

/// Create a symlink at target pointing to source.
///
/// # Preconditions
/// - `source` must exist
/// - `target` must not exist (or conflict resolution should handle it)
///
/// # Platform-specific behavior
/// - **Unix**: Uses `std::os::unix::fs::symlink`
/// - **Windows**: Requires Developer Mode or Administrator privileges
///   - Windows 11: No special permissions required
///   - Windows 10 1703+: Developer Mode enabled
///   - Older versions: Administrator privileges
pub(crate) fn create_symlink(source: &Path, target: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = target.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target).map_err(|e| Error::SymlinkFailed {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            cause: e,
        })?;
    }

    #[cfg(windows)]
    {
        let result = if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, target)
        } else {
            std::os::windows::fs::symlink_file(source, target)
        };

        result.map_err(|e| {
            // Check for permission error on Windows
            if e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) {
                Error::WindowsSymlinkPermission
            } else {
                Error::SymlinkFailed {
                    source: source.to_path_buf(),
                    target: target.to_path_buf(),
                    cause: e,
                }
            }
        })?;
    }

    Ok(())
}

/// Check whether `target` already resolves to the same file as `source`, so
/// creating a symlink would be a no-op. Any I/O error (e.g. a missing
/// target) is treated as "not up to date".
pub(crate) fn is_up_to_date(source: &Path, target: &Path) -> bool {
    matches!(
        (std::fs::canonicalize(target), std::fs::canonicalize(source)),
        (Ok(existing), Ok(intended)) if existing == intended
    )
}

#[cfg(all(test, feature = "impure-test"))]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_create_symlink_file() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.txt");
        let target = temp.path().join("target.txt");

        std::fs::write(&source, "hello").unwrap();

        create_symlink(&source, &target).unwrap();

        assert!(target.is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn test_create_symlink_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.txt");
        let target = temp.path().join("nested/dir/target.txt");

        std::fs::write(&source, "hello").unwrap();

        create_symlink(&source, &target).unwrap();

        assert!(target.is_symlink());
    }
}
