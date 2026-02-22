# kabu

[![Crates.io](https://img.shields.io/crates/v/kabu.svg)](https://crates.io/crates/kabu)
[![CI](https://github.com/h-michael/kabu/actions/workflows/ci.yml/badge.svg)](https://github.com/h-michael/kabu/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/kabu.svg)](LICENSE-MIT)

## What kabu solves

kabu automates repetitive setup when creating a git worktree / jj workspace.

- Create directories (`mkdir`)
- Create symlinks (`link`)
- Copy files (`copy`)
- Run hooks before/after add/remove (requires trust)

Supported VCS: **Git** (`git worktree`), **jj** (`jj workspace`), and colocated repositories.

## Quick Start

Install (see [INSTALL.md](INSTALL.md) for other methods):

```bash
cargo install kabu
```

Create `.kabu/config.yaml`:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/h-michael/kabu/main/schema/kabu.schema.json

on_conflict: backup

mkdir:
  - path: .cache

link:
  - source: .env.local

copy:
  - source: .env.example
    target: .env
```

Create a worktree/workspace:

```bash
kabu add ../feature-x
```

Validate config if needed:

```bash
kabu config validate
```

## Shell Integration

Shell integration is optional, but required for `kabu cd`.

Bash:

```bash
eval "$(kabu init bash)"
```

Zsh:

```zsh
eval "$(kabu init zsh)"
```

Fish:

```fish
kabu init fish | source
```

PowerShell:

```powershell
Invoke-Expression (& kabu init powershell | Out-String)
```

Elvish:

```elvish
eval (kabu init elvish | slurp)
```

## Common Tasks

### Add

```bash
# Create with setup
kabu add ../feature-x

# Create new branch/bookmark
kabu add -b feature-x ../feature-x

# Preview only
kabu add --dry-run ../feature-x

# Skip setup and run only VCS add
kabu add --no-setup ../feature-x
```

### List

```bash
kabu list
kabu ls
kabu list --path-only
```

### Remove

```bash
# Safe remove (warns on dirty or unpushed state)
kabu remove ../feature-x

# Remove current worktree/workspace
kabu remove --current

# Preview only
kabu remove --dry-run ../feature-x

# Skip checks and confirmation
kabu remove --force ../feature-x
```

### Jump to another worktree/workspace

```bash
# Requires shell integration
kabu cd

# Works without shell integration
cd "$(kabu path)"
```

## Configuration

Config files:

- Repo local: `.kabu/config.yaml` or `.kabu/config.toml`
- YAML is used when both exist
- JSON Schema: `schema/kabu.schema.json`

Use `{{...}}` placeholders in templates (for example `{{branch}}`, `{{repository}}`).

For full config keys and behavior, use CLI help:

```bash
kabu config --help
kabu config <subcommand> --help
```

## Safety Model

Hooks are opt-in by trust:

```bash
kabu trust
kabu trust --check
kabu untrust
```

`kabu remove` warns about:

- Uncommitted changes
- Unpushed commits (git)
- Commits not on remote bookmarks (jj)

Use `--force` only when you intentionally want to bypass checks.

## VCS Behavior (git / jj)

kabu auto-detects VCS and maps commands:

- git: `git worktree add/remove/list`
- jj: `jj workspace add/forget/list`
- colocated repo: jj commands are preferred

CLI terminology keeps `worktree/workspace` together to avoid ambiguity.

## Reference Map

- Command overview: `kabu --help`
- Command details: `kabu <subcommand> --help`
- Config reference: `kabu config --help`
- Man page: `kabu man`
- Task-oriented examples: [`examples/README.md`](examples/README.md)

View generated man page directly:

```bash
kabu man | man -l -
```

## License

This project is licensed under the MIT License. See [LICENSE-MIT](LICENSE-MIT).
