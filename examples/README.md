# kabu Configuration Examples

Example configuration files for various use cases. Browse through these examples to find patterns that fit your workflow.

## Available Examples

### Basic Examples

- **[basic.yaml](basic.yaml)**
  Demonstrates the three main operation types: `mkdir`, `link`, and `copy`. Shows conflict handling options and descriptions.

- **[worktree-path.yaml](worktree-path.yaml)**
  Examples of worktree path configuration with template variables (`{branch}`, `{repository}`). Useful for customizing worktree location and naming.

- **[glob-patterns.yaml](glob-patterns.yaml)**
  Shows how to use glob patterns in `link` operations. Includes `ignore_tracked` option for linking only untracked files.

- **[hooks-basic.yaml](hooks-basic.yaml)**
  Basic hook examples with template variables. Shows all hook types (`pre_add`, `post_add`, `pre_remove`, `post_remove`) and their execution order.

### Project-Specific Examples

- **[nodejs-project.yaml](nodejs-project.yaml)**
  Node.js project setup with dependency installation (npm/pnpm/yarn). Links environment files and creates build directories.

- **[rust-project.yaml](rust-project.yaml)**
  Rust project setup with shared build cache. Shows how to link `target` directory and run `cargo check`.

- **[mise-direnv.yaml](mise-direnv.yaml)**
  Integration with mise (formerly rtx) and direnv. Automatically installs tools and trusts direnv configuration.

- **[coding-agent.yaml](coding-agent.yaml)**
  Coding agent integration (Claude Code, OpenAI Codex CLI, Cursor, Windsurf, Aider). Shares project-specific coding agent configurations across worktrees.

## Configuration File Format

All examples use YAML format with JSON Schema validation support. The basic structure is:

```yaml
# Global options (optional)
on_conflict: backup  # abort, skip, overwrite, backup

# Auto cd after operations (optional, requires shell integration)
auto_cd:
  after_add: true     # cd to new worktree (default: true)
  after_remove: main  # cd after removing current worktree (default: main)

# Worktree path template (optional)
worktree:
  path_template: ../worktrees/{branch}

# Operations (all optional)
mkdir:
  - path: build

link:
  - source: .env.local

copy:
  - source: .env.example
    target: .env

# Hooks (optional, require trust)
hooks:
  post_add:
    - command: npm install
      description: Install dependencies
```

### JSON Schema

The configuration format is validated against `schema/kabu.schema.json`. You can use this with editors that support YAML schema validation (like VS Code with the YAML extension) for autocomplete and validation.

## Hook Security

Hooks require explicit trust before execution:

```bash
# Review and trust hooks
kabu trust

# Revoke trust
kabu untrust

# List all trusted repositories
kabu untrust --list
```

Hooks are supported on Unix-like systems (Linux, macOS) and Windows. On Windows, hooks run via an auto-detected shell (pwsh, powershell, Git Bash, or cmd). Override with `--hook-shell` or the `KABUHOOK_SHELL` environment variable.

## See Also

- [Main README](../README.md) - Full documentation
- [Installation Guide](../INSTALL.md) - How to install
