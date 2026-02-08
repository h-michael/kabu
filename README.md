# kabu

[![Crates.io](https://img.shields.io/crates/v/kabu.svg)](https://crates.io/crates/kabu)
[![CI](https://github.com/h-michael/kabu/actions/workflows/ci.yml/badge.svg)](https://github.com/h-michael/kabu/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/kabu.svg)](LICENSE-MIT)

CLI tool that enhances git worktree and jj workspace with automated setup and utilities.

**Supported VCS:**
- **Git** - `git worktree` operations
- **jj (Jujutsu)** - `jj workspace` operations (including colocated repositories)

> **Note:** This tool is under active development. Commands and configuration format may change in future versions.

## Problem

Every time you create a git worktree or jj workspace, you end up doing the same manual setup:

- Creating symlinks to config files like `.env.local`
- Copying `.env.example` to `.env`
- Creating cache directories

kabu reads `.kabu/config.yaml` (or `.kabu/config.toml`) from your repository and runs these tasks automatically when creating a worktree or workspace. It automatically detects whether you're in a git repository or jj repository and uses the appropriate commands.

## Installation

```bash
cargo install kabu
```

See [INSTALL.md](INSTALL.md) for other installation methods (mise, Nix, GitHub Releases).

## Quick Start

1.  **Create `.kabu/config.yaml` (or `.kabu/config.toml`)**: In your repository root, create a configuration file to define your worktree setup. Both YAML and TOML formats are supported (YAML takes priority if both exist).
    ```yaml
    # .kabu/config.yaml example
    mkdir:
      - path: build
        description: Build output directory
    link:
      - source: .env.local
        description: Local environment variables
    ```
2.  **Add a worktree**: Use `kabu add` to create a new worktree with the defined setup.
    ```bash
    kabu add ../my-feature-worktree
    ```
    This will create a new worktree at `../my-feature-worktree` and apply the `mkdir` and `link` operations defined in `.kabu/config.yaml`.

For comprehensive details on all commands, options, and configuration, please refer to the CLI's built-in help (`kabu --help` and `kabu <subcommand> --help`) and `man` pages (`kabu man`).

## VCS Support

kabu automatically detects the version control system in use:

| VCS | Detection | Commands Used |
|-----|-----------|---------------|
| **Git** | `.git` directory | `git worktree add/remove/list` |
| **jj (Jujutsu)** | `.jj` directory | `jj workspace add/forget/list` |
| **jj colocated** | Both `.git` and `.jj` | `jj workspace` commands |

All kabu commands work the same regardless of the underlying VCS. The tool transparently translates operations to the appropriate VCS commands.

**jj-specific notes:**
- jj "workspaces" serve a similar purpose to git "worktrees" (multiple working directories for the same repository)
- The "default" workspace in jj is treated as the main workspace
- Colocated repositories (jj on top of git) are fully supported

## Usage

### Creating new worktrees/workspaces

```bash
# Create a worktree/workspace with setup
kabu add ../feature-branch

# Create a new branch and worktree (git)
# or new workspace with bookmark (jj)
kabu add -b new-feature ../new-feature

# Interactive mode - select branch and path
kabu add --interactive

# Preview without executing
kabu add --dry-run ../test
```

### Listing worktrees/workspaces

```bash
# List all worktrees/workspaces with detailed information
kabu list
kabu ls  # Short alias

# Show header row with column names
kabu list --header

# List only paths (useful for scripting)
kabu list --path-only
kabu ls -p
```

**Status Symbols:**
- `*` = Uncommitted changes (modified, deleted, or untracked files)

**jj-specific columns:**
- Shows workspace name and change ID instead of branch name when applicable
- Displays bookmark name if associated with the workspace

### Removing worktrees/workspaces

```bash
# Remove a worktree/workspace with safety checks
kabu remove ../feature-branch

# Shorthand alias
kabu rm ../feature-branch

# Remove current worktree/workspace (the one you're in)
kabu remove --current

# Interactive mode - select worktrees/workspaces to remove
# Shows [current] marker for the one you're in
kabu remove --interactive

# Preview what would be removed
kabu remove --dry-run ../feature-branch

# Force removal (skip safety checks and confirmation)
kabu remove --force ../feature-branch
```

**Safety Checks:**

By default, `kabu remove` warns about:
- Modified files
- Deleted files
- Untracked files
- Unpushed commits (git) / commits not on remote bookmarks (jj)

Use `--force` to bypass all checks and confirmation prompts.

### Changing to selected worktree

```bash
# Interactively select and cd to a worktree
kabu cd

# Or get worktree path for scripting (works without shell integration)
cd "$(kabu path)"
```

**`kabu cd`**: **Requires shell integration** (`kabu init` - see [Shell Integration](#shell-integration) section). Displays an interactive fuzzy finder (on Unix) or selection menu (on Windows) and automatically changes to the selected worktree. If shell integration is not enabled, the command will display a helpful error message with setup instructions.

**`kabu path`**: Prints the selected worktree path to stdout. Works without shell integration. Useful for scripting or as a fallback.

### Configuration commands

```bash
# Show config format help
kabu config

# Create a new repo config
kabu config new

# Create a new global config
kabu config new --global

# Validate configuration
kabu config validate
```

### Hooks (trust required)

Hooks allow you to run custom commands before/after worktree operations:

```bash
# Review and trust hooks in .kabu/config.yaml
kabu trust

# Show hooks without trusting
kabu trust --show

# Check trust status (exit 0 if trusted, 1 if untrusted)
kabu trust --check

# Revoke trust
kabu untrust

# List all trusted repositories
kabu untrust --list
```

**Security:** For security, hooks require explicit trust via `kabu trust` before execution. See [Hooks Configuration](#hooks) below.

## Shell Integration

**Optional feature.** Shell integration is not required for basic kabu functionality (`add`, `remove`, `list`, `path`, etc.). It only adds quality-of-life improvements.

To enable shell completions and automatic hook trust warnings, add the following to your shell configuration:

**Bash** (`~/.bashrc`):
```bash
eval "$(kabu init bash)"
```

**Zsh** (`~/.zshrc`):
```zsh
eval "$(kabu init zsh)"
```

**Fish** (`~/.config/fish/config.fish`):
```fish
kabu init fish | source
```

**PowerShell** (profile):
```powershell
Invoke-Expression (& kabu init powershell | Out-String)
```

**Elvish** (`~/.config/elvish/rc.elv`):
```elvish
eval (kabu init elvish | slurp)
```

Shell integration provides:
- **Shell completions** for commands and options
- **`kabu cd` command** to interactively change directory to selected worktree (only available with shell integration)
- **Auto cd after add** - Automatically `cd` to newly created worktree (configurable via `auto_cd.after_add`)
- **Auto cd after remove** - Automatically `cd` when current worktree is removed (configurable via `auto_cd.after_remove`)
- **Automatic trust warnings** when entering directories with untrusted hooks

## Configuration

Create `.kabu/config.yaml` or `.kabu/config.toml` in your repository root. Both YAML and TOML formats are supported. If both files exist, YAML takes priority. See [examples/](examples/) for various use cases.

**JSON Schema:** The configuration format is validated against a JSON Schema located at `schema/kabu.schema.json`. This schema can be used with editors that support YAML schema validation for autocomplete and validation.

**Editor Integration:** To enable schema validation in VS Code or other editors using [yaml-language-server](https://github.com/redhat-developer/yaml-language-server#using-inlined-schema), add this comment at the top of your `.kabu/config.yaml`:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/h-michael/kabu/main/schema/kabu.schema.json

on_conflict: backup
```

### Basic configuration

```yaml
on_conflict: backup  # abort, skip, overwrite, backup

mkdir:
  - path: build
    description: Build output directory

link:
  - source: .env.local
    description: Local environment

copy:
  - source: .env.example
    target: .env
    description: Environment template
```

**Operations:**
- `mkdir` - Create directories
- `link` - Create symbolic links
- `copy` - Copy files or directories

**Examples:** [examples/basic.yaml](examples/basic.yaml)

### Worktree path configuration

Configure default worktree path with template variables:

```yaml
worktree:
  path_template: ../worktrees/{branch}
```

**Template variables:**
- `{{branch}}` or `{{ branch }}` - Branch name (e.g., `feature/foo`)
- `{{repository}}` or `{{ repository }}` - Repository name (e.g., `myrepo`)

**Examples:** [examples/worktree-path.yaml](examples/worktree-path.yaml)

### Glob patterns

Use glob patterns in `link` operations to match multiple files:

```yaml
link:
  - source: fixtures/*
    ignore_tracked: true
    description: Link untracked test fixtures
```

**Supported patterns:**
- `*` - matches any characters
- `?` - matches a single character
- `[...]` - matches character ranges
- `**` - matches directories recursively

**Options:**
- `ignore_tracked: true` - Skip git-tracked files (useful for linking only untracked files like local configs or test data)

**Examples:** [examples/glob-patterns.yaml](examples/glob-patterns.yaml)

### Hooks

Execute custom commands before/after worktree operations. **Requires explicit trust via `kabu trust`.**

```yaml
hooks:
  post_add:
    - command: npm install
      description: Install dependencies
    - command: mise install
      description: Install mise tools
```

**Hook types:**
- `pre_add` - Before worktree creation
- `post_add` - After worktree setup
- `pre_remove` - Before worktree removal
- `post_remove` - After worktree removal

**Template variables:**
- `{{worktree_path}}` - Full path to worktree
- `{{worktree_name}}` - Worktree directory name
- `{{branch}}` - Branch name
- `{{repo_root}}` - Repository root

**Environment variables:**

Hooks also receive the following environment variables, which contain raw (unescaped) values:
- `KABU_WORKTREE_PATH` - Full path to worktree
- `KABU_WORKTREE_NAME` - Worktree directory name
- `KABU_BRANCH` - Branch name (empty if detached)
- `KABU_REPO_ROOT` - Repository root
- `KABU_VCS_TYPE` - VCS type ("git" or "jj")
- `KABU_CHANGE_ID` - jj change ID (empty for git)
- `KABU_COMMIT_ID` - jj commit ID (empty for git)

Use environment variables when you need raw values without shell escaping (e.g., for tmux session management):
```yaml
hooks:
  post_add:
    - command: tmux new-session -d -s "kabu-$KABU_WORKTREE_NAME" -c "$KABU_WORKTREE_PATH"
      description: Create tmux session
  post_remove:
    - command: tmux kill-session -t "kabu-$KABU_WORKTREE_NAME" 2>/dev/null || true
      description: Remove tmux session
```

**Security:**
- Template variables are shell-escaped automatically (POSIX sh on Unix, PowerShell on Windows)
- Environment variables contain raw values and follow standard shell expansion rules
- Always quote environment variable expansions in hook commands (`"$KABU_*"`, `"$env:KABU_*"`, `"%KABU_*%"`) to avoid shell injection from untrusted values
- Must trust hooks via `kabu trust` before execution
- Changes require re-trusting

**Windows shells:**
- Default shell auto-detect order: `pwsh` → `powershell` → `bash` (Git Bash) → `cmd`
- Override with `--hook-shell` or `KABUHOOK_SHELL` (`pwsh`, `powershell`, `bash`, `cmd`, `wsl`)
- `--hook-shell` takes precedence over `KABUHOOK_SHELL`
- `wsl` is only used when explicitly set, because Windows paths may not map to WSL
- Environment variable syntax differs by shell: `$env:KABU_WORKTREE_NAME` (PowerShell), `%KABU_WORKTREE_NAME%` (cmd), `$KABU_WORKTREE_NAME` (bash/wsl)

**Examples:** [examples/hooks-basic.yaml](examples/hooks-basic.yaml), [examples/nodejs-project.yaml](examples/nodejs-project.yaml)

## Features

### Operations

| Operation | Description |
|-----------|-------------|
| `mkdir` | Create directories |
| `link` | Create symbolic links |
| `copy` | Copy files or directories |
| `hooks.*` | Run custom commands (requires trust) |

### Conflict handling

When a target file already exists, kabu can:

- `abort` - Stop immediately (default in non-interactive mode)
- `skip` - Skip the file and continue
- `overwrite` - Replace the existing file
- `backup` - Rename existing file to `.bak` and proceed

Set globally with `on_conflict`, per-operation, or via `--on-conflict` flag.

### Auto cd (shell integration)

Automatically change directory after worktree operations. **Requires shell integration.**

```yaml
auto_cd:
  after_add: true     # cd to new worktree after creation (default: true)
  after_remove: main  # cd after removing current worktree (default: main)
```

**`after_remove` options:**
- `main` - cd to main worktree
- `select` - Show interactive selection

### Other options

| Option | Description |
|--------|-------------|
| `--interactive`, `-i` | Select branch and path interactively |
| `--dry-run` | Preview actions without executing |
| `--quiet`, `-q` | Suppress output |
| `--no-setup` | Skip setup (run git worktree add only) |
| `--hook-shell <SHELL>` | Windows only: choose hook shell (`pwsh`, `powershell`, `bash`, `cmd`, `wsl`) |

## Command Options

### kabu add

Passes through git worktree options:

```
kabu add [OPTIONS] [PATH] [COMMITISH]

kabu Options:
  -i, --interactive         Interactive mode
      --on-conflict <MODE>  abort, skip, overwrite, backup
      --dry-run             Preview without executing
      --no-setup            Skip .kabu/config.yaml setup
      --hook-shell <SHELL>  Windows only: hook shell

git worktree Options:
  -b <name>                 Create new branch
  -B <name>                 Create or reset branch
  -f, --force               Force creation
  -d, --detach              Detach HEAD
      --no-checkout         Do not checkout after creation
      --lock                Lock worktree after creation
      --track / --no-track  Branch tracking
      --guess-remote        Guess remote for tracking
      --no-guess-remote     Do not guess remote

Shared:
  -q, --quiet               Suppress output
```

### kabu remove

Remove worktrees with safety checks (alias: `kabu rm`):

```
kabu remove [OPTIONS] [PATHS]...
kabu rm [OPTIONS] [PATHS]...

kabu Options:
  -i, --interactive         Select worktrees interactively
  -c, --current             Remove current worktree
      --dry-run             Preview without executing
      --hook-shell <SHELL>  Windows only: hook shell

git worktree Options:
  -f, --force               Force removal (skip all checks and prompts)

Shared:
  -q, --quiet               Suppress output
```

## Shell Completion

Generate completion scripts for your shell:

```bash
# Bash
kabu completions bash > ~/.local/share/bash-completion/completions/kabu

# Zsh (add ~/.zfunc to your fpath)
kabu completions zsh > ~/.zfunc/_kabu

# Fish
kabu completions fish > ~/.config/fish/completions/kabu.fish

# PowerShell (add to your profile)
kabu completions powershell >> $PROFILE

# PowerShell (or save to a file and dot-source it)
kabu completions powershell > _kabu.ps1
# Then add to $PROFILE: . /path/to/_kabu.ps1
```

Supported shells: bash, elvish, fish, powershell, zsh

### PowerShell note

To find your PowerShell profile path, run `echo $PROFILE`. If the profile file doesn't exist, create it with `New-Item -Path $PROFILE -ItemType File -Force`.

## Man Page

Generate and install the man page:

```bash
# Install to system
sudo kabu man > /usr/local/share/man/man1/kabu.1

# View without installing
kabu man | man -l -
```

## Platform Support

### Hooks

Hooks are supported on Unix-like systems (Linux, macOS) and Windows.

On Windows, hooks run via an auto-detected shell (`pwsh`, `powershell`, Git Bash,
or `cmd`). Use `KABUHOOK_SHELL` to force a specific shell if needed.

### Windows

On Windows, creating symbolic links requires one of the following:
- **Windows 11**: No special permissions required
- **Windows 10 Creators Update (1703) or later**: Enable [Developer Mode](https://blogs.windows.com/windowsdeveloper/2016/12/02/symlinks-windows-10/) in Settings > Update & Security > For Developers
- **Older Windows versions**: Run as administrator

Without these permissions, `[[link]]` operations will fail with a permission error.

For more information, see:
- [Symbolic Links - Win32 apps](https://learn.microsoft.com/en-us/windows/win32/fileio/symbolic-links)
- [CreateSymbolicLink API](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsymboliclinka)

## License

MIT OR Apache-2.0
