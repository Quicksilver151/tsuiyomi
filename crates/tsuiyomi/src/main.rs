use std::error::Error;

use tsuiyomi_core::{load_config, scan_library, Library};
use tsuiyomi_tui::AppState;

fn main() -> Result<(), Box<dyn Error>> {
    let config = load_config();

    let (library, status) = match config.library_path {
        Some(path) => {
            let placeholder = Library {
                root: path.clone(),
                ..Library::default()
            };
            match scan_library(&path) {
                Ok(scanned) => {
                    let status = scan_status(&scanned);
                    (scanned, status)
                }
                Err(error) => (placeholder, Some(error.to_string())),
            }
        }
        None => (
            Library::default(),
            Some("Set TSUIYOMI_LIBRARY to a Mihon local/ folder.".to_owned()),
        ),
    };

    tsuiyomi_tui::run(AppState::new(library, status))?;
    Ok(())
}

fn scan_status(library: &Library) -> Option<String> {
    if library.errors.is_empty() {
        None
    } else {
        Some(format!(
            "{} series, {} could not be read",
            library.series.len(),
            library.errors.len()
        ))
    }
}
