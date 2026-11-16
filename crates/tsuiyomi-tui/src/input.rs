use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Previous,
    Next,
    PreviousTab,
    NextTab,
    SelectTab(usize),
    Refresh,
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
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char(character) if character.is_ascii_digit() => {
            let digit = character.to_digit(10).unwrap_or(1) as usize;
            Action::SelectTab(digit.saturating_sub(1))
        }
        KeyCode::Up => Action::Previous,
        KeyCode::Down => Action::Next,
        KeyCode::Left | KeyCode::BackTab => Action::PreviousTab,
        KeyCode::Right | KeyCode::Tab => Action::NextTab,
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
