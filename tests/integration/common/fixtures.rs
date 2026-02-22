/// Minimal configuration with no operations
pub const MINIMAL_CONFIG: &str = r#"
# Minimal kabu configuration
"#;

/// Basic configuration with mkdir, link, and copy operations
pub const BASIC_CONFIG: &str = r#"
mkdir:
  - path: .cache
    description: "Create cache directory"
  - path: tmp

link:
  - source: local.env
    description: "Link environment file"

copy:
  - source: config.template
    target: config.local
    description: "Copy config template"
"#;

/// Configuration with hooks
pub const CONFIG_WITH_HOOKS: &str = r#"
hooks:
  pre_add:
    - command: "echo pre_add"
      description: "Pre-add hook"
  post_add:
    - command: "echo post_add"
      description: "Post-add hook"
  pre_remove:
    - command: "echo pre_remove"
      description: "Pre-remove hook"
  post_remove:
    - command: "echo post_remove"
      description: "Post-remove hook"

mkdir:
  - path: .local
"#;

/// Configuration with glob patterns
pub const CONFIG_WITH_GLOB: &str = r#"
link:
  - source: "fixtures/*.txt"
    description: Link all txt fixtures
"#;

/// Configuration with glob patterns and skip_tracked option
pub const CONFIG_WITH_GLOB_IGNORE_TRACKED: &str = r#"
link:
  - source: "fixtures/*"
    skip_tracked: true
    description: Link untracked fixtures only
"#;

/// Configuration with on_conflict option
pub const CONFIG_WITH_CONFLICT_SKIP: &str = r#"
on_conflict: skip

link:
  - source: local.env
"#;

pub const CONFIG_WITH_CONFLICT_BACKUP: &str = r#"
on_conflict: backup

link:
  - source: local.env
"#;

pub const CONFIG_WITH_CONFLICT_OVERWRITE: &str = r#"
on_conflict: overwrite

link:
  - source: local.env
"#;

/// Invalid configuration - absolute path
pub const INVALID_CONFIG_ABSOLUTE_PATH: &str = r#"
mkdir:
  - path: /absolute/path
"#;

/// Invalid configuration - path traversal
pub const INVALID_CONFIG_PATH_TRAVERSAL: &str = r#"
mkdir:
  - path: ../escape
"#;

/// Invalid configuration - duplicate targets
pub const INVALID_CONFIG_DUPLICATE_TARGETS: &str = r#"
link:
  - source: file1.txt
    target: same.txt
  - source: file2.txt
    target: same.txt
"#;

/// Configuration with failing pre_add hook
pub const CONFIG_WITH_FAILING_PRE_HOOK: &str = r#"
hooks:
  pre_add:
    - command: exit 1
      description: Failing pre-add hook
"#;

/// Configuration with failing post_add hook
pub const CONFIG_WITH_FAILING_POST_HOOK: &str = r#"
hooks:
  post_add:
    - command: exit 1
      description: Failing post-add hook

mkdir:
  - path: .cache
"#;
