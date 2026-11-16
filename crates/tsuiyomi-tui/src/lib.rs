//! Ratatui frontend for tsuiyomi.
//!
//! The TUI owns terminal setup, input, and rendering. All library data comes from
//! `tsuiyomi-core`.

mod app;
mod draw;
mod input;
mod layout;
mod palette;

pub use app::AppState;

use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
};
use ratatui::{layout::Rect, DefaultTerminal};
use ratatui_image::picker::Picker;

use crate::{
    draw::draw,
    input::Action,
    layout::{app_layout, contains, inner, library_layout, tab_layouts},
};

/// Run the TUI until the user quits.
pub fn run(mut app: AppState) -> io::Result<()> {
    let mut terminal = ratatui::init();

    // Queries the terminal for Kitty/Sixel/iTerm2 support and font size. Must happen
    // before any input events are read.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    app.set_picker(picker);

    // ratatui installs a panic hook that restores the terminal. Wrap it so a panic also
    // releases mouse capture.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        previous_hook(info);
    }));

    if let Err(error) = execute!(std::io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(error);
    }

    let result = event_loop(&mut terminal, &mut app);

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();

    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut AppState) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        let Some(action) = input::read()? else {
            continue;
        };

        match action {
            Action::Quit => return Ok(()),
            Action::Previous => app.previous(),
            Action::Next => app.next(),
            Action::PreviousPage => app.previous_page(),
            Action::NextPage => app.next_page(),
            Action::ScrollUp => app.previous(),
            Action::ScrollDown => app.next(),
            Action::PreviousTab => app.previous_tab(),
            Action::NextTab => app.next_tab(),
            Action::PreviousCategory => app.previous_category(),
            Action::NextCategory => app.next_category(),
            Action::SelectTab(index) => app.select_tab(index),
            Action::Refresh => app.refresh(),
            Action::Open => app.open(),
            Action::Back => app.back(),
            Action::ToggleFullscreen => app.toggle_fullscreen(),
            Action::OpenConfig => app.select_tab(4),
            Action::Redraw => {}
            Action::Click { column, row } => {
                let (width, height) = crossterm::terminal::size()?;
                handle_click(Rect::new(0, 0, width, height), app, column, row);
            }
        }
    }
}

fn handle_click(area: Rect, app: &mut AppState, column: u16, row: u16) {
    let layout = app_layout(area, app.fullscreen);

    if contains(layout.tabs, column, row) {
        if let Some(tab) = tab_layouts(layout.tabs)
            .into_iter()
            .find(|tab| contains(tab.area, column, row))
        {
            app.select_tab(tab.index);
        }
        return;
    }

    if app.tab != 0 {
        return;
    }

    let library = library_layout(layout.content);
    let list_area = inner(library.list);
    if contains(list_area, column, row) {
        let visible_row = (row - list_area.y) as usize;
        let index = app.list_state.offset() + visible_row;
        app.select_index(index);
    }
}
