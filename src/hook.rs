use crate::color::ColorScheme;
use crate::config::{HookEntry, Hooks};
use crate::error::{Error, Result};
use crate::output::Output;

use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, Stdio};

/// Template variables for hooks.
///
/// Provides context information that can be used in hook commands.
/// All variables are automatically shell-escaped when expanded.
pub(crate) struct HookEnv {
    /// Worktree/workspace path (unified name for both VCS)
    pub worktree_path: String,
    /// Worktree/workspace directory name (unified name for both VCS)
    pub worktree_name: String,
    /// Branch name (git) or bookmark name (jj), if applicable
    pub branch: Option<String>,
    /// Repository root path
    pub repo_root: String,
    /// VCS type: "git" or "jj"
    pub vcs_type: String,
    /// jj-specific: change ID (short form)
    pub change_id: Option<String>,
    /// jj-specific: commit ID (short form)
    pub commit_id: Option<String>,
    /// Windows-only: override hook shell selection
    #[cfg_attr(not(windows), allow(dead_code))]
    pub hook_shell: Option<String>,
}

impl HookEnv {
    /// Expand template variables in a command string.
    ///
    /// # Supported Variables (VCS-agnostic)
    /// - `{{worktree_path}}` / `{{workspace_path}}`: Full path to the worktree/workspace
    /// - `{{worktree_name}}` / `{{workspace_name}}`: Directory name
    /// - `{{branch}}` / `{{bookmark}}`: Branch name (git) or bookmark name (jj)
    /// - `{{repo_root}}`: Repository root path
    /// - `{{vcs_type}}`: VCS type ("git" or "jj")
    ///
    /// # jj-specific Variables
    /// - `{{change_id}}`: jj change ID (short form, empty for git)
    /// - `{{commit_id}}`: Commit/revision ID (short form)
    ///
    /// All values are automatically shell-escaped to prevent command injection.
    fn expand_template_with<E>(&self, cmd: &str, escape: E) -> String
    where
        E: Fn(&str) -> String,
    {
        let mut result = cmd.to_string();

        // VCS-agnostic variables (both names work)
        result = result.replace("{{worktree_path}}", &escape(&self.worktree_path));
        result = result.replace("{{workspace_path}}", &escape(&self.worktree_path));
        result = result.replace("{{worktree_name}}", &escape(&self.worktree_name));
        result = result.replace("{{workspace_name}}", &escape(&self.worktree_name));
        result = result.replace("{{repo_root}}", &escape(&self.repo_root));
        result = result.replace("{{vcs_type}}", &escape(&self.vcs_type));

        // Branch/bookmark (both names work)
        if let Some(branch) = &self.branch {
            result = result.replace("{{branch}}", &escape(branch));
            result = result.replace("{{bookmark}}", &escape(branch));
        } else {
            result = result.replace("{{branch}}", "");
            result = result.replace("{{bookmark}}", "");
        }

        // jj-specific variables
        if let Some(change_id) = &self.change_id {
            result = result.replace("{{change_id}}", &escape(change_id));
        } else {
            result = result.replace("{{change_id}}", "");
        }

        if let Some(commit_id) = &self.commit_id {
            result = result.replace("{{commit_id}}", &escape(commit_id));
        } else {
            result = result.replace("{{commit_id}}", "");
        }

        result
    }

    #[cfg(unix)]
    fn expand_template(&self, cmd: &str) -> String {
        self.expand_template_with(cmd, shell_escape)
    }

    /// Set environment variables on a Command for use in hook scripts.
    ///
    /// Unlike template variables, these are not shell-escaped, allowing hooks
    /// to use raw values via standard shell variable expansion (e.g., `$KABU_WORKTREE_NAME`).
    fn set_env(&self, cmd: &mut Command) {
        cmd.env("KABU_WORKTREE_PATH", &self.worktree_path);
        cmd.env("KABU_WORKTREE_NAME", &self.worktree_name);
        cmd.env("KABU_BRANCH", self.branch.as_deref().unwrap_or(""));
        cmd.env("KABU_REPO_ROOT", &self.repo_root);
        cmd.env("KABU_VCS_TYPE", &self.vcs_type);
        cmd.env("KABU_CHANGE_ID", self.change_id.as_deref().unwrap_or(""));
        cmd.env("KABU_COMMIT_ID", self.commit_id.as_deref().unwrap_or(""));
    }
}

/// Escape a string for safe use in shell commands.
///
/// # Safety
/// This prevents command injection by ensuring special shell characters
/// are treated as literal text.
#[cfg(unix)]
fn shell_escape(s: &str) -> String {
    // POSIX sh single-quote escaping: close quote, escape, reopen.
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn posix_shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn powershell_escape(s: &str) -> String {
    // PowerShell single-quote escaping: double single quotes inside a single-quoted string.
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
fn cmd_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for ch in s.chars() {
        match ch {
            '^' => escaped.push_str("^^"),
            '"' => escaped.push_str("^\""),
            '&' => escaped.push_str("^&"),
            '|' => escaped.push_str("^|"),
            '<' => escaped.push_str("^<"),
            '>' => escaped.push_str("^>"),
            '(' => escaped.push_str("^("),
            ')' => escaped.push_str("^)"),
            '%' => escaped.push_str("^%"),
            '!' => escaped.push_str("^!"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

/// Execute a single hook command
#[cfg(unix)]
fn execute_hook(command: &str, env: &HookEnv, working_dir: &Path) -> Result<()> {
    let shell = "sh";
    let shell_arg = "-c";

    // Expand template variables
    let expanded_command = env.expand_template(command);

    let mut cmd = Command::new(shell);
    cmd.arg(shell_arg)
        .arg(&expanded_command)
        .current_dir(working_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    env.set_env(&mut cmd);

    let status = cmd.status().map_err(|e| Error::HookExecutionFailed {
        command: command.to_string(),
        cause: e.to_string(),
    })?;

    if !status.success() {
        return Err(Error::HookFailed {
            command: command.to_string(),
            exit_code: status.code(),
            stderr: String::new(), // stderr is already displayed
        });
    }

    Ok(())
}

#[cfg(windows)]
fn execute_hook(command: &str, env: &HookEnv, working_dir: &Path) -> Result<()> {
    let shell = select_windows_shell_with_override(env.hook_shell.as_deref())?;

    let expanded_command = env.expand_template_with(command, |value| match shell.kind {
        WindowsShellKind::Pwsh | WindowsShellKind::PowerShell => powershell_escape(value),
        WindowsShellKind::Cmd => cmd_escape(value),
        WindowsShellKind::Bash | WindowsShellKind::Wsl => posix_shell_escape(value),
    });

    let mut cmd = Command::new(&shell.program);
    cmd.args(&shell.args_for_command(&expanded_command))
        .current_dir(working_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    env.set_env(&mut cmd);

    let status = cmd.status().map_err(|e| Error::HookExecutionFailed {
        command: command.to_string(),
        cause: e.to_string(),
    })?;

    if !status.success() {
        return Err(Error::HookFailed {
            command: command.to_string(),
            exit_code: status.code(),
            stderr: String::new(), // stderr is already displayed
        });
    }

    Ok(())
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowsShellKind {
    Pwsh,
    PowerShell,
    Cmd,
    Bash,
    Wsl,
}

#[cfg(windows)]
struct WindowsShell {
    kind: WindowsShellKind,
    program: String,
}

#[cfg(windows)]
impl WindowsShell {
    fn args_for_command(&self, command: &str) -> Vec<String> {
        match self.kind {
            WindowsShellKind::Pwsh | WindowsShellKind::PowerShell => vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                command.to_string(),
            ],
            WindowsShellKind::Cmd => {
                vec!["/V:OFF".to_string(), "/C".to_string(), command.to_string()]
            }
            WindowsShellKind::Bash => vec!["-lc".to_string(), command.to_string()],
            WindowsShellKind::Wsl => vec![
                "--".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                command.to_string(),
            ],
        }
    }
}

#[cfg(windows)]
fn select_windows_shell_with_override(override_value: Option<&str>) -> Result<WindowsShell> {
    let env_override = std::env::var("KABUHOOK_SHELL").ok();
    let override_value = override_value.or(env_override.as_deref());
    let path_var = std::env::var_os("PATH");
    let path_ext = std::env::var_os("PATHEXT");

    select_windows_shell_with(
        override_value,
        path_var.as_deref(),
        path_ext.as_deref(),
        &|path| path.is_file(),
    )
}

#[cfg(windows)]
fn select_windows_shell_with(
    override_value: Option<&str>,
    path_var: Option<&std::ffi::OsStr>,
    path_ext: Option<&std::ffi::OsStr>,
    is_file: &dyn Fn(&std::path::Path) -> bool,
) -> Result<WindowsShell> {
    if let Some(value) = override_value {
        return parse_windows_shell_override_with(value, path_var, path_ext, is_file);
    }

    let candidates = [
        (WindowsShellKind::Pwsh, &["pwsh"] as &[&str]),
        (WindowsShellKind::PowerShell, &["powershell"] as &[&str]),
        (WindowsShellKind::Bash, &["bash", "bash.exe"] as &[&str]),
        (WindowsShellKind::Cmd, &["cmd", "cmd.exe"] as &[&str]),
    ];

    for (kind, names) in candidates {
        if let Some(program) = find_command_with(names, path_var, path_ext, is_file) {
            return Ok(WindowsShell { kind, program });
        }
    }

    Err(Error::HookExecutionFailed {
        command: String::from(""),
        cause: String::from("No supported shell found for hooks on Windows"),
    })
}

#[cfg(windows)]
fn parse_windows_shell_override_with(
    value: &str,
    path_var: Option<&std::ffi::OsStr>,
    path_ext: Option<&std::ffi::OsStr>,
    is_file: &dyn Fn(&std::path::Path) -> bool,
) -> Result<WindowsShell> {
    let normalized = value.trim().to_ascii_lowercase();
    let (kind, names): (WindowsShellKind, &[&str]) = match normalized.as_str() {
        "pwsh" => (WindowsShellKind::Pwsh, &["pwsh"]),
        "powershell" => (WindowsShellKind::PowerShell, &["powershell"]),
        "bash" | "git-bash" | "gitbash" => (WindowsShellKind::Bash, &["bash", "bash.exe"]),
        "wsl" => (WindowsShellKind::Wsl, &["wsl", "wsl.exe"]),
        "cmd" | "cmd.exe" => (WindowsShellKind::Cmd, &["cmd", "cmd.exe"]),
        _ => {
            return Err(Error::HookExecutionFailed {
                command: String::from(""),
                cause: format!("Unsupported KABUHOOK_SHELL value: {value}"),
            });
        }
    };

    if let Some(program) = find_command_with(names, path_var, path_ext, is_file) {
        return Ok(WindowsShell { kind, program });
    }

    Err(Error::HookExecutionFailed {
        command: String::from(""),
        cause: format!("Requested shell not found for hooks: {value}"),
    })
}

#[cfg(windows)]
fn find_command_with(
    names: &[&str],
    path_var: Option<&std::ffi::OsStr>,
    path_ext: Option<&std::ffi::OsStr>,
    is_file: &dyn Fn(&std::path::Path) -> bool,
) -> Option<String> {
    let path_var = path_var?;
    let mut exts = vec!["".to_string()];
    if let Some(exts_var) = path_ext {
        for ext in exts_var.to_string_lossy().split(';') {
            if !ext.is_empty() {
                exts.push(ext.to_ascii_lowercase());
            }
        }
    }

    for dir in std::env::split_paths(&path_var) {
        for name in names {
            if let Some(found) = find_in_dir_with(&dir, name, &exts, is_file) {
                return Some(found);
            }
        }
    }

    None
}

#[cfg(windows)]
fn find_in_dir_with(
    dir: &std::path::Path,
    name: &str,
    exts: &[String],
    is_file: &dyn Fn(&std::path::Path) -> bool,
) -> Option<String> {
    let name_lower = name.to_ascii_lowercase();
    for ext in exts {
        let candidate = if ext.is_empty() || name_lower.ends_with(ext) {
            dir.join(name)
        } else {
            dir.join(format!("{name}{ext}"))
        };
        if is_file(&candidate) {
            return candidate.to_str().map(|s| s.to_string());
        }
    }
    None
}

/// Execute pre_add hooks
pub(crate) fn run_pre_add(
    hooks: &Hooks,
    env: &HookEnv,
    working_dir: &Path,
    output: &Output,
) -> Result<()> {
    for (i, entry) in hooks.pre_add.iter().enumerate() {
        output.hook_running(
            "pre_add",
            i + 1,
            hooks.pre_add.len(),
            &entry.command,
            entry.description.as_deref(),
        );
        execute_hook(&entry.command, env, working_dir)?;
        output.hook_separator();
    }
    Ok(())
}

/// Execute post_add hooks
pub(crate) fn run_post_add(
    hooks: &Hooks,
    env: &HookEnv,
    working_dir: &Path,
    output: &Output,
) -> Result<()> {
    for (i, entry) in hooks.post_add.iter().enumerate() {
        output.hook_running(
            "post_add",
            i + 1,
            hooks.post_add.len(),
            &entry.command,
            entry.description.as_deref(),
        );
        execute_hook(&entry.command, env, working_dir)?;
        output.hook_separator();
    }
    Ok(())
}

/// Execute pre_remove hooks
pub(crate) fn run_pre_remove(
    hooks: &Hooks,
    env: &HookEnv,
    working_dir: &Path,
    output: &Output,
) -> Result<()> {
    for (i, entry) in hooks.pre_remove.iter().enumerate() {
        output.hook_running(
            "pre_remove",
            i + 1,
            hooks.pre_remove.len(),
            &entry.command,
            entry.description.as_deref(),
        );
        execute_hook(&entry.command, env, working_dir)?;
        output.hook_separator();
    }
    Ok(())
}

/// Execute post_remove hooks
pub(crate) fn run_post_remove(
    hooks: &Hooks,
    env: &HookEnv,
    working_dir: &Path,
    output: &Output,
) -> Result<()> {
    for (i, entry) in hooks.post_remove.iter().enumerate() {
        output.hook_running(
            "post_remove",
            i + 1,
            hooks.post_remove.len(),
            &entry.command,
            entry.description.as_deref(),
        );
        execute_hook(&entry.command, env, working_dir)?;
        output.hook_separator();
    }
    Ok(())
}

/// Display dry-run output for hook entries.
pub(crate) fn dry_run_hooks(hook_type: &str, entries: &[HookEntry], output: &Output) {
    for entry in entries {
        let display = entry.description.as_deref().unwrap_or(&entry.command);
        output.dry_run(&format!("Would run {} hook: {}", hook_type, display));
    }
}

/// Display a list of hook entries with optional color formatting.
fn display_hook_entries(entries: &[HookEntry], hook_type: &str, use_color: bool) {
    if entries.is_empty() {
        return;
    }

    eprintln!();
    if use_color {
        eprintln!("{}", ColorScheme::hook_type(&format!("{}:", hook_type)));
    } else {
        eprintln!("{}:", hook_type);
    }

    for entry in entries {
        eprintln!("  {}", entry.command);
        if let Some(desc) = &entry.description {
            if use_color {
                eprintln!(
                    "  {} {}",
                    ColorScheme::hook_arrow("->"),
                    ColorScheme::hook_description(desc)
                );
            } else {
                eprintln!("  -> {}", desc);
            }
        } else if use_color {
            eprintln!(
                "  {} {}",
                ColorScheme::hook_arrow("->"),
                ColorScheme::dimmed("no description")
            );
        } else {
            eprintln!("  -> no description");
        }
    }
}

/// Display hooks for user review before trusting
pub(crate) fn display_hooks_for_review(hooks: &Hooks) {
    let use_color = std::io::stderr().is_terminal();

    if use_color {
        eprintln!(
            "{}",
            ColorScheme::warning("WARNING: Untrusted hooks detected in config file")
        );
    } else {
        eprintln!("WARNING: Untrusted hooks detected in config file");
    }
    eprintln!();
    eprintln!("Trusting will allow ALL hooks in this config to execute:");

    display_hook_entries(&hooks.pre_add, "pre_add", use_color);
    display_hook_entries(&hooks.post_add, "post_add", use_color);
    display_hook_entries(&hooks.pre_remove, "pre_remove", use_color);
    display_hook_entries(&hooks.post_remove, "post_remove", use_color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn test_escape(s: &str) -> String {
        shell_escape(s)
    }

    #[cfg(windows)]
    fn test_escape(s: &str) -> String {
        powershell_escape(s)
    }

    #[cfg(unix)]
    #[test]
    fn test_shell_escape_basic() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
    }

    #[cfg(unix)]
    #[test]
    fn test_shell_escape_single_quote() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("a'b'c"), "'a'\\''b'\\''c'");
    }

    #[cfg(unix)]
    #[test]
    fn test_shell_escape_special_chars() {
        // Test shell metacharacters are properly quoted
        assert_eq!(shell_escape("$HOME"), "'$HOME'");
        assert_eq!(shell_escape("`date`"), "'`date`'");
        assert_eq!(shell_escape("test & pause"), "'test & pause'");
        assert_eq!(shell_escape("foo | bar"), "'foo | bar'");
        assert_eq!(shell_escape("test > file"), "'test > file'");
    }

    #[cfg(unix)]
    #[test]
    fn test_shell_escape_empty() {
        assert_eq!(shell_escape(""), "''");
    }

    #[cfg(windows)]
    #[test]
    fn test_powershell_escape_basic() {
        assert_eq!(powershell_escape("hello"), "'hello'");
        assert_eq!(powershell_escape("hello world"), "'hello world'");
    }

    #[cfg(windows)]
    #[test]
    fn test_powershell_escape_single_quote() {
        assert_eq!(powershell_escape("it's"), "'it''s'");
        assert_eq!(powershell_escape("a'b'c"), "'a''b''c'");
    }

    #[cfg(windows)]
    #[test]
    fn test_powershell_escape_special_chars() {
        assert_eq!(powershell_escape("$env:USERPROFILE"), "'$env:USERPROFILE'");
        assert_eq!(powershell_escape("$(Get-Date)"), "'$(Get-Date)'");
        assert_eq!(powershell_escape("test & pause"), "'test & pause'");
        assert_eq!(powershell_escape("foo | bar"), "'foo | bar'");
    }

    #[cfg(windows)]
    #[test]
    fn test_powershell_escape_empty() {
        assert_eq!(powershell_escape(""), "''");
    }

    #[cfg(windows)]
    #[test]
    fn test_cmd_escape_basic() {
        assert_eq!(cmd_escape("hello"), "\"hello\"");
        assert_eq!(cmd_escape("hello world"), "\"hello world\"");
    }

    #[cfg(windows)]
    #[test]
    fn test_cmd_escape_special_chars() {
        assert_eq!(cmd_escape("a&b"), "\"a^&b\"");
        assert_eq!(cmd_escape("a|b"), "\"a^|b\"");
        assert_eq!(cmd_escape("a>b"), "\"a^>b\"");
        assert_eq!(cmd_escape("a<b"), "\"a^<b\"");
        assert_eq!(cmd_escape("a^b"), "\"a^^b\"");
        assert_eq!(cmd_escape("a%b"), "\"a^%b\"");
    }

    #[cfg(windows)]
    fn select_shell_for_test(
        override_value: Option<&str>,
        path_list: &[&str],
        path_exts: &[&str],
        existing: &[&str],
    ) -> Result<WindowsShell> {
        use std::collections::HashSet;

        let path_var = if path_list.is_empty() {
            None
        } else {
            Some(std::ffi::OsString::from(path_list.join(";")))
        };
        let path_ext = if path_exts.is_empty() {
            None
        } else {
            Some(std::ffi::OsString::from(path_exts.join(";")))
        };

        let existing: HashSet<std::path::PathBuf> =
            existing.iter().map(std::path::PathBuf::from).collect();

        select_windows_shell_with(
            override_value,
            path_var.as_deref(),
            path_ext.as_deref(),
            &|path| existing.contains(path),
        )
    }

    #[cfg(windows)]
    #[test]
    fn test_select_windows_shell_prefers_pwsh() {
        let shell = select_shell_for_test(
            None,
            &["C:\\bin"],
            &[".EXE"],
            &[
                "C:\\bin\\pwsh.exe",
                "C:\\bin\\powershell.exe",
                "C:\\bin\\cmd.exe",
            ],
        )
        .unwrap();
        assert_eq!(shell.kind, WindowsShellKind::Pwsh);
    }

    #[cfg(windows)]
    #[test]
    fn test_select_windows_shell_fallback_to_cmd() {
        let shell =
            select_shell_for_test(None, &["C:\\bin"], &[".EXE"], &["C:\\bin\\cmd.exe"]).unwrap();
        assert_eq!(shell.kind, WindowsShellKind::Cmd);
    }

    #[cfg(windows)]
    #[test]
    fn test_select_windows_shell_ignores_wsl_by_default() {
        let result = select_shell_for_test(None, &["C:\\bin"], &[".EXE"], &["C:\\bin\\wsl.exe"]);
        assert!(result.is_err());
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_shell_override() {
        let shell = select_shell_for_test(
            Some("cmd"),
            &["C:\\bin"],
            &[".EXE"],
            &["C:\\bin\\pwsh.exe", "C:\\bin\\cmd.exe"],
        )
        .unwrap();
        assert_eq!(shell.kind, WindowsShellKind::Cmd);
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_shell_override_wsl() {
        let shell =
            select_shell_for_test(Some("wsl"), &["C:\\bin"], &[".EXE"], &["C:\\bin\\wsl.exe"])
                .unwrap();
        assert_eq!(shell.kind, WindowsShellKind::Wsl);
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_shell_override_invalid_value() {
        let result = parse_windows_shell_override_with("invalid-shell", None, None, &|_| false);
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_template_basic() {
        let env = HookEnv {
            worktree_path: "/path/to/worktree".to_string(),
            worktree_name: "my-feature".to_string(),
            branch: Some("feature/test".to_string()),
            repo_root: "/path/to/repo".to_string(),
            vcs_type: "git".to_string(),
            change_id: None,
            commit_id: None,
            hook_shell: None,
        };

        let result = env.expand_template_with("echo {{worktree_name}}", test_escape);
        assert!(result.contains("my-feature"));
        assert!(result.contains("echo"));
    }

    #[test]
    fn test_expand_template_with_special_chars() {
        let env = HookEnv {
            worktree_path: "/path/with spaces/worktree".to_string(),
            worktree_name: "feat'ure".to_string(),
            branch: Some("fix/$bug".to_string()),
            repo_root: "/repo".to_string(),
            vcs_type: "git".to_string(),
            change_id: None,
            commit_id: None,
            hook_shell: None,
        };

        let result = env.expand_template_with("cd {{worktree_path}}", test_escape);
        // Should be properly escaped
        assert!(result.contains('\''));

        let result = env.expand_template_with("echo {{worktree_name}}", test_escape);
        assert!(result.contains('\''));

        let result = env.expand_template_with("git checkout {{branch}}", test_escape);
        assert!(result.contains('\''));
    }

    #[test]
    fn test_expand_template_no_branch() {
        let env = HookEnv {
            worktree_path: "/path/to/worktree".to_string(),
            worktree_name: "detached".to_string(),
            branch: None,
            repo_root: "/repo".to_string(),
            vcs_type: "git".to_string(),
            change_id: None,
            commit_id: None,
            hook_shell: None,
        };

        let result = env.expand_template_with("echo {{branch}}", test_escape);
        assert_eq!(result, "echo ");
    }

    #[test]
    fn test_expand_template_multiple_variables() {
        let env = HookEnv {
            worktree_path: "/worktree".to_string(),
            worktree_name: "test".to_string(),
            branch: Some("main".to_string()),
            repo_root: "/repo".to_string(),
            vcs_type: "git".to_string(),
            change_id: None,
            commit_id: None,
            hook_shell: None,
        };

        let result = env.expand_template_with(
            "cd {{worktree_path}} && git checkout {{branch}} && pwd",
            test_escape,
        );
        assert!(result.contains("/worktree"));
        assert!(result.contains("main"));
    }

    #[test]
    fn test_expand_template_jj_variables() {
        let env = HookEnv {
            worktree_path: "/workspace".to_string(),
            worktree_name: "feature".to_string(),
            branch: Some("my-bookmark".to_string()),
            repo_root: "/repo".to_string(),
            vcs_type: "jj".to_string(),
            change_id: Some("abc123def456".to_string()),
            commit_id: Some("xyz789".to_string()),
            hook_shell: None,
        };

        // Test jj-specific variables
        let result = env.expand_template_with("echo {{change_id}}", test_escape);
        assert!(result.contains("abc123def456"));

        let result = env.expand_template_with("echo {{commit_id}}", test_escape);
        assert!(result.contains("xyz789"));

        let result = env.expand_template_with("echo {{vcs_type}}", test_escape);
        assert!(result.contains("jj"));

        // Test alias variables work
        let result = env.expand_template_with("cd {{workspace_path}}", test_escape);
        assert!(result.contains("/workspace"));

        let result = env.expand_template_with("echo {{workspace_name}}", test_escape);
        assert!(result.contains("feature"));

        let result = env.expand_template_with("echo {{bookmark}}", test_escape);
        assert!(result.contains("my-bookmark"));
    }

    #[cfg(unix)]
    #[test]
    fn test_execute_hook_sets_env_variables_git() {
        let dir = tempfile::tempdir().unwrap();
        let env = HookEnv {
            worktree_path: "/path/to/worktree".to_string(),
            worktree_name: "my-feature".to_string(),
            branch: Some("feature/test".to_string()),
            repo_root: "/path/to/repo".to_string(),
            vcs_type: "git".to_string(),
            change_id: None,
            commit_id: None,
            hook_shell: None,
        };

        let output_file = dir.path().join("env_output.txt");

        let command = format!(
            concat!(
                "echo \"KABU_WORKTREE_PATH=$KABU_WORKTREE_PATH\" > {0} && ",
                "echo \"KABU_WORKTREE_NAME=$KABU_WORKTREE_NAME\" >> {0} && ",
                "echo \"KABU_BRANCH=$KABU_BRANCH\" >> {0} && ",
                "echo \"KABU_REPO_ROOT=$KABU_REPO_ROOT\" >> {0} && ",
                "echo \"KABU_VCS_TYPE=$KABU_VCS_TYPE\" >> {0} && ",
                "echo \"KABU_CHANGE_ID=$KABU_CHANGE_ID\" >> {0} && ",
                "echo \"KABU_COMMIT_ID=$KABU_COMMIT_ID\" >> {0}",
            ),
            output_file.display()
        );

        execute_hook(&command, &env, dir.path()).unwrap();

        let content = std::fs::read_to_string(&output_file).unwrap();
        assert!(content.contains("KABU_WORKTREE_PATH=/path/to/worktree"));
        assert!(content.contains("KABU_WORKTREE_NAME=my-feature"));
        assert!(content.contains("KABU_BRANCH=feature/test"));
        assert!(content.contains("KABU_REPO_ROOT=/path/to/repo"));
        assert!(content.contains("KABU_VCS_TYPE=git"));
        assert!(content.contains("KABU_CHANGE_ID=\n"));
        assert!(content.contains("KABU_COMMIT_ID=\n"));
    }

    #[cfg(unix)]
    #[test]
    fn test_execute_hook_sets_env_variables_jj() {
        let dir = tempfile::tempdir().unwrap();
        let env = HookEnv {
            worktree_path: "/path/to/workspace".to_string(),
            worktree_name: "my-workspace".to_string(),
            branch: None,
            repo_root: "/path/to/repo".to_string(),
            vcs_type: "jj".to_string(),
            change_id: Some("abc123".to_string()),
            commit_id: Some("def456".to_string()),
            hook_shell: None,
        };

        let output_file = dir.path().join("env_output.txt");

        let command = format!(
            concat!(
                "echo \"KABU_WORKTREE_PATH=$KABU_WORKTREE_PATH\" > {0} && ",
                "echo \"KABU_WORKTREE_NAME=$KABU_WORKTREE_NAME\" >> {0} && ",
                "echo \"KABU_BRANCH=$KABU_BRANCH\" >> {0} && ",
                "echo \"KABU_VCS_TYPE=$KABU_VCS_TYPE\" >> {0} && ",
                "echo \"KABU_CHANGE_ID=$KABU_CHANGE_ID\" >> {0} && ",
                "echo \"KABU_COMMIT_ID=$KABU_COMMIT_ID\" >> {0}",
            ),
            output_file.display()
        );

        execute_hook(&command, &env, dir.path()).unwrap();

        let content = std::fs::read_to_string(&output_file).unwrap();
        assert!(content.contains("KABU_WORKTREE_PATH=/path/to/workspace"));
        assert!(content.contains("KABU_WORKTREE_NAME=my-workspace"));
        assert!(content.contains("KABU_BRANCH=\n"));
        assert!(content.contains("KABU_VCS_TYPE=jj"));
        assert!(content.contains("KABU_CHANGE_ID=abc123"));
        assert!(content.contains("KABU_COMMIT_ID=def456"));
    }
}
