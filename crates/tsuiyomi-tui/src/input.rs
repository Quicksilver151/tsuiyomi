use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Previous,
    Next,
    PreviousTab,
    NextTab,
    PreviousCategory,
    NextCategory,
    SelectTab(usize),
    Refresh,
    ToggleFullscreen,
    OpenConfig,
    ShowLibrary,
    ScrollUp,
    ScrollDown,
    Click { column: u16, row: u16 },
}

/// Block until a meaningful input event arrives.
///
/// Mouse move and resize events are consumed without returning an action so the UI is only
/// redrawn when something changes.
pub fn read() -> io::Result<Option<Action>> {
    loop {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if let Some(action) = key_action(key.code) {
                    return Ok(Some(action));
                }
            }
            Event::Mouse(mouse) => {
                if let Some(action) = mouse_action(mouse.kind, mouse.column, mouse.row) {
                    return Ok(Some(action));
                }
            }
            _ => {}
        }
    }
}

fn key_action(code: KeyCode) -> Option<Action> {
    let action = match code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::ShowLibrary,
        KeyCode::Char('c') => Action::OpenConfig,
        KeyCode::Char('f') => Action::ToggleFullscreen,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('[') => Action::PreviousCategory,
        KeyCode::Char(']') => Action::NextCategory,
        KeyCode::Char(character) if character.is_ascii_digit() => {
            let digit = character.to_digit(10).unwrap_or(1) as usize;
            Action::SelectTab(digit.saturating_sub(1))
        }
        KeyCode::Up => Action::Previous,
        KeyCode::Down => Action::Next,
        KeyCode::Tab => Action::NextTab,
        KeyCode::BackTab => Action::PreviousTab,
        _ => return None,
    };
    Some(action)
}

fn mouse_action(kind: MouseEventKind, column: u16, row: u16) -> Option<Action> {
    let action = match kind {
        MouseEventKind::ScrollUp => Action::ScrollUp,
        MouseEventKind::ScrollDown => Action::ScrollDown,
        MouseEventKind::Down(MouseButton::Left) => Action::Click { column, row },
        _ => return None,
    };
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_do_not_control_tabs() {
        assert_eq!(key_action(KeyCode::Left), None);
        assert_eq!(key_action(KeyCode::Right), None);
    }

    #[test]
    fn tab_keys_control_tabs() {
        assert_eq!(key_action(KeyCode::Tab), Some(Action::NextTab));
        assert_eq!(key_action(KeyCode::BackTab), Some(Action::PreviousTab));
    }

    #[test]
    fn mouse_actions() {
        assert_eq!(
            mouse_action(MouseEventKind::Down(MouseButton::Left), 4, 7),
            Some(Action::Click { column: 4, row: 7 })
        );
        assert_eq!(
            mouse_action(MouseEventKind::ScrollUp, 0, 0),
            Some(Action::ScrollUp)
        );
        assert_eq!(
            mouse_action(MouseEventKind::ScrollDown, 0, 0),
            Some(Action::ScrollDown)
        );
        assert_eq!(mouse_action(MouseEventKind::Moved, 0, 0), None);
    }
}
