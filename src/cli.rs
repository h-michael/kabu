use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

/// CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "kabu")]
#[command(about = "Enhance git worktree and jj workspace with automated setup")]
#[command(version = VERSION_STRING)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu add ../new-worktree
    kabu list
    kabu remove ../new-worktree
    kabu config validate

WHAT THIS COMMAND DOES:
    - Detects git/jj automatically
    - Runs selected subcommand
    - Uses .kabu/config.yaml or .kabu/config.toml (YAML wins)

DISCOVERY MAP:
    - Worktree/workspace lifecycle: kabu add, kabu list, kabu remove
    - Shell navigation and integration: kabu path, kabu cd, kabu init, kabu completions
    - Configuration and schema: kabu config, kabu config schema
    - Hook trust management: kabu trust, kabu untrust

SAFETY NOTES:
    - hooks do not run until trusted (kabu trust)
    - remove warns before risky deletion unless --force is used

SEE ALSO:
    kabu <command> --help
    kabu man")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

const VERSION_STRING: &str = env!("KABU_VERSION_LABEL");

/// Available subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Add a new worktree/workspace with setup
    Add(AddArgs),

    /// Remove worktrees/workspaces with safety checks
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),

    /// List all worktrees/workspaces
    #[command(visible_alias = "ls")]
    List(ListArgs),

    /// Select a worktree/workspace and print its path
    Path(PathArgs),

    /// Change directory to a selected worktree/workspace (requires shell integration)
    Cd,

    /// Manage configuration (.kabu/config.yaml or .kabu/config.toml)
    Config(ConfigArgs),

    /// Trust hooks in config file for the current repository
    Trust(TrustArgs),

    /// Revoke trust for hooks in config file
    Untrust(UntrustArgs),

    /// Generate shell completion script
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },

    /// Print shell init script (completions + trust warning hook)
    Init(InitArgs),

    /// Generate man page
    Man,
}

/// Arguments for the `config` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu config new
    kabu config validate
    kabu config get auto_cd.after_remove
    kabu config schema > schema.json

WHAT THIS COMMAND DOES:
    - Creates, validates, and inspects .kabu config files
    - Supports YAML and TOML (YAML has higher priority)
    - Validates strict keys (unknown keys fail)

CONFIG KEYS (TOP LEVEL):
    on_conflict: abort | skip | overwrite | backup
    auto_cd:
      after_add: true | false
      after_remove: main | select
    worktree:
      path_template: string
      branch_template: string
    mkdir:
      - path: string
        description: string (optional)
    link:
      - source: string
        target: string (optional; default = source)
        skip_tracked: bool (optional; default = false)
        on_conflict: abort | skip | overwrite | backup (optional)
        description: string (optional)
    copy:
      - source: string
        target: string (optional; default = source)
        on_conflict: abort | skip | overwrite | backup (optional)
        description: string (optional)
    hooks:
      hook_shell: string (optional)
      pre_add/post_add/pre_remove/post_remove:
        - command: string (required)
          description: string (optional)
    ui:
      add_default_mode: existing | new
      show_key_hints: bool
      colors: accent/border/disabled/error/footer/header/label/muted/preview/
              search/selection_bg/selection_fg/text/title/warning

TEMPLATE VARIABLES:
    worktree.path_template:
      {{branch}}, {{repository}}
    worktree.branch_template:
      {{commitish}}, {{repository}}, {{strftime(...)}} (e.g. {{strftime(%Y%m%d)}})
    hooks.command:
      {{worktree_path}}, {{worktree_name}}, {{branch}}, {{repo_root}}, {{vcs_type}}
      jj-specific: {{change_id}}, {{commit_id}}
      aliases: {{workspace_path}}, {{workspace_name}}, {{bookmark}}

HOOK ENVIRONMENT VARIABLES:
    KABU_WORKTREE_PATH, KABU_WORKTREE_NAME, KABU_BRANCH, KABU_REPO_ROOT,
    KABU_VCS_TYPE, KABU_CHANGE_ID, KABU_COMMIT_ID

CONFLICT RESOLUTION PRIORITY:
    CLI --on-conflict > per-entry on_conflict > top-level on_conflict > prompt/default behavior

EXECUTION ORDER:
    add:    pre_add -> VCS add -> mkdir -> link -> copy -> post_add
    remove: pre_remove -> VCS remove -> post_remove

SAFETY NOTES:
    - Hooks require trust before execution (kabu trust)
    - Template placeholders use double braces: {{branch}}
    - Paths in mkdir/link/copy must be relative; unknown keys fail validation

SEE ALSO:
    kabu config <subcommand> --help
    kabu trust --help
    kabu man")]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub command: Option<ConfigCommand>,
}

/// Config file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ConfigFormatArg {
    #[default]
    Yaml,
    Toml,
}

impl std::str::FromStr for ConfigFormatArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yaml" | "yml" => Ok(Self::Yaml),
            "toml" => Ok(Self::Toml),
            _ => Err(format!("Invalid format: {s}. Valid values: yaml, toml")),
        }
    }
}

/// Config subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum ConfigCommand {
    /// Validate configuration file (.kabu/config.yaml or .kabu/config.toml)
    Validate,
    /// Generate JSON Schema for configuration
    Schema,
    /// Create a new configuration file
    New {
        /// Create global config at:
        ///   - $XDG_CONFIG_HOME/kabu/config.{yaml,toml} (if XDG_CONFIG_HOME is set)
        ///   - ~/.config/kabu/config.{yaml,toml} (Linux)
        ///   - ~/Library/Application Support/kabu/config.{yaml,toml} (macOS)
        ///   - %APPDATA%\kabu\config.{yaml,toml} (Windows)
        #[arg(short, long, verbatim_doc_comment)]
        global: bool,

        /// Config file format: yaml or toml (default: yaml)
        #[arg(short, long, default_value = "yaml")]
        format: ConfigFormatArg,

        /// Custom path for the config file
        #[arg(short, long)]
        path: Option<std::path::PathBuf>,

        /// Overwrite existing config file
        #[arg(short = 'O', long = "override")]
        override_existing: bool,

        /// Create .kabu/.gitignore to exclude config from git
        #[arg(long)]
        with_gitignore: bool,

        /// Do not create .kabu/.gitignore (skip prompt)
        #[arg(long, conflicts_with = "with_gitignore")]
        without_gitignore: bool,
    },
    /// Get a configuration value
    Get {
        /// Configuration key (e.g., auto_cd.after_remove)
        key: String,
    },
}

/// Arguments for the `add` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu add ../new-worktree
    kabu add -b feature-x ../feature-x
    kabu add --interactive
    kabu add --dry-run ../preview

WHAT THIS COMMAND DOES:
    - Creates a git worktree / jj workspace
    - Runs setup in fixed order: mkdir -> link -> copy
    - Can skip setup with --no-setup

VCS MAPPING:
    - git: git worktree add
    - jj:  jj workspace add
    - colocated (git+jj): jj workspace add is used

ARGUMENT RULES:
    - [PATH] is required unless --interactive, or path_template + branch (-b/-B/COMMITISH)
    - [COMMITISH] is optional revision/branch input
    - -b/-B creates or resets branch/bookmark depending on VCS

CONFLICT MODES:
    abort      stop on first conflict
    skip       keep existing target and continue
    overwrite  replace existing target
    backup     rename existing target with .bak then continue

ENVIRONMENT VARIABLES:
    KABU_ON_CONFLICT  default for --on-conflict
    KABUHOOK_SHELL    shell override for hook execution

SAFETY NOTES:
    - --dry-run previews actions without changes
    - --no-setup runs only VCS add
    - hooks run only after trust (kabu trust)
    - operation order is fixed and independent of YAML order

SEE ALSO:
    kabu remove --help
    kabu config --help")]
pub(crate) struct AddArgs {
    /// Path for the new worktree/workspace (required unless --interactive, or path_template + branch)
    pub path: Option<PathBuf>,

    /// Branch or commit to checkout (git) / revision (jj)
    pub commitish: Option<String>,

    // --- kabu Options ---
    /// Interactive mode: select branch and path interactively
    #[arg(short, long, help_heading = "kabu Options")]
    pub interactive: bool,

    /// How to handle conflicts: abort, skip, overwrite, backup
    #[arg(
        long,
        value_name = "MODE",
        help_heading = "kabu Options",
        env = "KABU_ON_CONFLICT"
    )]
    pub on_conflict: Option<OnConflictArg>,

    /// Preview actions without executing
    #[arg(long, help_heading = "kabu Options")]
    pub dry_run: bool,

    /// Skip config file setup, run worktree/workspace add only
    #[arg(long, help_heading = "kabu Options")]
    pub no_setup: bool,

    /// Windows-only: select hook shell (pwsh, powershell, bash, cmd, wsl)
    #[cfg(windows)]
    #[arg(
        long,
        value_name = "SHELL",
        help_heading = "kabu Options",
        value_parser = [
            "pwsh",
            "powershell",
            "bash",
            "git-bash",
            "gitbash",
            "cmd",
            "cmd.exe",
            "wsl"
        ]
    )]
    pub hook_shell: Option<String>,

    // --- git worktree Options ---
    /// Create a new branch <name> starting at <commitish>
    #[arg(
        short = 'b',
        value_name = "name",
        help_heading = "git worktree Options"
    )]
    pub new_branch: Option<String>,

    /// Create or reset branch <name> to <commitish>
    #[arg(
        short = 'B',
        value_name = "name",
        help_heading = "git worktree Options"
    )]
    pub new_branch_force: Option<String>,

    /// Force creation even if branch is checked out elsewhere
    #[arg(short, long, help_heading = "git worktree Options")]
    pub force: bool,

    /// Detach HEAD in the new worktree/workspace
    #[arg(short, long, help_heading = "git worktree Options")]
    pub detach: bool,

    /// Do not checkout after creation
    #[arg(long, help_heading = "git worktree Options")]
    pub no_checkout: bool,

    /// Lock the worktree after creation (git only)
    #[arg(long, help_heading = "git worktree Options")]
    pub lock: bool,

    /// Set up tracking for the branch
    #[arg(long, help_heading = "git worktree Options")]
    pub track: bool,

    /// Do not set up tracking
    #[arg(long, help_heading = "git worktree Options")]
    pub no_track: bool,

    /// Guess remote for tracking
    #[arg(long, help_heading = "git worktree Options")]
    pub guess_remote: bool,

    /// Do not guess remote
    #[arg(long, help_heading = "git worktree Options")]
    pub no_guess_remote: bool,

    // --- Shared Options ---
    /// Suppress output
    #[arg(short, long, help_heading = "Shared Options")]
    pub quiet: bool,

    /// When to use colored output (always, auto, never)
    #[arg(
        long,
        value_name = "WHEN",
        default_value = "auto",
        conflicts_with = "no_color",
        help_heading = "Shared Options"
    )]
    pub color: clap::ColorChoice,

    /// Disable colored output (equivalent to --color=never)
    #[arg(long, help_heading = "Shared Options")]
    pub no_color: bool,
}

/// Arguments for the `remove` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu remove ../old-worktree
    kabu remove --current
    kabu remove --interactive
    kabu remove --dry-run ../preview

WHAT THIS COMMAND DOES:
    - Removes git worktree / forgets jj workspace
    - Runs pre_remove and post_remove hooks if trusted
    - Supports interactive and current-target modes

VCS MAPPING:
    - git: git worktree remove
    - jj:  jj workspace forget
    - colocated (git+jj): jj workspace forget is used

TARGET SELECTION RULES:
    - [PATHS]... accepts multiple targets
    - --interactive selects targets from UI
    - --current targets the worktree/workspace containing cwd

SAFETY NOTES:
    - warns on dirty or unpushed state by default
    - --force bypasses checks and confirmation
    - main worktree/workspace cannot be removed

SEE ALSO:
    kabu list --help
    kabu trust --help")]
pub(crate) struct RemoveArgs {
    /// Worktree/workspace paths to remove (required unless --interactive or --current)
    pub paths: Vec<PathBuf>,

    // --- kabu Options ---
    /// Interactive mode: select worktrees/workspaces to remove
    #[arg(short, long, help_heading = "kabu Options")]
    pub interactive: bool,

    /// Remove the worktree/workspace containing the current directory
    #[arg(short = 'c', long, help_heading = "kabu Options")]
    pub current: bool,

    /// Preview actions without executing
    #[arg(long, help_heading = "kabu Options")]
    pub dry_run: bool,

    /// Windows-only: select hook shell (pwsh, powershell, bash, cmd, wsl)
    #[cfg(windows)]
    #[arg(
        long,
        value_name = "SHELL",
        help_heading = "kabu Options",
        value_parser = [
            "pwsh",
            "powershell",
            "bash",
            "git-bash",
            "gitbash",
            "cmd",
            "cmd.exe",
            "wsl"
        ]
    )]
    pub hook_shell: Option<String>,

    // --- git worktree Options ---
    /// Force removal even if worktree/workspace is dirty or locked
    #[arg(short, long, help_heading = "git worktree Options")]
    pub force: bool,

    // --- Shared Options ---
    /// Suppress output
    #[arg(short, long, help_heading = "Shared Options")]
    pub quiet: bool,

    /// When to use colored output (always, auto, never)
    #[arg(
        long,
        value_name = "WHEN",
        default_value = "auto",
        conflicts_with = "no_color",
        help_heading = "Shared Options"
    )]
    pub color: clap::ColorChoice,

    /// Disable colored output (equivalent to --color=never)
    #[arg(long, help_heading = "Shared Options")]
    pub no_color: bool,
}

/// Arguments for the `list` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu list
    kabu ls --header
    kabu list --path-only

WHAT THIS COMMAND DOES:
    - Lists worktrees/workspaces from git or jj
    - Shows branch/bookmark and commit/change information
    - Marks dirty entries with '*'

OUTPUT COLUMNS:
    PATH    worktree/workspace path
    BRANCH  git branch or jj bookmark/workspace label
    COMMIT  short commit hash (git) or change id (jj)
    STATUS  '*' when modified/deleted/untracked files exist

MODES:
    --path-only  print paths only (script-friendly)
    --header     include header row

SEE ALSO:
    kabu path --help
    kabu remove --help")]
pub(crate) struct ListArgs {
    /// Show only worktree/workspace paths
    #[arg(short, long)]
    pub path_only: bool,

    /// Show header row
    #[arg(long)]
    pub header: bool,

    /// When to use colored output (always, auto, never)
    #[arg(
        long,
        value_name = "WHEN",
        default_value = "auto",
        conflicts_with = "no_color",
        help_heading = "Shared Options"
    )]
    pub color: clap::ColorChoice,
    /// Disable colored output (equivalent to --color=never)
    #[arg(long, help_heading = "Shared Options")]
    pub no_color: bool,
}

/// Arguments for the `trust` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu trust
    kabu trust --show
    kabu trust --check
    kabu trust --yes

WHAT THIS COMMAND DOES:
    - Reviews and trusts hooks for a repository
    - Stores trust with a config snapshot hash
    - Requires re-trust when hooks/config change

HOOK EXECUTION MODEL:
    pre_add      before add (repo root)
    post_add     after add + setup (new worktree/workspace)
    pre_remove   before remove (target worktree/workspace)
    post_remove  after remove (repo root)

WINDOWS SHELL RESOLUTION:
    pwsh -> powershell -> git-bash -> cmd
    override: --hook-shell (add/remove) or KABUHOOK_SHELL

TRUST STORAGE:
    Default location: ~/.local/share/kabu/trusted/
    Repository identity + config snapshot are hashed for verification

SAFETY NOTES:
    - trusting allows all configured hooks to run
    - use --show to review commands before trusting
    - --check exits 0 when trusted, 1 when trust is required

SEE ALSO:
    kabu untrust --help
    kabu config --help")]
pub(crate) struct TrustArgs {
    /// Path to repository (defaults to current directory)
    pub path: Option<PathBuf>,

    /// Trust without confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Show trusted hooks without trusting
    #[arg(long)]
    pub show: bool,

    /// Check trust status (exit 0 if trusted, 1 if untrusted)
    #[arg(long, conflicts_with = "show")]
    pub check: bool,

    /// When to use colored output (always, auto, never)
    #[arg(
        long,
        value_name = "WHEN",
        default_value = "auto",
        conflicts_with = "no_color"
    )]
    pub color: clap::ColorChoice,

    /// Disable colored output (equivalent to --color=never)
    #[arg(long)]
    pub no_color: bool,
}

/// Arguments for the `init` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    eval \"$(kabu init bash)\"
    eval \"$(kabu init zsh)\"
    kabu init fish | source
    kabu init bash --print-full-init

WHAT THIS COMMAND DOES:
    - Generates shell integration script
    - Enables completion, kabu cd, and trust warnings

SUPPORTED SHELLS:
    bash, zsh, fish, powershell, elvish

INSTALL PATTERNS:
    bash:       eval \"$(kabu init bash)\"
    zsh:        eval \"$(kabu init zsh)\"
    fish:       kabu init fish | source
    powershell: Invoke-Expression (& kabu init powershell | Out-String)
    elvish:     eval (kabu init elvish | slurp)

TROUBLESHOOTING:
    1) reload shell session
    2) confirm binary is in PATH
    3) confirm init snippet is in shell rc/profile

SAFETY NOTES:
    - kabu cd requires shell integration
    - without integration, use: cd \"$(kabu path)\"

SEE ALSO:
    kabu path --help
    kabu completions --help")]
pub(crate) struct InitArgs {
    /// Shell to generate init script for
    pub shell: Shell,

    /// Print the full init script instead of the stub
    #[arg(long)]
    pub print_full_init: bool,
}

/// Arguments for the `path` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu path
    kabu path --main
    cd \"$(kabu path)\"

WHAT THIS COMMAND DOES:
    - Prints selected worktree/workspace path
    - --main returns the main worktree/workspace path

SELECTION BEHAVIOR:
    default   interactive selection UI
    --main    no interaction, prints main path directly

USAGE NOTES:
    - Intended for command substitution in shells
    - Works without shell integration unlike kabu cd

SEE ALSO:
    kabu cd
    kabu list --help")]
pub(crate) struct PathArgs {
    /// Print the main worktree/workspace path instead of interactive selection
    #[arg(long)]
    pub main: bool,
}

/// Arguments for the `untrust` subcommand.
#[derive(Parser, Debug)]
#[command(after_help = "\
QUICK EXAMPLES:
    kabu untrust
    kabu untrust --list
    kabu untrust /path/to/repo

WHAT THIS COMMAND DOES:
    - Revokes trust for one repository or lists trusted repositories

MODES:
    default      revoke trust for current (or given) repository
    --list       print all trusted repository entries

SEE ALSO:
    kabu trust --help")]
pub(crate) struct UntrustArgs {
    /// Path to repository (defaults to current directory)
    pub path: Option<PathBuf>,

    /// List all trusted repositories
    #[arg(long)]
    pub list: bool,
}

/// Conflict resolution mode from CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnConflictArg {
    Abort,
    Skip,
    Overwrite,
    Backup,
}

impl std::str::FromStr for OnConflictArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "abort" => Ok(Self::Abort),
            "skip" => Ok(Self::Skip),
            "overwrite" => Ok(Self::Overwrite),
            "backup" => Ok(Self::Backup),
            _ => Err(format!(
                "Invalid on-conflict mode: {s}. Valid values: abort, skip, overwrite, backup"
            )),
        }
    }
}

/// Parse CLI arguments.
pub(crate) fn parse() -> Cli {
    Cli::parse()
}

/// Build CLI for completion/man generation.
pub(crate) fn build() -> clap::Command {
    Cli::command()
}
