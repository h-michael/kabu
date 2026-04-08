use crate::error::{Error, Result};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Padding, Paragraph, Wrap};

const SIMPLE_SELECT_HINTS: &str =
    "[Enter] select  [Up/Down/Ctrl+P/N] move  [Esc] cancel  [F1] help";
const CONFIRM_HINTS: &str = "[Enter] yes  [N] no  [Esc] cancel  [F1] help";

use std::time::Duration;

use super::{
    UiLayout, UiTheme, is_help_key, read_key_event, truncate_text_for_width, with_terminal,
};

pub(crate) fn select_from_list(
    command_name: &str,
    breadcrumbs: &[&str],
    message: Option<&str>,
    items: &[String],
    theme: UiTheme,
) -> Result<String> {
    if items.is_empty() {
        return Err(Error::Selector {
            message: "No items to select".to_string(),
        });
    }

    with_terminal(|terminal| {
        run_simple_select(terminal, command_name, breadcrumbs, message, items, theme)
    })
}

pub(crate) fn confirm(
    command_name: &str,
    breadcrumbs: &[&str],
    message: &str,
    details: &[String],
    theme: UiTheme,
) -> Result<bool> {
    with_terminal(|terminal| {
        run_confirm(terminal, command_name, breadcrumbs, message, details, theme)
    })
}

struct SimpleSelectState {
    cursor: usize,
    show_help: bool,
}

impl SimpleSelectState {
    fn new() -> Self {
        Self {
            cursor: 0,
            show_help: false,
        }
    }

    fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_down(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if self.cursor + 1 < len {
            self.cursor += 1;
        }
    }
}

fn run_simple_select(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<Box<dyn std::io::Write>>>,
    command_name: &str,
    breadcrumbs: &[&str],
    message: Option<&str>,
    items: &[String],
    theme: UiTheme,
) -> Result<String> {
    let mut state = SimpleSelectState::new();

    loop {
        terminal
            .draw(|frame| {
                draw_simple_select(
                    frame,
                    &state,
                    command_name,
                    breadcrumbs,
                    message,
                    items,
                    theme,
                )
            })
            .map_err(|e| Error::Selector {
                message: format!("Failed to draw UI: {e}"),
            })?;

        if let Some(key) = read_key_event(Duration::from_millis(200))? {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Err(Error::Aborted);
            }

            // Toggle help modal (F1 or Alt+H)
            if is_help_key(&key) {
                state.show_help = !state.show_help;
                continue;
            }

            // When help is shown, only handle close keys
            if state.show_help {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => state.show_help = false,
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Esc => return Err(Error::Aborted),
                KeyCode::Enter => {
                    return items.get(state.cursor).cloned().ok_or(Error::Aborted);
                }
                KeyCode::Up => state.move_up(),
                KeyCode::Down => state.move_down(items.len()),
                KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => match c {
                    'p' => state.move_up(),
                    'n' => state.move_down(items.len()),
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn draw_simple_select(
    frame: &mut ratatui::Frame<'_>,
    state: &SimpleSelectState,
    command_name: &str,
    breadcrumbs: &[&str],
    message: Option<&str>,
    items: &[String],
    theme: UiTheme,
) {
    let layout = UiLayout::new(frame.area(), theme);
    layout.draw_header(frame, command_name, breadcrumbs, None);

    let (message_area, list_area) = if message.is_some() {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(layout.body);
        (Some(split[0]), split[1])
    } else {
        (None, layout.body)
    };

    if let (Some(message), Some(area)) = (message, message_area) {
        let message = truncate_text_for_width(message.to_string(), area.width);
        let message = Paragraph::new(message).style(theme.label_style());
        frame.render_widget(message, area);
    }

    let box_title = breadcrumbs.last().copied().unwrap_or("Options");
    let items = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let mut list_item = ListItem::new(truncate_text_for_width(
                item.clone(),
                list_area.width.saturating_sub(2),
            ));
            if idx == state.cursor {
                list_item = list_item.style(theme.selection_style());
            } else {
                list_item = list_item.style(theme.text_style());
            }
            list_item
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled(box_title, theme.title_style())),
        )
        .style(theme.text_style());
    frame.render_widget(list, list_area);

    layout.draw_footer(frame, SIMPLE_SELECT_HINTS);
    layout.draw_help_modal(frame, state.show_help);
}

fn run_confirm(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<Box<dyn std::io::Write>>>,
    command_name: &str,
    breadcrumbs: &[&str],
    message: &str,
    details: &[String],
    theme: UiTheme,
) -> Result<bool> {
    let mut show_help = false;

    loop {
        terminal
            .draw(|frame| {
                draw_confirm_dialog(
                    frame,
                    command_name,
                    breadcrumbs,
                    message,
                    details,
                    theme,
                    show_help,
                )
            })
            .map_err(|e| Error::Selector {
                message: format!("Failed to draw UI: {e}"),
            })?;

        if let Some(key) = read_key_event(Duration::from_millis(200))? {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Err(Error::Aborted);
            }

            // Toggle help modal (F1 or Alt+H)
            if is_help_key(&key) {
                show_help = !show_help;
                continue;
            }

            // When help is shown, only handle close keys
            if show_help {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => show_help = false,
                    _ => {}
                }
                continue;
            }

            match key.code {
                KeyCode::Esc => return Ok(false),
                KeyCode::Enter => return Ok(true),
                KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
                _ => {}
            }
        }
    }
}

fn draw_confirm_dialog(
    frame: &mut ratatui::Frame<'_>,
    command_name: &str,
    breadcrumbs: &[&str],
    message: &str,
    details: &[String],
    theme: UiTheme,
    show_help: bool,
) {
    let layout = UiLayout::new(frame.area(), theme);
    layout.draw_header(frame, command_name, breadcrumbs, None);

    let box_title = breadcrumbs.last().copied().unwrap_or("Confirm");
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        message.to_string(),
        theme.label_style(),
    )));
    if !details.is_empty() {
        lines.push(Line::from(Span::raw("")));
        lines.extend(
            details
                .iter()
                .map(|line| Line::from(Span::styled(line.clone(), theme.text_style()))),
        );
    }

    let body = Paragraph::new(lines)
        .style(theme.text_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style())
                .padding(Padding::new(1, 1, 0, 0))
                .title(Span::styled(box_title, theme.title_style())),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(body, layout.body);

    layout.draw_footer(frame, CONFIRM_HINTS);
    layout.draw_help_modal(frame, show_help);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_simple_select_state_new() {
        let state = SimpleSelectState::new();
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_simple_select_state_move_up() {
        let mut state = SimpleSelectState::new();
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
    fn test_simple_select_state_move_down() {
        let mut state = SimpleSelectState::new();
        let len = 3;

        state.move_down(len);
        assert_eq!(state.cursor, 1);

        state.move_down(len);
        assert_eq!(state.cursor, 2);

        // Should not exceed len - 1
        state.move_down(len);
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_simple_select_state_move_down_empty() {
        let mut state = SimpleSelectState::new();

        // Should not panic or change cursor when len is 0
        state.move_down(0);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_draw_simple_select_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = SimpleSelectState::new();
        let items = vec!["Option 1".to_string(), "Option 2".to_string()];
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_simple_select(frame, &state, "Test", &["Select"], None, &items, theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Verify title is rendered (header starts at row 0)
        // Format: " Test | Select" - command name in badge style
        let title_str: String = (0..20)
            .map(|x| {
                buffer
                    .cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(title_str.contains("Test"));
    }

    #[test]
    fn test_draw_simple_select_with_message() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = SimpleSelectState::new();
        let items = vec!["Item A".to_string(), "Item B".to_string()];
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_simple_select(
                    frame,
                    &state,
                    "Test",
                    &["Select"],
                    Some("Select an option"),
                    &items,
                    theme,
                );
            })
            .unwrap();

        // Just verify it renders without panic
        assert!(terminal.backend().buffer().area.width > 0);
    }

    #[test]
    fn test_draw_confirm_dialog_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = UiTheme::default();
        let details = vec!["Detail 1".to_string(), "Detail 2".to_string()];

        terminal
            .draw(|frame| {
                draw_confirm_dialog(
                    frame,
                    "test",
                    &["Confirm"],
                    "Are you sure?",
                    &details,
                    theme,
                    false,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Verify title is rendered (header starts at row 0)
        // Format: " test | Confirm" - need to check wider range
        let title_str: String = (0..20)
            .map(|x| {
                buffer
                    .cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(title_str.contains("test") || title_str.contains("Confirm"));
    }

    #[test]
    fn test_draw_confirm_dialog_empty_details() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = UiTheme::default();

        terminal
            .draw(|frame| {
                draw_confirm_dialog(frame, "test", &["Confirm"], "Message", &[], theme, false);
            })
            .unwrap();

        // Just verify it renders without panic
        assert!(terminal.backend().buffer().area.width > 0);
    }
}
