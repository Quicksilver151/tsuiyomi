use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crossterm::terminal;
use ratatui::{layout::Size, widgets::ListState};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, sliced::SlicedProtocol, Resize};
use tsuiyomi_core::{
    chapter::natural_cmp, chapter_pages, read_cover, read_page, save_config, scan_library, Chapter,
    Config, Library, LibrarySort, Page, ProgressStore, ReadingMode, Series, Theme,
};

use crate::{
    layout::TAB_COUNT,
    palette,
    scroll_worker::{BuildRequest, BuildResult, ScrollWorker},
};

/// Lines moved per arrow/wheel step in continuous scroll mode.
const SCROLL_LINES: i32 = 5;

/// Upper bound on decoded library tile covers kept in memory.
const TILE_COVER_CACHE: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibraryView {
    #[default]
    Series,
    Chapters,
    Reader,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MoreSection {
    #[default]
    Settings,
    Downloads,
    Categories,
    Statistics,
    Storage,
    About,
    Help,
}

pub const MORE_SECTIONS: [MoreSection; 7] = [
    MoreSection::Settings,
    MoreSection::Downloads,
    MoreSection::Categories,
    MoreSection::Statistics,
    MoreSection::Storage,
    MoreSection::About,
    MoreSection::Help,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Theme,
    Accent,
}

#[derive(Debug, Clone)]
pub struct PickerState {
    pub kind: PickerKind,
    pub selected: usize,
    pub custom: String,
    pub editing_custom: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrowseView {
    #[default]
    Sources,
    Local,
    Details,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MoreFocus {
    Sections,
    #[default]
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoreRow {
    Divider(&'static str),
    Setting(SettingId),
    Note(&'static str),
}

/// Selectable rows across all settings pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingId {
    ReadingMode,
    Zoom,
    PrefetchBehind,
    PrefetchAhead,
    CacheMargin,
    LibraryPath,
    DefaultCategory,
    LibrarySort,
    RescanLibrary,
    Theme,
    Accent,
    Fullscreen,
    ThumbnailSize,
    Incognito,
}

/// Settings pages mirroring Mihon's `SettingsMainScreen` entries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsPage {
    #[default]
    Main,
    Appearance,
    Library,
    Reader,
    Downloads,
    Tracking,
    Browse,
    Data,
    Security,
    Advanced,
    About,
}

pub const SETTINGS_PAGES: [SettingsPage; 10] = [
    SettingsPage::Appearance,
    SettingsPage::Library,
    SettingsPage::Reader,
    SettingsPage::Downloads,
    SettingsPage::Tracking,
    SettingsPage::Browse,
    SettingsPage::Data,
    SettingsPage::Security,
    SettingsPage::Advanced,
    SettingsPage::About,
];

const APPEARANCE_ROWS: [MoreRow; 7] = [
    MoreRow::Divider("Theme"),
    MoreRow::Setting(SettingId::Theme),
    MoreRow::Setting(SettingId::Accent),
    MoreRow::Divider("Display"),
    MoreRow::Setting(SettingId::Fullscreen),
    MoreRow::Divider("Thumbnails"),
    MoreRow::Setting(SettingId::ThumbnailSize),
];

const LIBRARY_ROWS: [MoreRow; 6] = [
    MoreRow::Divider("Storage"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Setting(SettingId::RescanLibrary),
    MoreRow::Divider("Display"),
    MoreRow::Setting(SettingId::LibrarySort),
    MoreRow::Setting(SettingId::DefaultCategory),
];

const READER_ROWS: [MoreRow; 7] = [
    MoreRow::Divider("Reading"),
    MoreRow::Setting(SettingId::ReadingMode),
    MoreRow::Divider("Continuous scroll"),
    MoreRow::Setting(SettingId::Zoom),
    MoreRow::Setting(SettingId::PrefetchBehind),
    MoreRow::Setting(SettingId::PrefetchAhead),
    MoreRow::Setting(SettingId::CacheMargin),
];

const DOWNLOADS_ROWS: [MoreRow; 3] = [
    MoreRow::Divider("Status"),
    MoreRow::Note("Download management is not implemented."),
    MoreRow::Note("Local library chapters are read directly from disk."),
];

const TRACKING_ROWS: [MoreRow; 3] = [
    MoreRow::Divider("Services"),
    MoreRow::Note("No tracking services configured."),
    MoreRow::Note("This build manages a local Mihon library only."),
];

const BROWSE_ROWS: [MoreRow; 5] = [
    MoreRow::Divider("Local source"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Note("Local source lists series directories in the library folder."),
    MoreRow::Note("Remote sources and Mihon extensions are not available."),
    MoreRow::Note("Browse reads the same local/ layout as Mihon."),
];

const DATA_ROWS: [MoreRow; 4] = [
    MoreRow::Divider("Storage"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Note("Progress: $XDG_DATA_HOME/tsuiyomi/progress.json"),
    MoreRow::Note("Backup import and export are not implemented yet."),
];

const SECURITY_ROWS: [MoreRow; 3] = [
    MoreRow::Divider("Privacy"),
    MoreRow::Setting(SettingId::Incognito),
    MoreRow::Note("Incognito stops writing reading progress."),
];

const ADVANCED_ROWS: [MoreRow; 4] = [
    MoreRow::Divider("Diagnostics"),
    MoreRow::Note("Log level comes from TSUIYOMI_LOG."),
    MoreRow::Note("Refresh scans synchronously and may block briefly."),
    MoreRow::Note("Scroll workers and page cache are tuned per session."),
];

impl SettingsPage {
    pub fn title(self) -> &'static str {
        match self {
            Self::Main => "Settings",
            Self::Appearance => "Appearance",
            Self::Library => "Library",
            Self::Reader => "Reader",
            Self::Downloads => "Downloads",
            Self::Tracking => "Tracking",
            Self::Browse => "Browse",
            Self::Data => "Data and storage",
            Self::Security => "Security",
            Self::Advanced => "Advanced",
            Self::About => "About",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Main => "",
            Self::Appearance => "Theme · accent · display",
            Self::Library => "Path · sort · categories",
            Self::Reader => "Mode · zoom · prefetch",
            Self::Downloads => "Not implemented",
            Self::Tracking => "Local library only",
            Self::Browse => "Local source",
            Self::Data => "Storage · backups",
            Self::Security => "Incognito",
            Self::Advanced => "Logging · diagnostics",
            Self::About => "Version · graphics",
        }
    }

    pub fn rows(self) -> &'static [MoreRow] {
        match self {
            Self::Main => &[],
            Self::Appearance => &APPEARANCE_ROWS,
            Self::Library => &LIBRARY_ROWS,
            Self::Reader => &READER_ROWS,
            Self::Downloads => &DOWNLOADS_ROWS,
            Self::Tracking => &TRACKING_ROWS,
            Self::Browse => &BROWSE_ROWS,
            Self::Data => &DATA_ROWS,
            Self::Security => &SECURITY_ROWS,
            Self::Advanced => &ADVANCED_ROWS,
            Self::About => &[],
        }
    }
}

pub const READING_MODES: [ReadingMode; 6] = [
    ReadingMode::Default,
    ReadingMode::LeftToRight,
    ReadingMode::RightToLeft,
    ReadingMode::Vertical,
    ReadingMode::Webtoon,
    ReadingMode::ContinuousVertical,
];

impl MoreSection {
    pub fn title(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::Downloads => "Downloads",
            Self::Categories => "Categories",
            Self::Statistics => "Statistics",
            Self::Storage => "Data and storage",
            Self::About => "About",
            Self::Help => "Help",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReaderMode {
    #[default]
    Page,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollCacheConfig {
    /// Pages loaded before the first visible page.
    pub prefetch_behind: usize,
    /// Pages loaded after the last visible page.
    pub prefetch_ahead: usize,
    /// Extra cached pages kept around the viewport before eviction.
    pub cache_margin: usize,
}

impl Default for ScrollCacheConfig {
    fn default() -> Self {
        Self {
            prefetch_behind: 2,
            prefetch_ahead: 2,
            cache_margin: 4,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollPageLayout {
    pub start: u32,
    pub rows: u16,
    pub cell_width: u16,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

pub struct ScrollState {
    pub generation: u64,
    pub layout: Vec<ScrollPageLayout>,
    pub protocols: Vec<Option<SlicedProtocol>>,
    pub requested: Vec<bool>,
    pub errors: Vec<Option<String>>,
    /// First visible row.
    pub offset: u32,
    pub width: u16,
    pub viewport_height: u16,
    pub total_rows: u32,
}

pub struct ReaderState {
    pub series_title: String,
    pub chapter_title: String,
    pub chapter_path: PathBuf,
    pub pages: Vec<Page>,
    /// Cached pixel dimensions, filled on first scroll layout.
    pub page_sizes: Vec<(u32, u32)>,
    pub page_index: usize,
    pub protocol: Option<StatefulProtocol>,
    pub scroll: Option<ScrollState>,
    /// New zoom/width layout being prepared while the old one stays on screen.
    pub pending_scroll: Option<ScrollState>,
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
    pub chapter_list_state: ListState,
    pub reader: Option<ReaderState>,
    pub image_picker: Option<Picker>,
    /// Decoded custom cover for the selected series.
    pub cover: Option<StatefulProtocol>,
    pub cover_path: Option<PathBuf>,
    pub progress: ProgressStore,
    /// Worker that decodes/resizes/encodes scroll pages off the UI thread.
    pub(crate) scroll_worker: Option<ScrollWorker>,
    pub scroll_generation: u64,
    /// Tunables for lazy scroll loading; a future settings menu will expose these.
    pub scroll_cache: ScrollCacheConfig,
    /// Scroll-mode zoom percentage. 100 = fit width without upscaling.
    pub zoom: u16,
    pub config: Config,
    pub browse_view: BrowseView,
    pub browse_selection: usize,
    pub browse_list_state: ListState,
    pub browse_detail: Option<PathBuf>,
    pub browse_columns: usize,
    pub browse_grid_offset: usize,
    pub library_columns: usize,
    pub library_grid_offset: usize,
    pub tile_covers: HashMap<PathBuf, StatefulProtocol>,
    pub more_section: MoreSection,
    pub more_focus: MoreFocus,
    pub settings_page: SettingsPage,
    pub setting_index: usize,
    pub theme: Theme,
    pub accent: Option<String>,
    pub picker: Option<PickerState>,
    pub quit_confirm: bool,
    pub status: Option<String>,
}

impl AppState {
    pub fn new(library: Library, status: Option<String>) -> Self {
        Self {
            tab: 0,
            fullscreen: false,
            view: LibraryView::Series,
            reader_mode: ReaderMode::Page,
            library,
            category_index: 0,
            selected: 0,
            chapter_index: 0,
            chapter_list_state: ListState::default(),
            reader: None,
            image_picker: None,
            cover: None,
            cover_path: None,
            progress: ProgressStore::default(),
            scroll_worker: None,
            scroll_generation: 0,
            scroll_cache: ScrollCacheConfig::default(),
            zoom: 100,
            config: Config::default(),
            browse_view: BrowseView::Sources,
            browse_selection: 0,
            browse_list_state: ListState::default(),
            browse_detail: None,
            browse_columns: 1,
            browse_grid_offset: 0,
            library_columns: 1,
            library_grid_offset: 0,
            tile_covers: HashMap::new(),
            more_section: MoreSection::Settings,
            more_focus: MoreFocus::Content,
            settings_page: SettingsPage::Main,
            setting_index: 0,
            theme: Theme::default(),
            accent: None,
            picker: None,
            quit_confirm: false,
            status,
        }
    }

    pub fn set_image_picker(&mut self, picker: Picker) {
        self.image_picker = Some(picker);
    }

    /// Load the selected series' custom cover if it changed.
    pub fn ensure_cover(&mut self) {
        let path = self
            .selected_series()
            .and_then(|series| series.cover.clone());
        self.ensure_cover_path(path);
    }

    pub fn ensure_cover_path(&mut self, path: Option<PathBuf>) {
        if self.cover_path == path {
            return;
        }
        self.cover_path = path.clone();
        self.cover = None;

        let Some(path) = path else {
            return;
        };
        let Some(picker) = self.image_picker.as_ref() else {
            return;
        };
        let Ok(bytes) = read_cover(&path) else {
            return;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            return;
        };
        self.cover = Some(picker.new_resize_protocol(image));
    }

    /// Load one library tile cover into the grid cache.
    pub fn ensure_tile_cover(&mut self, path: &Path) {
        if self.tile_covers.contains_key(path) {
            return;
        }
        let Some(picker) = self.image_picker.as_ref() else {
            return;
        };
        let Ok(bytes) = read_cover(path) else {
            return;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            return;
        };
        if self.tile_covers.len() >= TILE_COVER_CACHE {
            if let Some(oldest) = self.tile_covers.keys().next().cloned() {
                self.tile_covers.remove(&oldest);
            }
        }
        self.tile_covers
            .insert(path.to_path_buf(), picker.new_resize_protocol(image));
    }

    fn invalidate_cover(&mut self) {
        self.cover = None;
        self.cover_path = None;
    }

    pub fn browse_previous(&mut self) {
        let step = match self.browse_view {
            BrowseView::Local => self.browse_columns.max(1),
            _ => 1,
        };
        self.browse_selection = self.browse_selection.saturating_sub(step);
        self.select_browse_entry(self.browse_selection);
    }

    pub fn browse_next(&mut self) {
        let count = self.browse_item_count();
        if count > 0 {
            let step = match self.browse_view {
                BrowseView::Local => self.browse_columns.max(1),
                _ => 1,
            };
            self.browse_selection = (self.browse_selection + step).min(count - 1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    pub fn browse_previous_column(&mut self) {
        if self.browse_view == BrowseView::Local {
            self.browse_selection = self.browse_selection.saturating_sub(1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    pub fn browse_next_column(&mut self) {
        if self.browse_view == BrowseView::Local {
            let count = self.browse_item_count();
            if count > 0 {
                self.browse_selection = (self.browse_selection + 1).min(count - 1);
                self.select_browse_entry(self.browse_selection);
            }
        }
    }

    pub fn browse_open(&mut self) {
        match self.browse_view {
            BrowseView::Sources => {
                if self.browse_selection == 0 {
                    self.browse_view = BrowseView::Local;
                    self.browse_selection = 0;
                    self.select_browse_entry(0);
                }
            }
            BrowseView::Local => {
                let Some(path) = self
                    .selected_browse_series()
                    .map(|series| series.path.clone())
                else {
                    return;
                };
                self.browse_detail = Some(path);
                self.browse_view = BrowseView::Details;
            }
            BrowseView::Details => {
                if let Some(path) = self.browse_detail.clone() {
                    self.open_browse_series(&path);
                }
            }
        }
    }

    pub fn browse_parent(&mut self) {
        match self.browse_view {
            BrowseView::Details => {
                self.browse_view = BrowseView::Local;
                self.clamp_browse_selection();
            }
            BrowseView::Local => {
                self.browse_view = BrowseView::Sources;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::Sources => {}
        }
    }

    /// Rescan the local source when refreshing its item list.
    pub fn refresh_browse(&mut self) {
        if self.browse_view == BrowseView::Local {
            self.refresh();
        }
        self.clamp_browse_selection();
    }

    pub fn browse_item_count(&self) -> usize {
        match self.browse_view {
            BrowseView::Sources => 1,
            BrowseView::Local => self.library.series.len(),
            BrowseView::Details => 0,
        }
    }

    /// Indices into `library.series` for the Local source, sorted by title.
    pub fn browse_local_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.library.series.len()).collect();
        indices.sort_by(|left, right| {
            natural_cmp(
                &self.library.series[*left].title,
                &self.library.series[*right].title,
            )
        });
        indices
    }

    pub fn selected_browse_series(&self) -> Option<&Series> {
        if self.browse_view != BrowseView::Local {
            return None;
        }
        let index = *self.browse_local_indices().get(self.browse_selection)?;
        self.library.series.get(index)
    }

    pub fn browse_detail_series(&self) -> Option<&Series> {
        let path = self.browse_detail.as_deref()?;
        self.library
            .series
            .iter()
            .find(|series| series.path == path)
    }

    pub fn select_browse_index(&mut self, index: usize) {
        if index < self.browse_item_count() {
            self.browse_selection = index;
            self.select_browse_entry(index);
        }
    }

    fn clamp_browse_selection(&mut self) {
        let count = self.browse_item_count();
        if count == 0 {
            self.browse_selection = 0;
            self.browse_list_state.select(None);
        } else {
            self.browse_selection = self.browse_selection.min(count - 1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    fn select_browse_entry(&mut self, index: usize) {
        self.browse_list_state
            .select((self.browse_item_count() > 0).then_some(index));
    }

    fn open_browse_series(&mut self, path: &Path) {
        let category = self
            .library
            .series
            .iter()
            .find(|series| series.path == path)
            .and_then(|series| series.categories.first())
            .cloned();
        if let Some(category) = category {
            if let Some(index) = self
                .library
                .categories
                .iter()
                .position(|name| name == &category)
            {
                self.category_index = index;
            }
        }
        if let Some(position) = self
            .visible_indices()
            .into_iter()
            .position(|index| self.library.series[index].path == path)
        {
            self.view = LibraryView::Series;
            self.selected = position;
            self.status = None;
            self.open();
        }
    }

    pub(crate) fn set_scroll_worker(&mut self, worker: ScrollWorker) {
        worker.set_generation(self.scroll_generation);
        self.scroll_worker = Some(worker);
    }

    /// Apply any scroll page built by the worker. Returns whether the UI changed.
    pub fn poll_scroll_results(&mut self) -> bool {
        let mut changed = false;
        loop {
            let Some(result) = self
                .scroll_worker
                .as_ref()
                .and_then(ScrollWorker::try_result)
            else {
                break;
            };
            let Some(reader) = self.reader.as_mut() else {
                continue;
            };

            let generation = result.generation;
            let applied = if reader
                .pending_scroll
                .as_ref()
                .is_some_and(|scroll| scroll.generation == generation)
            {
                if let Some(scroll) = reader.pending_scroll.as_mut() {
                    apply_scroll_result(scroll, result)
                } else {
                    false
                }
            } else if reader
                .scroll
                .as_ref()
                .is_some_and(|scroll| scroll.generation == generation)
            {
                if let Some(scroll) = reader.scroll.as_mut() {
                    apply_scroll_result(scroll, result)
                } else {
                    false
                }
            } else {
                false
            };

            if applied {
                changed = true;
            }
        }

        if self.try_promote_pending() {
            changed = true;
        }
        changed
    }

    fn try_promote_pending(&mut self) -> bool {
        let Some(reader) = self.reader.as_mut() else {
            return false;
        };
        let Some(pending) = reader.pending_scroll.as_ref() else {
            return false;
        };
        if !scroll_visible_ready(pending) {
            return false;
        }

        let pending = reader.pending_scroll.take().expect("pending checked above");
        reader.page_index = page_at_offset(&pending);
        reader.scroll = Some(pending);
        true
    }

    fn bump_scroll_generation(&mut self) -> u64 {
        self.scroll_generation = self.scroll_generation.wrapping_add(1);
        if let Some(worker) = self.scroll_worker.as_ref() {
            worker.set_generation(self.scroll_generation);
        }
        self.scroll_generation
    }

    pub fn set_progress(&mut self, progress: ProgressStore) {
        self.progress = progress;
    }

    pub fn protocol_name(&self) -> &'static str {
        match self.image_picker.as_ref().map(Picker::protocol_type) {
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

    /// Indices into `library.series` for the current category, in display order.
    pub fn visible_indices(&self) -> Vec<usize> {
        let category = self.category_name();
        let mut indices: Vec<usize> = self
            .library
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| in_category(series, category))
            .map(|(index, _)| index)
            .collect();

        match self.config.library_sort {
            LibrarySort::Natural => {}
            LibrarySort::TitleAsc => indices.sort_by(|left, right| {
                self.library.series[*left]
                    .title
                    .to_lowercase()
                    .cmp(&self.library.series[*right].title.to_lowercase())
            }),
            LibrarySort::TitleDesc => indices.sort_by(|left, right| {
                self.library.series[*right]
                    .title
                    .to_lowercase()
                    .cmp(&self.library.series[*left].title.to_lowercase())
            }),
        }
        indices
    }

    pub fn visible_count(&self) -> usize {
        self.visible_indices().len()
    }

    pub fn selected_series(&self) -> Option<&Series> {
        let index = *self.visible_indices().get(self.selected)?;
        self.library.series.get(index)
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
                self.selected = self.selected.saturating_sub(self.library_columns.max(1));
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
                    self.selected = (self.selected + self.library_columns.max(1)).min(last);
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

    pub fn previous_column(&mut self) {
        if self.view == LibraryView::Series {
            self.selected = self.selected.saturating_sub(1);
            self.status = None;
        }
    }

    pub fn next_column(&mut self) {
        if self.view == LibraryView::Series {
            if let Some(last) = self.visible_count().checked_sub(1) {
                self.selected = (self.selected + 1).min(last);
                self.status = None;
            }
        }
    }

    pub fn next_page(&mut self) {
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            let page = self.reader.as_ref().map_or(0, |reader| reader.page_index);
            self.jump_to_page(page + 1);
        } else {
            self.change_page(true);
        }
    }

    pub fn previous_page(&mut self) {
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            let (page, aligned) = {
                let Some(reader) = self.reader.as_ref() else {
                    return;
                };
                let Some(scroll) = reader.scroll.as_ref() else {
                    return;
                };
                let page = reader.page_index;
                let current = scroll.offset as i32;
                let start = scroll.layout.get(page).map_or(0, |page| page.start) as i32;
                (page, current <= start)
            };
            if aligned {
                self.jump_to_page(page.saturating_sub(1));
            } else {
                self.jump_to_page(page);
            }
        } else {
            self.change_page(false);
        }
    }

    pub fn next_screen(&mut self) {
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            self.scroll_lines(self.scroll_page_step());
        } else {
            self.change_page(true);
        }
    }

    pub fn previous_screen(&mut self) {
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            self.scroll_lines(-self.scroll_page_step());
        } else {
            self.change_page(false);
        }
    }

    pub fn set_reading_mode(&mut self, mode: ReadingMode) {
        self.reader_mode = match mode {
            ReadingMode::LeftToRight | ReadingMode::RightToLeft | ReadingMode::Vertical => {
                ReaderMode::Page
            }
            ReadingMode::Default | ReadingMode::Webtoon | ReadingMode::ContinuousVertical => {
                ReaderMode::Scroll
            }
        };
    }

    /// Apply the persisted config to runtime state.
    pub fn set_config(&mut self, mut config: Config) {
        self.reader_mode = ReaderMode::Page;
        self.set_reading_mode(config.reading_mode);
        self.theme = config.theme;
        self.accent = config.accent.clone();
        palette::apply(self.theme, self.accent.as_deref());
        self.fullscreen = config.fullscreen;
        config.thumbnail_width = config.thumbnail_width.clamp(8, 28);
        self.zoom = config.reader_zoom.clamp(50, 300);
        self.scroll_cache = ScrollCacheConfig {
            prefetch_behind: config.scroll_prefetch_behind.min(16),
            prefetch_ahead: config.scroll_prefetch_ahead.min(16),
            cache_margin: config.scroll_cache_margin.min(32),
        };
        self.config = config;
        if let Some(name) = self.config.default_category.as_deref() {
            if let Some(index) = self
                .library
                .categories
                .iter()
                .position(|category| category == name)
            {
                self.category_index = index;
            }
        }
    }

    /// Persist reading progress unless Incognito mode is on.
    fn record_progress(&mut self, chapter: &std::path::Path, page: usize, pages: usize) {
        if self.config.incognito {
            return;
        }
        let _ = self.progress.record(chapter, page, pages);
    }

    /// Save current reader and appearance settings back to the config file.
    pub fn persist_settings(&mut self) {
        self.config.theme = self.theme;
        self.config.accent = self.accent.clone();
        self.config.reader_zoom = self.zoom;
        self.config.scroll_prefetch_behind = self.scroll_cache.prefetch_behind;
        self.config.scroll_prefetch_ahead = self.scroll_cache.prefetch_ahead;
        self.config.scroll_cache_margin = self.scroll_cache.cache_margin;
        // Tests must not touch the user's real config file.
        if !cfg!(test) {
            save_config(&self.config);
        }
    }

    /// Apply a theme immediately and persist it.
    pub fn apply_theme(&mut self, theme: Theme) {
        self.theme = theme;
        palette::apply(self.theme, self.accent.as_deref());
        self.persist_settings();
    }

    /// Apply an accent override (`None` = theme default) and persist it.
    pub fn apply_accent(&mut self, accent: Option<String>) {
        self.accent = accent;
        palette::apply(self.theme, self.accent.as_deref());
        self.persist_settings();
    }

    /// Human-readable label for the current accent setting.
    pub fn accent_name(&self) -> String {
        let Some(value) = self.accent.as_deref() else {
            return "Default".to_owned();
        };
        let base = palette::palette_for(self.theme);
        palette::accent_choices(&base)
            .into_iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(value))
            .map(|(name, _)| {
                let mut chars = name.chars();
                let first = chars
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or_default();
                format!("{first}{}", chars.as_str())
            })
            .unwrap_or_else(|| value.to_owned())
    }

    fn accent_choice_count(&self) -> usize {
        palette::accent_choices(&palette::palette_for(self.theme)).len() + 1
    }

    /// Open the highlighted settings page, picker, toggle, or action.
    pub fn activate_setting(&mut self) {
        if self.more_section != MoreSection::Settings {
            return;
        }
        if self.settings_page == SettingsPage::Main {
            if let Some(page) = SETTINGS_PAGES.get(self.setting_index).copied() {
                self.settings_page = page;
                self.setting_index = 0;
            }
            return;
        }

        let Some(setting) = self.setting_at(self.setting_index) else {
            return;
        };
        match setting {
            SettingId::Theme => self.open_theme_picker(),
            SettingId::Accent => self.open_accent_picker(),
            SettingId::Fullscreen => {
                self.config.fullscreen = !self.config.fullscreen;
                self.fullscreen = self.config.fullscreen;
                self.persist_settings();
            }
            SettingId::Incognito => {
                self.config.incognito = !self.config.incognito;
                self.persist_settings();
            }
            SettingId::RescanLibrary => self.refresh(),
            SettingId::ReadingMode | SettingId::LibrarySort | SettingId::DefaultCategory => {
                self.adjust_setting(1)
            }
            _ => {}
        }
    }

    /// Selectable rows in the current Settings view.
    fn setting_count(&self) -> usize {
        if self.settings_page == SettingsPage::Main {
            SETTINGS_PAGES.len()
        } else {
            self.settings_page
                .rows()
                .iter()
                .filter(|row| matches!(row, MoreRow::Setting(_)))
                .count()
        }
    }

    /// Setting under a selectable-row index.
    fn setting_at(&self, index: usize) -> Option<SettingId> {
        if self.settings_page == SettingsPage::Main {
            return None;
        }
        self.settings_page
            .rows()
            .iter()
            .filter_map(|row| match row {
                MoreRow::Setting(setting) => Some(*setting),
                _ => None,
            })
            .nth(index)
    }

    fn open_theme_picker(&mut self) {
        let selected = palette::THEMES
            .iter()
            .position(|theme| *theme == self.theme)
            .unwrap_or(0);
        self.picker = Some(PickerState {
            kind: PickerKind::Theme,
            selected,
            custom: String::new(),
            editing_custom: false,
        });
    }

    fn open_accent_picker(&mut self) {
        let base = palette::palette_for(self.theme);
        let choices = palette::accent_choices(&base);
        let mut selected = 0;
        let mut custom = String::new();
        if let Some(value) = self.accent.as_deref() {
            match choices
                .iter()
                .position(|(name, _)| name.eq_ignore_ascii_case(value))
            {
                Some(index) => selected = index,
                None => {
                    selected = choices.len();
                    custom = value.to_owned();
                }
            }
        }
        self.picker = Some(PickerState {
            kind: PickerKind::Accent,
            selected,
            custom,
            editing_custom: false,
        });
    }

    fn picker_len(&self, kind: PickerKind) -> usize {
        match kind {
            PickerKind::Theme => palette::THEMES.len(),
            PickerKind::Accent => self.accent_choice_count(),
        }
    }

    pub fn picker_previous(&mut self) {
        self.move_picker(-1);
    }

    pub fn picker_next(&mut self) {
        self.move_picker(1);
    }

    fn move_picker(&mut self, delta: i32) {
        let Some(kind) = self.picker.as_ref().map(|picker| picker.kind) else {
            return;
        };
        let len = self.picker_len(kind) as i32;
        if len == 0 {
            return;
        }
        if let Some(picker) = &mut self.picker {
            picker.selected = (picker.selected as i32 + delta).rem_euclid(len) as usize;
            picker.editing_custom = false;
        }
        self.preview_picker();
    }

    /// Apply the highlighted picker choice to the palette without persisting it.
    fn preview_picker(&self) {
        let Some(picker) = self.picker.as_ref() else {
            palette::apply(self.theme, self.accent.as_deref());
            return;
        };
        match picker.kind {
            PickerKind::Theme => {
                let theme = palette::THEMES
                    .get(picker.selected)
                    .copied()
                    .unwrap_or(self.theme);
                palette::apply(theme, self.accent.as_deref());
            }
            PickerKind::Accent => {
                let base = palette::palette_for(self.theme);
                let choices = palette::accent_choices(&base);
                if picker.selected < choices.len() {
                    let name = choices[picker.selected].0;
                    let accent = (name != "default").then(|| name.to_owned());
                    palette::apply(self.theme, accent.as_deref());
                } else if palette::parse_hex(&picker.custom).is_some() {
                    palette::apply(self.theme, Some(picker.custom.as_str()));
                } else {
                    palette::apply(self.theme, self.accent.as_deref());
                }
            }
        }
    }

    pub fn picker_back(&mut self) {
        let editing = self
            .picker
            .as_ref()
            .map(|picker| picker.editing_custom)
            .unwrap_or(false);
        if editing {
            if let Some(picker) = &mut self.picker {
                picker.editing_custom = false;
            }
            self.preview_picker();
        } else {
            self.picker = None;
            palette::apply(self.theme, self.accent.as_deref());
        }
    }

    pub fn picker_char(&mut self, character: char) {
        if !character.is_ascii_hexdigit() && character != '#' {
            return;
        }
        if self.picker.as_ref().map(|picker| picker.kind) != Some(PickerKind::Accent) {
            return;
        }
        let custom_index = self.accent_choice_count() - 1;
        if let Some(picker) = &mut self.picker {
            picker.selected = custom_index;
            picker.editing_custom = true;
            if character == '#' {
                if picker.custom.is_empty() {
                    picker.custom.push('#');
                }
            } else if picker.custom.chars().filter(|c| *c != '#').count() < 6 {
                picker.custom.push(character);
            }
        }
        self.preview_picker();
    }

    pub fn picker_backspace(&mut self) {
        if self.picker.as_ref().map(|picker| picker.kind) != Some(PickerKind::Accent) {
            return;
        }
        let custom_index = self.accent_choice_count() - 1;
        if let Some(picker) = &mut self.picker {
            picker.selected = custom_index;
            picker.editing_custom = true;
            picker.custom.pop();
        }
        self.preview_picker();
    }

    pub fn picker_confirm(&mut self) {
        let Some(picker) = self.picker.clone() else {
            return;
        };
        match picker.kind {
            PickerKind::Theme => {
                if let Some(theme) = palette::THEMES.get(picker.selected).copied() {
                    self.apply_theme(theme);
                }
                self.picker = None;
            }
            PickerKind::Accent => {
                let choices = palette::accent_choices(&palette::palette_for(self.theme));
                if picker.selected < choices.len() {
                    let name = choices[picker.selected].0;
                    let accent = (name != "default").then(|| name.to_owned());
                    self.apply_accent(accent);
                    self.picker = None;
                } else if palette::parse_hex(&picker.custom).is_some() {
                    let digits = picker.custom.trim().trim_start_matches('#');
                    self.apply_accent(Some(format!("#{digits}")));
                    self.picker = None;
                } else if let Some(picker) = &mut self.picker {
                    picker.editing_custom = true;
                }
            }
        }
    }

    pub fn next_more_section(&mut self) {
        let index = MORE_SECTIONS
            .iter()
            .position(|section| *section == self.more_section)
            .unwrap_or(0);
        self.set_more_section((index + 1).min(MORE_SECTIONS.len() - 1));
    }

    pub fn previous_more_section(&mut self) {
        let index = MORE_SECTIONS
            .iter()
            .position(|section| *section == self.more_section)
            .unwrap_or(0);
        self.set_more_section(index.saturating_sub(1));
    }

    pub fn set_more_section(&mut self, index: usize) {
        if let Some(section) = MORE_SECTIONS.get(index) {
            self.more_section = *section;
            self.settings_page = SettingsPage::Main;
            self.setting_index = 0;
        }
    }

    pub fn focus_more_content(&mut self) {
        self.more_focus = MoreFocus::Content;
    }

    pub fn leave_more(&mut self) {
        if self.more_focus == MoreFocus::Content {
            if self.more_section == MoreSection::Settings
                && self.settings_page != SettingsPage::Main
            {
                // Return to the category that opened this page instead of the top.
                if let Some(index) = SETTINGS_PAGES
                    .iter()
                    .position(|page| *page == self.settings_page)
                {
                    self.setting_index = index;
                }
                self.settings_page = SettingsPage::Main;
            } else {
                self.more_focus = MoreFocus::Sections;
            }
        } else {
            self.select_tab(0);
        }
    }

    pub fn previous_setting(&mut self) {
        if self.more_section != MoreSection::Settings {
            return;
        }
        self.setting_index = self.setting_index.saturating_sub(1);
    }

    pub fn next_setting(&mut self) {
        if self.more_section != MoreSection::Settings {
            return;
        }
        let count = self.setting_count();
        if count == 0 {
            return;
        }
        self.setting_index = (self.setting_index + 1).min(count - 1);
    }

    pub fn adjust_setting(&mut self, delta: i32) {
        if self.more_section != MoreSection::Settings || self.settings_page == SettingsPage::Main {
            return;
        }
        let Some(setting) = self.setting_at(self.setting_index) else {
            return;
        };

        match setting {
            SettingId::ReadingMode => {
                let index = READING_MODES
                    .iter()
                    .position(|mode| *mode == self.config.reading_mode)
                    .unwrap_or(0);
                let next = (index as i32 + delta).rem_euclid(READING_MODES.len() as i32) as usize;
                self.config.reading_mode = READING_MODES[next];
                self.set_reading_mode(self.config.reading_mode);
                if self.view == LibraryView::Reader {
                    match self.reader_mode {
                        ReaderMode::Page => self.load_page(),
                        ReaderMode::Scroll => self.build_scroll(terminal_width()),
                    }
                }
            }
            SettingId::Zoom => {
                let zoom = (i32::from(self.zoom) + delta * 10).clamp(50, 300) as u16;
                self.set_zoom(zoom);
            }
            SettingId::PrefetchBehind => {
                let value = (self.scroll_cache.prefetch_behind as i32 + delta).clamp(0, 8);
                self.scroll_cache.prefetch_behind = value as usize;
            }
            SettingId::PrefetchAhead => {
                let value = (self.scroll_cache.prefetch_ahead as i32 + delta).clamp(0, 8);
                self.scroll_cache.prefetch_ahead = value as usize;
            }
            SettingId::CacheMargin => {
                let value = (self.scroll_cache.cache_margin as i32 + delta).clamp(0, 12);
                self.scroll_cache.cache_margin = value as usize;
            }
            SettingId::LibrarySort => {
                let sorts = [
                    LibrarySort::Natural,
                    LibrarySort::TitleAsc,
                    LibrarySort::TitleDesc,
                ];
                let index = sorts
                    .iter()
                    .position(|sort| *sort == self.config.library_sort)
                    .unwrap_or(0);
                let next = (index as i32 + delta).rem_euclid(sorts.len() as i32) as usize;
                self.config.library_sort = sorts[next];
                self.clamp_selection();
            }
            SettingId::DefaultCategory => {
                let count = self.library.categories.len();
                if count == 0 {
                    return;
                }
                let current = self
                    .config
                    .default_category
                    .as_deref()
                    .and_then(|name| {
                        self.library
                            .categories
                            .iter()
                            .position(|category| category == name)
                    })
                    .unwrap_or(self.category_index);
                let next = (current as i32 + delta).rem_euclid(count as i32) as usize;
                self.config.default_category = Some(self.library.categories[next].clone());
                self.category_index = next;
                self.reset_selection();
            }
            SettingId::Theme => {
                let index = palette::THEMES
                    .iter()
                    .position(|theme| *theme == self.theme)
                    .unwrap_or(0);
                let next = (index as i32 + delta).rem_euclid(palette::THEMES.len() as i32) as usize;
                self.theme = palette::THEMES[next];
                palette::apply(self.theme, self.accent.as_deref());
            }
            SettingId::Accent => {
                let choices = palette::accent_choices(&palette::palette_for(self.theme));
                let current = self
                    .accent
                    .as_deref()
                    .and_then(|value| {
                        choices
                            .iter()
                            .position(|(name, _)| name.eq_ignore_ascii_case(value))
                    })
                    .unwrap_or(0);
                let next = (current as i32 + delta).rem_euclid(choices.len() as i32) as usize;
                self.accent = (next != 0).then(|| choices[next].0.to_owned());
                palette::apply(self.theme, self.accent.as_deref());
            }
            SettingId::Fullscreen => {
                self.config.fullscreen = !self.config.fullscreen;
                self.fullscreen = self.config.fullscreen;
            }
            SettingId::ThumbnailSize => {
                self.config.thumbnail_width =
                    (i32::from(self.config.thumbnail_width) + delta).clamp(8, 28) as u16;
            }
            SettingId::Incognito => {
                self.config.incognito = !self.config.incognito;
            }
            SettingId::LibraryPath | SettingId::RescanLibrary => return,
        }

        self.persist_settings();
    }

    pub fn set_setting_ratio(&mut self, index: usize, ratio: f64) {
        let ratio = ratio.clamp(0.0, 1.0);
        let Some(setting) = self.setting_at(index) else {
            return;
        };
        self.setting_index = index;
        match setting {
            SettingId::Zoom => {
                let zoom = (50.0 + ratio * 250.0).round() as u16;
                self.set_zoom(zoom);
            }
            SettingId::PrefetchBehind => {
                self.scroll_cache.prefetch_behind = (ratio * 8.0).round() as usize;
            }
            SettingId::PrefetchAhead => {
                self.scroll_cache.prefetch_ahead = (ratio * 8.0).round() as usize;
            }
            SettingId::CacheMargin => {
                self.scroll_cache.cache_margin = (ratio * 12.0).round() as usize;
            }
            SettingId::ThumbnailSize => {
                self.config.thumbnail_width = (8.0 + ratio * 20.0).round() as u16;
            }
            _ => return,
        }
        self.persist_settings();
    }

    pub fn zoom_in(&mut self) {
        self.set_zoom(self.zoom.saturating_add(10).min(300));
    }

    pub fn zoom_out(&mut self) {
        self.set_zoom(self.zoom.saturating_sub(10).max(50));
    }

    fn set_zoom(&mut self, zoom: u16) {
        if self.zoom == zoom {
            return;
        }
        self.zoom = zoom;
        if self.view == LibraryView::Reader && self.reader_mode == ReaderMode::Scroll {
            self.build_scroll(terminal_width());
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
                let start = scroll.layout.get(page).map_or(0, |page| page.start);
                let max_scroll = scroll
                    .total_rows
                    .saturating_sub(u32::from(scroll.viewport_height));
                scroll.offset = start.min(max_scroll);
                reader.page_index = page;
                reader.error = None;
                (reader.chapter_path.clone(), reader.pages.len())
            };
            self.record_progress(&chapter_path, page, pages);
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
            self.record_progress(&chapter_path, page, pages);
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
            page_sizes: Vec::new(),
            page_index: start_page,
            protocol: None,
            scroll: None,
            pending_scroll: None,
            error: None,
        });
        self.view = LibraryView::Reader;
        self.status = None;
        if let Some((path, page, pages)) = self.reader.as_ref().map(|reader| {
            (
                reader.chapter_path.clone(),
                reader.page_index,
                reader.pages.len(),
            )
        }) {
            self.record_progress(&path, page, pages);
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

        if let Some((path, page, pages)) = self.reader.as_ref().map(|reader| {
            (
                reader.chapter_path.clone(),
                reader.page_index,
                reader.pages.len(),
            )
        }) {
            self.record_progress(&path, page, pages);
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

        let Some(picker) = self.image_picker.as_ref() else {
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
        let Some(picker) = self.image_picker.clone() else {
            self.set_reader_error("No terminal graphics backend.");
            return;
        };

        let font = picker.font_size();
        let font_width = u32::from(font.width.max(1));
        let font_height = u32::from(font.height.max(1));
        let available_pixel_width = u32::from(width.max(1)) * font_width;
        let zoom = f64::from(self.zoom) / 100.0;

        let sizes = match self.page_sizes(&pages) {
            Ok(sizes) => sizes,
            Err(error) => {
                self.set_reader_error(error);
                return;
            }
        };

        let mut layout = Vec::with_capacity(sizes.len());
        let mut total_rows = 0u32;
        for (source_width, source_height) in sizes {
            let fit_scale = (f64::from(available_pixel_width) / f64::from(source_width)).min(1.0);
            let scale = fit_scale * zoom;
            let pixel_width = (f64::from(source_width) * scale).round().max(1.0) as u32;
            let rows =
                ((f64::from(source_height) * scale / f64::from(font_height)).ceil() as u32).max(1);
            if rows > u32::from(u16::MAX) {
                self.set_reader_error("A page is too tall for scroll mode.");
                return;
            }

            let pixel_height = rows * font_height;
            let cell_width = pixel_width.div_ceil(font_width).max(1);
            if cell_width > u32::from(u16::MAX) {
                self.set_reader_error("A page is too wide for scroll mode.");
                return;
            }

            layout.push(ScrollPageLayout {
                start: total_rows,
                rows: rows as u16,
                cell_width: cell_width as u16,
                pixel_width,
                pixel_height,
            });
            total_rows = total_rows.saturating_add(rows);
        }

        let viewport_height = self
            .reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .map_or(10, |scroll| scroll.viewport_height)
            .max(1);
        let page_index = self.reader.as_ref().map_or(0, |reader| reader.page_index);
        let start_row = layout.get(page_index).map_or(0, |page| page.start);
        let total_rows = total_rows.max(1);
        let max_scroll = total_rows.saturating_sub(u32::from(viewport_height));
        let offset = start_row.min(max_scroll);
        let page_count = layout.len();
        let generation = self.bump_scroll_generation();

        if let Some(reader) = self.reader.as_mut() {
            reader.error = None;
            let state = ScrollState {
                generation,
                layout,
                protocols: (0..page_count).map(|_| None).collect(),
                requested: vec![false; page_count],
                errors: vec![None; page_count],
                offset,
                width: width.max(1),
                viewport_height,
                total_rows,
            };
            if reader.scroll.is_some() {
                reader.pending_scroll = Some(state);
            } else {
                reader.scroll = Some(state);
            }
        }
    }

    fn page_sizes(&mut self, pages: &[Page]) -> Result<Vec<(u32, u32)>, String> {
        if let Some(reader) = self.reader.as_ref() {
            if reader.page_sizes.len() == pages.len() {
                return Ok(reader.page_sizes.clone());
            }
        }

        let mut sizes = Vec::with_capacity(pages.len());
        for page in pages {
            let bytes = read_page(page).map_err(|error| error.to_string())?;
            let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|error| format!("{}: {error}", page.name))?;
            let size = reader
                .into_dimensions()
                .map_err(|error| format!("{}: {error}", page.name))?;
            sizes.push(size);
        }

        if let Some(reader) = self.reader.as_mut() {
            reader.page_sizes = sizes.clone();
        }
        Ok(sizes)
    }

    pub(crate) fn ensure_scroll_width(&mut self, width: u16) {
        let needs_rebuild = self
            .reader
            .as_ref()
            .and_then(|reader| reader.pending_scroll.as_ref().or(reader.scroll.as_ref()))
            .is_some_and(|scroll| scroll.width != width.max(1));
        if needs_rebuild {
            self.build_scroll(width);
        }
    }

    pub(crate) fn ensure_scroll_view(&mut self, viewport_height: u16) {
        let (load_start, load_end, first, last, page, pending) = {
            let Some(reader) = self.reader.as_mut() else {
                return;
            };
            let pending = reader.pending_scroll.is_some();
            let scroll = if pending {
                let Some(scroll) = reader.pending_scroll.as_mut() else {
                    return;
                };
                scroll
            } else {
                let Some(scroll) = reader.scroll.as_mut() else {
                    return;
                };
                scroll
            };

            scroll.viewport_height = viewport_height.max(1);
            let max_scroll = scroll
                .total_rows
                .saturating_sub(u32::from(scroll.viewport_height));
            if scroll.offset > max_scroll {
                scroll.offset = max_scroll;
            }

            let top = scroll.offset;
            let bottom = top.saturating_add(u32::from(scroll.viewport_height));
            let first = scroll
                .layout
                .iter()
                .position(|page| page.start.saturating_add(u32::from(page.rows)) > top)
                .unwrap_or(0);
            let last = scroll
                .layout
                .iter()
                .rposition(|page| page.start < bottom)
                .unwrap_or(first);
            let load_start = first.saturating_sub(self.scroll_cache.prefetch_behind);
            let load_end = (last + self.scroll_cache.prefetch_ahead)
                .min(scroll.layout.len().saturating_sub(1));
            let page = page_at_offset(scroll);

            (load_start, load_end, first, last, page, pending)
        };

        if !pending {
            if let Some(reader) = self.reader.as_mut() {
                reader.page_index = page;
            }
        }

        let mut to_request = Vec::new();
        if let Some(reader) = self.reader.as_mut() {
            if !reader.pages.is_empty() {
                let scroll = if pending {
                    reader.pending_scroll.as_mut()
                } else {
                    reader.scroll.as_mut()
                };
                if let Some(scroll) = scroll {
                    for index in load_start..=load_end {
                        if scroll.protocols.get(index).is_some_and(Option::is_some)
                            || scroll.requested.get(index).is_some_and(|value| *value)
                            || scroll.errors.get(index).is_some_and(Option::is_some)
                        {
                            continue;
                        }
                        if let Some(requested) = scroll.requested.get_mut(index) {
                            *requested = true;
                        }
                        to_request.push(index);
                    }
                }
            }
        }

        if !to_request.is_empty() {
            if let Some(worker) = self.scroll_worker.as_ref() {
                let requests: Vec<(usize, Page, ScrollPageLayout, u64)> = match self.reader.as_ref()
                {
                    Some(reader) => {
                        let scroll = if pending {
                            reader.pending_scroll.as_ref()
                        } else {
                            reader.scroll.as_ref()
                        };
                        match scroll {
                            Some(scroll) => to_request
                                .iter()
                                .filter_map(|index| {
                                    Some((
                                        *index,
                                        reader.pages.get(*index)?.clone(),
                                        scroll.layout.get(*index).copied()?,
                                        scroll.generation,
                                    ))
                                })
                                .collect(),
                            None => Vec::new(),
                        }
                    }
                    None => Vec::new(),
                };

                if let Some(picker) = self.image_picker.clone() {
                    for (index, page, layout, generation) in requests {
                        worker.request(BuildRequest {
                            generation,
                            index,
                            page,
                            layout,
                            picker: picker.clone(),
                        });
                    }
                }
            } else {
                for index in to_request {
                    self.load_scroll_page(index);
                }
            }
        }

        // Keep memory bounded: drop protocols far outside the viewport.
        if let Some(reader) = self.reader.as_mut() {
            let scroll = if pending {
                reader.pending_scroll.as_mut()
            } else {
                reader.scroll.as_mut()
            };
            if let Some(scroll) = scroll {
                let keep_start = first.saturating_sub(self.scroll_cache.cache_margin);
                let keep_end = last + self.scroll_cache.cache_margin;
                for (index, protocol) in scroll.protocols.iter_mut().enumerate() {
                    if index < keep_start || index > keep_end {
                        *protocol = None;
                        if let Some(requested) = scroll.requested.get_mut(index) {
                            *requested = false;
                        }
                    }
                }
            }
        }

        self.try_promote_pending();
    }

    fn load_scroll_page(&mut self, index: usize) {
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        let pending = reader.pending_scroll.is_some();
        let scroll = if pending {
            reader.pending_scroll.as_ref()
        } else {
            reader.scroll.as_ref()
        };
        let Some(scroll) = scroll else {
            return;
        };
        let Some(page) = reader.pages.get(index).cloned() else {
            return;
        };
        let Some(layout) = scroll.layout.get(index).copied() else {
            return;
        };
        let Some(picker) = self.image_picker.clone() else {
            self.set_reader_error("No terminal graphics backend.");
            return;
        };

        let result = build_scroll_protocol(&picker, &page, layout);
        if let Some(reader) = self.reader.as_mut() {
            let scroll = if pending {
                reader.pending_scroll.as_mut()
            } else {
                reader.scroll.as_mut()
            };
            if let Some(scroll) = scroll {
                if let Some(requested) = scroll.requested.get_mut(index) {
                    *requested = false;
                }
                match result {
                    Ok(protocol) => {
                        if let Some(slot) = scroll.protocols.get_mut(index) {
                            *slot = Some(protocol);
                        }
                        if let Some(error) = scroll.errors.get_mut(index) {
                            *error = None;
                        }
                    }
                    Err(error) => {
                        if let Some(slot) = scroll.errors.get_mut(index) {
                            *slot = Some(error);
                        }
                    }
                }
            }
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

            let max_scroll = scroll
                .total_rows
                .saturating_sub(u32::from(scroll.viewport_height))
                as i32;
            let current = scroll.offset as i32;
            let next = (current + delta).clamp(0, max_scroll);
            scroll.offset = next as u32;
            let at_end = next >= max_scroll;

            let mut page = scroll
                .layout
                .iter()
                .rposition(|page| page.start as i32 <= next)
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
            self.record_progress(&chapter_path, page, pages);
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
        let index = index.min(TAB_COUNT - 1);
        if index == 3 && self.tab != 3 {
            self.browse_view = BrowseView::Sources;
            self.browse_selection = 0;
            self.select_browse_entry(0);
        }
        self.tab = index;
    }

    pub fn next_tab(&mut self) {
        self.select_tab((self.tab + 1) % TAB_COUNT);
    }

    pub fn previous_tab(&mut self) {
        self.select_tab(if self.tab == 0 {
            TAB_COUNT - 1
        } else {
            self.tab - 1
        });
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
                self.invalidate_cover();
                self.tile_covers.clear();
                self.view = LibraryView::Series;
                self.reader = None;
                self.chapter_index = 0;
                self.chapter_list_state.select(None);
                self.category_index = self
                    .config
                    .default_category
                    .as_deref()
                    .and_then(|name| {
                        self.library
                            .categories
                            .iter()
                            .position(|category| category == name)
                    })
                    .unwrap_or_else(|| {
                        self.category_index
                            .min(self.library.categories.len().saturating_sub(1))
                    });
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
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(count - 1);
        }
    }
}

fn apply_scroll_result(scroll: &mut ScrollState, result: BuildResult) -> bool {
    if let Some(requested) = scroll.requested.get_mut(result.index) {
        *requested = false;
    }
    match result.result {
        Ok(protocol) => {
            if let Some(slot) = scroll.protocols.get_mut(result.index) {
                *slot = Some(protocol);
            }
            if let Some(error) = scroll.errors.get_mut(result.index) {
                *error = None;
            }
            true
        }
        Err(error) => {
            if let Some(slot) = scroll.errors.get_mut(result.index) {
                *slot = Some(error);
            }
            true
        }
    }
}

fn scroll_visible_ready(scroll: &ScrollState) -> bool {
    if scroll.layout.is_empty() {
        return true;
    }

    let top = scroll.offset;
    let bottom = top.saturating_add(u32::from(scroll.viewport_height));
    let first = scroll
        .layout
        .iter()
        .position(|page| page.start.saturating_add(u32::from(page.rows)) > top)
        .unwrap_or(0);
    let last = scroll
        .layout
        .iter()
        .rposition(|page| page.start < bottom)
        .unwrap_or(first);

    (first..=last).all(|index| {
        scroll.protocols.get(index).is_some_and(Option::is_some)
            || scroll.errors.get(index).is_some_and(Option::is_some)
    })
}

fn page_at_offset(scroll: &ScrollState) -> usize {
    let max_scroll = scroll
        .total_rows
        .saturating_sub(u32::from(scroll.viewport_height));
    if scroll.offset >= max_scroll {
        return scroll.layout.len().saturating_sub(1);
    }
    scroll
        .layout
        .iter()
        .rposition(|page| page.start <= scroll.offset)
        .unwrap_or(0)
}

pub(crate) fn build_scroll_protocol(
    picker: &Picker,
    page: &Page,
    layout: ScrollPageLayout,
) -> Result<SlicedProtocol, String> {
    let bytes = read_page(page).map_err(|error| error.to_string())?;
    let image =
        image::load_from_memory(&bytes).map_err(|error| format!("{}: {error}", page.name))?;
    let image = image.resize_exact(
        layout.pixel_width,
        layout.pixel_height,
        image::imageops::FilterType::Triangle,
    );
    SlicedProtocol::new_with_resize(
        picker,
        image,
        Size::new(layout.cell_width, layout.rows),
        Resize::Fit(None),
    )
    .map_err(|error| error.to_string())
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
    use std::time::Duration;

    use ratatui_image::picker::Picker;

    use crate::scroll_worker::ScrollWorker;

    use super::*;

    fn series(title: &str, categories: &[&str]) -> Series {
        Series {
            path: title.into(),
            title: title.to_owned(),
            author: None,
            artist: None,
            description: String::new(),
            tags: Vec::new(),
            status: None,
            cover: None,
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
            scanlator: None,
            date: None,
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
            status: None,
            cover: None,
            categories: vec!["Default".to_owned()],
            chapters: vec![Chapter {
                path: root.path().to_path_buf(),
                title: "Chapter 1".to_owned(),
                number: Some(1.0),
                kind: tsuiyomi_core::ChapterKind::Directory,
                scanlator: None,
                date: None,
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
        app.set_image_picker(Picker::halfblocks());
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
        assert_eq!(scroll.layout.len(), 2);
        assert!(scroll.total_rows > 0);
        let offset = scroll.offset;
        assert!(offset > 0);

        app.set_scroll_worker(ScrollWorker::spawn());
        app.ensure_scroll_view(10);
        for _ in 0..200 {
            if app.poll_scroll_results() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app
            .reader
            .as_ref()
            .unwrap()
            .scroll
            .as_ref()
            .unwrap()
            .protocols
            .iter()
            .any(Option::is_some));

        app.previous_page();
        assert_eq!(app.reader.as_ref().unwrap().page_index, 0);
        assert_eq!(
            app.reader.as_ref().unwrap().scroll.as_ref().unwrap().offset,
            0
        );

        app.next_page();
        let scroll = app.reader.as_ref().unwrap().scroll.as_ref().unwrap();
        let page_two_start = scroll.layout[1].start;
        let max_scroll = scroll
            .total_rows
            .saturating_sub(u32::from(scroll.viewport_height));
        let expected = page_two_start.min(max_scroll);
        assert_eq!(app.reader.as_ref().unwrap().page_index, 1);
        assert_eq!(scroll.offset, expected);

        app.next();
        assert!(app.reader.as_ref().unwrap().scroll.as_ref().unwrap().offset > expected);

        for _ in 0..100 {
            app.next();
        }
        assert_eq!(app.reader.as_ref().unwrap().page_index, 1);

        app.previous_page();
        assert_eq!(
            app.reader.as_ref().unwrap().scroll.as_ref().unwrap().offset,
            expected
        );

        app.previous_page();
        assert_eq!(app.reader.as_ref().unwrap().page_index, 0);
        assert_eq!(
            app.reader.as_ref().unwrap().scroll.as_ref().unwrap().offset,
            0
        );

        let old_generation = app
            .reader
            .as_ref()
            .unwrap()
            .scroll
            .as_ref()
            .unwrap()
            .generation;
        app.zoom_in();
        assert!(app.reader.as_ref().unwrap().pending_scroll.is_some());
        assert!(app
            .reader
            .as_ref()
            .unwrap()
            .scroll
            .as_ref()
            .unwrap()
            .protocols
            .iter()
            .any(Option::is_some));
        app.ensure_scroll_view(10);
        for _ in 0..200 {
            app.poll_scroll_results();
            if app.reader.as_ref().unwrap().pending_scroll.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.reader.as_ref().unwrap().pending_scroll.is_none());
        let scroll = app.reader.as_ref().unwrap().scroll.as_ref().unwrap();
        assert_ne!(scroll.generation, old_generation);
        assert!(scroll.protocols.iter().any(Option::is_some));

        app.toggle_reader_mode();
        assert_eq!(app.reader_mode, ReaderMode::Page);
        assert!(app.reader.as_ref().unwrap().protocol.is_some());

        app.back();
        assert_eq!(app.view, LibraryView::Chapters);
    }

    #[test]
    fn more_focus_navigation() {
        let mut app = AppState::default();
        assert_eq!(app.more_focus, MoreFocus::Content);

        app.leave_more();
        assert_eq!(app.more_focus, MoreFocus::Sections);

        app.next_more_section();
        assert_eq!(app.more_section, MoreSection::Downloads);
        // Browsing the section list must not enter the section.
        assert_eq!(app.more_focus, MoreFocus::Sections);
        app.previous_more_section();
        assert_eq!(app.more_section, MoreSection::Settings);
        assert_eq!(app.more_focus, MoreFocus::Sections);

        app.leave_more();
        assert_eq!(app.tab, 0);
    }

    #[test]
    fn set_config_applies_theme_and_accent() {
        let mut app = AppState::default();
        let config = Config {
            theme: Theme::TokyoNight,
            accent: Some("yellow".to_owned()),
            ..Config::default()
        };

        app.set_config(config);

        assert_eq!(app.theme, Theme::TokyoNight);
        assert_eq!(app.accent.as_deref(), Some("yellow"));
        assert_eq!(
            palette::current().accent,
            palette::palette_for(Theme::TokyoNight).yellow
        );
    }

    #[test]
    fn browse_sources_opens_the_local_source() {
        let mut app = AppState::new(library(), None);
        app.tab = 3;

        assert_eq!(app.browse_view, BrowseView::Sources);
        assert_eq!(app.browse_item_count(), 1);

        app.browse_open();

        assert_eq!(app.browse_view, BrowseView::Local);
        assert_eq!(app.browse_item_count(), 2);
        let titles: Vec<&str> = app
            .browse_local_indices()
            .into_iter()
            .map(|index| app.library.series[index].title.as_str())
            .collect();
        assert_eq!(titles, ["Azumanga", "Berserk"]);
    }

    #[test]
    fn browse_opens_local_chapters_without_leaving_browse() {
        let mut app = AppState::new(library(), None);
        app.tab = 3;
        app.browse_view = BrowseView::Local;
        let position = app
            .browse_local_indices()
            .into_iter()
            .position(|index| app.library.series[index].title == "Berserk")
            .unwrap();
        app.select_browse_index(position);

        app.browse_open();
        assert_eq!(app.browse_view, BrowseView::Details);
        assert_eq!(
            app.browse_detail_series()
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );

        app.browse_open();
        assert_eq!(app.tab, 3);
        assert_eq!(app.view, LibraryView::Chapters);
        assert_eq!(
            app.selected_series().map(|series| series.title.as_str()),
            Some("Berserk")
        );
        assert_eq!(
            app.browse_detail_series()
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );
    }

    #[test]
    fn library_grid_arrows_move_by_row_and_column() {
        let mut library = library();
        for index in 0..6 {
            library
                .series
                .push(series(&format!("Series {index}"), &["Default"]));
        }
        let mut app = AppState::new(library, None);
        app.library_columns = 3;

        app.next();
        assert_eq!(app.selected, 3);
        app.next_column();
        assert_eq!(app.selected, 4);
        app.previous();
        assert_eq!(app.selected, 1);
        app.previous_column();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn browse_local_grid_uses_grid_navigation() {
        let mut library = library();
        for index in 0..6 {
            library
                .series
                .push(series(&format!("Series {index}"), &["Default"]));
        }
        let mut app = AppState::new(library, None);
        app.tab = 3;
        app.browse_view = BrowseView::Local;
        app.browse_columns = 2;

        app.browse_next();
        assert_eq!(app.browse_selection, 2);
        app.browse_next_column();
        assert_eq!(app.browse_selection, 3);
        app.browse_previous();
        assert_eq!(app.browse_selection, 1);
        app.browse_previous_column();
        assert_eq!(app.browse_selection, 0);
    }

    #[test]
    fn browse_parent_returns_to_the_source_list() {
        let mut app = AppState::new(library(), None);
        app.tab = 3;
        app.browse_view = BrowseView::Local;

        app.browse_parent();

        assert_eq!(app.browse_view, BrowseView::Sources);
        assert_eq!(app.browse_selection, 0);
    }

    #[test]
    fn loads_the_selected_series_cover() {
        let root = tempfile::tempdir().unwrap();
        let cover_path = root.path().join("cover.jpg");
        image::RgbImage::from_pixel(20, 30, image::Rgb([20, 40, 60]))
            .save(&cover_path)
            .unwrap();

        let mut library = library();
        library.series[0].cover = Some(cover_path.clone());
        let mut app = AppState::new(library, None);
        app.set_image_picker(Picker::halfblocks());

        app.ensure_cover();
        assert_eq!(app.cover_path.as_deref(), Some(cover_path.as_path()));
        assert!(app.cover.is_some());

        app.selected = 1;
        app.ensure_cover();
        assert!(app.cover_path.is_none());
        assert!(app.cover.is_none());
    }

    #[test]
    fn settings_menu_opens_pages_and_esc_unwinds() {
        let mut app = AppState {
            tab: 4,
            more_section: MoreSection::Settings,
            ..AppState::default()
        };
        // SETTINGS_PAGES order: Appearance, Library, Reader, ...
        app.setting_index = 2;
        app.activate_setting();
        assert_eq!(app.settings_page, SettingsPage::Reader);
        assert_eq!(app.setting_index, 0);

        app.leave_more();
        assert_eq!(app.settings_page, SettingsPage::Main);
        assert_eq!(app.more_focus, MoreFocus::Content);
        // The category that was entered stays selected.
        assert_eq!(app.setting_index, 2);

        app.leave_more();
        assert_eq!(app.more_focus, MoreFocus::Sections);

        app.leave_more();
        assert_eq!(app.tab, 0);
    }

    #[test]
    fn library_page_changes_sort_and_default_category() {
        let mut library = library();
        library.series = vec![series("Zeta", &["Default"]), series("Alpha", &["Default"])];
        let mut app = AppState::new(library, None);
        app.more_section = MoreSection::Settings;
        app.settings_page = SettingsPage::Library;

        // LIBRARY_ROWS selectable order: path, rescan, sort, category.
        app.setting_index = 2;
        app.adjust_setting(1);
        assert_eq!(app.config.library_sort, LibrarySort::TitleAsc);
        let titles: Vec<&str> = app
            .visible_indices()
            .into_iter()
            .map(|index| app.library.series[index].title.as_str())
            .collect();
        assert_eq!(titles, ["Alpha", "Zeta"]);

        app.setting_index = 3;
        app.adjust_setting(1);
        assert_eq!(app.config.default_category.as_deref(), Some("Reading"));
        assert_eq!(app.category_name(), Some("Reading"));

        app.adjust_setting(-1);
        assert_eq!(app.config.default_category.as_deref(), Some("Default"));
        assert_eq!(app.category_name(), Some("Default"));
    }

    #[test]
    fn incognito_stops_writing_progress() {
        let mut app = AppState::new(library(), None);
        app.more_section = MoreSection::Settings;
        app.settings_page = SettingsPage::Security;
        app.setting_index = 0;

        app.activate_setting();
        assert!(app.config.incognito);

        let chapter = PathBuf::from("/tmp/library/Chapter 1");
        app.record_progress(&chapter, 3, 10);
        assert!(app.progress.entry(&chapter).is_none());

        app.activate_setting();
        assert!(!app.config.incognito);
        app.record_progress(&chapter, 3, 10);
        assert_eq!(
            app.progress.entry(&chapter).map(|entry| entry.page),
            Some(3)
        );
    }

    #[test]
    fn theme_picker_applies_and_persists_selection() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Appearance,
            setting_index: 0,
            ..AppState::default()
        };

        app.activate_setting();
        assert_eq!(
            app.picker.as_ref().map(|picker| picker.kind),
            Some(PickerKind::Theme)
        );

        app.picker_next();
        app.picker_confirm();
        assert!(app.picker.is_none());

        let expected = palette::THEMES[1];
        assert_eq!(app.theme, expected);
        assert_eq!(app.config.theme, expected);
        assert_eq!(palette::current().base, palette::palette_for(expected).base);
    }

    #[test]
    fn theme_picker_previews_and_restores_on_cancel() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Appearance,
            setting_index: 0,
            ..AppState::default()
        };
        app.activate_setting();

        let original = app.theme;
        let original_base = palette::palette_for(original).base;
        app.picker_next();
        let preview = palette::THEMES[1];
        assert_eq!(palette::current().base, palette::palette_for(preview).base);
        assert_eq!(app.theme, original);

        app.picker_back();
        assert!(app.picker.is_none());
        assert_eq!(app.theme, original);
        assert_eq!(palette::current().base, original_base);
    }

    #[test]
    fn thumbnail_size_setting_clamps_to_grid_limits() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Appearance,
            setting_index: 3,
            ..AppState::default()
        };
        assert_eq!(app.config.thumbnail_width, 14);

        app.adjust_setting(1);
        assert_eq!(app.config.thumbnail_width, 15);
        app.set_setting_ratio(3, 1.0);
        assert_eq!(app.config.thumbnail_width, 28);
        app.adjust_setting(1);
        assert_eq!(app.config.thumbnail_width, 28);
        app.set_setting_ratio(3, 0.0);
        assert_eq!(app.config.thumbnail_width, 8);
    }

    #[test]
    fn fullscreen_setting_controls_the_startup_default() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Appearance,
            setting_index: 2,
            ..AppState::default()
        };
        assert!(!app.config.fullscreen);

        app.activate_setting();
        assert!(app.config.fullscreen);
        assert!(app.fullscreen);

        app.adjust_setting(1);
        assert!(!app.config.fullscreen);
        assert!(!app.fullscreen);
    }

    #[test]
    fn accent_picker_accepts_named_choice() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Appearance,
            setting_index: 1,
            ..AppState::default()
        };

        app.activate_setting();
        let picker = app.picker.as_ref().unwrap();
        assert_eq!(picker.kind, PickerKind::Accent);
        assert_eq!(picker.selected, 0);
        assert_eq!(app.accent_name(), "Default");

        app.picker_next();
        assert_eq!(
            palette::current().accent,
            palette::palette_for(app.theme).blue
        );
        app.picker_confirm();

        assert_eq!(app.accent.as_deref(), Some("blue"));
        assert_eq!(app.config.accent.as_deref(), Some("blue"));
        assert_eq!(app.accent_name(), "Blue");
        assert_eq!(
            palette::current().accent,
            palette::palette_for(app.theme).blue
        );
    }

    #[test]
    fn accent_picker_accepts_custom_hex_with_backspace() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Appearance,
            setting_index: 1,
            ..AppState::default()
        };
        app.activate_setting();

        for character in "#ff00aa".chars() {
            app.picker_char(character);
        }
        app.picker_backspace();
        // Five hex digits is incomplete, so Enter only enters edit mode.
        app.picker_confirm();
        assert!(app.picker.as_ref().unwrap().editing_custom);

        app.picker_char('a');
        app.picker_confirm();

        assert!(app.picker.is_none());
        assert_eq!(app.accent.as_deref(), Some("#ff00aa"));
        assert_eq!(
            palette::parse_hex(app.accent.as_deref().unwrap()),
            Some(ratatui::style::Color::Rgb(255, 0, 170))
        );
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
