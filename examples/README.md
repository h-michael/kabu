# kabu Examples (Task-Oriented)

This directory is organized by task so you can pick one file and start quickly.

## Index

| File | Task | Best for | OS |
| --- | --- | --- | --- |
| [`minimal.yaml`](minimal.yaml) | Smallest valid config | First run / smoke test | All |
| [`basic-setup.yaml`](basic-setup.yaml) | mkdir + link + copy | Typical daily usage | All |
| [`path-template.yaml`](path-template.yaml) | Worktree/workspace location templates | Standardized path layout | All |
| [`branch-template.yaml`](branch-template.yaml) | Interactive branch naming templates | Teams with branch naming conventions | All |
| [`glob-untracked.yaml`](glob-untracked.yaml) | Glob links + `skip_tracked` | Local fixtures and local-only files | All |
| [`hooks-safe.yaml`](hooks-safe.yaml) | Hook lifecycle + trust model | Setup/cleanup automation | Unix + Windows |
| [`nodejs-bootstrap.yaml`](nodejs-bootstrap.yaml) | Node.js bootstrap hooks | npm-based projects | Unix + Windows |
| [`rust-bootstrap.yaml`](rust-bootstrap.yaml) | Rust bootstrap hooks | cargo-based projects | Unix + Windows |
| [`mise-direnv.yaml`](mise-direnv.yaml) | mise + direnv automation | Toolchain + env bootstrap | Unix + Windows |
| [`coding-agent.yaml`](coding-agent.yaml) | Coding-agent dotfile sharing | Personal local AI tooling | All |

## Suggested Flow

1. Copy `minimal.yaml` to `.kabu/config.yaml`.
2. Move to `basic-setup.yaml`.
3. Add one specialized example based on your workflow.
4. Validate with `kabu config validate`.

## Notes

- Config keys are strict (`additionalProperties: false`).
- Use `{{...}}` placeholders for templates.
- Hooks require explicit trust: `kabu trust`.

See also: [`../README.md`](../README.md)
