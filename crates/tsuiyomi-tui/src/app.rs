use std::path::PathBuf;

use crossterm::terminal;
use ratatui::{layout::Size, widgets::ListState};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, sliced::SlicedProtocol, Resize};
use tsuiyomi_core::{
    chapter_pages, read_page, scan_library, Chapter, Library, Page, ProgressStore, Series,
};

use crate::layout::TAB_COUNT;

/// Lines moved per arrow/wheel step in continuous scroll mode.
const SCROLL_LINES: i32 = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibraryView {
    #[default]
    Series,
    Chapters,
    Reader,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReaderMode {
    #[default]
    Page,
    Scroll,
}

pub struct ScrollState {
    pub protocol: SlicedProtocol,
    /// Signed Y offset of the tall image; 0 is the top.
    pub offset: i16,
    pub width: u16,
    pub viewport_height: u16,
    pub total_rows: u16,
    /// Row at which each page starts.
    pub page_starts: Vec<u16>,
}

pub struct ReaderState {
    pub series_title: String,
    pub chapter_title: String,
    pub chapter_path: PathBuf,
    pub pages: Vec<Page>,
    pub page_index: usize,
    pub protocol: Option<StatefulProtocol>,
    pub scroll: Option<ScrollState>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct AppState {
    pub tab: usize,
    pub fullscreen: bool,
    pub view: LibraryView,
    pub reader_mode: ReaderMode,
    pub library: Library,
    pub category_index: usize,
    pub selected: usize,
    pub chapter_index: usize,
    pub list_state: ListState,
    pub chapter_list_state: ListState,
    pub reader: Option<ReaderState>,
    pub picker: Option<Picker>,
    pub progress: ProgressStore,
    pub status: Option<String>,
}

impl AppState {
    pub fn new(library: Library, status: Option<String>) -> Self {
        let mut app = Self {
            tab: 0,
            fullscreen: false,
            view: LibraryView::Series,
            reader_mode: ReaderMode::Page,
            library,
            category_index: 0,
            selected: 0,
            chapter_index: 0,
            list_state: ListState::default(),
            chapter_list_state: ListState::default(),
            reader: None,
            picker: None,
            progress: ProgressStore::default(),
            status,
        };
        if app.visible_count() > 0 {
            app.list_state.select(Some(0));
        }
        app
    }

    pub fn set_picker(&mut self, picker: Picker) {
        self.picker = Some(picker);
    }

    pub fn set_progress(&mut self, progress: ProgressStore) {
        self.progress = progress;
    }

    pub fn protocol_name(&self) -> &'static str {
        match self.picker.as_ref().map(Picker::protocol_type) {
            Some(ratatui_image::picker::ProtocolType::Kitty) => "Kitty",
            Some(ratatui_image::picker::ProtocolType::Sixel) => "Sixel",
            Some(ratatui_image::picker::ProtocolType::Iterm2) => "iTerm2",
            Some(ratatui_image::picker::ProtocolType::Halfblocks) => "Halfblocks",
            _ => "none",
        }
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

    pub fn selected_chapter(&self) -> Option<&Chapter> {
        self.selected_series()?.chapters.get(self.chapter_index)
    }

    pub fn chapter_count(&self) -> usize {
        self.selected_series()
            .map_or(0, |series| series.chapters.len())
    }

    /// Enter chapters, open a chapter, or turn the page.
    pub fn open(&mut self) {
        match self.view {
            LibraryView::Series => {
                if self.selected_series().is_some() {
                    self.view = LibraryView::Chapters;
                    self.chapter_index = 0;
                    self.status = None;
                    let count = self.chapter_count();
                    self.chapter_list_state.select((count > 0).then_some(0));
                }
            }
            LibraryView::Chapters => self.open_chapter(),
            LibraryView::Reader => self.next_page(),
        }
    }

    /// Back out of the reader/chapter list, or return to the Library tab.
    pub fn back(&mut self) {
        match self.view {
            LibraryView::Reader => {
                self.reader = None;
                self.view = LibraryView::Chapters;
                self.status = None;
            }
            LibraryView::Chapters => {
                self.view = LibraryView::Series;
                self.chapter_index = 0;
                self.status = None;
                self.chapter_list_state.select(None);
            }
            LibraryView::Series => self.select_tab(0),
        }
    }

    pub fn previous(&mut self) {
        match self.view {
            LibraryView::Series => {
                self.selected = self.selected.saturating_sub(1);
                self.status = None;
            }
            LibraryView::Chapters => {
                self.chapter_index = self.chapter_index.saturating_sub(1);
                self.status = None;
            }
            LibraryView::Reader => {
                if self.reader_mode == ReaderMode::Scroll {
                    self.scroll_lines(-SCROLL_LINES);
                } else {
                    self.change_page(false);
                }
            }
        }
    }

    pub fn next(&mut self) {
        match self.view {
            LibraryView::Series => {
                if let Some(last) = self.visible_count().checked_sub(1) {
                    self.selected = (self.selected + 1).min(last);
                    self.status = None;
                }
            }
            LibraryView::Chapters => {
                if let Some(last) = self.chapter_count().checked_sub(1) {
                    self.chapter_index = (self.chapter_index + 1).min(last);
                    self.status = None;
                }
            }
            LibraryView::Reader => {
                if self.reader_mode == ReaderMode::Scroll {
                    self.scroll_lines(SCROLL_LINES);
                } else {
                    self.change_page(true);
                }
            }
        }
    }

    pub fn next_page(&mut self) {
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            self.scroll_lines(self.scroll_page_step());
        } else {
            self.change_page(true);
        }
    }

    pub fn previous_page(&mut self) {
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            self.scroll_lines(-self.scroll_page_step());
        } else {
            self.change_page(false);
        }
    }

    pub fn toggle_reader_mode(&mut self) {
        self.reader_mode = match self.reader_mode {
            ReaderMode::Page => ReaderMode::Scroll,
            ReaderMode::Scroll => ReaderMode::Page,
        };

        if self.view != LibraryView::Reader {
            return;
        }

        match self.reader_mode {
            ReaderMode::Page => self.load_page(),
            ReaderMode::Scroll => self.build_scroll(terminal_width()),
        }
    }

    pub fn jump_to_page(&mut self, page: usize) {
        if self.view != LibraryView::Reader {
            return;
        }

        if self.reader_mode == ReaderMode::Scroll {
            let (chapter_path, pages) = {
                let Some(reader) = self.reader.as_mut() else {
                    return;
                };
                let Some(scroll) = reader.scroll.as_mut() else {
                    return;
                };
                let page = page.min(reader.pages.len().saturating_sub(1));
                let start = scroll.page_starts.get(page).copied().unwrap_or(0);
                let max_scroll = scroll.total_rows.saturating_sub(scroll.viewport_height);
                scroll.offset = -(start.min(max_scroll) as i16);
                reader.page_index = page;
                reader.error = None;
                (reader.chapter_path.clone(), reader.pages.len())
            };
            let _ = self.progress.record(&chapter_path, page, pages);
        } else {
            let (chapter_path, pages) = {
                let Some(reader) = self.reader.as_mut() else {
                    return;
                };
                let page = page.min(reader.pages.len().saturating_sub(1));
                reader.page_index = page;
                (reader.chapter_path.clone(), reader.pages.len())
            };
            self.load_page();
            let _ = self.progress.record(&chapter_path, page, pages);
        }
    }

    pub fn select_index(&mut self, index: usize) {
        match self.view {
            LibraryView::Series if index < self.visible_count() => {
                self.selected = index;
                self.status = None;
            }
            LibraryView::Chapters if index < self.chapter_count() => {
                self.chapter_index = index;
                self.status = None;
            }
            _ => {}
        }
    }

    fn open_chapter(&mut self) {
        let Some(chapter) = self.selected_chapter().cloned() else {
            return;
        };
        let series_title = self
            .selected_series()
            .map(|series| series.title.clone())
            .unwrap_or_default();

        let pages = match chapter_pages(&chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.status = Some(error.to_string());
                self.view = LibraryView::Chapters;
                self.reader = None;
                return;
            }
        };
        if pages.is_empty() {
            self.status = Some(format!("{} has no pages", chapter.title));
            self.view = LibraryView::Chapters;
            self.reader = None;
            return;
        }

        let start_page = self
            .progress
            .entry(&chapter.path)
            .map(|entry| entry.page)
            .unwrap_or(0)
            .min(pages.len() - 1);

        self.reader = Some(ReaderState {
            series_title,
            chapter_title: chapter.title,
            chapter_path: chapter.path,
            pages,
            page_index: start_page,
            protocol: None,
            scroll: None,
            error: None,
        });
        self.view = LibraryView::Reader;
        self.status = None;
        if let Some(reader) = self.reader.as_ref() {
            let _ =
                self.progress
                    .record(&reader.chapter_path, reader.page_index, reader.pages.len());
        }
        match self.reader_mode {
            ReaderMode::Page => self.load_page(),
            ReaderMode::Scroll => self.build_scroll(terminal_width()),
        }
    }

    fn change_page(&mut self, forward: bool) {
        {
            let Some(reader) = self.reader.as_mut() else {
                return;
            };

            if forward {
                if reader.page_index + 1 < reader.pages.len() {
                    reader.page_index += 1;
                }
            } else {
                reader.page_index = reader.page_index.saturating_sub(1);
            }
        }

        if let Some(reader) = self.reader.as_ref() {
            let _ =
                self.progress
                    .record(&reader.chapter_path, reader.page_index, reader.pages.len());
        }

        self.load_page();
    }

    pub fn next_chapter(&mut self) {
        if self.view != LibraryView::Reader || self.chapter_index == 0 {
            return;
        }
        self.chapter_index -= 1;
        self.open_chapter();
    }

    pub fn previous_chapter(&mut self) {
        if self.view != LibraryView::Reader || self.chapter_index + 1 >= self.chapter_count() {
            return;
        }
        self.chapter_index += 1;
        self.open_chapter();
    }

    fn load_page(&mut self) {
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        let Some(page) = reader.pages.get(reader.page_index).cloned() else {
            reader.error = Some("Page is out of range.".to_owned());
            reader.protocol = None;
            return;
        };

        let bytes = match read_page(&page) {
            Ok(bytes) => bytes,
            Err(error) => {
                reader.error = Some(error.to_string());
                reader.protocol = None;
                return;
            }
        };
        let image = match image::load_from_memory(&bytes) {
            Ok(image) => image,
            Err(error) => {
                reader.error = Some(format!("{}: {error}", page.name));
                reader.protocol = None;
                return;
            }
        };

        let Some(picker) = self.picker.as_ref() else {
            reader.error = Some("No terminal graphics backend.".to_owned());
            reader.protocol = None;
            return;
        };

        reader.protocol = Some(picker.new_resize_protocol(image));
        reader.error = None;
    }

    fn build_scroll(&mut self, width: u16) {
        let Some(pages) = self.reader.as_ref().map(|reader| reader.pages.clone()) else {
            return;
        };
        let Some(picker) = self.picker.clone() else {
            self.set_reader_error("No terminal graphics backend.");
            return;
        };

        let font = picker.font_size();
        let font_width = u32::from(font.width.max(1));
        let font_height = u32::from(font.height.max(1));

        let mut images = Vec::with_capacity(pages.len());
        let mut max_width = 1u32;
        let mut total_height = 0u32;
        for page in &pages {
            let bytes = match read_page(page) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.set_reader_error(error.to_string());
                    return;
                }
            };
            let image = match image::load_from_memory(&bytes) {
                Ok(image) => image,
                Err(error) => {
                    self.set_reader_error(format!("{}: {error}", page.name));
                    return;
                }
            };
            max_width = max_width.max(image.width());
            total_height = total_height.saturating_add(image.height());
            images.push(image);
        }

        // Keep a sanity cap; a 100-page chapter at full resolution is not viable yet.
        if total_height > 64_000_000 {
            self.set_reader_error("Chapter is too large for scroll mode.");
            return;
        }

        let mut canvas = image::RgbaImage::new(max_width, total_height);
        let mut starts_px = Vec::with_capacity(images.len());
        let mut y = 0;
        for image in &images {
            starts_px.push(y);
            image::imageops::overlay(&mut canvas, &image.to_rgba8(), 0, i64::from(y));
            y += image.height();
        }

        let target_width = u32::from(width.max(1));
        let scale = (target_width as f32 * font_width as f32) / max_width as f32;
        let scaled_height = (total_height as f32 * scale).ceil() as u32;
        let total_rows = scaled_height.div_ceil(font_height).max(1);
        if total_rows > i16::MAX as u32 {
            self.set_reader_error("Chapter is too tall for scroll mode.");
            return;
        }

        let page_pixels: Vec<u32> = starts_px;

        let image = image::DynamicImage::ImageRgba8(canvas);
        let protocol = match SlicedProtocol::new_with_resize(
            &picker,
            image,
            Size::new(target_width as u16, total_rows as u16),
            Resize::Fit(None),
        ) {
            Ok(protocol) => protocol,
            Err(error) => {
                self.set_reader_error(error.to_string());
                return;
            }
        };

        let total_rows = protocol.size().height.max(1);
        let page_starts: Vec<u16> = page_pixels
            .iter()
            .map(|pixels| ((*pixels as f32 / total_height as f32) * f32::from(total_rows)) as u16)
            .collect();
        let viewport_height = self
            .reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .map_or(10, |scroll| scroll.viewport_height)
            .max(1);
        let page_index = self.reader.as_ref().map_or(0, |reader| reader.page_index);
        let start_row = page_starts.get(page_index).copied().unwrap_or(0);
        let max_scroll = total_rows.saturating_sub(viewport_height);
        let offset = -(start_row.min(max_scroll) as i16);

        if let Some(reader) = self.reader.as_mut() {
            reader.error = None;
            reader.scroll = Some(ScrollState {
                protocol,
                offset,
                width: width.max(1),
                viewport_height,
                total_rows,
                page_starts,
            });
        }
    }

    pub(crate) fn ensure_scroll_width(&mut self, width: u16) {
        let needs_rebuild = self
            .reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .is_some_and(|scroll| scroll.width != width.max(1));
        if needs_rebuild {
            self.build_scroll(width);
        }
    }

    fn scroll_lines(&mut self, delta: i32) {
        let (changed, chapter_path, pages, page) = {
            let Some(reader) = self.reader.as_mut() else {
                return;
            };
            let Some(scroll) = reader.scroll.as_mut() else {
                return;
            };

            let max_scroll = scroll.total_rows.saturating_sub(scroll.viewport_height) as i32;
            let current = -(scroll.offset as i32);
            let next = (current + delta).clamp(0, max_scroll);
            scroll.offset = -(next as i16);
            let at_end = next >= max_scroll;

            let mut page = scroll
                .page_starts
                .iter()
                .rposition(|start| i32::from(*start) <= next)
                .unwrap_or(0);
            if at_end && !reader.pages.is_empty() {
                page = reader.pages.len() - 1;
            }
            page = page.min(reader.pages.len().saturating_sub(1));
            let changed = reader.page_index != page;
            reader.page_index = page;
            reader.error = None;

            (
                changed,
                reader.chapter_path.clone(),
                reader.pages.len(),
                page,
            )
        };

        if changed {
            let _ = self.progress.record(&chapter_path, page, pages);
        }
    }

    fn scroll_page_step(&self) -> i32 {
        self.reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .map_or(10, |scroll| {
                i32::from(scroll.viewport_height.saturating_sub(2).max(1))
            })
    }

    fn set_reader_error(&mut self, message: impl Into<String>) {
        if let Some(reader) = self.reader.as_mut() {
            reader.error = Some(message.into());
            reader.protocol = None;
            reader.scroll = None;
        }
    }

    pub fn next_category(&mut self) {
        if self.view != LibraryView::Series {
            return;
        }
        let count = self.library.categories.len();
        if count == 0 {
            return;
        }
        self.category_index = (self.category_index + 1) % count;
        self.reset_selection();
    }

    pub fn previous_category(&mut self) {
        if self.view != LibraryView::Series {
            return;
        }
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
        self.tab = (self.tab + 1) % TAB_COUNT;
    }

    pub fn previous_tab(&mut self) {
        self.tab = if self.tab == 0 {
            TAB_COUNT - 1
        } else {
            self.tab - 1
        };
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
                self.view = LibraryView::Series;
                self.reader = None;
                self.chapter_index = 0;
                self.chapter_list_state.select(None);
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
        self.status = None;
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

fn terminal_width() -> u16 {
    terminal::size()
        .map(|(width, _)| width.max(1))
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use ratatui_image::picker::Picker;

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

    fn chapter(title: &str) -> Chapter {
        Chapter {
            path: title.into(),
            title: title.to_owned(),
            number: Some(1.0),
            kind: tsuiyomi_core::ChapterKind::Directory,
        }
    }

    fn library() -> Library {
        let mut berserk = series("Berserk", &["Default", "Reading"]);
        berserk.chapters = vec![chapter("Chapter 1"), chapter("Chapter 2")];
        Library {
            root: "/tmp/library".into(),
            series: vec![series("Azumanga", &["Default"]), berserk],
            categories: vec!["Default".to_owned(), "Reading".to_owned()],
            errors: Vec::new(),
        }
    }

    #[test]
    fn tabs_wrap_around() {
        let mut app = AppState::default();

        app.previous_tab();
        assert_eq!(app.tab, TAB_COUNT - 1);

        app.next_tab();
        assert_eq!(app.tab, 0);
    }

    #[test]
    fn opens_and_closes_the_chapter_view() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        assert_eq!(
            app.selected_series().map(|series| series.title.as_str()),
            Some("Berserk")
        );

        app.open();
        assert_eq!(app.view, LibraryView::Chapters);
        assert_eq!(app.chapter_count(), 2);
        assert_eq!(app.chapter_index, 0);

        app.next();
        assert_eq!(app.chapter_index, 1);

        app.back();
        assert_eq!(app.view, LibraryView::Series);
    }

    #[test]
    fn chapter_status_does_not_leak_between_series() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.open();
        app.open();
        assert!(app.status.is_some());

        app.back();
        assert!(app.status.is_none());

        app.open();
        assert!(app.status.is_none());
    }

    #[test]
    fn opens_the_reader_and_turns_pages() {
        let root = tempfile::tempdir().unwrap();
        for name in ["1.png", "2.png"] {
            let image = image::RgbImage::from_pixel(100, 400, image::Rgb([180, 190, 254]));
            image.save(root.path().join(name)).unwrap();
        }

        let series = Series {
            path: root.path().to_path_buf(),
            title: "Test".to_owned(),
            author: None,
            artist: None,
            description: String::new(),
            tags: Vec::new(),
            categories: vec!["Default".to_owned()],
            chapters: vec![Chapter {
                path: root.path().to_path_buf(),
                title: "Chapter 1".to_owned(),
                number: Some(1.0),
                kind: tsuiyomi_core::ChapterKind::Directory,
            }],
            warnings: Vec::new(),
        };
        let library = Library {
            root: root.path().to_path_buf(),
            series: vec![series],
            categories: vec!["Default".to_owned()],
            errors: Vec::new(),
        };

        let mut app = AppState::new(library, None);
        app.set_picker(Picker::halfblocks());
        let mut progress = ProgressStore::load(&root.path().join("progress.json")).unwrap();
        progress.record(root.path(), 1, 2).unwrap();
        app.set_progress(progress);

        app.open();
        app.open();

        assert_eq!(app.view, LibraryView::Reader);
        let reader = app.reader.as_ref().unwrap();
        assert_eq!(reader.pages.len(), 2);
        assert!(reader.protocol.is_some());
        assert_eq!(reader.page_index, 1);

        app.previous_page();
        assert_eq!(app.reader.as_ref().unwrap().page_index, 0);

        app.next_page();
        assert_eq!(app.reader.as_ref().unwrap().page_index, 1);

        app.toggle_reader_mode();
        assert_eq!(app.reader_mode, ReaderMode::Scroll);
        let scroll = app.reader.as_ref().unwrap().scroll.as_ref().unwrap();
        assert_eq!(scroll.page_starts.len(), 2);
        assert!(scroll.total_rows > 0);
        let offset = scroll.offset;
        assert!(offset < 0);

        app.next();
        assert!(app.reader.as_ref().unwrap().scroll.as_ref().unwrap().offset < offset);

        for _ in 0..100 {
            app.next();
        }
        assert_eq!(app.reader.as_ref().unwrap().page_index, 1);

        app.toggle_reader_mode();
        assert_eq!(app.reader_mode, ReaderMode::Page);
        assert!(app.reader.as_ref().unwrap().protocol.is_some());

        app.back();
        assert_eq!(app.view, LibraryView::Chapters);
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
