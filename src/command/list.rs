//! List worktrees/workspaces command implementation.
//!
//! Lists all git worktrees or jj workspaces with detailed information including
//! branch/bookmark, commit hash, and status indicators.

use crate::cli::ListArgs;
use crate::color::{self, ColorConfig, ColorScheme};
use crate::config;
use crate::error::{Error, Result};
use crate::output::Output;
use crate::vcs::{self, UnpushedInfo, WorkspaceInfo, WorkspaceStatus};

/// Enriched workspace info for display purposes.
struct DisplayWorkspace {
    path: String,
    branch: String,
    head: String,
    status: WorkspaceStatus,
    unpushed: UnpushedInfo,
    upstream: Option<String>,
    is_locked: bool,
}

pub(crate) fn run(args: ListArgs, color: ColorConfig) -> Result<()> {
    let output = Output::new(false, color);

    let provider = vcs::get_provider()?;

    if !provider.is_inside_repo() {
        return Err(Error::NotInAnyRepo);
    }

    let repo_root = provider.main_workspace_root()?;
    if let Ok(config) = config::load_merged(&repo_root) {
        color::set_cli_theme(&config.ui.colors);
    }

    let workspaces = provider.list_workspaces()?;

    if args.path_only {
        print_path_only(&workspaces, &output, args.header);
        return Ok(());
    }

    // Pre-fetch all data before printing
    let display_data = enrich_workspaces(&workspaces, provider.as_ref())?;

    // Calculate max path length and max branch length for alignment
    let (max_path, max_branch) = display_data.iter().fold((0, 0), |(max_p, max_b), ws| {
        (max_p.max(ws.path.len()), max_b.max(ws.branch.len()))
    });

    if args.header {
        print_header(&output, max_path, max_branch, color);
    }

    for ws in display_data {
        display_workspace(&ws, &output, max_path, max_branch, color);
    }

    Ok(())
}

fn print_path_only(workspaces: &[WorkspaceInfo], output: &Output, header: bool) {
    if header {
        output.list("PATH");
    }
    for ws in workspaces {
        output.list(&ws.path.display().to_string());
    }
}

fn enrich_workspaces(
    workspaces: &[WorkspaceInfo],
    provider: &dyn vcs::VcsProvider,
) -> Result<Vec<DisplayWorkspace>> {
    let mut display_data = Vec::new();
    for ws in workspaces {
        // Get status and unpushed commits (best effort)
        let status = provider.workspace_status(&ws.path).unwrap_or_default();
        let unpushed = provider.workspace_unpushed(&ws.path).unwrap_or_default();
        let upstream = provider.get_upstream(&ws.path).unwrap_or(None);

        display_data.push(DisplayWorkspace {
            path: ws.path.display().to_string(),
            branch: branch_display(&ws.branch),
            head: ws.head.clone(),
            status,
            unpushed,
            upstream,
            is_locked: ws.is_locked,
        });
    }
    Ok(display_data)
}

fn print_header(output: &Output, max_path: usize, max_branch: usize, color: ColorConfig) {
    let path = "PATH";
    let branch = "BRANCH";
    let commit = "COMMIT";
    let status = "STATUS";

    let line = format!(
        "{path:<p_width$} {branch:<b_width$} {commit:<c_width$} {status}",
        p_width = max_path,
        b_width = max_branch,
        c_width = 7,
    );
    if color.is_enabled() {
        output.list(&ColorScheme::header(&line));
    } else {
        output.list(&line);
    }
}

fn display_workspace(
    ws: &DisplayWorkspace,
    output: &Output,
    max_path: usize,
    max_branch: usize,
    color: ColorConfig,
) {
    let short_hash = ws.head.chars().take(7).collect::<String>();
    let status_str = format_status(&ws.status, &ws.unpushed, &ws.upstream, ws.is_locked);

    let line = if color.is_enabled() {
        format!(
            "{path:<p_width$} {branch} {hash} {status}",
            path = ws.path,
            p_width = max_path,
            branch = ColorScheme::branch(&format!("{:<width$}", ws.branch, width = max_branch)),
            hash = ColorScheme::hash(&format!("{:<width$}", short_hash, width = 7)),
            status = ColorScheme::dimmed(&status_str),
        )
    } else {
        format!(
            "{path:<p_width$} {branch:<b_width$} {hash:<c_width$} {status}",
            path = ws.path,
            p_width = max_path,
            branch = ws.branch,
            b_width = max_branch,
            hash = short_hash,
            c_width = 7,
            status = status_str,
        )
    };

    output.list(&line);
}

fn branch_display(branch: &Option<String>) -> String {
    branch
        .as_ref()
        .and_then(|b| b.strip_prefix("refs/heads/"))
        .unwrap_or("(detached)")
        .to_string()
}

fn format_status(
    status: &WorkspaceStatus,
    unpushed: &UnpushedInfo,
    upstream: &Option<String>,
    is_locked: bool,
) -> String {
    let mut parts = Vec::new();
    if status.has_uncommitted_changes {
        let mut changes = Vec::new();
        if status.modified_count > 0 {
            changes.push("modified");
        }
        if status.deleted_count > 0 {
            changes.push("deleted");
        }
        if status.untracked_count > 0 {
            changes.push("untracked");
        }
        if !changes.is_empty() {
            parts.push(changes.join(", "));
        }
    }

    if unpushed.has_unpushed {
        let unpushed_str = if let Some(upstream_name) = upstream {
            format!("unpushed: {} (vs {})", unpushed.count, upstream_name)
        } else {
            format!("unpushed: {}", unpushed.count)
        };
        parts.push(unpushed_str);
    }

    if is_locked {
        parts.push("locked".to_string());
    }

    if parts.is_empty() {
        "up to date".to_string()
    } else {
        parts.join(" | ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::UnpushedInfo;

    #[test]
    fn test_branch_display_regular() {
        let branch = Some("refs/heads/main".to_string());
        assert_eq!(branch_display(&branch), "main");
    }

    #[test]
    fn test_branch_display_detached() {
        assert_eq!(branch_display(&None), "(detached)");
    }

    #[test]
    fn test_format_status_all() {
        use crate::vcs::WorkspaceStatus;

        let status = WorkspaceStatus {
            has_uncommitted_changes: true,
            modified_count: 2,
            deleted_count: 1,
            untracked_count: 3,
        };
        let unpushed = UnpushedInfo {
            has_unpushed: true,
            count: 5,
        };
        let upstream = Some("origin/main".to_string());
        let result = format_status(&status, &unpushed, &upstream, true);
        assert_eq!(
            result,
            "modified, deleted, untracked | unpushed: 5 (vs origin/main) | locked"
        );
    }

    #[test]
    fn test_format_status_clean() {
        use crate::vcs::WorkspaceStatus;

        let status = WorkspaceStatus {
            has_uncommitted_changes: false,
            modified_count: 0,
            deleted_count: 0,
            untracked_count: 0,
        };
        let unpushed = UnpushedInfo {
            has_unpushed: false,
            count: 0,
        };
        let result = format_status(&status, &unpushed, &None, false);
        assert_eq!(result, "up to date");
    }
}
