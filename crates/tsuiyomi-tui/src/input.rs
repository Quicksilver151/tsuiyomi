use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Previous,
    Next,
    PreviousPage,
    NextPage,
    PreviousScreen,
    NextScreen,
    PreviousChapter,
    NextChapter,
    PreviousTab,
    NextTab,
    PreviousCategory,
    NextCategory,
    SelectTab(usize),
    Refresh,
    Open,
    Back,
    ToggleFullscreen,
    ToggleReaderMode,
    ZoomIn,
    ZoomOut,
    OpenConfig,
    Redraw,
    ScrollUp,
    ScrollDown,
    Click { column: u16, row: u16 },
}

/// Read one terminal event and translate it into an optional action.
///
/// The event loop polls before calling this, so it never blocks indefinitely and can
/// drain background scroll-page results between inputs.
pub fn read() -> io::Result<Option<Action>> {
    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => Ok(key_action(key.code)),
        Event::Mouse(mouse) => Ok(mouse_action(mouse.kind, mouse.column, mouse.row)),
        Event::Resize(_, _) => Ok(Some(Action::Redraw)),
        _ => Ok(None),
    }
}

fn key_action(code: KeyCode) -> Option<Action> {
    let action = match code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::Back,
        KeyCode::Enter => Action::Open,
        KeyCode::Char('c') => Action::OpenConfig,
        KeyCode::Char('f') => Action::ToggleFullscreen,
        KeyCode::Char('m') => Action::ToggleReaderMode,
        KeyCode::Char('+' | '=') => Action::ZoomIn,
        KeyCode::Char('-' | '_') => Action::ZoomOut,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('[') => Action::PreviousCategory,
        KeyCode::Char(']') => Action::NextCategory,
        KeyCode::Char('n') => Action::NextChapter,
        KeyCode::Char('p') => Action::PreviousChapter,
        KeyCode::Char(character) if character.is_ascii_digit() => {
            let digit = character.to_digit(10).unwrap_or(1) as usize;
            Action::SelectTab(digit.saturating_sub(1))
        }
        KeyCode::Up => Action::Previous,
        KeyCode::Down => Action::Next,
        KeyCode::Left => Action::PreviousPage,
        KeyCode::Right => Action::NextPage,
        KeyCode::PageUp => Action::PreviousScreen,
        KeyCode::PageDown | KeyCode::Char(' ') => Action::NextScreen,
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
        assert_eq!(key_action(KeyCode::Left), Some(Action::PreviousPage));
        assert_eq!(key_action(KeyCode::Right), Some(Action::NextPage));
    }

    #[test]
    fn page_keys() {
        assert_eq!(key_action(KeyCode::PageUp), Some(Action::PreviousScreen));
        assert_eq!(key_action(KeyCode::PageDown), Some(Action::NextScreen));
        assert_eq!(key_action(KeyCode::Char(' ')), Some(Action::NextScreen));
        assert_eq!(key_action(KeyCode::Left), Some(Action::PreviousPage));
        assert_eq!(key_action(KeyCode::Right), Some(Action::NextPage));
        assert_eq!(key_action(KeyCode::Char('n')), Some(Action::NextChapter));
        assert_eq!(
            key_action(KeyCode::Char('p')),
            Some(Action::PreviousChapter)
        );
        assert_eq!(
            key_action(KeyCode::Char('m')),
            Some(Action::ToggleReaderMode)
        );
        assert_eq!(key_action(KeyCode::Char('+')), Some(Action::ZoomIn));
        assert_eq!(key_action(KeyCode::Char('-')), Some(Action::ZoomOut));
    }

    #[test]
    fn tab_keys_control_tabs() {
        assert_eq!(key_action(KeyCode::Tab), Some(Action::NextTab));
        assert_eq!(key_action(KeyCode::BackTab), Some(Action::PreviousTab));
    }

    #[test]
    fn enter_and_escape_are_open_and_back() {
        assert_eq!(key_action(KeyCode::Enter), Some(Action::Open));
        assert_eq!(key_action(KeyCode::Esc), Some(Action::Back));
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
