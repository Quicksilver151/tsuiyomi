use ratatui::widgets::ListState;
use tsuiyomi_core::{scan_library, Library};

use crate::layout::TAB_COUNT;

#[derive(Debug, Default)]
pub struct AppState {
    pub tab: usize,
    pub library: Library,
    pub selected: usize,
    pub list_state: ListState,
    pub status: Option<String>,
}

impl AppState {
    pub fn new(library: Library, status: Option<String>) -> Self {
        let mut list_state = ListState::default();
        if !library.series.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            tab: 0,
            library,
            selected: 0,
            list_state,
            status,
        }
    }

    pub fn selected_series(&self) -> Option<&tsuiyomi_core::Series> {
        self.library.series.get(self.selected)
    }

    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn next(&mut self) {
        if let Some(last) = self.library.series.len().checked_sub(1) {
            self.selected = (self.selected + 1).min(last);
        }
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.library.series.len() {
            self.selected = index;
        }
    }

    pub fn select_tab(&mut self, index: usize) {
        self.tab = index.min(TAB_COUNT - 1);
    }

    pub fn next_tab(&mut self) {
        self.tab = (self.tab + 1).min(TAB_COUNT - 1);
    }

    pub fn previous_tab(&mut self) {
        self.tab = self.tab.saturating_sub(1);
    }

    /// Rescan the library root, keeping the selection if possible.
    pub fn refresh(&mut self) {
        if self.library.root.as_os_str().is_empty() {
            self.status = Some("No library configured. Set TSUIYOMI_LIBRARY.".to_owned());
            return;
        }

        match scan_library(&self.library.root) {
            Ok(library) => {
                self.library = library;
                self.clamp_selection();
                self.status = if self.library.errors.is_empty() {
                    None
                } else {
                    Some(format!(
                        "{} series could not be read",
                        self.library.errors.len()
                    ))
                };
            }
            Err(error) => self.status = Some(error.to_string()),
        }
    }

    fn clamp_selection(&mut self) {
        if self.library.series.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            self.selected = self.selected.min(self.library.series.len() - 1);
        }
    }
}
