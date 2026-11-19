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
    Confirm,
    Search,
    Latest,
    Extensions,
    Stores,
    AddStore,
    DeleteStore,
    UpdateExtension,
    Pin,
    HostToggle,
    Char(char),
    Backspace,
    Click { column: u16, row: u16 },
}

/// Read one terminal event and translate it into an optional action.
///
/// The event loop polls before calling this, so it never blocks indefinitely and can
/// drain background scroll-page results between inputs. `text_mode` routes printable
/// keys to `Action::Char` so dialogs can accept typed input.
pub fn read(text_mode: bool) -> io::Result<Option<Action>> {
    Ok(action_from_event(event::read()?, text_mode))
}

pub(crate) fn action_from_event(event: Event, text_mode: bool) -> Option<Action> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => key_action(key.code, text_mode),
        Event::Mouse(mouse) => mouse_action(mouse.kind, mouse.column, mouse.row),
        Event::Resize(_, _) => Some(Action::Redraw),
        _ => None,
    }
}

fn key_action(code: KeyCode, text_mode: bool) -> Option<Action> {
    if text_mode {
        let action = match code {
            KeyCode::Esc => Action::Back,
            KeyCode::Enter => Action::Open,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Char(character) => Action::Char(character),
            KeyCode::Up => Action::Previous,
            KeyCode::Down => Action::Next,
            _ => return None,
        };
        return Some(action);
    }

    let action = match code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('y') => Action::Confirm,
        KeyCode::Char('s' | '/') => Action::Search,
        KeyCode::Char('l') => Action::Latest,
        KeyCode::Char('e') => Action::Extensions,
        KeyCode::Char('t') => Action::Stores,
        KeyCode::Char('a') => Action::AddStore,
        KeyCode::Char('d') => Action::DeleteStore,
        KeyCode::Char('u') => Action::UpdateExtension,
        KeyCode::Char('P') => Action::Pin,
        KeyCode::Char('H') => Action::HostToggle,
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
        KeyCode::Backspace => Action::Back,
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
        assert_eq!(key_action(KeyCode::Left, false), Some(Action::PreviousPage));
        assert_eq!(key_action(KeyCode::Right, false), Some(Action::NextPage));
    }

    #[test]
    fn page_keys() {
        assert_eq!(
            key_action(KeyCode::PageUp, false),
            Some(Action::PreviousScreen)
        );
        assert_eq!(
            key_action(KeyCode::PageDown, false),
            Some(Action::NextScreen)
        );
        assert_eq!(
            key_action(KeyCode::Char(' '), false),
            Some(Action::NextScreen)
        );
        assert_eq!(key_action(KeyCode::Left, false), Some(Action::PreviousPage));
        assert_eq!(key_action(KeyCode::Right, false), Some(Action::NextPage));
        assert_eq!(
            key_action(KeyCode::Char('n'), false),
            Some(Action::NextChapter)
        );
        assert_eq!(
            key_action(KeyCode::Char('p'), false),
            Some(Action::PreviousChapter)
        );
        assert_eq!(
            key_action(KeyCode::Char('m'), false),
            Some(Action::ToggleReaderMode)
        );
        assert_eq!(key_action(KeyCode::Char('+'), false), Some(Action::ZoomIn));
        assert_eq!(key_action(KeyCode::Char('-'), false), Some(Action::ZoomOut));
    }

    #[test]
    fn tab_keys_control_tabs() {
        assert_eq!(key_action(KeyCode::Tab, false), Some(Action::NextTab));
        assert_eq!(
            key_action(KeyCode::BackTab, false),
            Some(Action::PreviousTab)
        );
    }

    #[test]
    fn enter_and_escape_are_open_and_back() {
        assert_eq!(key_action(KeyCode::Enter, false), Some(Action::Open));
        assert_eq!(key_action(KeyCode::Esc, false), Some(Action::Back));
        assert_eq!(key_action(KeyCode::Backspace, false), Some(Action::Back));
        assert_eq!(key_action(KeyCode::Char('y'), false), Some(Action::Confirm));
    }

    #[test]
    fn text_mode_routes_printable_keys_to_char() {
        assert_eq!(
            key_action(KeyCode::Char('f'), true),
            Some(Action::Char('f'))
        );
        assert_eq!(
            key_action(KeyCode::Char('#'), true),
            Some(Action::Char('#'))
        );
        assert_eq!(
            key_action(KeyCode::Backspace, true),
            Some(Action::Backspace)
        );
        assert_eq!(key_action(KeyCode::Enter, true), Some(Action::Open));
        assert_eq!(key_action(KeyCode::Esc, true), Some(Action::Back));
        assert_eq!(key_action(KeyCode::Up, true), Some(Action::Previous));
        assert_eq!(key_action(KeyCode::Down, true), Some(Action::Next));
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
