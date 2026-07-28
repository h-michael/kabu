use crate::error::{Error, Result};
use crate::{config, vcs};

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use unicode_width::UnicodeWidthStr;

use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

mod add;
mod conflict;
mod path;
mod remove;
mod select;
mod worktree_list;

pub(crate) use add::{AddInteractiveInput, WorktreeSummary, run_add_interactive};
pub(crate) use conflict::{ConflictChoice, prompt_conflict};
pub(crate) use path::run_path_interactive;
pub(crate) use remove::{SafetyWarning, run_remove_confirmation, run_remove_selection};

#[derive(Debug, Clone)]
struct TextInputState {
    value: String,
    cursor: usize,
}

impl TextInputState {
    fn new(initial: String) -> Self {
        let cursor = initial.chars().count();
        Self {
            value: initial,
            cursor,
        }
    }

    fn insert_char(&mut self, c: char) {
        let idx = self.byte_index(self.cursor);
        self.value.insert(idx, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let idx = self.byte_index(self.cursor - 1);
        self.value.remove(idx);
        self.cursor -= 1;
    }

    fn delete_char(&mut self) {
        let len = self.value.chars().count();
        if self.cursor >= len {
            return;
        }
        let idx = self.byte_index(self.cursor);
        self.value.remove(idx);
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_right(&mut self) {
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    fn move_to_end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    fn kill_to_end(&mut self) {
        let idx = self.byte_index(self.cursor);
        self.value.truncate(idx);
    }

    fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.value.chars().collect();
        let mut pos = self.cursor;
        while pos > 0 && chars[pos - 1] == ' ' {
            pos -= 1;
        }
        while pos > 0 && chars[pos - 1] != ' ' {
            pos -= 1;
        }
        let start_byte = self.byte_index(pos);
        let end_byte = self.byte_index(self.cursor);
        self.value.drain(start_byte..end_byte);
        self.cursor = pos;
    }

    fn byte_index(&self, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        self.value
            .char_indices()
            .nth(char_index)
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| self.value.len())
    }

    fn text_before_cursor(&self) -> &str {
        let idx = self.byte_index(self.cursor);
        &self.value[..idx]
    }
}

fn set_search_cursor(frame: &mut ratatui::Frame<'_>, area: Rect, input: &TextInputState) {
    let padding_left = 1u16;
    let padding_top = 0u16;
    let x_offset = input.text_before_cursor().width();
    let x = area
        .x
        .saturating_add(1)
        .saturating_add(padding_left)
        .saturating_add(x_offset as u16);
    let y = area.y.saturating_add(1).saturating_add(padding_top);
    frame.set_cursor_position((x, y));
}

// Shared step name constants for breadcrumb navigation
const STEP_ACTION: &str = "Choose action";
const STEP_BRANCH: &str = "Select branch";
const STEP_BASE: &str = "Branch from";
const STEP_COMMIT: &str = "Commit hash";
const STEP_BRANCH_NAME: &str = "Branch name";
const STEP_WORKTREE_PATH: &str = "Worktree path";
const STEP_SELECT_WORKTREE: &str = "Select worktrees";
const STEP_CONFIRM: &str = "Confirm";
const STEP_CONFLICT: &str = "Resolve conflict";

// Shared layout constants
const HEADER_HEIGHT: u16 = 1;
const FOOTER_HEIGHT: u16 = 1;
const BODY_PADDING: u16 = 1;

/// Calculate footer height based on theme settings (key hints at bottom)
fn footer_height(theme: UiTheme) -> u16 {
    if theme.show_key_hints {
        FOOTER_HEIGHT
    } else {
        0
    }
}

/// Pre-computed layout areas for interactive UIs
pub(crate) struct UiLayout {
    pub header: Rect,
    pub body: Rect,
    footer: Option<Rect>,
    theme: UiTheme,
}

impl UiLayout {
    /// Compute layout areas from frame size
    pub fn new(size: Rect, theme: UiTheme) -> Self {
        let footer_h = footer_height(theme);
        let body_height = size
            .height
            .saturating_sub(HEADER_HEIGHT + BODY_PADDING + footer_h);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(HEADER_HEIGHT),
                Constraint::Length(BODY_PADDING),
                Constraint::Length(body_height),
                Constraint::Length(footer_h),
            ])
            .split(size);

        Self {
            header: chunks[0],
            body: chunks[2],
            footer: if footer_h > 0 { Some(chunks[3]) } else { None },
            theme,
        }
    }

    /// Draw header with breadcrumb
    pub fn draw_header(
        &self,
        frame: &mut ratatui::Frame<'_>,
        command_name: &str,
        breadcrumbs: &[&str],
        context: Option<String>,
    ) {
        let line = render_breadcrumb_line(command_name, breadcrumbs, context, self.theme);
        frame.render_widget(Paragraph::new(vec![line]), self.header);
    }

    /// Draw footer with key hints (if enabled)
    pub fn draw_footer(&self, frame: &mut ratatui::Frame<'_>, key_hints: &str) {
        if let Some(area) = self.footer {
            let line = Line::from(Span::styled(key_hints, self.theme.footer_style()));
            frame.render_widget(Paragraph::new(vec![line]), area);
        }
    }

    /// Draw help modal overlay
    pub fn draw_help_modal(&self, frame: &mut ratatui::Frame<'_>, show: bool) {
        if show {
            draw_help_modal(frame, self.theme);
        }
    }
}

/// Renders a breadcrumb line for the header.
///
/// Format: `[command_name] crumb1 > crumb2 > crumb3 | context`
/// The command name has bold + background color, the last crumb is highlighted with accent color.
/// Optional context is displayed after a separator on the right side.
fn render_breadcrumb_line<'a>(
    command_name: &'a str,
    breadcrumbs: &[&'a str],
    context: Option<String>,
    theme: UiTheme,
) -> ratatui::text::Line<'a> {
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};

    let mut spans = vec![
        Span::styled(
            format!(" {command_name} "),
            Style::default()
                .fg(theme.selection_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", theme.muted_style()),
    ];

    for (i, crumb) in breadcrumbs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" > ", theme.muted_style()));
        }
        if i == breadcrumbs.len() - 1 {
            spans.push(Span::styled(*crumb, theme.accent_style()));
        } else {
            spans.push(Span::styled(*crumb, theme.header_style()));
        }
    }

    if let Some(ctx) = context {
        spans.push(Span::styled(" | ", theme.muted_style()));
        spans.push(Span::styled(ctx, theme.label_style()));
    }

    Line::from(spans)
}

pub(crate) fn resolve_ui_theme() -> Result<UiTheme> {
    let provider = vcs::get_provider()?;
    let repo_root = provider.main_workspace_root()?;
    let config = config::load(&repo_root)?.unwrap_or_default();
    Ok(UiTheme::from_ui(&config.ui))
}

/// Reads the next key event from the terminal, filtering out non-key events.
///
/// Returns `Ok(Some(key))` when a key press or repeat event is received,
/// `Ok(None)` on timeout, or `Err` on I/O failure.
///
/// # Filtering behavior
/// - `KeyEventKind::Press` and `KeyEventKind::Repeat`: Returned to caller
/// - `KeyEventKind::Release`: Filtered out (a single key press produces both Press and Release
///   events; processing both would trigger duplicate actions)
/// - Non-key events (mouse, resize, etc.): Filtered out
///
/// # Note
/// This function is not unit-tested because it depends on terminal I/O. The filtering
/// logic is intentionally simple to minimize the risk of bugs.
pub(crate) fn read_key_event(timeout: Duration) -> Result<Option<KeyEvent>> {
    let mut poll_timeout = timeout;
    loop {
        if !event::poll(poll_timeout).map_err(|e| Error::Selector {
            message: format!("Failed to read input: {e}"),
        })? {
            return Ok(None);
        }
        let event = event::read().map_err(|e| Error::Selector {
            message: format!("Failed to read input: {e}"),
        })?;
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Release {
                // After filtering an event, poll immediately for the next one without
                // resetting the overall timeout (avoids blocking on filtered events).
                poll_timeout = Duration::from_millis(0);
                continue;
            }
            return Ok(Some(key));
        }
        // Non-key event received; continue polling immediately.
        poll_timeout = Duration::from_millis(0);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct UiTheme {
    border: Color,
    text: Color,
    accent: Color,
    header: Color,
    footer: Color,
    title: Color,
    label: Color,
    muted: Color,
    disabled: Color,
    search: Color,
    preview: Color,
    selection_bg: Color,
    selection_fg: Color,
    warning: Color,
    error: Color,
    pub show_key_hints: bool,
    pub add_default_mode: config::AddDefaultMode,
}

impl UiTheme {
    pub(crate) fn from_ui(ui: &config::Ui) -> Self {
        let mut theme = UiTheme::default();
        if let Some(color) = ui.colors.border {
            theme.border = map_ui_color(color);
        }
        if let Some(color) = ui.colors.text {
            theme.text = map_ui_color(color);
        }
        if let Some(color) = ui.colors.accent {
            theme.accent = map_ui_color(color);
        }
        if let Some(color) = ui.colors.header {
            theme.header = map_ui_color(color);
        }
        if let Some(color) = ui.colors.footer {
            theme.footer = map_ui_color(color);
        }
        if let Some(color) = ui.colors.title {
            theme.title = map_ui_color(color);
        }
        if let Some(color) = ui.colors.label {
            theme.label = map_ui_color(color);
        }
        if let Some(color) = ui.colors.muted {
            theme.muted = map_ui_color(color);
        }
        if let Some(color) = ui.colors.disabled {
            theme.disabled = map_ui_color(color);
        }
        if let Some(color) = ui.colors.search {
            theme.search = map_ui_color(color);
        }
        if let Some(color) = ui.colors.preview {
            theme.preview = map_ui_color(color);
        }
        if let Some(color) = ui.colors.selection_bg {
            theme.selection_bg = map_ui_color(color);
        }
        if let Some(color) = ui.colors.selection_fg {
            theme.selection_fg = map_ui_color(color);
        }
        if let Some(color) = ui.colors.warning {
            theme.warning = map_ui_color(color);
        }
        if let Some(color) = ui.colors.error {
            theme.error = map_ui_color(color);
        }
        theme.show_key_hints = ui.show_key_hints();
        theme.add_default_mode = ui.add_default_mode();
        theme
    }

    fn text_style(self) -> Style {
        Style::default().fg(self.text)
    }

    fn header_style(self) -> Style {
        Style::default().fg(self.header)
    }

    fn footer_style(self) -> Style {
        Style::default().fg(self.footer)
    }

    fn title_style(self) -> Style {
        Style::default().fg(self.title)
    }

    fn label_style(self) -> Style {
        Style::default().fg(self.label)
    }

    fn disabled_style(self) -> Style {
        Style::default().fg(self.disabled)
    }

    fn muted_style(self) -> Style {
        Style::default().fg(self.muted)
    }

    fn accent_style(self) -> Style {
        Style::default().fg(self.accent)
    }

    fn search_style(self) -> Style {
        Style::default().fg(self.search)
    }

    fn preview_style(self) -> Style {
        Style::default().fg(self.preview)
    }

    fn border_style(self) -> Style {
        Style::default().fg(self.border)
    }

    fn selection_style(self) -> Style {
        Style::default().fg(self.selection_fg).bg(self.selection_bg)
    }

    fn warning_style(self) -> Style {
        Style::default().fg(self.warning)
    }

    fn error_style(self) -> Style {
        Style::default().fg(self.error)
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            border: Color::DarkGray,
            text: Color::White,
            accent: Color::Cyan,
            header: Color::LightCyan,
            footer: Color::Gray,
            title: Color::LightBlue,
            label: Color::LightMagenta,
            muted: Color::Gray,
            disabled: Color::DarkGray,
            search: Color::LightGreen,
            preview: Color::White,
            selection_bg: Color::Blue,
            selection_fg: Color::Black,
            warning: Color::Yellow,
            error: Color::Red,
            show_key_hints: true,
            add_default_mode: config::AddDefaultMode::New,
        }
    }
}

fn map_ui_color(color: config::UiColor) -> Color {
    match color {
        config::UiColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
        config::UiColor::Named(name) => match name {
            config::UiColorName::Default => Color::Reset,
            config::UiColorName::Black => Color::Black,
            config::UiColorName::Red => Color::Red,
            config::UiColorName::Green => Color::Green,
            config::UiColorName::Yellow => Color::Yellow,
            config::UiColorName::Blue => Color::Blue,
            config::UiColorName::Magenta => Color::Magenta,
            config::UiColorName::Cyan => Color::Cyan,
            config::UiColorName::Gray => Color::Gray,
            config::UiColorName::DarkGray => Color::DarkGray,
            config::UiColorName::LightRed => Color::LightRed,
            config::UiColorName::LightGreen => Color::LightGreen,
            config::UiColorName::LightYellow => Color::LightYellow,
            config::UiColorName::LightBlue => Color::LightBlue,
            config::UiColorName::LightMagenta => Color::LightMagenta,
            config::UiColorName::LightCyan => Color::LightCyan,
            config::UiColorName::White => Color::White,
        },
    }
}

fn with_terminal<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&mut Terminal<CrosstermBackend<Box<dyn Write>>>) -> Result<T>,
{
    let mut guard = TerminalGuard::new()?;
    let result = f(guard.terminal_mut());
    let restore_result = guard.restore();
    if let (Err(_), Err(restore_err)) = (&result, &restore_result) {
        // Log restore failure when f() also failed (both errors occurred)
        eprintln!("Warning: Failed to restore terminal: {restore_err}");
    }
    // Propagate f()'s error if it failed, otherwise propagate restore error
    result.and_then(|v| restore_result.map(|()| v))
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Box<dyn Write>>>> {
    enable_raw_mode().map_err(|e| Error::Selector {
        message: format!("Failed to enable raw mode: {e}"),
    })?;

    let writer = select_writer().inspect_err(|_| {
        let _ = disable_raw_mode();
    })?;
    let backend = CrosstermBackend::new(writer);
    let mut terminal = Terminal::new(backend).map_err(|e| {
        let _ = disable_raw_mode();
        Error::Selector {
            message: format!("Failed to initialize terminal: {e}"),
        }
    })?;

    if let Err(err) = terminal.backend_mut().execute(EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(Error::Selector {
            message: format!("Failed to enter alternate screen: {err}"),
        });
    }

    if let Err(err) = terminal.clear() {
        let _ = terminal.backend_mut().execute(LeaveAlternateScreen);
        let _ = disable_raw_mode();
        return Err(Error::Selector {
            message: format!("Failed to clear terminal: {err}"),
        });
    }

    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Box<dyn Write>>>) -> Result<()> {
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .map_err(|e| Error::Selector {
            message: format!("Failed to leave alternate screen: {e}"),
        })?;
    disable_raw_mode().map_err(|e| Error::Selector {
        message: format!("Failed to disable raw mode: {e}"),
    })?;
    terminal.show_cursor().map_err(|e| Error::Selector {
        message: format!("Failed to restore cursor: {e}"),
    })?;
    Ok(())
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Box<dyn Write>>>,
    restored: bool,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        let terminal = setup_terminal()?;
        Ok(Self {
            terminal,
            restored: false,
        })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Box<dyn Write>>> {
        &mut self.terminal
    }

    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        restore_terminal(&mut self.terminal)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn select_writer() -> Result<Box<dyn Write>> {
    #[cfg(unix)]
    {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|e| {
                // Check raw errno for non-interactive conditions:
                // - ENOENT: /dev/tty doesn't exist
                // - ENXIO: No controlling terminal
                // - ENOTTY: Not a terminal device
                match e.raw_os_error() {
                    Some(libc::ENOENT | libc::ENXIO | libc::ENOTTY) => Error::NonInteractive,
                    _ => Error::Internal(format!("Failed to open /dev/tty: {e}")),
                }
            })?;
        Ok(Box::new(file))
    }
    #[cfg(windows)]
    {
        Ok(Box::new(std::io::stdout()))
    }
}

fn truncate_text_for_width(text: String, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let max = width as usize;
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut current_width = 0usize;
        for ch in line.chars() {
            // Skip control characters (width returns None for them)
            let Some(char_width) = unicode_width::UnicodeWidthChar::width(ch) else {
                continue;
            };
            if current_width + char_width > max {
                break;
            }
            out.push(ch);
            current_width += char_width;
        }
    }
    out
}

/// Check if key event is help toggle (F1)
fn is_help_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::F(1)
}

/// Draw help modal overlay
fn draw_help_modal(frame: &mut ratatui::Frame<'_>, theme: UiTheme) {
    let size = frame.area();

    let modal_width = std::cmp::min(58, size.width.saturating_sub(4));
    let modal_height = std::cmp::min(23, size.height.saturating_sub(4));
    let modal_x = (size.width.saturating_sub(modal_width)) / 2;
    let modal_y = (size.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    let lines = vec![
        Line::from(Span::styled("Navigation", theme.title_style())),
        Line::from(vec![
            Span::styled("  [Up/Ctrl+P]    ", theme.accent_style()),
            Span::raw("Move up"),
        ]),
        Line::from(vec![
            Span::styled("  [Down/Ctrl+N]  ", theme.accent_style()),
            Span::raw("Move down"),
        ]),
        Line::from(""),
        Line::from(Span::styled("Actions", theme.title_style())),
        Line::from(vec![
            Span::styled("  [Enter]        ", theme.accent_style()),
            Span::raw("Select / Confirm"),
        ]),
        Line::from(vec![
            Span::styled("  [Tab]          ", theme.accent_style()),
            Span::raw("Next step"),
        ]),
        Line::from(vec![
            Span::styled("  [Shift+Tab]    ", theme.accent_style()),
            Span::raw("Previous step"),
        ]),
        Line::from(vec![
            Span::styled("  [Space]        ", theme.accent_style()),
            Span::raw("Toggle selection"),
        ]),
        Line::from(vec![
            Span::styled("  [Esc]          ", theme.accent_style()),
            Span::raw("Cancel"),
        ]),
        Line::from(""),
        Line::from(Span::styled("Text Input", theme.title_style())),
        Line::from(vec![
            Span::styled("  [Left/Ctrl+F]  ", theme.accent_style()),
            Span::raw("Move cursor forward"),
        ]),
        Line::from(vec![
            Span::styled("  [Right/Ctrl+B] ", theme.accent_style()),
            Span::raw("Move cursor backward"),
        ]),
        Line::from(vec![
            Span::styled("  [Home/Ctrl+A]  ", theme.accent_style()),
            Span::raw("Move to line start"),
        ]),
        Line::from(vec![
            Span::styled("  [End/Ctrl+E]   ", theme.accent_style()),
            Span::raw("Move to line end"),
        ]),
        Line::from(vec![
            Span::styled("  [Ctrl+D]       ", theme.accent_style()),
            Span::raw("Delete character"),
        ]),
        Line::from(vec![
            Span::styled("  [Ctrl+K]       ", theme.accent_style()),
            Span::raw("Delete to end of line"),
        ]),
        Line::from(vec![
            Span::styled("  [Ctrl+U]       ", theme.accent_style()),
            Span::raw("Clear line"),
        ]),
        Line::from(vec![
            Span::styled("  [Ctrl+W]       ", theme.accent_style()),
            Span::raw("Delete word backward"),
        ]),
        Line::from(vec![
            Span::styled("  type           ", theme.accent_style()),
            Span::raw("Enter text / Search"),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style())
        .title(Span::styled(" Key Bindings ", theme.title_style()))
        .padding(Padding::new(1, 1, 0, 0));

    let paragraph = Paragraph::new(lines).style(theme.text_style()).block(block);

    frame.render_widget(Clear, modal_area);
    frame.render_widget(paragraph, modal_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{UiColor, UiColorName, UiColors};

    #[test]
    fn test_ui_theme_default() {
        let theme = UiTheme::default();
        assert_eq!(theme.border, Color::DarkGray);
        assert_eq!(theme.text, Color::White);
        assert_eq!(theme.accent, Color::Cyan);
        assert_eq!(theme.header, Color::LightCyan);
        assert_eq!(theme.footer, Color::Gray);
        assert_eq!(theme.title, Color::LightBlue);
        assert_eq!(theme.label, Color::LightMagenta);
        assert_eq!(theme.muted, Color::Gray);
        assert_eq!(theme.disabled, Color::DarkGray);
        assert_eq!(theme.search, Color::LightGreen);
        assert_eq!(theme.preview, Color::White);
        assert_eq!(theme.selection_bg, Color::Blue);
        assert_eq!(theme.selection_fg, Color::Black);
        assert_eq!(theme.warning, Color::Yellow);
        assert_eq!(theme.error, Color::Red);
    }

    #[test]
    fn test_ui_theme_from_ui_partial() {
        let ui = config::Ui {
            colors: UiColors {
                border: Some(UiColor::Named(UiColorName::Red)),
                accent: Some(UiColor::Rgb(255, 128, 0)),
                ..Default::default()
            },
            ..Default::default()
        };
        let theme = UiTheme::from_ui(&ui);

        assert_eq!(theme.border, Color::Red);
        assert_eq!(theme.accent, Color::Rgb(255, 128, 0));
        // Unset colors should use defaults
        assert_eq!(theme.text, Color::White);
        assert_eq!(theme.header, Color::LightCyan);
        // Unset UI options should use defaults
        assert!(theme.show_key_hints);
        assert_eq!(theme.add_default_mode, config::AddDefaultMode::New);
    }

    #[test]
    fn test_map_ui_color_named() {
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::Default)),
            Color::Reset
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::Black)),
            Color::Black
        );
        assert_eq!(map_ui_color(UiColor::Named(UiColorName::Red)), Color::Red);
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::Green)),
            Color::Green
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::Yellow)),
            Color::Yellow
        );
        assert_eq!(map_ui_color(UiColor::Named(UiColorName::Blue)), Color::Blue);
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::Magenta)),
            Color::Magenta
        );
        assert_eq!(map_ui_color(UiColor::Named(UiColorName::Cyan)), Color::Cyan);
        assert_eq!(map_ui_color(UiColor::Named(UiColorName::Gray)), Color::Gray);
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::DarkGray)),
            Color::DarkGray
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::LightRed)),
            Color::LightRed
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::LightGreen)),
            Color::LightGreen
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::LightYellow)),
            Color::LightYellow
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::LightBlue)),
            Color::LightBlue
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::LightMagenta)),
            Color::LightMagenta
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::LightCyan)),
            Color::LightCyan
        );
        assert_eq!(
            map_ui_color(UiColor::Named(UiColorName::White)),
            Color::White
        );
    }

    #[test]
    fn test_map_ui_color_rgb() {
        assert_eq!(map_ui_color(UiColor::Rgb(255, 0, 0)), Color::Rgb(255, 0, 0));
        assert_eq!(map_ui_color(UiColor::Rgb(0, 255, 0)), Color::Rgb(0, 255, 0));
        assert_eq!(map_ui_color(UiColor::Rgb(0, 0, 255)), Color::Rgb(0, 0, 255));
        assert_eq!(
            map_ui_color(UiColor::Rgb(128, 128, 128)),
            Color::Rgb(128, 128, 128)
        );
    }

    #[test]
    fn test_truncate_text_for_width_basic() {
        assert_eq!(truncate_text_for_width("hello".to_string(), 10), "hello");
        assert_eq!(
            truncate_text_for_width("hello world".to_string(), 5),
            "hello"
        );
        assert_eq!(truncate_text_for_width("hello".to_string(), 3), "hel");
    }

    #[test]
    fn test_truncate_text_for_width_zero() {
        assert_eq!(truncate_text_for_width("hello".to_string(), 0), "");
    }

    #[test]
    fn test_truncate_text_for_width_multiline() {
        let text = "line one\nline two\nline three".to_string();
        let result = truncate_text_for_width(text, 4);
        assert_eq!(result, "line\nline\nline");
    }

    #[test]
    fn test_truncate_text_for_width_unicode() {
        // CJK characters have display width of 2 each
        // width=6 can fit 3 CJK characters (3 * 2 = 6)
        assert_eq!(
            truncate_text_for_width("日本語テスト".to_string(), 6),
            "日本語"
        );
        // width=3 can only fit 1 CJK character (1 * 2 = 2, next would be 4 > 3)
        assert_eq!(truncate_text_for_width("日本語テスト".to_string(), 3), "日");
        // Greek letters have display width of 1 each (like ASCII)
        assert_eq!(truncate_text_for_width("αβγδε".to_string(), 3), "αβγ");
    }

    #[test]
    fn test_truncate_text_for_width_control_chars() {
        // Control characters (ESC, NUL, etc.) are stripped
        // Note: Only the control char itself is removed, not the entire ANSI sequence
        assert_eq!(truncate_text_for_width("a\x1bb".to_string(), 2), "ab");
        assert_eq!(truncate_text_for_width("a\x00b\x01c".to_string(), 3), "abc");
        // Bell and backspace stripped
        assert_eq!(truncate_text_for_width("a\x07b\x08c".to_string(), 3), "abc");
    }

    #[test]
    fn test_ui_theme_styles() {
        let theme = UiTheme::default();

        // Verify style methods return expected styles
        assert_eq!(theme.text_style().fg, Some(Color::White));
        assert_eq!(theme.header_style().fg, Some(Color::LightCyan));
        assert_eq!(theme.footer_style().fg, Some(Color::Gray));
        assert_eq!(theme.title_style().fg, Some(Color::LightBlue));
        assert_eq!(theme.label_style().fg, Some(Color::LightMagenta));
        assert_eq!(theme.disabled_style().fg, Some(Color::DarkGray));
        assert_eq!(theme.search_style().fg, Some(Color::LightGreen));
        assert_eq!(theme.preview_style().fg, Some(Color::White));
        assert_eq!(theme.border_style().fg, Some(Color::DarkGray));
        assert_eq!(theme.warning_style().fg, Some(Color::Yellow));
        assert_eq!(theme.error_style().fg, Some(Color::Red));

        // Selection style has both fg and bg
        let selection = theme.selection_style();
        assert_eq!(selection.fg, Some(Color::Black));
        assert_eq!(selection.bg, Some(Color::Blue));
    }

    #[test]
    fn test_ui_theme_default_show_key_hints() {
        let theme = UiTheme::default();
        assert!(theme.show_key_hints);
    }

    #[test]
    fn test_ui_theme_default_add_default_mode() {
        let theme = UiTheme::default();
        assert_eq!(theme.add_default_mode, config::AddDefaultMode::New);
    }

    #[test]
    fn test_ui_theme_from_ui_show_key_hints_false() {
        let ui = config::Ui {
            show_key_hints: Some(false),
            ..Default::default()
        };
        let theme = UiTheme::from_ui(&ui);
        assert!(!theme.show_key_hints);
    }

    #[test]
    fn test_ui_theme_from_ui_add_default_mode_new() {
        let ui = config::Ui {
            add_default_mode: Some(config::AddDefaultMode::New),
            ..Default::default()
        };
        let theme = UiTheme::from_ui(&ui);
        assert_eq!(theme.add_default_mode, config::AddDefaultMode::New);
    }
}
