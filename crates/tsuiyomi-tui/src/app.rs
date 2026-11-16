use ratatui::widgets::ListState;
use tsuiyomi_core::{scan_library, Library, Series};

use crate::layout::TAB_COUNT;

#[derive(Debug, Default)]
pub struct AppState {
    pub tab: usize,
    pub fullscreen: bool,
    pub library: Library,
    pub category_index: usize,
    pub selected: usize,
    pub list_state: ListState,
    pub status: Option<String>,
}

impl AppState {
    pub fn new(library: Library, status: Option<String>) -> Self {
        let mut app = Self {
            tab: 0,
            fullscreen: false,
            library,
            category_index: 0,
            selected: 0,
            list_state: ListState::default(),
            status,
        };
        if app.visible_count() > 0 {
            app.list_state.select(Some(0));
        }
        app
    }

    pub fn category_name(&self) -> Option<&str> {
        self.library
            .categories
            .get(self.category_index)
            .map(String::as_str)
    }

    pub fn visible_count(&self) -> usize {
        let category = self.category_name();
        self.library
            .series
            .iter()
            .filter(|series| in_category(series, category))
            .count()
    }

    pub fn selected_series(&self) -> Option<&Series> {
        let category = self.category_name();
        self.library
            .series
            .iter()
            .filter(|series| in_category(series, category))
            .nth(self.selected)
    }

    pub fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn next(&mut self) {
        if let Some(last) = self.visible_count().checked_sub(1) {
            self.selected = (self.selected + 1).min(last);
        }
    }

    pub fn select_index(&mut self, index: usize) {
        if index < self.visible_count() {
            self.selected = index;
        }
    }

    pub fn next_category(&mut self) {
        let count = self.library.categories.len();
        if count == 0 {
            return;
        }
        self.category_index = (self.category_index + 1) % count;
        self.reset_selection();
    }

    pub fn previous_category(&mut self) {
        let count = self.library.categories.len();
        if count == 0 {
            return;
        }
        self.category_index = if self.category_index == 0 {
            count - 1
        } else {
            self.category_index - 1
        };
        self.reset_selection();
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

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
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
                self.category_index = self
                    .category_index
                    .min(self.library.categories.len().saturating_sub(1));
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

    fn reset_selection(&mut self) {
        self.selected = 0;
        let count = self.visible_count();
        self.list_state.select((count > 0).then_some(0));
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            self.selected = self.selected.min(count - 1);
        }
    }
}

fn in_category(series: &Series, category: Option<&str>) -> bool {
    category.is_none_or(|category| series.categories.iter().any(|item| item == category))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(title: &str, categories: &[&str]) -> Series {
        Series {
            path: title.into(),
            title: title.to_owned(),
            author: None,
            artist: None,
            description: String::new(),
            tags: Vec::new(),
            categories: categories.iter().map(|item| (*item).to_owned()).collect(),
            chapters: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn library() -> Library {
        Library {
            root: "/tmp/library".into(),
            series: vec![
                series("Azumanga", &["Default"]),
                series("Berserk", &["Default", "Reading"]),
            ],
            categories: vec!["Default".to_owned(), "Reading".to_owned()],
            errors: Vec::new(),
        }
    }

    #[test]
    fn category_navigation_filters_the_list() {
        let mut app = AppState::new(library(), None);

        assert_eq!(app.category_name(), Some("Default"));
        assert_eq!(app.visible_count(), 2);

        app.next_category();
        assert_eq!(app.category_name(), Some("Reading"));
        assert_eq!(app.visible_count(), 1);
        assert_eq!(
            app.selected_series().map(|series| series.title.as_str()),
            Some("Berserk")
        );

        app.next_category();
        assert_eq!(app.category_name(), Some("Default"));

        app.previous_category();
        assert_eq!(app.category_name(), Some("Reading"));
    }
}
