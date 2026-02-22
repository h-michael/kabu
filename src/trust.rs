//! Hook trust management system
//!
//! This module manages whether git hooks in `.kabu/config.yaml` have been explicitly reviewed
//! and trusted by the user. Trust is based on a SHA256 hash of the hook commands and
//! descriptions, combined with the primary worktree path.
//!
//! ## Design Overview
//!
//! **Primary Worktree Path Usage**: Uses the primary (main) worktree path for hashing instead
//! of the current repository path. This allows all worktrees created from the same main
//! worktree to share the same trust state. When a user creates multiple worktrees (e.g.,
//! feature branches), they all trust the same hooks without re-approval.
//!
//! **Nested Directory Structure**: Trust files are stored in directories named after the
//! primary worktree path (e.g., `~/.local/share/kabu/trusted/-foo-bar-baz/{hash}.yaml`).
//! This structure provides performance benefits: when many repositories are trusted, the
//! nested directory avoids scanning hundreds of files in a single flat directory.
//!
//! **Automatic Cleanup of Old Files**: When hooks are re-trusted with new content,
//! all previous trust files for that primary worktree are deleted. This prevents
//! reversion attacks where an attacker could restore old `.kabu/config.yaml` and use previously
//! trusted hooks. Only the current hooks hash is valid for a given primary worktree.
//!
//! **Path Verification**: The main_worktree_path is stored in the trust file and
//! verified during `is_trusted()`. This prevents symlink attacks where an attacker could
//! replace the worktree directory with a symlink to a different path with old trusted hooks.
//!
//! **Empty Hooks Are Implicitly Trusted**: If `.kabu/config.yaml` has no hooks (or is empty),
//! `is_trusted()` returns true immediately without checking disk. Rationale: no hooks means
//! nothing to trust; the user cannot create a `.kabu/config.yaml` without hooks and then claim
//! hooks exist.

use crate::config::{Config, ConfigSnapshot};
use crate::error::{Error, Result};

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(all(test, feature = "impure-test"))]
use crate::config::HookEntry;

const TRUST_DIR_NAME: &str = "kabu/trusted";
const TRUST_VERSION: u32 = 1;

#[cfg(all(test, feature = "impure-test"))]
thread_local! {
    static TEST_TRUST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Represents a trusted configuration entry.
///
/// Fields:
/// - `version`: Version of the trust format (currently 1).
/// - `main_worktree_path`: The main worktree path (where `.git` is a directory).
///   All worktrees created from this path share the same trust. Stored for verification
///   during trust checks to prevent symlink attacks.
/// - `trusted_at`: RFC3339 timestamp of when the configuration was trusted.
/// - `config_snapshot`: Snapshot of the entire configuration at trust time for detecting changes.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TrustEntry {
    pub version: u32,
    pub main_worktree_path: PathBuf,
    pub trusted_at: String,
    pub config_snapshot: ConfigSnapshot,
}

/// Get trust storage directory with versioning support.
/// Uses XDG_DATA_HOME or falls back to ~/.local/share on Linux
/// Path format: ~/.local/share/kabu/trusted/v1/
fn trust_dir() -> Result<PathBuf> {
    #[cfg(all(test, feature = "impure-test"))]
    {
        if let Some(path) = TEST_TRUST_DIR.with(|dir| dir.borrow().clone()) {
            return Ok(path);
        }
    }
    if let Some(path) = std::env::var_os("KABU_TRUST_DIR") {
        return Ok(PathBuf::from(path));
    }
    let base = dirs::data_dir().ok_or(Error::TrustStorageNotFound)?;
    Ok(base
        .join(TRUST_DIR_NAME)
        .join(format!("v{}", TRUST_VERSION)))
}

#[cfg(all(test, feature = "impure-test"))]
fn set_test_trust_dir(path: PathBuf) {
    TEST_TRUST_DIR.with(|dir| {
        *dir.borrow_mut() = Some(path);
    });
}

/// Compute SHA256 hash of full configuration.
///
/// Combines the main worktree path and entire configuration (defaults, worktree,
/// hooks, mkdir, link, copy operations) into a single hash. This hash is used as
/// the trust file identifier. If any configuration changes, the hash changes, requiring re-trust.
///
/// Hash includes (field-by-field with JSON serialization):
/// - Canonicalized main_worktree_path (prevents different path representations)
/// - defaults (OnConflict option)
/// - worktree (path_template, branch_template)
/// - hooks (pre_add, post_add, pre_remove, post_remove)
/// - mkdir operations (path, description)
/// - link operations (source, target, on_conflict, description, skip_tracked)
/// - copy operations (source, target, on_conflict, description)
///
/// **Stability**: Uses explicit JSON serialization to ensure the hash remains stable
/// across Rust compiler versions. JSON's stable text representation ensures consistency.
///
/// **Field-by-field approach**: Each field is serialized independently to be more
/// stable than serializing the entire Config at once, which could be affected by
/// struct field ordering changes.
///
/// Returns the hash as a lowercase hex string (64 hex characters for SHA256).
pub(crate) fn compute_hash(main_worktree_path: &Path, config: &Config) -> Result<String> {
    let mut hasher = Sha256::new();

    // Canonicalize to ensure stable representation across systems
    let canonical_path =
        main_worktree_path
            .canonicalize()
            .map_err(|e| Error::TrustVerificationFailed {
                message: format!("Failed to canonicalize worktree path: {}", e),
            })?;

    // Hash the main worktree path
    let path_str = canonical_path.to_string_lossy();
    hasher.update(path_str.as_bytes());
    hasher.update(b"\n");

    // Create a ConfigSnapshot from the Config for stable hashing
    let snapshot = ConfigSnapshot::from_config(config);

    // Serialize the snapshot as JSON for stable representation
    let snapshot_json =
        serde_json::to_string(&snapshot).map_err(|e| Error::TrustFileSerialization {
            message: format!("Failed to serialize config snapshot: {}", e),
        })?;
    hasher.update(snapshot_json.as_bytes());

    Ok(format!("{:x}", hasher.finalize()))
}

/// Convert main worktree path to directory name for nested storage.
///
/// Hashes the path to create an OS-independent, filesystem-safe directory name.
/// Uses SHA256 hash of the path (first 16 hex chars) to avoid OS-specific invalid characters.
///
/// Example outputs:
/// - "/home/user/myrepo" → "a1b2c3d4e5f6g7h8"
/// - "C:\Users\user\repo" → "f8e7d6c5b4a39291"
///
/// Purpose: Organize trust files into subdirectories per main worktree path while
/// ensuring compatibility across Unix and Windows filesystems.
/// Performance benefit: Prevents a single directory from containing thousands of files
/// when many repositories are trusted. Instead, each main worktree gets its own directory.
fn main_worktree_dir_name(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let path_str = path.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();

    // Use first 16 hex characters for reasonable uniqueness while keeping directory names shorter
    format!("{:x}", hash)[..16].to_string()
}

/// Check if configuration is trusted for the given main worktree.
///
/// Returns true if:
/// 1. No hooks are defined (empty configuration)
/// 2. A trust file exists with a matching hash AND the stored main_worktree_path matches
///
/// Returns false if:
/// 1. Hooks exist but no trust file found
/// 2. Trust file exists but main_worktree_path doesn't match (symlink attack protection)
/// 3. Trust file is corrupted
///
/// **TOCTOU Safety**: Directly attempts to read the file without pre-checking existence.
/// "File not found" errors are treated as Ok(false) for atomicity. If the file is deleted
/// between check and read, we get the correct result (not trusted).
///
/// **Path Verification**: Prevents symlink attacks where an attacker could replace the
/// worktree with a symlink to a different location containing old trusted hooks.
pub(crate) fn is_trusted(main_worktree_path: &Path, config: &Config) -> Result<bool> {
    if !config.hooks.has_hooks() {
        return Ok(true); // No hooks = implicitly trusted
    }

    // Canonicalize path - fail if it doesn't exist, ensuring consistent behavior
    let canonical_path =
        main_worktree_path
            .canonicalize()
            .map_err(|e| Error::TrustVerificationFailed {
                message: format!("Failed to canonicalize worktree path: {}", e),
            })?;

    let hash = compute_hash(&canonical_path, config)?;
    let dir_name = main_worktree_dir_name(&canonical_path);
    let trust_file = trust_dir()?.join(&dir_name).join(format!("{}.yaml", hash));

    // TOCTOU-safe: Attempt to read file directly instead of checking existence first.
    // If file is deleted between our check and read, we get correct result (not trusted).
    let content = match fs::read_to_string(&trust_file) {
        Ok(content) => content,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    // Verify stored main_worktree_path matches current path
    let entry: TrustEntry =
        serde_yaml::from_str(&content).map_err(|e| Error::TrustFileCorrupted {
            message: e.to_string(),
        })?;

    if entry.main_worktree_path != canonical_path {
        return Ok(false);
    }

    Ok(true)
}

/// Read a trust entry for a repository to get previous configuration snapshot.
///
/// Returns the TrustEntry if found and valid, or None if no trust file exists.
/// Validates that main_worktree_path matches the stored path to prevent using
/// misplaced or tampered trust files.
///
/// **Note**: Typically only one trust file exists per repository. If multiple files
/// are present, returns the first one found (filesystem order-dependent).
/// Used to detect configuration changes and display diffs before re-trusting.
pub(crate) fn read_trust_entry(main_worktree_path: &Path) -> Result<Option<TrustEntry>> {
    let canonical_path =
        main_worktree_path
            .canonicalize()
            .map_err(|e| Error::TrustVerificationFailed {
                message: format!("Failed to canonicalize worktree path: {}", e),
            })?;
    let dir_name = main_worktree_dir_name(&canonical_path);
    let trust_dir = trust_dir()?;
    let repo_trust_dir = trust_dir.join(&dir_name);

    // If directory doesn't exist, no trust files
    if !repo_trust_dir.exists() {
        return Ok(None);
    }

    // Try to find and read the trust file. Validates that main_worktree_path matches
    // the stored path to prevent using misplaced or tampered trust files.
    match fs::read_dir(&repo_trust_dir) {
        Ok(entries) => {
            // Get the first trust file found (typically only one exists)
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "yaml") {
                    let content = fs::read_to_string(&path)?;
                    let trust_entry: TrustEntry = serde_yaml::from_str(&content).map_err(|e| {
                        Error::TrustFileSerialization {
                            message: format!("Failed to parse trust file: {}", e),
                        }
                    })?;

                    // Verify main_worktree_path matches to prevent using misplaced files
                    if trust_entry.main_worktree_path != canonical_path {
                        return Ok(None);
                    }

                    return Ok(Some(trust_entry));
                }
            }
            Ok(None)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Trust configuration for a repository by marking it as reviewed and approved.
///
/// Creates a trust file at `~/.local/share/kabu/trusted/v1/{primary-worktree-dir}/{hash}.yaml`
/// containing the main_worktree_path, timestamp, and full configuration snapshot.
///
/// **Automatic Cleanup**: Deletes all old trust files in the same directory before creating
/// the new one. This prevents reversion attacks where an attacker could restore an old
/// `.kabu/config.yaml` and use previously trusted configuration with different content.
///
/// Reversion Attack Scenario (prevented by this cleanup):
/// 1. User trusts config (file hash=ABC123 created)
/// 2. Attacker modifies `.kabu/config.yaml` with dangerous commands
/// 3. User runs `kabu trust` again with new content (file hash=XYZ789 created)
/// 4. Attacker reverts `.kabu/config.yaml` to original content (hash=ABC123)
/// 5. OLD BEHAVIOR: hash=ABC123 still exists, config trusted without re-review
/// 6. NEW BEHAVIOR: hash=ABC123 was deleted in step 3, reversion fails
pub(crate) fn trust(main_worktree_path: &Path, config: &Config) -> Result<()> {
    if !config.hooks.has_hooks() {
        return Ok(());
    }

    // Canonicalize path - fail if it doesn't exist
    let canonical_path =
        main_worktree_path
            .canonicalize()
            .map_err(|e| Error::TrustVerificationFailed {
                message: format!("Failed to canonicalize worktree path: {}", e),
            })?;

    let trust_base_path = trust_dir()?;
    let dir_name = main_worktree_dir_name(&canonical_path);
    let trust_repo_path = trust_base_path.join(&dir_name);
    fs::create_dir_all(&trust_repo_path)?;

    // Remove old trust files for this main_worktree_path (before saving new one).
    // Security: Prevents reversion attacks by ensuring only the current config hash
    // is valid for this primary worktree. If .kabu/config.yaml is reverted to an older
    // version, the old hash file won't exist and re-trust will be required.
    if trust_repo_path.exists() {
        for entry in fs::read_dir(&trust_repo_path)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yaml") {
                let _ = fs::remove_file(&path);
            }
        }
    }

    let hash = compute_hash(&canonical_path, config)?;
    let trust_file = trust_repo_path.join(format!("{}.yaml", hash));

    let snapshot = ConfigSnapshot::from_config(config);
    let entry = TrustEntry {
        version: TRUST_VERSION,
        main_worktree_path: canonical_path,
        trusted_at: Utc::now().to_rfc3339(),
        config_snapshot: snapshot,
    };

    let content = serde_yaml::to_string(&entry).map_err(|e| Error::TrustFileSerialization {
        message: e.to_string(),
    })?;

    fs::write(&trust_file, content)?;

    Ok(())
}

/// Remove trust for configuration of a repository.
///
/// Deletes the trust file matching the current configuration hash. Returns true if a trust file
/// existed and was deleted, false if no trust file was found for this configuration.
pub(crate) fn untrust(main_worktree_path: &Path, config: &Config) -> Result<bool> {
    if !config.hooks.has_hooks() {
        return Ok(false);
    }

    // Canonicalize path - fail if it doesn't exist
    let canonical_path =
        main_worktree_path
            .canonicalize()
            .map_err(|e| Error::TrustVerificationFailed {
                message: format!("Failed to canonicalize worktree path: {}", e),
            })?;

    let hash = compute_hash(&canonical_path, config)?;
    let dir_name = main_worktree_dir_name(&canonical_path);
    let trust_file = trust_dir()?.join(&dir_name).join(format!("{}.yaml", hash));

    if trust_file.exists() {
        fs::remove_file(&trust_file)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// List all trusted repository hooks.
///
/// Traverses the nested directory structure at `~/.local/share/kabu/trusted/`
/// and returns all TrustEntry objects found. Each directory under `trusted/` represents
/// a different main_worktree_path, and each `.yaml` file within that directory represents
/// a different trusted hook configuration.
///
/// Returns an empty vector if no trust files exist.
/// Logs warnings to stderr for corrupted or unreadable files, but continues processing.
pub(crate) fn list_trusted() -> Result<Vec<TrustEntry>> {
    let trust_path = trust_dir()?;

    if !trust_path.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for dir_entry in fs::read_dir(&trust_path)?.flatten() {
        let dir_path = dir_entry.path();
        if dir_path.is_dir() {
            for file_entry in fs::read_dir(&dir_path)?.flatten() {
                let file_path = file_entry.path();
                if file_path.extension().is_some_and(|e| e == "yaml") {
                    match fs::read_to_string(&file_path) {
                        Ok(content) => match serde_yaml::from_str::<TrustEntry>(&content) {
                            Ok(trust_entry) => entries.push(trust_entry),
                            Err(e) => {
                                eprintln!(
                                    "Failed to parse trust file: {}\n         {}",
                                    file_path.display(),
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            eprintln!(
                                "Failed to read trust file: {}\n         {}",
                                file_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(entries)
}

#[cfg(all(test, feature = "impure-test"))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::{AutoCd, Config, Hooks, Mkdir, Ui, Worktree};
    use std::sync::OnceLock;
    use tempfile::TempDir;

    fn init_test_data_dir() {
        static DATA_DIR: OnceLock<TempDir> = OnceLock::new();
        let dir = DATA_DIR.get_or_init(|| TempDir::new().unwrap());
        set_test_trust_dir(dir.path().to_path_buf());
    }

    fn create_test_config() -> Config {
        Config {
            on_conflict: None,
            auto_cd: AutoCd::default(),
            worktree: Worktree {
                path_template: None,
                branch_template: None,
            },
            ui: Ui::default(),
            hooks: Hooks::default(),
            mkdir: Vec::new(),
            link: Vec::new(),
            copy: Vec::new(),
        }
    }

    #[test]
    fn test_compute_hash_empty_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config();

        let hash = compute_hash(temp_dir.path(), &config).unwrap();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters
    }

    #[test]
    fn test_compute_hash_different_hooks() {
        let temp_dir = TempDir::new().unwrap();

        let mut config1 = create_test_config();
        config1.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test1'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        let mut config2 = create_test_config();
        config2.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test2'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        let hash1 = compute_hash(temp_dir.path(), &config1).unwrap();
        let hash2 = compute_hash(temp_dir.path(), &config2).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_different_mkdir() {
        let temp_dir = TempDir::new().unwrap();
        use std::path::PathBuf;

        let mut config1 = create_test_config();
        config1.mkdir = vec![Mkdir {
            path: PathBuf::from("dir1"),
            description: None,
        }];

        let mut config2 = create_test_config();
        config2.mkdir = vec![Mkdir {
            path: PathBuf::from("dir2"),
            description: None,
        }];

        let hash1 = compute_hash(temp_dir.path(), &config1).unwrap();
        let hash2 = compute_hash(temp_dir.path(), &config2).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_hash_different_worktree() {
        let temp_dir = TempDir::new().unwrap();

        let mut config1 = create_test_config();
        config1.worktree.path_template = Some("../wt1".to_string());

        let mut config2 = create_test_config();
        config2.worktree.path_template = Some("../wt2".to_string());

        let hash1 = compute_hash(temp_dir.path(), &config1).unwrap();
        let hash2 = compute_hash(temp_dir.path(), &config2).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_is_trusted_empty_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config();

        assert!(is_trusted(temp_dir.path(), &config).unwrap());
    }

    #[test]
    fn test_trust_and_untrust() {
        init_test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config();
        config.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Initially not trusted
        assert!(!is_trusted(temp_dir.path(), &config).unwrap());

        // Trust the config
        trust(temp_dir.path(), &config).unwrap();
        assert!(is_trusted(temp_dir.path(), &config).unwrap());

        // Untrust the config
        assert!(untrust(temp_dir.path(), &config).unwrap());
        assert!(!is_trusted(temp_dir.path(), &config).unwrap());

        // Untrusting again should return false
        assert!(!untrust(temp_dir.path(), &config).unwrap());
    }

    #[test]
    fn test_list_trusted_no_error() {
        init_test_data_dir();
        // Just ensure list_trusted() doesn't error
        // Length may vary depending on system state
        let _entries = list_trusted().unwrap();
    }

    #[test]
    fn test_list_trusted_with_trusted_repo() {
        init_test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config();
        config.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test'".to_string(),
                description: None,
            }],
            post_add: vec![HookEntry {
                command: "npm install".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Trust the config
        trust(temp_dir.path(), &config).unwrap();

        // List should include this repo
        let entries = list_trusted().unwrap();
        let canonical_path = temp_dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| temp_dir.path().to_path_buf());

        let found = entries
            .iter()
            .any(|e| e.main_worktree_path == canonical_path);
        assert!(found, "Trusted repository should be in the list");

        // Cleanup
        untrust(temp_dir.path(), &config).unwrap();
    }

    #[test]
    fn test_list_trusted_multiple_repos() {
        init_test_data_dir();
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let mut config1 = create_test_config();
        config1.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'repo1'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        let mut config2 = create_test_config();
        config2.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'repo2'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Trust both repos
        trust(temp_dir1.path(), &config1).unwrap();
        trust(temp_dir2.path(), &config2).unwrap();

        // List should include both
        let entries = list_trusted().unwrap();
        let canonical_path1 = temp_dir1
            .path()
            .canonicalize()
            .unwrap_or_else(|_| temp_dir1.path().to_path_buf());
        let canonical_path2 = temp_dir2
            .path()
            .canonicalize()
            .unwrap_or_else(|_| temp_dir2.path().to_path_buf());

        let found1 = entries
            .iter()
            .any(|e| e.main_worktree_path == canonical_path1);
        let found2 = entries
            .iter()
            .any(|e| e.main_worktree_path == canonical_path2);

        assert!(found1, "First trusted repository should be in the list");
        assert!(found2, "Second trusted repository should be in the list");

        // Cleanup
        untrust(temp_dir1.path(), &config1).unwrap();
        untrust(temp_dir2.path(), &config2).unwrap();
    }

    #[test]
    fn test_trust_entry_contains_main_worktree_path() {
        init_test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config();
        config.hooks = Hooks {
            hook_shell: None,
            pre_add: vec![HookEntry {
                command: "echo 'pre'".to_string(),
                description: None,
            }],
            post_add: vec![HookEntry {
                command: "npm install".to_string(),
                description: None,
            }],
            pre_remove: vec![HookEntry {
                command: "echo 'cleanup'".to_string(),
                description: None,
            }],
            post_remove: vec![HookEntry {
                command: "./scripts/cleanup.sh".to_string(),
                description: None,
            }],
        };

        // Trust the config
        trust(temp_dir.path(), &config).unwrap();

        // Find the entry
        let entries = list_trusted().unwrap();
        let canonical_path = temp_dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| temp_dir.path().to_path_buf());

        let entry = entries
            .iter()
            .find(|e| e.main_worktree_path == canonical_path)
            .expect("Should find trusted entry");

        // Verify main_worktree_path is set correctly
        assert_eq!(entry.main_worktree_path, canonical_path);

        // Verify trusted_at is set
        assert!(!entry.trusted_at.is_empty());

        // Cleanup
        untrust(temp_dir.path(), &config).unwrap();
    }

    #[test]
    fn test_is_trusted_hooks_changed() {
        init_test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let mut config1 = create_test_config();
        config1.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'original'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Trust the original config
        trust(temp_dir.path(), &config1).unwrap();
        assert!(is_trusted(temp_dir.path(), &config1).unwrap());

        // Change the hooks (different command)
        let mut config2 = create_test_config();
        config2.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'modified'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Should not be trusted anymore
        assert!(!is_trusted(temp_dir.path(), &config2).unwrap());

        // Cleanup
        untrust(temp_dir.path(), &config1).unwrap();
    }

    #[test]
    fn test_is_trusted_hooks_removed() {
        init_test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config();
        config.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Trust the config
        trust(temp_dir.path(), &config).unwrap();
        assert!(is_trusted(temp_dir.path(), &config).unwrap());

        // Remove hooks (empty config)
        let empty_config = create_test_config();

        // Empty hooks should be implicitly trusted
        assert!(is_trusted(temp_dir.path(), &empty_config).unwrap());

        // Cleanup
        untrust(temp_dir.path(), &config).unwrap();
    }

    #[test]
    fn test_is_trusted_different_main_worktree_path() {
        use std::fs;

        init_test_data_dir();
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let mut config = create_test_config();
        config.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Trust config for temp_dir1
        trust(temp_dir1.path(), &config).unwrap();
        assert!(is_trusted(temp_dir1.path(), &config).unwrap());

        // Manually create trust file with different main_worktree_path
        let hash = compute_hash(temp_dir1.path(), &config).unwrap();
        let dir_name = main_worktree_dir_name(
            &temp_dir1
                .path()
                .canonicalize()
                .unwrap_or_else(|_| temp_dir1.path().to_path_buf()),
        );
        let trust_subdir = trust_dir().unwrap().join(&dir_name);
        fs::create_dir_all(&trust_subdir).unwrap();
        let trust_file = trust_subdir.join(format!("{}.yaml", hash));

        // Overwrite with different main_worktree_path
        let snapshot = ConfigSnapshot::from_config(&config);
        let fake_entry = TrustEntry {
            version: TRUST_VERSION,
            main_worktree_path: temp_dir2.path().canonicalize().unwrap(),
            trusted_at: chrono::Utc::now().to_rfc3339(),
            config_snapshot: snapshot,
        };
        let content = serde_yaml::to_string(&fake_entry).unwrap();
        fs::write(&trust_file, content).unwrap();

        // Should not be trusted for temp_dir1 anymore (main_worktree_path mismatch)
        assert!(!is_trusted(temp_dir1.path(), &config).unwrap());

        // Cleanup
        untrust(temp_dir1.path(), &config).unwrap();
    }

    #[test]
    fn test_is_trusted_corrupted_trust_file() {
        use std::fs;

        init_test_data_dir();
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config();
        config.hooks = Hooks {
            pre_add: vec![HookEntry {
                command: "echo 'test'".to_string(),
                description: None,
            }],
            ..Default::default()
        };

        // Trust the config
        trust(temp_dir.path(), &config).unwrap();
        assert!(is_trusted(temp_dir.path(), &config).unwrap());

        // Corrupt the trust file
        let hash = compute_hash(temp_dir.path(), &config).unwrap();
        let canonical_path = temp_dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| temp_dir.path().to_path_buf());
        let dir_name = main_worktree_dir_name(&canonical_path);
        let trust_file = trust_dir()
            .unwrap()
            .join(&dir_name)
            .join(format!("{}.yaml", hash));
        fs::write(&trust_file, "invalid yaml content {{{").unwrap();

        // Should return an error
        let result = is_trusted(temp_dir.path(), &config);
        assert!(result.is_err());

        // Cleanup (remove corrupted file)
        fs::remove_file(&trust_file).ok();
    }
}
