use crate::error::{Error, Result};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, io};

use super::{
    STEP_ACTION, STEP_BASE, STEP_BRANCH, STEP_BRANCH_NAME, STEP_COMMIT, STEP_CONFIRM,
    STEP_WORKTREE_PATH, TextInputState, UiLayout, UiTheme, is_help_key, read_key_event,
    set_search_cursor, with_terminal,
};

#[derive(Debug, Clone)]
pub(crate) struct BranchChoice {
    pub branch: String,
    pub create_new: bool,
    pub base_commitish: Option<String>,
}

type LogFetcher = Arc<dyn Fn(&str, usize) -> Result<Vec<String>> + Send + Sync>;
type PathSuggester = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
type BranchNameSuggester = Arc<dyn Fn(&str) -> String + Send + Sync>;
type BranchNameValidator = Arc<dyn Fn(&str) -> Result<Option<String>> + Send + Sync>;

pub(crate) struct AddInteractiveInput {
    pub local_branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub used_branches: HashMap<String, PathBuf>,
    pub current_dir: PathBuf,
    pub existing_worktrees: Vec<WorktreeSummary>,
    pub log_limit: usize,
    pub fetch_log: LogFetcher,
    pub initial_path: Option<PathBuf>,
    pub suggest_path: Option<PathSuggester>,
    pub suggest_branch_name: Option<BranchNameSuggester>,
    pub validate_branch_name: BranchNameValidator,
    pub theme: UiTheme,
}

pub(crate) struct WorktreeSummary {
    pub path: PathBuf,
    pub branch: Option<String>,
}

pub(crate) struct AddInteractiveResult {
    pub branch_choice: BranchChoice,
    pub path: PathBuf,
}

pub(crate) fn run_add_interactive(input: AddInteractiveInput) -> Result<AddInteractiveResult> {
    with_terminal(|terminal| run_add_ui(terminal, input))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddStep {
    ModeSelect,
    Branch,
    NewBaseSelect,
    NewCommitInput,
    NewBranchName,
    Path,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchTab {
    Existing,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchPurpose {
    UseExisting,
    NewBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewBranchOrigin {
    Base,
    Commit,
}

fn build_breadcrumb(state: &AddUiState) -> Vec<&'static str> {
    let mut crumbs = vec![STEP_ACTION];

    match state.step {
        AddStep::ModeSelect => {}
        AddStep::Branch => {
            crumbs.push(STEP_BRANCH);
        }
        AddStep::NewBaseSelect => {
            crumbs.push(STEP_BRANCH);
            crumbs.push(STEP_BASE);
        }
        AddStep::NewCommitInput => {
            crumbs.push(STEP_BRANCH);
            crumbs.push(STEP_COMMIT);
        }
        AddStep::NewBranchName => {
            crumbs.push(STEP_BRANCH);
            match state.new_branch_origin {
                Some(NewBranchOrigin::Base) => crumbs.push(STEP_BASE),
                Some(NewBranchOrigin::Commit) => crumbs.push(STEP_COMMIT),
                None => {}
            }
            crumbs.push(STEP_BRANCH_NAME);
        }
        AddStep::Path => {
            crumbs.push(STEP_BRANCH);
            if state.branch_tab == BranchTab::New {
                match state.new_branch_origin {
                    Some(NewBranchOrigin::Base) => crumbs.push(STEP_BASE),
                    Some(NewBranchOrigin::Commit) => crumbs.push(STEP_COMMIT),
                    None => {}
                }
                crumbs.push(STEP_BRANCH_NAME);
            }
            crumbs.push(STEP_WORKTREE_PATH);
        }
        AddStep::Confirm => {
            crumbs.push(STEP_BRANCH);
            if state.branch_tab == BranchTab::New {
                match state.new_branch_origin {
                    Some(NewBranchOrigin::Base) => crumbs.push(STEP_BASE),
                    Some(NewBranchOrigin::Commit) => crumbs.push(STEP_COMMIT),
                    None => {}
                }
                crumbs.push(STEP_BRANCH_NAME);
            }
            crumbs.push(STEP_WORKTREE_PATH);
            crumbs.push(STEP_CONFIRM);
        }
    }

    crumbs
}

#[derive(Debug, Clone)]
struct BranchItem {
    name: String,
    in_use_by: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum BranchRow {
    Action(NewBranchAction),
    Header(String),
    Existing(BranchItem),
}

impl BranchRow {
    fn is_selectable(&self) -> bool {
        match self {
            BranchRow::Action(_) => true,
            BranchRow::Header(_) => false,
            BranchRow::Existing(item) => item.in_use_by.is_none(),
        }
    }

    fn display(&self) -> String {
        match self {
            BranchRow::Action(NewBranchAction::BaseBranch) => "+ Select base branch".to_string(),
            BranchRow::Action(NewBranchAction::Commit) => "+ New branch from commit".to_string(),
            BranchRow::Header(label) => label.clone(),
            BranchRow::Existing(item) => {
                if let Some(path) = item.in_use_by.as_ref() {
                    format!("{}  [in use: {}]", item.name, path.display())
                } else {
                    item.name.clone()
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum NewBranchAction {
    BaseBranch,
    Commit,
}

#[derive(Debug, Clone)]
struct AddUiState {
    step: AddStep,
    branch_tab: BranchTab,
    branch_purpose: BranchPurpose,
    branch_rows: Vec<BranchRow>,
    branch_cursor: usize,
    branch_query: TextInputState,
    matches: Vec<BranchRow>,
    selected_branch: Option<BranchItem>,
    new_branch_origin: Option<NewBranchOrigin>,
    base_branch: Option<String>,
    commit_input: TextInputState,
    commit_error: Option<String>,
    branch_name_input: TextInputState,
    path_input: TextInputState,
    branch_name_error: Option<String>,
    confirm_error: Option<String>,
    preview_branch: Option<String>,
    preview_log: Vec<String>,
    preview_cache: HashMap<String, Vec<String>>,
    show_help: bool,
}

impl AddUiState {
    fn new(input: &AddInteractiveInput) -> Self {
        use crate::config::AddDefaultMode;
        let initial_path = input
            .initial_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let branch_tab = match input.theme.add_default_mode {
            AddDefaultMode::Existing => BranchTab::Existing,
            AddDefaultMode::New => BranchTab::New,
        };

        Self {
            step: AddStep::ModeSelect,
            branch_tab,
            branch_purpose: BranchPurpose::UseExisting,
            branch_rows: Vec::new(),
            branch_cursor: 0,
            branch_query: TextInputState::new(String::new()),
            matches: Vec::new(),
            selected_branch: None,
            new_branch_origin: None,
            base_branch: None,
            commit_input: TextInputState::new(String::new()),
            commit_error: None,
            branch_name_input: TextInputState::new(String::new()),
            path_input: TextInputState::new(initial_path),
            branch_name_error: None,
            confirm_error: None,
            preview_branch: None,
            preview_log: Vec::new(),
            preview_cache: HashMap::new(),
            show_help: false,
        }
    }

    fn current_branch_row(&self) -> Option<&BranchRow> {
        self.matches.get(self.branch_cursor)
    }

    fn move_branch_up(&mut self) {
        if self.branch_cursor == 0 || self.matches.is_empty() {
            return;
        }
        let mut cursor = self.branch_cursor;
        while cursor > 0 {
            cursor -= 1;
            if let Some(row) = self.matches.get(cursor)
                && row.is_selectable()
            {
                self.branch_cursor = cursor;
                return;
            }
        }
    }

    fn move_branch_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        let mut cursor = self.branch_cursor;
        while cursor + 1 < self.matches.len() {
            cursor += 1;
            if let Some(row) = self.matches.get(cursor)
                && row.is_selectable()
            {
                self.branch_cursor = cursor;
                return;
            }
        }
    }

    fn move_branch_to_next_selectable(&mut self) {
        while let Some(row) = self.matches.get(self.branch_cursor) {
            if row.is_selectable() {
                break;
            }
            if self.branch_cursor + 1 >= self.matches.len() {
                break;
            }
            self.branch_cursor += 1;
        }
    }
}

fn run_add_ui(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<Box<dyn std::io::Write>>>,
    input: AddInteractiveInput,
) -> Result<AddInteractiveResult> {
    let mut state = AddUiState::new(&input);
    update_branch_rows(&mut state, &input);
    filter_branch_rows(&mut state);

    loop {
        terminal
            .draw(|frame| draw_add_ui(frame, &mut state, &input))
            .map_err(|e| Error::Selector {
                message: format!("Failed to draw UI: {e}"),
            })?;

        if let Some(key) = read_key_event(Duration::from_millis(200))?
            && handle_add_event(&mut state, &input, key)?
        {
            let branch_choice = build_branch_choice(&state)?;
            let path = PathBuf::from(state.path_input.value.clone());
            return Ok(AddInteractiveResult {
                branch_choice,
                path,
            });
        }
    }
}

fn update_branch_rows(state: &mut AddUiState, input: &AddInteractiveInput) {
    let mut rows = Vec::new();

    if state.branch_tab == BranchTab::New && state.branch_purpose == BranchPurpose::UseExisting {
        rows.push(BranchRow::Action(NewBranchAction::BaseBranch));
        rows.push(BranchRow::Action(NewBranchAction::Commit));
        state.branch_rows = rows;
        state.branch_cursor = 0;
        state.move_branch_to_next_selectable();
        return;
    }

    if state.branch_purpose == BranchPurpose::NewBase {
        if !input.local_branches.is_empty() {
            rows.push(BranchRow::Header("-- Local --".to_string()));
        }
        for name in &input.local_branches {
            rows.push(BranchRow::Existing(BranchItem {
                name: name.clone(),
                in_use_by: None,
            }));
        }
        if !input.remote_branches.is_empty() {
            rows.push(BranchRow::Header("-- Remote --".to_string()));
        }
        for name in &input.remote_branches {
            rows.push(BranchRow::Existing(BranchItem {
                name: name.clone(),
                in_use_by: None,
            }));
        }
    } else {
        for name in &input.local_branches {
            let in_use_by = input.used_branches.get(name).cloned();
            rows.push(BranchRow::Existing(BranchItem {
                name: name.clone(),
                in_use_by,
            }));
        }
    }

    state.branch_rows = rows;
    state.branch_cursor = 0;
    state.move_branch_to_next_selectable();
}

fn filter_branch_rows(state: &mut AddUiState) {
    if state.branch_query.value.is_empty() {
        state.matches = state.branch_rows.clone();
        state.branch_cursor = 0;
        state.move_branch_to_next_selectable();
        return;
    }

    let query = state.branch_query.value.to_lowercase();
    state.matches = state
        .branch_rows
        .iter()
        .filter(|row| row.display().to_lowercase().contains(&query))
        .cloned()
        .collect();
    state.branch_cursor = 0;
    state.move_branch_to_next_selectable();
}

fn build_branch_choice(state: &AddUiState) -> Result<BranchChoice> {
    let branch = state
        .selected_branch
        .as_ref()
        .map(|item| item.name.clone())
        .or_else(|| state.base_branch.clone())
        .ok_or(Error::Aborted)?;

    let create_new = state.branch_tab == BranchTab::New;
    let base_commitish = match state.new_branch_origin {
        Some(NewBranchOrigin::Commit) => Some(state.commit_input.value.clone()),
        Some(NewBranchOrigin::Base) => state.base_branch.clone(),
        None => None,
    };

    let branch = if create_new {
        state.branch_name_input.value.clone()
    } else {
        branch
    };

    Ok(BranchChoice {
        branch,
        create_new,
        base_commitish,
    })
}

fn handle_add_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Err(Error::Aborted);
    }

    // Toggle help modal (F1 or Alt+H)
    if is_help_key(&key) {
        state.show_help = !state.show_help;
        return Ok(false);
    }

    // When help is shown, only handle close keys
    if state.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => state.show_help = false,
            _ => {}
        }
        return Ok(false);
    }

    if state.step != AddStep::Confirm {
        state.confirm_error = None;
    }

    match state.step {
        AddStep::ModeSelect => {
            let changed = handle_mode_select_event(state, key)?;
            if changed {
                update_branch_rows(state, input);
                filter_branch_rows(state);
            }
            Ok(false)
        }
        AddStep::Branch => handle_branch_step_event(state, input, key),
        AddStep::NewBaseSelect => handle_new_base_select_event(state, input, key),
        AddStep::NewCommitInput => handle_commit_input_event(state, input, key),
        AddStep::NewBranchName => handle_branch_name_event(state, input, key),
        AddStep::Path => handle_path_event(state, input, key),
        AddStep::Confirm => handle_confirm_event(state, input, key),
    }
}

fn handle_mode_select_event(state: &mut AddUiState, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Up => {
            state.branch_tab = BranchTab::New;
            return Ok(true);
        }
        KeyCode::Down => {
            state.branch_tab = BranchTab::Existing;
            return Ok(true);
        }
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => match c {
            'p' => {
                state.branch_tab = BranchTab::New;
                return Ok(true);
            }
            'n' => {
                state.branch_tab = BranchTab::Existing;
                return Ok(true);
            }
            _ => {}
        },
        KeyCode::Enter => {
            state.step = AddStep::Branch;
            state.branch_purpose = BranchPurpose::UseExisting;
            return Ok(true);
        }
        _ => {}
    }
    Ok(false)
}

fn handle_branch_step_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    let search_enabled =
        !(state.branch_tab == BranchTab::New && state.branch_purpose == BranchPurpose::UseExisting);

    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Tab => {
            state.step = AddStep::Path;
        }
        KeyCode::BackTab => {
            state.branch_query.clear();
            filter_branch_rows(state);
            state.step = AddStep::ModeSelect;
        }
        KeyCode::Up => state.move_branch_up(),
        KeyCode::Down => state.move_branch_down(),
        KeyCode::Home => {
            if search_enabled {
                state.branch_query.move_to_start();
            }
        }
        KeyCode::End => {
            if search_enabled {
                state.branch_query.move_to_end();
            }
        }
        KeyCode::Left => {
            if search_enabled {
                state.branch_query.move_left();
            }
        }
        KeyCode::Right => {
            if search_enabled {
                state.branch_query.move_right();
            }
        }
        KeyCode::Backspace => {
            if search_enabled {
                state.branch_query.backspace();
                filter_branch_rows(state);
            }
        }
        KeyCode::Enter => {
            if let Some(row) = state.current_branch_row().cloned() {
                match row {
                    BranchRow::Action(action) => {
                        state.branch_tab = BranchTab::New;
                        state.branch_purpose = BranchPurpose::NewBase;
                        state.step = match action {
                            NewBranchAction::BaseBranch => AddStep::NewBaseSelect,
                            NewBranchAction::Commit => AddStep::NewCommitInput,
                        };
                        update_branch_rows(state, input);
                        filter_branch_rows(state);
                    }
                    BranchRow::Existing(item) => {
                        if item.in_use_by.is_none() {
                            state.selected_branch = Some(item);
                            if state.branch_tab == BranchTab::Existing {
                                apply_path_suggestion(state, input);
                                state.step = AddStep::Path;
                            } else {
                                state.branch_purpose = BranchPurpose::NewBase;
                                state.base_branch =
                                    state.selected_branch.as_ref().map(|b| b.name.clone());
                                state.new_branch_origin = Some(NewBranchOrigin::Base);
                                state.step = AddStep::NewBranchName;
                            }
                        }
                    }
                    BranchRow::Header(_) => {}
                }
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'p' => state.move_branch_up(),
                    'n' => state.move_branch_down(),
                    'a' => {
                        if search_enabled {
                            state.branch_query.move_to_start();
                        }
                    }
                    'e' => {
                        if search_enabled {
                            state.branch_query.move_to_end();
                        }
                    }
                    'f' => {
                        if search_enabled {
                            state.branch_query.move_right();
                        }
                    }
                    'b' => {
                        if search_enabled {
                            state.branch_query.move_left();
                        }
                    }
                    'd' => {
                        if search_enabled {
                            state.branch_query.delete_char();
                            filter_branch_rows(state);
                        }
                    }
                    'k' => {
                        if search_enabled {
                            state.branch_query.kill_to_end();
                            filter_branch_rows(state);
                        }
                    }
                    'u' => {
                        if search_enabled {
                            state.branch_query.clear();
                            filter_branch_rows(state);
                        }
                    }
                    'w' => {
                        if search_enabled {
                            state.branch_query.delete_word_backward();
                            filter_branch_rows(state);
                        }
                    }
                    _ => {}
                }
            } else if search_enabled {
                state.branch_query.insert_char(c);
                filter_branch_rows(state);
            }
        }
        _ => {}
    }

    if state.branch_tab == BranchTab::Existing {
        refresh_branch_preview(state, input);
    }

    Ok(false)
}

fn handle_new_base_select_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Tab => state.step = AddStep::NewBranchName,
        KeyCode::BackTab => {
            state.base_branch = None;
            state.new_branch_origin = None;
            state.branch_tab = BranchTab::New;
            state.branch_purpose = BranchPurpose::UseExisting;
            update_branch_rows(state, input);
            filter_branch_rows(state);
            state.step = AddStep::Branch;
        }
        KeyCode::Up => state.move_branch_up(),
        KeyCode::Down => state.move_branch_down(),
        KeyCode::Home => state.branch_query.move_to_start(),
        KeyCode::End => state.branch_query.move_to_end(),
        KeyCode::Left => state.branch_query.move_left(),
        KeyCode::Right => state.branch_query.move_right(),
        KeyCode::Backspace => {
            state.branch_query.backspace();
            filter_branch_rows(state);
        }
        KeyCode::Enter => {
            if let Some(BranchRow::Existing(item)) = state.current_branch_row().cloned() {
                state.base_branch = Some(item.name.clone());
                state.new_branch_origin = Some(NewBranchOrigin::Base);
                state.branch_name_input = TextInputState::new(String::new());
                if let Some(suggest) = input.suggest_branch_name.as_ref() {
                    let name = (suggest)(&item.name);
                    state.branch_name_input = TextInputState::new(name);
                }
                update_branch_name_validation(state, input);
                state.step = AddStep::NewBranchName;
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'p' => state.move_branch_up(),
                    'n' => state.move_branch_down(),
                    'a' => state.branch_query.move_to_start(),
                    'e' => state.branch_query.move_to_end(),
                    'f' => state.branch_query.move_right(),
                    'b' => state.branch_query.move_left(),
                    'd' => {
                        state.branch_query.delete_char();
                        filter_branch_rows(state);
                    }
                    'k' => {
                        state.branch_query.kill_to_end();
                        filter_branch_rows(state);
                    }
                    'u' => {
                        state.branch_query.clear();
                        filter_branch_rows(state);
                    }
                    'w' => {
                        state.branch_query.delete_word_backward();
                        filter_branch_rows(state);
                    }
                    _ => {}
                }
            } else {
                state.branch_query.insert_char(c);
                filter_branch_rows(state);
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_commit_input_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Tab => {
            if validate_commit_input(state) {
                state.new_branch_origin = Some(NewBranchOrigin::Commit);
                state.step = AddStep::NewBranchName;
            }
        }
        KeyCode::BackTab => {
            state.commit_input.clear();
            state.commit_error = None;
            state.new_branch_origin = None;
            state.branch_tab = BranchTab::New;
            state.branch_purpose = BranchPurpose::UseExisting;
            update_branch_rows(state, input);
            filter_branch_rows(state);
            state.step = AddStep::Branch;
        }
        KeyCode::Home => state.commit_input.move_to_start(),
        KeyCode::End => state.commit_input.move_to_end(),
        KeyCode::Left => state.commit_input.move_left(),
        KeyCode::Right => state.commit_input.move_right(),
        KeyCode::Backspace => {
            state.commit_input.backspace();
            state.commit_error = None;
        }
        KeyCode::Enter => {
            if validate_commit_input(state) {
                state.new_branch_origin = Some(NewBranchOrigin::Commit);
                state.step = AddStep::NewBranchName;
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'u' => {
                        state.commit_input.clear();
                        state.commit_error = None;
                    }
                    'a' => state.commit_input.move_to_start(),
                    'e' => state.commit_input.move_to_end(),
                    'f' => state.commit_input.move_right(),
                    'b' => state.commit_input.move_left(),
                    'd' => {
                        state.commit_input.delete_char();
                        state.commit_error = None;
                    }
                    'k' => {
                        state.commit_input.kill_to_end();
                        state.commit_error = None;
                    }
                    'w' => {
                        state.commit_input.delete_word_backward();
                        state.commit_error = None;
                    }
                    _ => {}
                }
            } else {
                state.commit_input.insert_char(c);
                state.commit_error = None;
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_branch_name_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Tab => {
            update_branch_name_validation(state, input);
            apply_path_suggestion(state, input);
            state.step = AddStep::Path;
        }
        KeyCode::BackTab => {
            state.branch_name_input.clear();
            state.branch_name_error = None;
            state.step = match state.new_branch_origin {
                Some(NewBranchOrigin::Commit) => AddStep::NewCommitInput,
                Some(NewBranchOrigin::Base) => AddStep::NewBaseSelect,
                None => AddStep::Branch,
            }
        }
        KeyCode::Home => state.branch_name_input.move_to_start(),
        KeyCode::End => state.branch_name_input.move_to_end(),
        KeyCode::Left => state.branch_name_input.move_left(),
        KeyCode::Right => state.branch_name_input.move_right(),
        KeyCode::Backspace => {
            state.branch_name_input.backspace();
            update_branch_name_validation(state, input);
        }
        KeyCode::Enter => {
            update_branch_name_validation(state, input);
            if state.branch_name_error.is_none() {
                apply_path_suggestion(state, input);
                state.step = AddStep::Path;
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'u' => {
                        state.branch_name_input.clear();
                        update_branch_name_validation(state, input);
                    }
                    'a' => state.branch_name_input.move_to_start(),
                    'e' => state.branch_name_input.move_to_end(),
                    'f' => state.branch_name_input.move_right(),
                    'b' => state.branch_name_input.move_left(),
                    'd' => {
                        state.branch_name_input.delete_char();
                        update_branch_name_validation(state, input);
                    }
                    'k' => {
                        state.branch_name_input.kill_to_end();
                        update_branch_name_validation(state, input);
                    }
                    'w' => {
                        state.branch_name_input.delete_word_backward();
                        update_branch_name_validation(state, input);
                    }
                    _ => {}
                }
            } else {
                state.branch_name_input.insert_char(c);
                update_branch_name_validation(state, input);
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_path_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Tab => state.step = AddStep::Confirm,
        KeyCode::BackTab => {
            state.path_input.clear();
            state.step = if state.branch_tab == BranchTab::New {
                AddStep::NewBranchName
            } else {
                AddStep::Branch
            }
        }
        KeyCode::Home => state.path_input.move_to_start(),
        KeyCode::End => state.path_input.move_to_end(),
        KeyCode::Left => state.path_input.move_left(),
        KeyCode::Right => state.path_input.move_right(),
        KeyCode::Backspace => state.path_input.backspace(),
        KeyCode::Enter => {
            let resolved = resolved_worktree_path(input, &state.path_input.value);
            let path_error = path_validation_error(&state.path_input.value);
            let fs_error = path_fs_error(resolved.as_ref());
            let used = worktree_exists(resolved.as_ref(), input);
            if path_error.is_none() && fs_error.is_none() && !used {
                state.step = AddStep::Confirm;
            }
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'u' => state.path_input.clear(),
                    'a' => state.path_input.move_to_start(),
                    'e' => state.path_input.move_to_end(),
                    'f' => state.path_input.move_right(),
                    'b' => state.path_input.move_left(),
                    'd' => state.path_input.delete_char(),
                    'k' => state.path_input.kill_to_end(),
                    'w' => state.path_input.delete_word_backward(),
                    _ => {}
                }
            } else {
                state.path_input.insert_char(c);
            }
        }
        _ => {}
    }

    Ok(false)
}

fn handle_confirm_event(
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Esc => return Err(Error::Aborted),
        KeyCode::Enter => {
            if let Some(message) = validate_confirm_state(state) {
                state.confirm_error = Some(message);
                return Ok(false);
            }
            let validation = validate_path(&state.path_input.value);
            let resolved = resolved_worktree_path(input, &state.path_input.value);
            let path_error = path_validation_error(&state.path_input.value);
            let fs_error = path_fs_error(resolved.as_ref());
            let used = worktree_exists(resolved.as_ref(), input);
            if validation == PathValidation::Ok
                && path_error.is_none()
                && fs_error.is_none()
                && !used
                && state.branch_name_error.is_none()
            {
                state.confirm_error = None;
                return Ok(true);
            }
        }
        KeyCode::BackTab => {
            state.confirm_error = None;
            state.step = AddStep::Path;
        }
        _ => {}
    }
    Ok(false)
}

fn update_branch_name_validation(state: &mut AddUiState, input: &AddInteractiveInput) {
    let name = state.branch_name_input.value.trim();
    if name.is_empty() {
        state.branch_name_error = Some("Branch name is required".to_string());
        return;
    }
    if input.local_branches.iter().any(|b| b == name) {
        state.branch_name_error = Some("Branch already exists".to_string());
        return;
    }
    match (input.validate_branch_name)(name) {
        Ok(None) => state.branch_name_error = None,
        Ok(Some(message)) => state.branch_name_error = Some(message),
        Err(err) => state.branch_name_error = Some(err.to_string()),
    }
}

fn draw_add_ui(
    frame: &mut ratatui::Frame<'_>,
    state: &mut AddUiState,
    input: &AddInteractiveInput,
) {
    let layout = UiLayout::new(frame.area(), input.theme);

    layout.draw_header(frame, "Add", &build_breadcrumb(state), build_context(state));

    match state.step {
        AddStep::ModeSelect => draw_mode_select(frame, state, input, layout.body),
        AddStep::Branch => draw_branch_step(frame, state, input, layout.body),
        AddStep::NewBaseSelect => draw_branch_step(frame, state, input, layout.body),
        AddStep::NewCommitInput => draw_commit_step(frame, state, input, layout.body),
        AddStep::NewBranchName => draw_branch_name_step(frame, state, input, layout.body),
        AddStep::Path => draw_path_step(frame, state, input, layout.body),
        AddStep::Confirm => draw_confirm_step(frame, state, input, layout.body),
    }

    layout.draw_footer(frame, get_footer_hints(state));
    layout.draw_help_modal(frame, state.show_help);
}

/// Build context string for header (Branch: xxx / Path: yyy)
fn build_context(state: &AddUiState) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(branch) = &state.selected_branch {
        parts.push(format!("Branch: {}", branch.name));
    } else if let Some(base) = &state.base_branch {
        parts.push(format!("Branch: {base}"));
    } else if !state.branch_name_input.value.is_empty() {
        parts.push(format!("Branch: {}", state.branch_name_input.value));
    }
    if !state.path_input.value.is_empty() {
        parts.push(format!("Path: {}", state.path_input.value));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" / "))
    }
}

/// Get key hints for footer based on current step
fn get_footer_hints(state: &AddUiState) -> &'static str {
    match state.step {
        AddStep::ModeSelect => "[Enter] select  [Up/Down/Ctrl+P/N] move  [Esc] cancel  [F1] help",
        AddStep::Branch => {
            let search_enabled = !(state.branch_tab == BranchTab::New
                && state.branch_purpose == BranchPurpose::UseExisting);
            if search_enabled {
                "[Enter] select  [Up/Down/Ctrl+P/N] move  type: search  [Tab] next  [Shift+Tab] back  [Esc] cancel  [F1] help"
            } else {
                "[Enter] select  [Up/Down/Ctrl+P/N] move  [Tab] next  [Shift+Tab] back  [Esc] cancel  [F1] help"
            }
        }
        AddStep::NewBaseSelect => {
            "[Enter] select  [Up/Down/Ctrl+P/N] move  type: search  [Tab] next  [Shift+Tab] back  [Esc] cancel  [F1] help"
        }
        AddStep::NewCommitInput | AddStep::NewBranchName | AddStep::Path => {
            "[Enter/Tab] next  [Shift+Tab] back  [Esc] cancel  [F1] help"
        }
        AddStep::Confirm => "[Enter] confirm  [Shift+Tab] back  [Esc] cancel  [F1] help",
    }
}

fn draw_mode_select(
    frame: &mut ratatui::Frame<'_>,
    state: &AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    let options = [
        "Create a new branch for this worktree".to_string(),
        "Use existing branch for this worktree".to_string(),
    ];
    let cursor = if state.branch_tab == BranchTab::New {
        0
    } else {
        1
    };

    let items = options
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let style = if idx == cursor {
                input.theme.selection_style()
            } else {
                input.theme.text_style()
            };
            ListItem::new(Line::from(item.clone())).style(style)
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(input.theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled(STEP_ACTION, input.theme.title_style())),
        )
        .style(input.theme.text_style());

    frame.render_widget(list, area);
}

fn draw_branch_step(
    frame: &mut ratatui::Frame<'_>,
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    let show_preview = state.step == AddStep::NewBaseSelect
        || (state.step == AddStep::Branch && state.branch_tab == BranchTab::Existing);
    let show_search =
        !(state.branch_tab == BranchTab::New && state.branch_purpose == BranchPurpose::UseExisting);

    let columns = if show_preview {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(100)])
            .split(area)
    };

    let left = if show_search {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(columns[0])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(columns[0])
    };

    if show_search {
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(input.theme.border_style())
            .padding(Padding::new(1, 1, 0, 0))
            .title(Span::styled("Search", input.theme.title_style()));
        let search_line = Line::from(vec![
            Span::styled("", input.theme.search_style()),
            Span::styled(
                &state.branch_query.value,
                input.theme.search_style().add_modifier(Modifier::BOLD),
            ),
        ]);
        frame.render_widget(Paragraph::new(search_line).block(search_block), left[0]);
    }

    let items = state
        .matches
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let text = row.display();
            let mut style = if idx == state.branch_cursor {
                input.theme.selection_style()
            } else {
                input.theme.text_style()
            };
            if !row.is_selectable() {
                style = input.theme.disabled_style();
            }
            ListItem::new(Line::from(text)).style(style)
        })
        .collect::<Vec<_>>();

    let selectable_count = state
        .matches
        .iter()
        .filter(|row| row.is_selectable())
        .count();
    let total_selectable = state
        .branch_rows
        .iter()
        .filter(|row| row.is_selectable())
        .count();
    let title = if state.step == AddStep::NewBaseSelect {
        format!("{} ({}/{})", STEP_BASE, selectable_count, total_selectable)
    } else {
        format!(
            "{} ({}/{})",
            STEP_BRANCH, selectable_count, total_selectable
        )
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(input.theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled(title, input.theme.title_style())),
        )
        .style(input.theme.text_style());
    let list_area = if show_search { left[1] } else { left[0] };
    frame.render_widget(list, list_area);

    if show_preview && columns.len() > 1 {
        draw_branch_preview(frame, state, input, columns[1]);
    }

    if show_search {
        set_search_cursor(frame, left[0], &state.branch_query);
    }
}

fn draw_branch_preview(
    frame: &mut ratatui::Frame<'_>,
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    refresh_branch_preview(state, input);

    let header = if let Some(branch) = &state.preview_branch {
        format!("History ({})", branch)
    } else {
        "History".to_string()
    };

    let mut lines = Vec::new();
    if let Some(branch) = &state.preview_branch {
        lines.push(Line::from(Span::styled(
            branch.clone(),
            input.theme.label_style(),
        )));
    }
    if !state.preview_log.is_empty() {
        lines.push(Line::from(Span::raw("")));
        for line in &state.preview_log {
            lines.push(Line::from(Span::styled(
                line.clone(),
                input.theme.preview_style(),
            )));
        }
    }

    let preview = Paragraph::new(lines)
        .style(input.theme.preview_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(input.theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled(header, input.theme.title_style())),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(preview, area);
}

#[cfg(all(test, feature = "impure-test"))]
fn draw_text_step(
    frame: &mut ratatui::Frame<'_>,
    input: &AddInteractiveInput,
    title: &str,
    text: &TextInputState,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(input.theme.border_style())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled(title, input.theme.title_style()));
    let line = Line::from(vec![
        Span::styled(format!("{title}: "), input.theme.label_style()),
        Span::styled(&text.value, input.theme.text_style()),
    ]);
    frame.render_widget(Paragraph::new(line).block(block), area);
    set_input_cursor(frame, area, &format!("{title}: "), text);
}

fn draw_commit_step(
    frame: &mut ratatui::Frame<'_>,
    state: &AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(input.theme.border_style())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled(STEP_COMMIT, input.theme.title_style()));
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Commit: ", input.theme.label_style()),
        Span::styled(&state.commit_input.value, input.theme.text_style()),
    ]));
    if let Some(error) = &state.commit_error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            input.theme.error_style(),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
    set_input_cursor(frame, area, "Commit: ", &state.commit_input);
}

fn draw_branch_name_step(
    frame: &mut ratatui::Frame<'_>,
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(input.theme.border_style())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled(STEP_BRANCH_NAME, input.theme.title_style()));

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Branch: ", input.theme.label_style()),
        Span::styled(&state.branch_name_input.value, input.theme.text_style()),
    ]));

    if let Some(error) = &state.branch_name_error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            input.theme.error_style(),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
    set_input_cursor(frame, area, "Branch: ", &state.branch_name_input);
}

fn draw_path_step(
    frame: &mut ratatui::Frame<'_>,
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(input.theme.border_style())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled(STEP_WORKTREE_PATH, input.theme.title_style()));

    let path_value = state.path_input.value.clone();
    let resolved = resolved_worktree_path(input, &path_value);
    let validation = path_validation_error(&path_value);
    let fs_error = path_fs_error(resolved.as_ref());
    let worktree_warning =
        worktree_info_warning(validate_path(&path_value), resolved.as_ref(), input);

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Path: ", input.theme.label_style()),
        Span::styled(&path_value, input.theme.text_style()),
    ]));

    if let Some(resolved) = resolved.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("Full Path: ", input.theme.label_style()),
            Span::styled(resolved.display().to_string(), input.theme.text_style()),
        ]));
    }

    if let Some(message) = validation.as_ref().or(fs_error.as_ref()) {
        lines.push(Line::from(Span::styled(
            format!("Validation: {message}"),
            input.theme.error_style(),
        )));
    }

    if let Some(warn) = worktree_warning {
        lines.push(Line::from(Span::styled(
            format!("Worktree Info: {warn}"),
            input.theme.warning_style(),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
    set_input_cursor(frame, area, "Path: ", &state.path_input);
}

fn draw_confirm_step(
    frame: &mut ratatui::Frame<'_>,
    state: &mut AddUiState,
    input: &AddInteractiveInput,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(input.theme.border_style())
        .padding(Padding::new(1, 1, 0, 0))
        .title(Span::styled(STEP_CONFIRM, input.theme.title_style()));

    let branch = if !state.branch_name_input.value.is_empty() {
        state.branch_name_input.value.clone()
    } else if let Some(selected) = state.selected_branch.as_ref() {
        selected.name.clone()
    } else if let Some(base) = state.base_branch.as_ref() {
        base.clone()
    } else {
        String::new()
    };

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Press Enter to confirm",
        input.theme.label_style(),
    )));
    if let Some(message) = &state.confirm_error {
        lines.push(Line::from(Span::styled(
            message.clone(),
            input.theme.error_style(),
        )));
    }
    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        format!("Branch: {branch}"),
        input.theme.text_style(),
    )));
    lines.push(Line::from(Span::styled(
        format!("Worktree Path: {}", state.path_input.value),
        input.theme.text_style(),
    )));

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn apply_path_suggestion(state: &mut AddUiState, input: &AddInteractiveInput) {
    if !state.path_input.value.is_empty() {
        return;
    }

    let branch = if !state.branch_name_input.value.is_empty() {
        state.branch_name_input.value.clone()
    } else if let Some(selected) = state.selected_branch.as_ref() {
        selected.name.clone()
    } else if let Some(base) = state.base_branch.as_ref() {
        base.clone()
    } else {
        String::new()
    };

    if branch.is_empty() {
        return;
    }

    if let Some(suggest) = input.suggest_path.as_ref()
        && let Some(path) = (suggest)(&branch)
    {
        state.path_input = TextInputState::new(path);
    }
}

fn validate_commit_input(state: &mut AddUiState) -> bool {
    if state.commit_input.value.trim().is_empty() {
        state.commit_error = Some("Commit is required".to_string());
        return false;
    }
    state.commit_error = None;
    true
}

fn validate_confirm_state(state: &AddUiState) -> Option<String> {
    if state.branch_tab == BranchTab::Existing {
        if state.selected_branch.is_none() {
            return Some("Select a branch".to_string());
        }
        return None;
    }

    if state.new_branch_origin.is_none() {
        return Some("Select a base branch or commit".to_string());
    }

    if state.branch_name_input.value.trim().is_empty() {
        return Some("Branch name is required".to_string());
    }

    if let Some(message) = state.branch_name_error.as_ref() {
        return Some(message.clone());
    }

    None
}

fn validate_path(path: &str) -> PathValidation {
    if path.is_empty() {
        return PathValidation::Empty;
    }
    PathValidation::Ok
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathValidation {
    Ok,
    Empty,
}

fn path_validation_label(validation: PathValidation) -> String {
    match validation {
        PathValidation::Ok => "OK".to_string(),
        PathValidation::Empty => "Path is required".to_string(),
    }
}

fn path_validation_error(path: &str) -> Option<String> {
    match validate_path(path) {
        PathValidation::Ok => None,
        other => Some(path_validation_label(other)),
    }
}

fn resolved_worktree_path(input: &AddInteractiveInput, path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        Some(normalize_path(&path_buf))
    } else {
        // Use current_dir as base, matching git worktree add behavior
        Some(normalize_path(&input.current_dir.join(path_buf)))
    }
}

fn worktree_info_warning(
    validation: PathValidation,
    resolved: Option<&PathBuf>,
    input: &AddInteractiveInput,
) -> Option<String> {
    if validation != PathValidation::Ok {
        return None;
    }
    let path = resolved?;
    if let Some(existing) = input
        .existing_worktrees
        .iter()
        .find(|entry| entry.path == *path)
    {
        if let Some(branch) = existing.branch.as_deref() {
            return Some(format!("Worktree already exists (branch: {branch})"));
        }
        return Some("Worktree already exists".to_string());
    }
    None
}

fn worktree_exists(resolved: Option<&PathBuf>, input: &AddInteractiveInput) -> bool {
    resolved
        .map(|path| {
            input
                .existing_worktrees
                .iter()
                .any(|entry| entry.path == *path)
        })
        .unwrap_or(false)
}

fn path_fs_error(resolved: Option<&PathBuf>) -> Option<String> {
    let path = resolved?;
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                match fs::read_dir(path) {
                    Ok(mut entries) => {
                        if entries.next().is_some() {
                            return Some("Directory is not empty".to_string());
                        }
                    }
                    Err(_) => {
                        return Some("Failed to read directory".to_string());
                    }
                }
            } else {
                return Some("Path exists and is not a directory".to_string());
            }
        }
        Err(err) => {
            if err.kind() != io::ErrorKind::NotFound {
                return Some("Failed to access path".to_string());
            }
        }
    }
    None
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if let Some(last) = parts.last() {
                    if matches!(
                        last,
                        std::path::Component::RootDir | std::path::Component::Prefix(_)
                    ) {
                        continue;
                    }
                    parts.pop();
                }
            }
            std::path::Component::Prefix(_) => {
                parts.clear();
                parts.push(component);
            }
            std::path::Component::RootDir => {
                // On Windows, RootDir follows Prefix (e.g. C:\), so preserve
                // the already-pushed Prefix instead of clearing.
                if !parts
                    .last()
                    .is_some_and(|c| matches!(c, std::path::Component::Prefix(_)))
                {
                    parts.clear();
                }
                parts.push(component);
            }
            other => parts.push(other),
        }
    }
    let mut normalized = PathBuf::new();
    for part in parts {
        normalized.push(part.as_os_str());
    }
    normalized
}

fn refresh_branch_preview(state: &mut AddUiState, input: &AddInteractiveInput) {
    if state.branch_tab != BranchTab::Existing && state.step != AddStep::NewBaseSelect {
        state.preview_branch = None;
        state.preview_log.clear();
        return;
    }

    let branch = match state.current_branch_row() {
        Some(BranchRow::Existing(item)) => item.name.clone(),
        _ => {
            state.preview_branch = None;
            state.preview_log.clear();
            return;
        }
    };

    if state.preview_branch.as_deref() == Some(&branch) {
        return;
    }

    if let Some(cached) = state.preview_cache.get(&branch) {
        state.preview_log = cached.clone();
        state.preview_branch = Some(branch);
        return;
    }

    let log = (input.fetch_log)(&branch, input.log_limit).unwrap_or_default();
    state.preview_cache.insert(branch.clone(), log.clone());
    state.preview_log = log;
    state.preview_branch = Some(branch);
}

fn set_input_cursor(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    prefix: &str,
    input: &TextInputState,
) {
    let padding_left = 1u16;
    let padding_top = 0u16;
    // Calculate display width of prefix
    let prefix_width = prefix.width();
    // Calculate display width of text before cursor
    let text_before_cursor: String = input.value.chars().take(input.cursor).collect();
    let text_width = text_before_cursor.width();
    let x_offset = prefix_width + text_width;
    let x = area
        .x
        .saturating_add(1)
        .saturating_add(padding_left)
        .saturating_add(x_offset as u16);
    let y = area.y.saturating_add(1).saturating_add(padding_top);
    frame.set_cursor_position((x, y));
}

#[cfg(all(test, feature = "impure-test"))]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_input() -> AddInteractiveInput {
        AddInteractiveInput {
            local_branches: vec!["main".to_string(), "feature/test".to_string()],
            remote_branches: vec!["origin/main".to_string()],
            used_branches: HashMap::new(),
            current_dir: PathBuf::from("/tmp/repo"),
            existing_worktrees: vec![],
            log_limit: 5,
            fetch_log: Arc::new(|_, _| Ok(vec![])),
            initial_path: None,
            suggest_path: None,
            suggest_branch_name: None,
            validate_branch_name: Arc::new(|_| Ok(None)),
            theme: UiTheme::default(),
        }
    }

    fn create_key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    // TextInputState tests
    #[test]
    fn test_text_input_state_new() {
        let state = TextInputState::new("hello".to_string());
        assert_eq!(state.value, "hello");
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn test_text_input_state_new_empty() {
        let state = TextInputState::new(String::new());
        assert_eq!(state.value, "");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_text_input_state_insert_char() {
        let mut state = TextInputState::new(String::new());
        state.insert_char('a');
        assert_eq!(state.value, "a");
        assert_eq!(state.cursor, 1);

        state.insert_char('b');
        assert_eq!(state.value, "ab");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_text_input_state_insert_char_middle() {
        let mut state = TextInputState::new("ac".to_string());
        state.cursor = 1;
        state.insert_char('b');
        assert_eq!(state.value, "abc");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_text_input_state_backspace() {
        let mut state = TextInputState::new("abc".to_string());
        state.backspace();
        assert_eq!(state.value, "ab");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_text_input_state_backspace_empty() {
        let mut state = TextInputState::new(String::new());
        state.backspace();
        assert_eq!(state.value, "");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_text_input_state_backspace_middle() {
        let mut state = TextInputState::new("abc".to_string());
        state.cursor = 2;
        state.backspace();
        assert_eq!(state.value, "ac");
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn test_text_input_state_move_left() {
        let mut state = TextInputState::new("abc".to_string());
        assert_eq!(state.cursor, 3);

        state.move_left();
        assert_eq!(state.cursor, 2);

        state.move_left();
        assert_eq!(state.cursor, 1);

        state.move_left();
        assert_eq!(state.cursor, 0);

        // Should not go below 0
        state.move_left();
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_text_input_state_move_right() {
        let mut state = TextInputState::new("abc".to_string());
        state.cursor = 0;

        state.move_right();
        assert_eq!(state.cursor, 1);

        state.move_right();
        assert_eq!(state.cursor, 2);

        state.move_right();
        assert_eq!(state.cursor, 3);

        // Should not exceed length
        state.move_right();
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn test_text_input_state_clear() {
        let mut state = TextInputState::new("hello world".to_string());
        state.clear();
        assert_eq!(state.value, "");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_text_input_state_byte_index() {
        let state = TextInputState::new("hello".to_string());
        assert_eq!(state.byte_index(0), 0);
        assert_eq!(state.byte_index(2), 2);
        assert_eq!(state.byte_index(5), 5);
    }

    #[test]
    fn test_text_input_state_byte_index_unicode() {
        let state = TextInputState::new("日本語".to_string());
        assert_eq!(state.byte_index(0), 0);
        assert_eq!(state.byte_index(1), 3); // Each Japanese char is 3 bytes
        assert_eq!(state.byte_index(2), 6);
    }

    // BranchRow tests
    #[test]
    fn test_branch_row_is_selectable_action() {
        let row = BranchRow::Action(NewBranchAction::BaseBranch);
        assert!(row.is_selectable());
    }

    #[test]
    fn test_branch_row_is_selectable_header() {
        let row = BranchRow::Header("-- Local --".to_string());
        assert!(!row.is_selectable());
    }

    #[test]
    fn test_branch_row_is_selectable_existing_available() {
        let row = BranchRow::Existing(BranchItem {
            name: "main".to_string(),
            in_use_by: None,
        });
        assert!(row.is_selectable());
    }

    #[test]
    fn test_branch_row_is_selectable_existing_in_use() {
        let row = BranchRow::Existing(BranchItem {
            name: "main".to_string(),
            in_use_by: Some(PathBuf::from("/tmp/worktree")),
        });
        assert!(!row.is_selectable());
    }

    #[test]
    fn test_branch_row_display_action_base() {
        let row = BranchRow::Action(NewBranchAction::BaseBranch);
        assert_eq!(row.display(), "+ Select base branch");
    }

    #[test]
    fn test_branch_row_display_action_commit() {
        let row = BranchRow::Action(NewBranchAction::Commit);
        assert_eq!(row.display(), "+ New branch from commit");
    }

    #[test]
    fn test_branch_row_display_header() {
        let row = BranchRow::Header("-- Remote --".to_string());
        assert_eq!(row.display(), "-- Remote --");
    }

    #[test]
    fn test_branch_row_display_existing() {
        let row = BranchRow::Existing(BranchItem {
            name: "feature/test".to_string(),
            in_use_by: None,
        });
        assert_eq!(row.display(), "feature/test");
    }

    #[test]
    fn test_branch_row_display_existing_in_use() {
        let row = BranchRow::Existing(BranchItem {
            name: "main".to_string(),
            in_use_by: Some(PathBuf::from("/tmp/worktree")),
        });
        assert_eq!(row.display(), "main  [in use: /tmp/worktree]");
    }

    // AddUiState tests
    #[test]
    fn test_add_ui_state_new() {
        let input = create_test_input();
        let state = AddUiState::new(&input);

        assert_eq!(state.step, AddStep::ModeSelect);
        assert_eq!(state.branch_tab, BranchTab::New);
        assert_eq!(state.branch_cursor, 0);
        assert!(state.branch_query.value.is_empty());
        assert!(state.selected_branch.is_none());
    }

    #[test]
    fn test_add_ui_state_new_with_initial_path() {
        let mut input = create_test_input();
        input.initial_path = Some(PathBuf::from("worktrees/test"));
        let state = AddUiState::new(&input);

        assert_eq!(state.path_input.value, "worktrees/test");
    }

    #[test]
    fn test_add_ui_state_current_branch_row() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.matches = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Existing(BranchItem {
                name: "feature".to_string(),
                in_use_by: None,
            }),
        ];

        state.branch_cursor = 0;
        assert!(matches!(
            state.current_branch_row(),
            Some(BranchRow::Existing(item)) if item.name == "main"
        ));

        state.branch_cursor = 1;
        assert!(matches!(
            state.current_branch_row(),
            Some(BranchRow::Existing(item)) if item.name == "feature"
        ));
    }

    #[test]
    fn test_add_ui_state_move_branch_up() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.matches = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Existing(BranchItem {
                name: "feature".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_cursor = 1;

        state.move_branch_up();
        assert_eq!(state.branch_cursor, 0);

        // Should not go below 0
        state.move_branch_up();
        assert_eq!(state.branch_cursor, 0);
    }

    #[test]
    fn test_add_ui_state_move_branch_up_skips_headers() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.matches = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Header("-- Remote --".to_string()),
            BranchRow::Existing(BranchItem {
                name: "origin/main".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_cursor = 2;

        state.move_branch_up();
        // Should skip header and go to index 0
        assert_eq!(state.branch_cursor, 0);
    }

    #[test]
    fn test_add_ui_state_move_branch_down() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.matches = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Existing(BranchItem {
                name: "feature".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_cursor = 0;

        state.move_branch_down();
        assert_eq!(state.branch_cursor, 1);

        // Should not exceed length
        state.move_branch_down();
        assert_eq!(state.branch_cursor, 1);
    }

    #[test]
    fn test_add_ui_state_move_branch_down_skips_headers() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.matches = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Header("-- Remote --".to_string()),
            BranchRow::Existing(BranchItem {
                name: "origin/main".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_cursor = 0;

        state.move_branch_down();
        // Should skip header and go to index 2
        assert_eq!(state.branch_cursor, 2);
    }

    #[test]
    fn test_add_ui_state_move_branch_to_next_selectable() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.matches = vec![
            BranchRow::Header("-- Local --".to_string()),
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_cursor = 0;

        state.move_branch_to_next_selectable();
        assert_eq!(state.branch_cursor, 1);
    }

    // Path validation tests
    #[test]
    fn test_validate_path_ok() {
        assert_eq!(validate_path("worktrees/test"), PathValidation::Ok);
    }

    #[test]
    fn test_validate_path_empty() {
        assert_eq!(validate_path(""), PathValidation::Empty);
    }

    #[test]
    fn test_path_validation_label_ok() {
        assert_eq!(path_validation_label(PathValidation::Ok), "OK");
    }

    #[test]
    fn test_path_validation_label_empty() {
        assert_eq!(
            path_validation_label(PathValidation::Empty),
            "Path is required"
        );
    }

    #[test]
    fn test_path_validation_error_ok() {
        assert!(path_validation_error("some/path").is_none());
    }

    #[test]
    fn test_path_validation_error_empty() {
        assert_eq!(
            path_validation_error(""),
            Some("Path is required".to_string())
        );
    }

    // normalize_path tests
    #[test]
    fn test_normalize_path_simple() {
        let path = PathBuf::from("/home/user/project");
        assert_eq!(normalize_path(&path), PathBuf::from("/home/user/project"));
    }

    #[test]
    fn test_normalize_path_with_dot() {
        let path = PathBuf::from("/home/./user/./project");
        assert_eq!(normalize_path(&path), PathBuf::from("/home/user/project"));
    }

    #[test]
    fn test_normalize_path_with_parent() {
        let path = PathBuf::from("/home/user/../user/project");
        assert_eq!(normalize_path(&path), PathBuf::from("/home/user/project"));
    }

    #[test]
    fn test_normalize_path_complex() {
        let path = PathBuf::from("/home/user/foo/../bar/./baz");
        assert_eq!(normalize_path(&path), PathBuf::from("/home/user/bar/baz"));
    }

    #[test]
    fn test_normalize_path_parent_over_root() {
        let path = PathBuf::from("/../../tmp");
        assert_eq!(normalize_path(&path), PathBuf::from("/tmp"));
    }

    // resolved_worktree_path tests
    #[test]
    fn test_resolved_worktree_path_empty() {
        let input = create_test_input();
        assert!(resolved_worktree_path(&input, "").is_none());
    }

    #[test]
    fn test_resolved_worktree_path_relative() {
        let input = create_test_input();
        let result = resolved_worktree_path(&input, "worktrees/test");
        // Relative paths are resolved against current_dir (matching git worktree behavior)
        assert_eq!(result, Some(PathBuf::from("/tmp/repo/worktrees/test")));
    }

    #[test]
    fn test_resolved_worktree_path_relative_uses_current_dir() {
        // Test that relative paths are resolved against current_dir, not repo_root
        let mut input = create_test_input();
        // Set current_dir to a subdirectory of repo
        input.current_dir = PathBuf::from("/tmp/repo/subdir");
        let result = resolved_worktree_path(&input, "worktrees/test");
        // Should resolve against current_dir
        assert_eq!(
            result,
            Some(PathBuf::from("/tmp/repo/subdir/worktrees/test"))
        );
    }

    #[test]
    fn test_resolved_worktree_path_absolute() {
        let input = create_test_input();
        let result = resolved_worktree_path(&input, "/absolute/path");
        assert_eq!(result, Some(PathBuf::from("/absolute/path")));
    }

    // path_fs_error tests
    #[test]
    fn test_path_fs_error_empty_dir() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_path_buf();
        assert!(path_fs_error(Some(&path)).is_none());
    }

    #[test]
    fn test_path_fs_error_non_empty_dir() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();
        let path = dir.path().to_path_buf();
        assert_eq!(
            path_fs_error(Some(&path)),
            Some("Directory is not empty".to_string())
        );
    }

    #[test]
    fn test_path_fs_error_file_path() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();
        assert_eq!(
            path_fs_error(Some(&file_path)),
            Some("Path exists and is not a directory".to_string())
        );
    }

    // worktree_exists tests
    #[test]
    fn test_worktree_exists_false() {
        let input = create_test_input();
        let path = PathBuf::from("/tmp/new-worktree");
        assert!(!worktree_exists(Some(&path), &input));
    }

    #[test]
    fn test_worktree_exists_true() {
        let mut input = create_test_input();
        input.existing_worktrees = vec![WorktreeSummary {
            path: PathBuf::from("/tmp/existing"),
            branch: Some("main".to_string()),
        }];
        let path = PathBuf::from("/tmp/existing");
        assert!(worktree_exists(Some(&path), &input));
    }

    #[test]
    fn test_worktree_exists_none_path() {
        let input = create_test_input();
        assert!(!worktree_exists(None, &input));
    }

    // worktree_info_warning tests
    #[test]
    fn test_worktree_info_warning_none_when_invalid() {
        let input = create_test_input();
        let path = PathBuf::from("/tmp/test");
        assert!(worktree_info_warning(PathValidation::Empty, Some(&path), &input).is_none());
    }

    #[test]
    fn test_worktree_info_warning_none_when_no_path() {
        let input = create_test_input();
        assert!(worktree_info_warning(PathValidation::Ok, None, &input).is_none());
    }

    #[test]
    fn test_worktree_info_warning_exists_with_branch() {
        let mut input = create_test_input();
        input.existing_worktrees = vec![WorktreeSummary {
            path: PathBuf::from("/tmp/existing"),
            branch: Some("feature".to_string()),
        }];
        let path = PathBuf::from("/tmp/existing");
        let result = worktree_info_warning(PathValidation::Ok, Some(&path), &input);
        assert_eq!(
            result,
            Some("Worktree already exists (branch: feature)".to_string())
        );
    }

    #[test]
    fn test_worktree_info_warning_exists_without_branch() {
        let mut input = create_test_input();
        input.existing_worktrees = vec![WorktreeSummary {
            path: PathBuf::from("/tmp/existing"),
            branch: None,
        }];
        let path = PathBuf::from("/tmp/existing");
        let result = worktree_info_warning(PathValidation::Ok, Some(&path), &input);
        assert_eq!(result, Some("Worktree already exists".to_string()));
    }

    // build_branch_choice tests
    #[test]
    fn test_build_branch_choice_existing_branch() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_tab = BranchTab::Existing;
        state.selected_branch = Some(BranchItem {
            name: "feature/test".to_string(),
            in_use_by: None,
        });

        let result = build_branch_choice(&state).unwrap();
        assert_eq!(result.branch, "feature/test");
        assert!(!result.create_new);
        assert!(result.base_commitish.is_none());
    }

    #[test]
    fn test_build_branch_choice_new_branch_from_base() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_tab = BranchTab::New;
        state.base_branch = Some("main".to_string());
        state.new_branch_origin = Some(NewBranchOrigin::Base);
        state.branch_name_input = TextInputState::new("feature/new".to_string());

        let result = build_branch_choice(&state).unwrap();
        assert_eq!(result.branch, "feature/new");
        assert!(result.create_new);
        assert_eq!(result.base_commitish, Some("main".to_string()));
    }

    #[test]
    fn test_build_branch_choice_new_branch_from_commit() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_tab = BranchTab::New;
        state.base_branch = Some("main".to_string());
        state.new_branch_origin = Some(NewBranchOrigin::Commit);
        state.commit_input = TextInputState::new("abc123".to_string());
        state.branch_name_input = TextInputState::new("feature/from-commit".to_string());

        let result = build_branch_choice(&state).unwrap();
        assert_eq!(result.branch, "feature/from-commit");
        assert!(result.create_new);
        assert_eq!(result.base_commitish, Some("abc123".to_string()));
    }

    #[test]
    fn test_build_branch_choice_aborted_no_selection() {
        let input = create_test_input();
        let state = AddUiState::new(&input);

        let result = build_branch_choice(&state);
        assert!(result.is_err());
    }

    // filter_branch_rows tests
    #[test]
    fn test_filter_branch_rows_empty_query() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_rows = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Existing(BranchItem {
                name: "feature".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_query = TextInputState::new(String::new());

        filter_branch_rows(&mut state);

        assert_eq!(state.matches.len(), 2);
    }

    #[test]
    fn test_filter_branch_rows_with_query() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_rows = vec![
            BranchRow::Existing(BranchItem {
                name: "main".to_string(),
                in_use_by: None,
            }),
            BranchRow::Existing(BranchItem {
                name: "feature".to_string(),
                in_use_by: None,
            }),
        ];
        state.branch_query = TextInputState::new("feat".to_string());

        filter_branch_rows(&mut state);

        assert_eq!(state.matches.len(), 1);
        assert!(matches!(
            &state.matches[0],
            BranchRow::Existing(item) if item.name == "feature"
        ));
    }

    #[test]
    fn test_filter_branch_rows_case_insensitive() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_rows = vec![BranchRow::Existing(BranchItem {
            name: "Feature/Test".to_string(),
            in_use_by: None,
        })];
        state.branch_query = TextInputState::new("FEATURE".to_string());

        filter_branch_rows(&mut state);

        assert_eq!(state.matches.len(), 1);
    }

    // Event handler tests
    #[test]
    fn test_handle_mode_select_event_escape() {
        let mut state = AddUiState::new(&create_test_input());
        state.step = AddStep::ModeSelect;

        let key = create_key_event(KeyCode::Esc, KeyModifiers::NONE);
        let result = handle_mode_select_event(&mut state, key);

        assert!(result.is_err());
    }

    #[test]
    fn test_handle_mode_select_event_up() {
        let mut state = AddUiState::new(&create_test_input());
        state.step = AddStep::ModeSelect;
        state.branch_tab = BranchTab::Existing;

        let key = create_key_event(KeyCode::Up, KeyModifiers::NONE);
        let result = handle_mode_select_event(&mut state, key).unwrap();

        assert!(result);
        assert_eq!(state.branch_tab, BranchTab::New);
    }

    #[test]
    fn test_handle_mode_select_event_down() {
        let mut state = AddUiState::new(&create_test_input());
        state.step = AddStep::ModeSelect;
        state.branch_tab = BranchTab::New;

        let key = create_key_event(KeyCode::Down, KeyModifiers::NONE);
        let result = handle_mode_select_event(&mut state, key).unwrap();

        assert!(result);
        assert_eq!(state.branch_tab, BranchTab::Existing);
    }

    #[test]
    fn test_handle_mode_select_event_enter() {
        let mut state = AddUiState::new(&create_test_input());
        state.step = AddStep::ModeSelect;

        let key = create_key_event(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_mode_select_event(&mut state, key).unwrap();

        assert!(result);
        assert_eq!(state.step, AddStep::Branch);
    }

    #[test]
    fn test_handle_mode_select_event_ctrl_p() {
        let mut state = AddUiState::new(&create_test_input());
        state.branch_tab = BranchTab::Existing;

        let key = create_key_event(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let result = handle_mode_select_event(&mut state, key).unwrap();

        assert!(result);
        assert_eq!(state.branch_tab, BranchTab::New);
    }

    #[test]
    fn test_handle_mode_select_event_ctrl_n() {
        let mut state = AddUiState::new(&create_test_input());
        state.branch_tab = BranchTab::New;

        let key = create_key_event(KeyCode::Char('n'), KeyModifiers::CONTROL);
        let result = handle_mode_select_event(&mut state, key).unwrap();

        assert!(result);
        assert_eq!(state.branch_tab, BranchTab::Existing);
    }

    #[test]
    fn test_handle_add_event_ctrl_c_aborts() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);

        let key = create_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = handle_add_event(&mut state, &input, key);

        assert!(result.is_err());
    }

    #[test]
    fn test_handle_commit_input_event_char_input() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewCommitInput;

        let key = create_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        handle_commit_input_event(&mut state, &input, key).unwrap();

        assert_eq!(state.commit_input.value, "a");
    }

    #[test]
    fn test_handle_commit_input_event_ctrl_u_clears() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewCommitInput;
        state.commit_input = TextInputState::new("abc123".to_string());

        let key = create_key_event(KeyCode::Char('u'), KeyModifiers::CONTROL);
        handle_commit_input_event(&mut state, &input, key).unwrap();

        assert_eq!(state.commit_input.value, "");
    }

    #[test]
    fn test_handle_commit_input_event_arrow_keys() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.commit_input = TextInputState::new("abc".to_string());
        assert_eq!(state.commit_input.cursor, 3);

        let key = create_key_event(KeyCode::Left, KeyModifiers::NONE);
        handle_commit_input_event(&mut state, &input, key).unwrap();
        assert_eq!(state.commit_input.cursor, 2);

        let key = create_key_event(KeyCode::Right, KeyModifiers::NONE);
        handle_commit_input_event(&mut state, &input, key).unwrap();
        assert_eq!(state.commit_input.cursor, 3);
    }

    #[test]
    fn test_handle_commit_input_event_enter_empty_sets_error() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewCommitInput;
        state.commit_input = TextInputState::new(String::new());

        let key = create_key_event(KeyCode::Enter, KeyModifiers::NONE);
        handle_commit_input_event(&mut state, &input, key).unwrap();

        assert_eq!(state.step, AddStep::NewCommitInput);
        assert_eq!(state.commit_error, Some("Commit is required".to_string()));
    }

    #[test]
    fn test_handle_commit_input_event_tab_empty_sets_error() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewCommitInput;
        state.commit_input = TextInputState::new(String::new());

        let key = create_key_event(KeyCode::Tab, KeyModifiers::NONE);
        handle_commit_input_event(&mut state, &input, key).unwrap();

        assert_eq!(state.step, AddStep::NewCommitInput);
        assert_eq!(state.commit_error, Some("Commit is required".to_string()));
    }

    #[test]
    fn test_handle_path_event_char_input() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::Path;
        state.path_input = TextInputState::new(String::new());

        let key = create_key_event(KeyCode::Char('t'), KeyModifiers::NONE);
        handle_path_event(&mut state, &input, key).unwrap();

        assert_eq!(state.path_input.value, "t");
    }

    #[test]
    fn test_handle_path_event_ctrl_u_clears() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::Path;
        state.path_input = TextInputState::new("worktrees/test".to_string());

        let key = create_key_event(KeyCode::Char('u'), KeyModifiers::CONTROL);
        handle_path_event(&mut state, &input, key).unwrap();

        assert_eq!(state.path_input.value, "");
    }

    #[test]
    fn test_handle_path_event_tab_goes_to_confirm() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::Path;

        let key = create_key_event(KeyCode::Tab, KeyModifiers::NONE);
        handle_path_event(&mut state, &input, key).unwrap();

        assert_eq!(state.step, AddStep::Confirm);
    }

    #[test]
    fn test_handle_confirm_event_missing_branch_sets_error() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::Confirm;
        state.branch_tab = BranchTab::Existing;
        state.path_input = TextInputState::new("worktrees/test".to_string());

        let key = create_key_event(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_confirm_event(&mut state, &input, key).unwrap();

        assert!(!result);
        assert_eq!(state.confirm_error, Some("Select a branch".to_string()));
    }

    // Rendering tests with TestBackend
    #[test]
    fn test_draw_mode_select_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let state = AddUiState::new(&input);

        terminal
            .draw(|frame| {
                draw_mode_select(frame, &state, &input, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_header_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let state = AddUiState::new(&input);

        terminal
            .draw(|frame| {
                let layout = UiLayout::new(frame.area(), input.theme);
                layout.draw_header(
                    frame,
                    "Add",
                    &build_breadcrumb(&state),
                    build_context(&state),
                );
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_header_with_selection() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.selected_branch = Some(BranchItem {
            name: "feature/test".to_string(),
            in_use_by: None,
        });
        state.path_input = TextInputState::new("worktrees/test".to_string());

        terminal
            .draw(|frame| {
                let layout = UiLayout::new(frame.area(), input.theme);
                layout.draw_header(
                    frame,
                    "Add",
                    &build_breadcrumb(&state),
                    build_context(&state),
                );
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_text_step_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let text = TextInputState::new("abc123".to_string());

        terminal
            .draw(|frame| {
                draw_text_step(frame, &input, STEP_COMMIT, &text, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_branch_name_step_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_name_input = TextInputState::new("feature/new".to_string());

        terminal
            .draw(|frame| {
                draw_branch_name_step(frame, &mut state, &input, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_branch_name_step_with_error() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_name_input = TextInputState::new("main".to_string());
        state.branch_name_error = Some("Branch already exists".to_string());

        terminal
            .draw(|frame| {
                draw_branch_name_step(frame, &mut state, &input, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_path_step_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.path_input = TextInputState::new("worktrees/test".to_string());

        terminal
            .draw(|frame| {
                draw_path_step(frame, &mut state, &input, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_confirm_step_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.selected_branch = Some(BranchItem {
            name: "feature/test".to_string(),
            in_use_by: None,
        });
        state.path_input = TextInputState::new("worktrees/test".to_string());

        terminal
            .draw(|frame| {
                draw_confirm_step(frame, &mut state, &input, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_add_ui_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        update_branch_rows(&mut state, &input);
        filter_branch_rows(&mut state);

        terminal
            .draw(|frame| {
                draw_add_ui(frame, &mut state, &input);
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_branch_step_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::Branch;
        state.branch_tab = BranchTab::Existing;
        update_branch_rows(&mut state, &input);
        filter_branch_rows(&mut state);

        terminal
            .draw(|frame| {
                draw_branch_step(frame, &mut state, &input, frame.area());
            })
            .unwrap();

        assert!(terminal.backend().buffer().area.width > 0);
    }

    // apply_path_suggestion tests
    #[test]
    fn test_apply_path_suggestion_already_has_path() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.path_input = TextInputState::new("existing/path".to_string());
        state.selected_branch = Some(BranchItem {
            name: "main".to_string(),
            in_use_by: None,
        });

        apply_path_suggestion(&mut state, &input);

        // Should not change existing path
        assert_eq!(state.path_input.value, "existing/path");
    }

    #[test]
    fn test_apply_path_suggestion_no_branch() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.path_input = TextInputState::new(String::new());

        apply_path_suggestion(&mut state, &input);

        // Should remain empty with no branch selected
        assert_eq!(state.path_input.value, "");
    }

    #[test]
    fn test_apply_path_suggestion_with_suggester() {
        let mut input = create_test_input();
        input.suggest_path = Some(Arc::new(|branch: &str| {
            Some(format!("worktrees/{}", branch.replace('/', "-")))
        }));
        let mut state = AddUiState::new(&input);
        state.path_input = TextInputState::new(String::new());
        state.selected_branch = Some(BranchItem {
            name: "feature/test".to_string(),
            in_use_by: None,
        });

        apply_path_suggestion(&mut state, &input);

        assert_eq!(state.path_input.value, "worktrees/feature-test");
    }

    #[test]
    fn test_apply_path_suggestion_new_branch_created_from_base() {
        // Scenario: base branch is "foo", and user creates new branch "foo-bar"
        // Path suggestion should use the new branch name, not the base branch
        let mut input = create_test_input();
        input.suggest_path = Some(Arc::new(|branch: &str| {
            Some(format!("worktrees/{}", branch))
        }));
        let mut state = AddUiState::new(&input);
        state.path_input = TextInputState::new(String::new());
        state.base_branch = Some("foo".to_string()); // Base branch
        state.branch_name_input = TextInputState::new("foo-bar".to_string()); // New branch name

        apply_path_suggestion(&mut state, &input);

        // Should use the new branch name "foo-bar", not base branch "foo"
        assert_eq!(state.path_input.value, "worktrees/foo-bar");
    }

    // update_branch_rows tests
    #[test]
    fn test_update_branch_rows_existing_tab() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_tab = BranchTab::Existing;
        state.branch_purpose = BranchPurpose::UseExisting;

        update_branch_rows(&mut state, &input);

        assert_eq!(state.branch_rows.len(), 2); // main, feature/test
        assert!(matches!(&state.branch_rows[0], BranchRow::Existing(_)));
    }

    #[test]
    fn test_update_branch_rows_new_tab_use_existing() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_tab = BranchTab::New;
        state.branch_purpose = BranchPurpose::UseExisting;

        update_branch_rows(&mut state, &input);

        assert_eq!(state.branch_rows.len(), 2); // Two action rows
        assert!(matches!(
            &state.branch_rows[0],
            BranchRow::Action(NewBranchAction::BaseBranch)
        ));
        assert!(matches!(
            &state.branch_rows[1],
            BranchRow::Action(NewBranchAction::Commit)
        ));
    }

    #[test]
    fn test_update_branch_rows_new_base_purpose() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.branch_tab = BranchTab::New;
        state.branch_purpose = BranchPurpose::NewBase;

        update_branch_rows(&mut state, &input);

        // Should have headers and branches
        assert!(state.branch_rows.len() > 2);
        assert!(matches!(&state.branch_rows[0], BranchRow::Header(_)));
    }

    // BackTab state cleanup tests
    #[test]
    fn test_backtab_from_new_base_select_clears_state() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewBaseSelect;
        state.base_branch = Some("main".to_string());
        state.new_branch_origin = Some(NewBranchOrigin::Base);

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let _ = handle_new_base_select_event(&mut state, &input, key);

        assert_eq!(state.step, AddStep::Branch);
        assert!(state.base_branch.is_none());
        assert!(state.new_branch_origin.is_none());
    }

    #[test]
    fn test_backtab_from_commit_input_clears_state() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewCommitInput;
        state.commit_input = TextInputState::new("abc1234".to_string());
        state.new_branch_origin = Some(NewBranchOrigin::Commit);

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let _ = handle_commit_input_event(&mut state, &input, key);

        assert_eq!(state.step, AddStep::Branch);
        assert_eq!(state.commit_input.value, "");
        assert_eq!(state.commit_input.cursor, 0);
        assert!(state.new_branch_origin.is_none());
    }

    #[test]
    fn test_backtab_from_branch_name_to_base_clears_state() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewBranchName;
        state.branch_name_input = TextInputState::new("my-branch".to_string());
        state.branch_name_error = Some("error".to_string());
        state.new_branch_origin = Some(NewBranchOrigin::Base);

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let _ = handle_branch_name_event(&mut state, &input, key);

        assert_eq!(state.step, AddStep::NewBaseSelect);
        assert_eq!(state.branch_name_input.value, "");
        assert_eq!(state.branch_name_input.cursor, 0);
        assert!(state.branch_name_error.is_none());
    }

    #[test]
    fn test_backtab_from_branch_name_to_commit_clears_state() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewBranchName;
        state.branch_name_input = TextInputState::new("my-branch".to_string());
        state.branch_name_error = Some("error".to_string());
        state.new_branch_origin = Some(NewBranchOrigin::Commit);

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let _ = handle_branch_name_event(&mut state, &input, key);

        assert_eq!(state.step, AddStep::NewCommitInput);
        assert_eq!(state.branch_name_input.value, "");
        assert!(state.branch_name_error.is_none());
    }

    #[test]
    fn test_backtab_from_branch_name_to_branch_clears_state() {
        let input = create_test_input();
        let mut state = AddUiState::new(&input);
        state.step = AddStep::NewBranchName;
        state.branch_name_input = TextInputState::new("my-branch".to_string());
        state.branch_name_error = Some("error".to_string());
        state.new_branch_origin = None;

        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let _ = handle_branch_name_event(&mut state, &input, key);

        assert_eq!(state.step, AddStep::Branch);
        assert_eq!(state.branch_name_input.value, "");
        assert!(state.branch_name_error.is_none());
    }

    fn make_input(value: &str, cursor: usize) -> TextInputState {
        TextInputState {
            value: value.to_string(),
            cursor,
        }
    }

    #[test]
    fn test_move_to_start() {
        let mut s = make_input("hello", 3);
        s.move_to_start();
        assert_eq!(s.cursor, 0);
        assert_eq!(s.value, "hello");
    }

    #[test]
    fn test_move_to_end() {
        let mut s = make_input("hello", 2);
        s.move_to_end();
        assert_eq!(s.cursor, 5);
        assert_eq!(s.value, "hello");
    }

    #[test]
    fn test_kill_to_end_mid() {
        let mut s = make_input("hello world", 5);
        s.kill_to_end();
        assert_eq!(s.value, "hello");
        assert_eq!(s.cursor, 5);
    }

    #[test]
    fn test_kill_to_end_at_end() {
        let mut s = make_input("hello", 5);
        s.kill_to_end();
        assert_eq!(s.value, "hello");
        assert_eq!(s.cursor, 5);
    }

    #[test]
    fn test_delete_word_backward_simple() {
        let mut s = make_input("foo bar", 7);
        s.delete_word_backward();
        assert_eq!(s.value, "foo ");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn test_delete_word_backward_trailing_space() {
        let mut s = make_input("foo bar  ", 9);
        s.delete_word_backward();
        assert_eq!(s.value, "foo ");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn test_delete_word_backward_at_start() {
        let mut s = make_input("foo", 0);
        s.delete_word_backward();
        assert_eq!(s.value, "foo");
        assert_eq!(s.cursor, 0);
    }
}
