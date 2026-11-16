use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Previous,
    Next,
    SelectTab(usize),
}

impl Action {
    pub fn read() -> io::Result<Option<Self>> {
        let Event::Key(key) = event::read()? else {
            return Ok(None);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(None);
        }

        let action = match key.code {
            KeyCode::Char('q') => Self::Quit,
            KeyCode::Char(character) if character.is_ascii_digit() => {
                Self::SelectTab(character.to_digit(10).unwrap_or_default() as usize)
            }
            KeyCode::Up | KeyCode::BackTab => Self::Previous,
            KeyCode::Down | KeyCode::Tab => Self::Next,
            _ => return Ok(None),
        };

        Ok(Some(action))
    }
}
