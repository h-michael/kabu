use crate::error::{Error, Result};
use crate::vcs::WorkspaceInfo;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use indexmap::IndexSet;
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Nucleo, Utf32String};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph, Wrap};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{
    STEP_SELECT_WORKTREE, TextInputState, UiLayout, UiTheme, is_help_key, read_key_event,
    set_search_cursor, truncate_text_for_width, with_terminal,
};

const SINGLE_MODE_HINTS: &str =
    "[Enter] select  [Up/Down/Ctrl+P/N] move  type: search  [Esc] cancel  [F1] help";
const MULTI_MODE_HINTS: &str = "[Enter] confirm  [Space] toggle  [Up/Down/Ctrl+P/N] move  type: search  [Esc] cancel  [F1] help";

#[derive(Debug, Clone)]
pub(crate) struct WorktreeEntry {
    pub display: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectMode {
    Single,
    Multi,
}

/// Builds a list of worktree entries for display.
///
/// # Arguments
/// * `workspaces` - List of workspace information from VCS
/// * `include_main` - If true, includes the main workspace; if false, filters it out
/// * `current_dir` - Optional current directory to mark with [current]
///
/// # Display format
/// Each entry shows: `{path} ({branch})[main][locked][current]`
/// - `[main]` indicator only shown when `include_main` is true
/// - `[locked]` indicator shown for locked workspaces
/// - `[current]` indicator shown for the workspace containing current_dir
pub(crate) fn build_worktree_entries(
    workspaces: &[WorkspaceInfo],
    include_main: bool,
    current_dir: Option<&std::path::Path>,
) -> Vec<WorktreeEntry> {
    workspaces
        .iter()
        .filter(|ws| include_main || !ws.is_main)
        .map(|ws| {
            let branch_info = ws
                .branch
                .as_ref()
                .and_then(|b| b.strip_prefix("refs/heads/"))
                .unwrap_or("(detached)");
            let main_info = if include_main && ws.is_main {
                " [main]"
            } else {
                ""
            };
            let lock_info = if ws.is_locked { " [locked]" } else { "" };
            let is_current = current_dir
                .map(|dir| dir.starts_with(&ws.path))
                .unwrap_or(false);
            let current_info = if is_current { " [current]" } else { "" };
            WorktreeEntry {
                display: format!(
                    "{} ({}){}{}{}",
                    ws.path.display(),
                    branch_info,
                    main_info,
                    lock_info,
                    current_info
                ),
                path: ws.path.clone(),
            }
        })
        .collect()
}

pub(crate) fn select_worktrees(
    entries: &[WorktreeEntry],
    mode: SelectMode,
    command_name: &str,
    breadcrumbs: &[&str],
    theme: UiTheme,
) -> Result<Vec<PathBuf>> {
    if entries.is_empty() {
        return Err(Error::Selector {
            message: "No items to select".to_string(),
        });
    }

    with_terminal(|terminal| {
        run_worktree_list(terminal, entries, mode, command_name, breadcrumbs, theme)
    })
}

fn run_worktree_list(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<Box<dyn std::io::Write>>>,
    entries: &[WorktreeEntry],
    mode: SelectMode,
    command_name: &str,
    breadcrumbs: &[&str],
    theme: UiTheme,
) -> Result<Vec<PathBuf>> {
    let mut state = WorktreeListState::new();
    let mut matcher = NucleoState::new(entries)?;
    matcher.update_query(&state.query.value);
    matcher.tick(&mut state)?;
    let mut last_tick = Instant::now();

    loop {
        matcher.tick(&mut state)?;
        terminal
            .draw(|frame| draw_worktree_list(frame, &state, mode, command_name, breadcrumbs, theme))
            .map_err(|e| Error::Selector {
                message: format!("Failed to draw UI: {e}"),
            })?;

        let timeout = Duration::from_millis(200);
        let elapsed = last_tick.elapsed();
        if elapsed >= timeout {
            last_tick = Instant::now();
        }

        if let Some(key) = read_key_event(timeout)? {
            match handle_key_event(&mut state, mode, key)? {
                InputAction::None => {}
                InputAction::QueryChanged => matcher.update_query(&state.query.value),
                InputAction::Accept => return finalize_selection(&state, mode),
            }
        }
    }
}

struct WorktreeListState {
    query: TextInputState,
    cursor: usize,
    selected: IndexSet<PathBuf>,
    matches: Vec<WorktreeEntry>,
    show_help: bool,
}

impl WorktreeListState {
    fn new() -> Self {
        Self {
            query: TextInputState::new(String::new()),
            cursor: 0,
            selected: IndexSet::new(),
            matches: Vec::new(),
            show_help: false,
        }
    }

    fn current_entry(&self) -> Option<&WorktreeEntry> {
        self.matches.get(self.cursor)
    }

    fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.cursor + 1 < self.matches.len() {
            self.cursor += 1;
        }
    }

    fn reset_cursor_if_needed(&mut self) {
        if self.matches.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.matches.len() {
            self.cursor = self.matches.len() - 1;
        }
    }
}

struct NucleoState {
    nucleo: Nucleo<WorktreeEntry>,
    last_query: String,
}

impl NucleoState {
    fn new(entries: &[WorktreeEntry]) -> Result<Self> {
        let notify = Arc::new(|| {});
        let nucleo = Nucleo::new(Config::DEFAULT, notify, None, 1);
        let injector = nucleo.injector();

        for entry in entries {
            let item = entry.clone();
            injector.push(item, |item, columns| {
                if let Some(column) = columns.first_mut() {
                    *column = Utf32String::from(item.display.as_str());
                }
            });
        }

        Ok(Self {
            nucleo,
            last_query: String::new(),
        })
    }

    fn update_query(&mut self, query: &str) {
        let append = query.starts_with(&self.last_query);
        self.nucleo
            .pattern
            .reparse(0, query, CaseMatching::Smart, Normalization::Smart, append);
        self.last_query = query.to_string();
    }

    fn tick(&mut self, state: &mut WorktreeListState) -> Result<()> {
        self.nucleo.tick(10);
        let snapshot = self.nucleo.snapshot();
        state.matches = snapshot
            .matched_items(0..snapshot.matched_item_count())
            .map(|item| item.data.clone())
            .collect();
        state.reset_cursor_if_needed();
        Ok(())
    }
}

enum InputAction {
    None,
    QueryChanged,
    Accept,
}

fn handle_key_event(
    state: &mut WorktreeListState,
    mode: SelectMode,
    key: KeyEvent,
) -> Result<InputAction> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Err(Error::Aborted);
    }

    // Toggle help modal (F1 or Alt+H)
    if is_help_key(&key) {
        state.show_help = !state.show_help;
        return Ok(InputAction::None);
    }

    // When help is shown, only handle close keys
    if state.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => state.show_help = false,
            _ => {}
        }
        return Ok(InputAction::None);
    }

    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Enter => return Ok(InputAction::Accept),
        KeyCode::Up => state.move_up(),
        KeyCode::Down => state.move_down(),
        KeyCode::Home => state.query.move_to_start(),
        KeyCode::End => state.query.move_to_end(),
        KeyCode::Left => state.query.move_left(),
        KeyCode::Right => state.query.move_right(),
        KeyCode::Backspace => {
            state.query.backspace();
            return Ok(InputAction::QueryChanged);
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'p' => state.move_up(),
                    'n' => state.move_down(),
                    'a' => state.query.move_to_start(),
                    'e' => state.query.move_to_end(),
                    'f' => state.query.move_right(),
                    'b' => state.query.move_left(),
                    'd' => {
                        state.query.delete_char();
                        return Ok(InputAction::QueryChanged);
                    }
                    'k' => {
                        state.query.kill_to_end();
                        return Ok(InputAction::QueryChanged);
                    }
                    'u' => {
                        state.query.clear();
                        return Ok(InputAction::QueryChanged);
                    }
                    'w' => {
                        state.query.delete_word_backward();
                        return Ok(InputAction::QueryChanged);
                    }
                    _ => {}
                }
            } else if c == ' ' && mode == SelectMode::Multi {
                if let Some(entry) = state.current_entry() {
                    let path = entry.path.clone();
                    if !state.selected.shift_remove(&path) {
                        state.selected.insert(path);
                    }
                }
            } else {
                state.query.insert_char(c);
                return Ok(InputAction::QueryChanged);
            }
        }
        KeyCode::Tab => {}
        _ => {}
    }

    Ok(InputAction::None)
}

fn draw_worktree_list(
    frame: &mut ratatui::Frame<'_>,
    state: &WorktreeListState,
    mode: SelectMode,
    command_name: &str,
    breadcrumbs: &[&str],
    theme: UiTheme,
) {
    let layout = UiLayout::new(frame.area(), theme);
    layout.draw_header(frame, command_name, breadcrumbs, None);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(layout.body);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(body_chunks[0]);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled("Search", theme.title_style()));
    let search_line = Line::from(vec![
        Span::styled("", theme.search_style()),
        Span::styled(
            &state.query.value,
            theme.search_style().add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(search_line).block(search_block),
        left_chunks[0],
    );
    set_search_cursor(frame, left_chunks[0], &state.query);

    let items: Vec<ListItem> = state
        .matches
        .iter()
        .enumerate()
        .map(|(pos, entry)| {
            let prefix = if mode == SelectMode::Multi {
                if state.selected.contains(&entry.path) {
                    "[x] "
                } else {
                    "[ ] "
                }
            } else {
                ""
            };
            let line = format!("{prefix}{}", entry.display);
            let style = if pos == state.cursor {
                theme.selection_style()
            } else {
                theme.text_style()
            };
            ListItem::new(Line::from(line)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled(STEP_SELECT_WORKTREE, theme.title_style())),
        )
        .style(theme.text_style());
    frame.render_widget(list, left_chunks[1]);

    let preview = state
        .current_entry()
        .map(|entry| entry.display.clone())
        .unwrap_or_else(|| "-".to_string());
    let preview = truncate_text_for_width(preview, body_chunks[1].width.saturating_sub(2));
    let preview_widget = Paragraph::new(preview)
        .style(theme.text_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled("Worktree Info", theme.title_style())),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(preview_widget, body_chunks[1]);

    let hints = match mode {
        SelectMode::Single => SINGLE_MODE_HINTS,
        SelectMode::Multi => MULTI_MODE_HINTS,
    };
    layout.draw_footer(frame, hints);
    layout.draw_help_modal(frame, state.show_help);
}

fn finalize_selection(state: &WorktreeListState, mode: SelectMode) -> Result<Vec<PathBuf>> {
    let mut selected = Vec::new();
    match mode {
        SelectMode::Single => {
            if let Some(entry) = state.current_entry() {
                selected.push(entry.path.clone());
            }
        }
        SelectMode::Multi => {
            selected.extend(state.selected.iter().cloned());
        }
    }

    if selected.is_empty() {
        return Err(Error::Aborted);
    }

    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    // build_worktree_entries tests

    #[test]
    fn test_build_worktree_entries_includes_main() {
        let worktrees = vec![WorkspaceInfo {
            path: PathBuf::from("/repo/.git"),
            head: "abc123".to_string(),
            branch: Some("refs/heads/main".to_string()),
            is_main: true,
            is_locked: false,
            workspace_name: None,
        }];

        let result = build_worktree_entries(&worktrees, true, None);

        assert_eq!(result.len(), 1);
        assert!(result[0].display.contains("[main]"));
    }

    #[test]
    fn test_build_worktree_entries_filters_main() {
        let worktrees = vec![
            WorkspaceInfo {
                path: PathBuf::from("/repo/.git"),
                head: "abc123".to_string(),
                branch: Some("refs/heads/main".to_string()),
                is_main: true,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-1"),
                head: "def456".to_string(),
                branch: Some("refs/heads/feature-1".to_string()),
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
        ];

        let result = build_worktree_entries(&worktrees, false, None);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, PathBuf::from("/repo/feature-1"));
    }

    #[test]
    fn test_build_worktree_entries_empty_list() {
        let worktrees: Vec<WorkspaceInfo> = vec![];

        let result = build_worktree_entries(&worktrees, true, None);

        assert!(result.is_empty());
    }

    #[test]
    fn test_build_worktree_entries_only_main_returns_empty_when_filtered() {
        let worktrees = vec![WorkspaceInfo {
            path: PathBuf::from("/repo/.git"),
            head: "abc123".to_string(),
            branch: Some("refs/heads/main".to_string()),
            is_main: true,
            is_locked: false,
            workspace_name: None,
        }];

        let result = build_worktree_entries(&worktrees, false, None);

        assert!(result.is_empty());
    }

    #[test]
    fn test_build_worktree_entries_preserves_order() {
        let worktrees = vec![
            WorkspaceInfo {
                path: PathBuf::from("/repo/.git"),
                head: "abc123".to_string(),
                branch: Some("refs/heads/main".to_string()),
                is_main: true,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-1"),
                head: "def456".to_string(),
                branch: Some("refs/heads/feature-1".to_string()),
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-2"),
                head: "ghi789".to_string(),
                branch: Some("refs/heads/feature-2".to_string()),
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
        ];

        let result = build_worktree_entries(&worktrees, false, None);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, PathBuf::from("/repo/feature-1"));
        assert_eq!(result[1].path, PathBuf::from("/repo/feature-2"));
    }

    #[test]
    fn test_build_worktree_entries_multiple_with_main_included() {
        let worktrees = vec![
            WorkspaceInfo {
                path: PathBuf::from("/repo/.git"),
                head: "abc123".to_string(),
                branch: Some("refs/heads/main".to_string()),
                is_main: true,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-branch"),
                head: "def456".to_string(),
                branch: Some("refs/heads/feature".to_string()),
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/locked-branch"),
                head: "ghi789".to_string(),
                branch: Some("refs/heads/locked".to_string()),
                is_main: false,
                is_locked: true,
                workspace_name: None,
            },
        ];

        let result = build_worktree_entries(&worktrees, true, None);

        assert_eq!(result.len(), 3);
        assert!(result[0].display.contains("[main]"));
        assert!(!result[1].display.contains("[main]"));
        assert!(!result[1].display.contains("[locked]"));
        assert!(result[2].display.contains("[locked]"));
    }

    #[test]
    fn test_build_worktree_entries_formats_display() {
        let worktrees = vec![
            WorkspaceInfo {
                path: PathBuf::from("/repo/.git"),
                head: "abc123".to_string(),
                branch: Some("refs/heads/main".to_string()),
                is_main: true,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-1"),
                head: "def456".to_string(),
                branch: Some("refs/heads/feature-1".to_string()),
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-2"),
                head: "ghi789".to_string(),
                branch: None,
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-3"),
                head: "jkl012".to_string(),
                branch: Some("refs/heads/feature-3".to_string()),
                is_main: false,
                is_locked: true,
                workspace_name: None,
            },
        ];

        let result = build_worktree_entries(&worktrees, false, None);

        assert_eq!(result.len(), 3);
        assert!(result[0].display.contains("feature-1"));
        assert!(result[1].display.contains("(detached)"));
        assert!(result[2].display.contains("[locked]"));
    }

    #[test]
    fn test_build_worktree_entries_detached_head() {
        let worktrees = vec![WorkspaceInfo {
            path: PathBuf::from("/repo/detached"),
            head: "abc123".to_string(),
            branch: None,
            is_main: false,
            is_locked: false,
            workspace_name: None,
        }];

        let result = build_worktree_entries(&worktrees, true, None);

        assert_eq!(result.len(), 1);
        assert!(result[0].display.contains("(detached)"));
    }

    #[test]
    fn test_build_worktree_entries_current_marker() {
        let worktrees = vec![
            WorkspaceInfo {
                path: PathBuf::from("/repo/.git"),
                head: "abc123".to_string(),
                branch: Some("refs/heads/main".to_string()),
                is_main: true,
                is_locked: false,
                workspace_name: None,
            },
            WorkspaceInfo {
                path: PathBuf::from("/repo/feature-1"),
                head: "def456".to_string(),
                branch: Some("refs/heads/feature-1".to_string()),
                is_main: false,
                is_locked: false,
                workspace_name: None,
            },
        ];

        // current_dir is inside feature-1 worktree
        let current_dir = PathBuf::from("/repo/feature-1/src");
        let result = build_worktree_entries(&worktrees, false, Some(&current_dir));

        assert_eq!(result.len(), 1);
        assert!(result[0].display.contains("[current]"));
    }

    #[test]
    fn test_build_worktree_entries_no_current_marker() {
        let worktrees = vec![WorkspaceInfo {
            path: PathBuf::from("/repo/feature-1"),
            head: "def456".to_string(),
            branch: Some("refs/heads/feature-1".to_string()),
            is_main: false,
            is_locked: false,
            workspace_name: None,
        }];

        // current_dir is NOT inside this worktree
        let current_dir = PathBuf::from("/other/dir");
        let result = build_worktree_entries(&worktrees, true, Some(&current_dir));

        assert_eq!(result.len(), 1);
        assert!(!result[0].display.contains("[current]"));
    }

    #[test]
    fn test_build_worktree_entries_current_marker_none() {
        let worktrees = vec![WorkspaceInfo {
            path: PathBuf::from("/repo/feature-1"),
            head: "def456".to_string(),
            branch: Some("refs/heads/feature-1".to_string()),
            is_main: false,
            is_locked: false,
            workspace_name: None,
        }];

        // current_dir is None
        let result = build_worktree_entries(&worktrees, true, None);

        assert_eq!(result.len(), 1);
        assert!(!result[0].display.contains("[current]"));
    }

    // WorktreeEntry and SelectMode tests

    fn create_test_entries() -> Vec<WorktreeEntry> {
        vec![
            WorktreeEntry {
                display: "main".to_string(),
                path: PathBuf::from("/repo"),
            },
            WorktreeEntry {
                display: "feature-a".to_string(),
                path: PathBuf::from("/repo-feature-a"),
            },
            WorktreeEntry {
                display: "feature-b".to_string(),
                path: PathBuf::from("/repo-feature-b"),
            },
        ]
    }

    #[test]
    fn test_worktree_entry_creation() {
        let entry = WorktreeEntry {
            display: "test-branch".to_string(),
            path: PathBuf::from("/path/to/worktree"),
        };
        assert_eq!(entry.display, "test-branch");
        assert_eq!(entry.path, PathBuf::from("/path/to/worktree"));
    }

    #[test]
    fn test_select_mode_equality() {
        assert_eq!(SelectMode::Single, SelectMode::Single);
        assert_eq!(SelectMode::Multi, SelectMode::Multi);
        assert_ne!(SelectMode::Single, SelectMode::Multi);
    }

    #[test]
    fn test_worktree_list_state_new() {
        let state = WorktreeListState::new();
        assert_eq!(state.query.value, "");
        assert_eq!(state.cursor, 0);
        assert!(state.selected.is_empty());
        assert!(state.matches.is_empty());
    }

    #[test]
    fn test_worktree_list_state_move_up() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();
        state.cursor = 2;

        state.move_up();
        assert_eq!(state.cursor, 1);

        state.move_up();
        assert_eq!(state.cursor, 0);

        // Should not go below 0
        state.move_up();
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_worktree_list_state_move_down() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        state.move_down();
        assert_eq!(state.cursor, 1);

        state.move_down();
        assert_eq!(state.cursor, 2);

        // Should not exceed matches.len() - 1
        state.move_down();
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_worktree_list_state_current_entry() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        let entry = state.current_entry().unwrap();
        assert_eq!(entry.display, "main");

        state.cursor = 1;
        let entry = state.current_entry().unwrap();
        assert_eq!(entry.display, "feature-a");
    }

    #[test]
    fn test_worktree_list_state_current_entry_empty() {
        let state = WorktreeListState::new();
        assert!(state.current_entry().is_none());
    }

    #[test]
    fn test_worktree_list_state_reset_cursor_if_needed() {
        let mut state = WorktreeListState::new();
        state.cursor = 5;
        state.matches = create_test_entries(); // 3 entries

        state.reset_cursor_if_needed();
        assert_eq!(state.cursor, 2); // Should be clamped to len - 1

        state.matches.clear();
        state.cursor = 5;
        state.reset_cursor_if_needed();
        assert_eq!(state.cursor, 0); // Should be 0 when empty
    }

    #[test]
    fn test_worktree_list_state_selection() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        // Add to selection
        state.selected.insert(PathBuf::from("/repo"));
        assert!(state.selected.contains(&PathBuf::from("/repo")));
        assert_eq!(state.selected.len(), 1);

        // Add another
        state.selected.insert(PathBuf::from("/repo-feature-a"));
        assert_eq!(state.selected.len(), 2);

        // Remove from selection
        state.selected.shift_remove(&PathBuf::from("/repo"));
        assert!(!state.selected.contains(&PathBuf::from("/repo")));
        assert_eq!(state.selected.len(), 1);
    }

    // handle_key_event tests

    fn create_key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_handle_key_event_escape_aborts() {
        let mut state = WorktreeListState::new();
        let key = create_key_event(KeyCode::Esc, KeyModifiers::NONE);

        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_key_event_ctrl_c_aborts() {
        let mut state = WorktreeListState::new();
        let key = create_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_key_event_enter_accepts() {
        let mut state = WorktreeListState::new();
        let key = create_key_event(KeyCode::Enter, KeyModifiers::NONE);

        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::Accept)));
    }

    #[test]
    fn test_handle_key_event_arrow_keys() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        // Down arrow
        let key = create_key_event(KeyCode::Down, KeyModifiers::NONE);
        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::None)));
        assert_eq!(state.cursor, 1);

        // Up arrow
        let key = create_key_event(KeyCode::Up, KeyModifiers::NONE);
        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::None)));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_handle_key_event_ctrl_navigation() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        // Ctrl+n (down)
        let key = create_key_event(KeyCode::Char('n'), KeyModifiers::CONTROL);
        handle_key_event(&mut state, SelectMode::Single, key).unwrap();
        assert_eq!(state.cursor, 1);

        // Ctrl+p (up)
        let key = create_key_event(KeyCode::Char('p'), KeyModifiers::CONTROL);
        handle_key_event(&mut state, SelectMode::Single, key).unwrap();
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_handle_key_event_query_input() {
        let mut state = WorktreeListState::new();

        // Type 'a'
        let key = create_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::QueryChanged)));
        assert_eq!(state.query.value, "a");

        // Type 'b'
        let key = create_key_event(KeyCode::Char('b'), KeyModifiers::NONE);
        handle_key_event(&mut state, SelectMode::Single, key).unwrap();
        assert_eq!(state.query.value, "ab");
    }

    #[test]
    fn test_handle_key_event_backspace() {
        let mut state = WorktreeListState::new();
        state.query = TextInputState::new("test".to_string());

        let key = create_key_event(KeyCode::Backspace, KeyModifiers::NONE);
        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::QueryChanged)));
        assert_eq!(state.query.value, "tes");
    }

    #[test]
    fn test_handle_key_event_ctrl_u_clears_query() {
        let mut state = WorktreeListState::new();
        state.query = TextInputState::new("test query".to_string());

        let key = create_key_event(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::QueryChanged)));
        assert_eq!(state.query.value, "");
    }

    #[test]
    fn test_handle_key_event_space_toggles_selection_multi() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        // Space in Multi mode toggles selection
        let key = create_key_event(KeyCode::Char(' '), KeyModifiers::NONE);
        handle_key_event(&mut state, SelectMode::Multi, key).unwrap();
        assert!(state.selected.contains(&PathBuf::from("/repo")));

        // Space again deselects
        let key = create_key_event(KeyCode::Char(' '), KeyModifiers::NONE);
        handle_key_event(&mut state, SelectMode::Multi, key).unwrap();
        assert!(!state.selected.contains(&PathBuf::from("/repo")));
    }

    #[test]
    fn test_handle_key_event_space_in_single_mode_adds_to_query() {
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();

        // Space in Single mode adds to query
        let key = create_key_event(KeyCode::Char(' '), KeyModifiers::NONE);
        let result = handle_key_event(&mut state, SelectMode::Single, key);
        assert!(matches!(result, Ok(InputAction::QueryChanged)));
        assert_eq!(state.query.value, " ");
        assert!(state.selected.is_empty());
    }

    // draw_worktree_list tests

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_draw_worktree_list_renders_single_mode() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_worktree_list(
                    frame,
                    &state,
                    SelectMode::Single,
                    "test",
                    &[STEP_SELECT_WORKTREE],
                    theme,
                );
            })
            .unwrap();

        // Verify it renders without panic
        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_worktree_list_renders_multi_mode() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();
        state.selected.insert(PathBuf::from("/repo"));
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_worktree_list(
                    frame,
                    &state,
                    SelectMode::Multi,
                    "test",
                    &[STEP_SELECT_WORKTREE],
                    theme,
                );
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_worktree_list_with_query() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();
        state.query = TextInputState::new("feature".to_string());
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_worktree_list(
                    frame,
                    &state,
                    SelectMode::Single,
                    "test",
                    &[STEP_SELECT_WORKTREE],
                    theme,
                );
            })
            .unwrap();

        // Verify search query is rendered in the Search box
        // Layout: header(1) + body_padding(1) = 2 (body start)
        // Search box: top border at y=2, content at y=3, bottom border at y=4
        let buffer = terminal.backend().buffer();
        let search_row: String = (0..buffer.area.width)
            .map(|x| {
                buffer
                    .cell((x, 3))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(search_row.contains("feature"));
    }

    #[test]
    fn test_draw_worktree_list_empty_matches() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = WorktreeListState::new();
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_worktree_list(
                    frame,
                    &state,
                    SelectMode::Single,
                    "test",
                    &[STEP_SELECT_WORKTREE],
                    theme,
                );
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_worktree_list_cursor_position() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = WorktreeListState::new();
        state.matches = create_test_entries();
        state.cursor = 1; // Select second item
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_worktree_list(
                    frame,
                    &state,
                    SelectMode::Single,
                    "test",
                    &[STEP_SELECT_WORKTREE],
                    theme,
                );
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }
}
