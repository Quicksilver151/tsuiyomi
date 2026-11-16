mod ui;
mod usr;
mod utils;

use std::error::Error;

use ratatui::DefaultTerminal;

use ui::input::Action;
use ui::{ui, AppState};

fn main() -> Result<(), Box<dyn Error>> {
    let config = usr::load_config();
    let mut app_state = AppState::default();

    match config.library_path {
        Some(path) => match utils::files::get_anime_folder_contents(&path) {
            Ok(list) => app_state.anime.list = list,
            Err(error) => {
                app_state.anime.status =
                    Some(format!("Failed to read {}: {error}", path.display()));
            }
        },
        None => {
            app_state.anime.status =
                Some("Set TSUIYOMI_LIBRARY to a Mihon local/ folder.".to_owned());
        }
    }

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app_state);
    ratatui::restore();

    result?;
    Ok(())
}

fn run_app(terminal: &mut DefaultTerminal, app_state: &mut AppState) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app_state))?;

        let Some(action) = Action::read()? else {
            continue;
        };

        match action {
            Action::Quit => return Ok(()),
            Action::Previous => app_state.anime.previous(),
            Action::Next => app_state.anime.next(),
            Action::SelectTab(index) => app_state.tab = index.min(ui::TAB_COUNT - 1),
        }
    }
}
