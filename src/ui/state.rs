use crate::utils::files::Anime;

#[derive(Debug, Default)]
pub struct AppState {
    pub tab: usize,
    pub anime: AnimeState,
}

#[derive(Debug, Default)]
pub struct AnimeState {
    pub current: usize,
    pub list: Vec<Anime>,
    pub status: Option<String>,
}

impl AnimeState {
    pub fn previous(&mut self) {
        self.current = self.current.saturating_sub(1);
    }

    pub fn next(&mut self) {
        if let Some(last) = self.list.len().checked_sub(1) {
            self.current = (self.current + 1).min(last);
        }
    }
}
