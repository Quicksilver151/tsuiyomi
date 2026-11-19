use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use crossterm::terminal;
use ratatui::{layout::Size, widgets::ListState};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, sliced::SlicedProtocol, Resize};
use tsuiyomi_core::progress_key as remote_progress_key;
use tsuiyomi_core::{
    chapter::natural_cmp, chapter_dir, chapter_pages, downloads_dir, ensure_runtime, fetch_bytes,
    host_data_dir, is_downloaded, page_dimensions, read_cover, read_page, remote_cover_path,
    save_config, scan_library, Chapter, ChapterKind, Config, ExtensionInfo, HostClient,
    HostProcess, Library, LibrarySort, Page, PageSource, ProgressStore, ReadingMode, RemoteChapter,
    RemoteFavorite, RemoteManga, RemoteSeries, RemoteSource, Series, Theme,
};

use crate::{
    download_worker::{DownloadRequest, DownloadWorker},
    layout::TAB_COUNT,
    palette,
    scroll_worker::{BuildRequest, BuildResult, ScrollWorker, SizeRequest, SizeWorker},
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
    /// Saved remote manga: cover, metadata, and chapters, like Browse details.
    RemoteDetails,
}

#[derive(Default)]
pub struct LibraryRemoteState {
    pub manga: Option<RemoteManga>,
    pub chapters: Vec<RemoteChapter>,
    pub chapter_index: usize,
    pub list_state: ListState,
}

/// Where the reader was opened from, so closing it returns to the right place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReaderOrigin {
    #[default]
    Library,
    BrowseLocal,
    Remote,
    LibraryRemote,
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

/// Top-level Browse tabs, mirroring Mihon's Sources / Extensions / Migrate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrowseTab {
    #[default]
    Sources,
    Extensions,
    Migrate,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrowseView {
    #[default]
    Sources,
    Local,
    Chapters,
    RemoteResults,
    RemoteDetails,
    GlobalResults,
    Extensions,
    Stores,
    Migrate,
}

#[derive(Debug, Clone)]
pub struct GlobalGroup {
    pub source: RemoteSource,
    pub mangas: Vec<RemoteManga>,
}

/// One rendered row in the Sources list. Headers are not selectable.
pub enum SourceRow {
    Header(String),
    Local,
    Remote(usize),
}

/// A selectable source in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRef {
    Local,
    Remote(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAction {
    SearchRemote,
    GlobalSearch,
    AddExtensionStore,
    SetHostUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    pub action: TextAction,
    pub title: String,
    pub value: String,
}

pub struct RemoteState {
    pub sources: Vec<RemoteSource>,
    pub source: Option<RemoteSource>,
    pub results: Vec<RemoteManga>,
    pub query: String,
    pub page: i32,
    pub has_next_page: bool,
    pub manga: Option<RemoteManga>,
    pub chapters: Vec<RemoteChapter>,
    pub chapter_index: usize,
    pub reader: bool,
    pub status: Option<String>,
    pub error: bool,
    pub columns: usize,
    pub grid_offset: usize,
    pub tile_covers: HashMap<String, StatefulProtocol>,
    pub extensions: Vec<ExtensionInfo>,
    pub repos: Vec<String>,
    /// Base URL of the host that is actually in use.
    pub host: Option<String>,
    /// Persistent hint, e.g. an ignored `host_url` that was actually a repository URL.
    pub host_hint: Option<String>,
    pub favorites: Vec<RemoteFavorite>,
    pub favorites_loaded: bool,
    pub global_query: String,
    pub global_groups: Vec<GlobalGroup>,
    pub global_group: usize,
    pub global_item: usize,
    /// Where RemoteDetails returns to (regular results or global search).
    pub remote_details_origin: BrowseView,
    pub extension_list_state: ListState,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            source: None,
            results: Vec::new(),
            query: String::new(),
            page: 1,
            has_next_page: false,
            manga: None,
            chapters: Vec::new(),
            chapter_index: 0,
            reader: false,
            status: None,
            error: false,
            columns: 1,
            grid_offset: 0,
            tile_covers: HashMap::new(),
            extensions: Vec::new(),
            repos: Vec::new(),
            host: None,
            host_hint: None,
            favorites: Vec::new(),
            favorites_loaded: false,
            global_query: String::new(),
            global_groups: Vec::new(),
            global_group: 0,
            global_item: 0,
            remote_details_origin: BrowseView::RemoteResults,
            extension_list_state: ListState::default(),
        }
    }
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
    HostUrl,
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

const BROWSE_ROWS: [MoreRow; 6] = [
    MoreRow::Divider("Sources"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Setting(SettingId::HostUrl),
    MoreRow::Note("Local source lists series directories in the library folder."),
    MoreRow::Note("Extension host runs APKs. Add repositories in Browse → Extensions."),
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
    /// Source page this segment belongs to.
    pub page: usize,
    /// Vertical crop inside the source page, in source pixels.
    pub source_y: u32,
    pub source_height: u32,
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
    pub library_remote: LibraryRemoteState,
    pub category_index: usize,
    pub selected: usize,
    pub chapter_index: usize,
    pub chapter_list_state: ListState,
    pub reader: Option<ReaderState>,
    pub reader_origin: ReaderOrigin,
    pub image_picker: Option<Picker>,
    /// Decoded custom cover for the selected series.
    pub progress: ProgressStore,
    /// Worker that decodes/resizes/encodes scroll pages off the UI thread.
    pub(crate) scroll_worker: Option<ScrollWorker>,
    pub(crate) size_worker: Option<SizeWorker>,
    pub(crate) download_worker: Option<DownloadWorker>,
    pub size_generation: u64,
    pub scroll_generation: u64,
    /// Tunables for lazy scroll loading; a future settings menu will expose these.
    pub scroll_cache: ScrollCacheConfig,
    /// Scroll-mode zoom percentage. 100 = fit width without upscaling.
    pub zoom: u16,
    pub config: Config,
    pub browse_view: BrowseView,
    pub browse_tab: BrowseTab,
    /// Inner Sources view, restored when returning to the Sources tab.
    pub sources_view: Option<BrowseView>,
    pub extension_query: String,
    pub extension_search: bool,
    /// Local series and chapter selected in Browse; independent of the Library tab.
    pub browse_local_series: Option<PathBuf>,
    pub browse_local_chapter: usize,
    pub browse_local_list_state: ListState,
    pub host_client: Option<HostClient>,
    /// Start the bundled host the first time Browse is opened.
    pub host_autostart: bool,
    /// Bundled host process; `None` when using an external `host_url`.
    pub host_process: Option<HostProcess>,
    pub remote: RemoteState,
    pub text_input: Option<TextInput>,
    pub browse_selection: usize,
    pub browse_list_state: ListState,
    pub browse_columns: usize,
    pub browse_grid_offset: usize,
    pub library_columns: usize,
    pub library_grid_offset: usize,
    pub tile_covers: HashMap<PathBuf, StatefulProtocol>,
    /// Remote covers that recently failed to load, so drawing does not retry every frame.
    pub failed_covers: HashSet<String>,
    pub more_section: MoreSection,
    pub more_focus: MoreFocus,
    pub settings_page: SettingsPage,
    pub setting_index: usize,
    pub theme: Theme,
    pub accent: Option<String>,
    pub picker: Option<PickerState>,
    pub quit_confirm: bool,
    /// Chapter download currently running, if any.
    pub downloading: Option<String>,
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
            library_remote: LibraryRemoteState::default(),
            category_index: 0,
            selected: 0,
            chapter_index: 0,
            chapter_list_state: ListState::default(),
            reader: None,
            reader_origin: ReaderOrigin::Library,
            image_picker: None,
            progress: ProgressStore::default(),
            scroll_worker: None,
            size_worker: None,
            download_worker: None,
            size_generation: 0,
            scroll_generation: 0,
            scroll_cache: ScrollCacheConfig::default(),
            zoom: 100,
            config: Config::default(),
            browse_view: BrowseView::Sources,
            browse_tab: BrowseTab::Sources,
            sources_view: None,
            extension_query: String::new(),
            extension_search: false,
            browse_local_series: None,
            browse_local_chapter: 0,
            browse_local_list_state: ListState::default(),
            host_client: None,
            host_autostart: true,
            host_process: None,
            remote: RemoteState::default(),
            text_input: None,
            browse_selection: 0,
            browse_list_state: ListState::default(),
            browse_columns: 1,
            browse_grid_offset: 0,
            library_columns: 1,
            library_grid_offset: 0,
            tile_covers: HashMap::new(),
            failed_covers: HashSet::new(),
            more_section: MoreSection::Settings,
            more_focus: MoreFocus::Content,
            settings_page: SettingsPage::Main,
            setting_index: 0,
            theme: Theme::default(),
            accent: None,
            picker: None,
            quit_confirm: false,
            downloading: None,
            status,
        }
    }

    pub fn set_image_picker(&mut self, picker: Picker) {
        self.image_picker = Some(picker);
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

    pub fn browse_previous(&mut self) {
        let step = match self.browse_view {
            BrowseView::Local | BrowseView::RemoteResults => self.browse_columns.max(1),
            _ => 1,
        };
        self.browse_selection = self.browse_selection.saturating_sub(step);
        self.select_browse_entry(self.browse_selection);
    }

    pub fn browse_next(&mut self) {
        let count = self.browse_item_count();
        if count > 0 {
            let step = match self.browse_view {
                BrowseView::Local | BrowseView::RemoteResults => self.browse_columns.max(1),
                _ => 1,
            };
            self.browse_selection = (self.browse_selection + step).min(count - 1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    pub fn browse_previous_column(&mut self) {
        if matches!(
            self.browse_view,
            BrowseView::Local | BrowseView::RemoteResults
        ) {
            self.browse_selection = self.browse_selection.saturating_sub(1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    pub fn browse_next_column(&mut self) {
        if matches!(
            self.browse_view,
            BrowseView::Local | BrowseView::RemoteResults
        ) {
            let count = self.browse_item_count();
            if count > 0 {
                self.browse_selection = (self.browse_selection + 1).min(count - 1);
                self.select_browse_entry(self.browse_selection);
            }
        }
    }

    pub fn browse_open(&mut self) {
        match self.browse_view {
            BrowseView::Sources => match self.selected_source_ref() {
                Some(SourceRef::Local) => {
                    self.browse_view = BrowseView::Local;
                    self.browse_selection = 0;
                    self.select_browse_entry(0);
                }
                Some(SourceRef::Remote(index)) => {
                    let Some(source) = self.remote.sources.get(index).cloned() else {
                        return;
                    };
                    self.remote.source = Some(source);
                    self.remote.query.clear();
                    self.remote.page = 1;
                    self.load_remote_results(false);
                    self.browse_view = BrowseView::RemoteResults;
                    self.browse_selection = 0;
                    self.browse_grid_offset = 0;
                    self.select_browse_entry(0);
                }
                None => {}
            },
            BrowseView::Local => {
                let Some(path) = self
                    .selected_browse_series()
                    .map(|series| series.path.clone())
                else {
                    return;
                };
                // Go straight to the story screen: chapter list on the left, cover and
                // metadata on the right. Remote sources work the same way.
                self.open_browse_series(&path);
            }
            BrowseView::Chapters => self.open_browse_local_chapter(self.browse_selection),
            BrowseView::RemoteResults => {
                let Some(manga) = self.remote.results.get(self.browse_selection).cloned() else {
                    return;
                };
                self.remote.manga = Some(manga);
                self.remote.remote_details_origin = BrowseView::RemoteResults;
                self.load_remote_details();
                self.load_remote_chapters();
                self.browse_view = BrowseView::RemoteDetails;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::RemoteDetails => self.open_remote_chapter(self.browse_selection),
            BrowseView::Extensions => self.toggle_selected_extension(),
            BrowseView::Stores | BrowseView::GlobalResults | BrowseView::Migrate => {}
        }
    }

    pub fn browse_parent(&mut self) {
        match self.browse_view {
            BrowseView::Chapters => {
                self.browse_view = BrowseView::Local;
                self.clamp_browse_selection();
            }
            BrowseView::RemoteDetails => {
                self.browse_view = self.remote.remote_details_origin;
                self.browse_selection = 0;
                self.browse_grid_offset = 0;
                self.select_browse_entry(0);
            }
            BrowseView::GlobalResults => {
                self.browse_view = BrowseView::Sources;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::Stores => {
                self.browse_view = BrowseView::Extensions;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::Local | BrowseView::RemoteResults => {
                self.browse_view = BrowseView::Sources;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            // Esc leaves Browse only from the Sources root. Extensions and Migrate stay.
            BrowseView::Sources => self.select_tab(0),
            BrowseView::Extensions | BrowseView::Migrate => {}
        }
    }

    /// Refresh the active Browse list.
    pub fn refresh_browse(&mut self) {
        match self.browse_view {
            BrowseView::Sources => self.load_remote_sources(),
            BrowseView::Local => self.refresh(),
            BrowseView::RemoteResults => {
                let search = !self.remote.query.is_empty();
                self.load_remote_results(search);
            }
            BrowseView::RemoteDetails => self.load_remote_chapters(),
            BrowseView::Extensions | BrowseView::Stores => self.load_extensions(),
            BrowseView::Chapters => self.refresh(),
            BrowseView::GlobalResults | BrowseView::Migrate => {}
        }
        self.clamp_browse_selection();
    }

    pub fn browse_item_count(&self) -> usize {
        match self.browse_view {
            BrowseView::Sources => 1 + self.remote.sources.len(),
            BrowseView::Local => self.library.series.len(),
            BrowseView::Chapters => self.browse_local_chapter_count(),
            BrowseView::RemoteResults => self.remote.results.len(),
            BrowseView::RemoteDetails => self.remote.chapters.len(),
            BrowseView::Extensions => self.filtered_extension_count(),
            BrowseView::Stores => self.remote.repos.len(),
            BrowseView::GlobalResults | BrowseView::Migrate => 0,
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
        let path = self.browse_local_series.as_deref()?;
        self.library
            .series
            .iter()
            .find(|series| series.path == path)
    }

    /// Local series open in Browse. Independent of the Library tab's selection.
    pub fn selected_browse_local_series(&self) -> Option<&Series> {
        let path = self.browse_local_series.as_deref()?;
        self.library
            .series
            .iter()
            .find(|series| series.path == path)
    }

    pub fn browse_local_chapter_count(&self) -> usize {
        self.selected_browse_local_series()
            .map_or(0, |series| series.chapters.len())
    }

    pub fn selected_browse_local_chapter(&self) -> Option<&Chapter> {
        self.selected_browse_local_series()?
            .chapters
            .get(self.browse_local_chapter)
    }

    pub fn selected_remote_manga(&self) -> Option<&RemoteManga> {
        self.remote.manga.as_ref()
    }

    pub fn selected_remote_chapter(&self) -> Option<&RemoteChapter> {
        if self.browse_view != BrowseView::RemoteDetails {
            return None;
        }
        self.remote.chapters.get(self.browse_selection)
    }

    pub fn selected_extension(&self) -> Option<&ExtensionInfo> {
        let index = *self
            .filtered_extension_indices()
            .get(self.browse_selection)?;
        self.remote.extensions.get(index)
    }

    pub fn selected_store(&self) -> Option<&String> {
        self.remote.repos.get(self.browse_selection)
    }

    pub fn set_browse_tab(&mut self, tab: BrowseTab) {
        if tab == self.browse_tab {
            return;
        }
        if self.browse_tab == BrowseTab::Sources {
            self.sources_view = Some(self.browse_view);
        }
        self.browse_tab = tab;
        self.extension_search = false;
        match tab {
            BrowseTab::Sources => {
                self.browse_view = self.sources_view.unwrap_or(BrowseView::Sources);
            }
            BrowseTab::Extensions => {
                self.browse_view = BrowseView::Extensions;
                if self.remote.extensions.is_empty() {
                    self.load_extensions();
                }
            }
            BrowseTab::Migrate => self.browse_view = BrowseView::Migrate,
        }
        self.browse_selection = 0;
        self.select_browse_entry(0);
    }

    pub fn next_browse_tab(&mut self) {
        let next = match self.browse_tab {
            BrowseTab::Sources => BrowseTab::Extensions,
            BrowseTab::Extensions => BrowseTab::Migrate,
            BrowseTab::Migrate => BrowseTab::Sources,
        };
        self.set_browse_tab(next);
    }

    pub fn previous_browse_tab(&mut self) {
        let previous = match self.browse_tab {
            BrowseTab::Sources => BrowseTab::Migrate,
            BrowseTab::Extensions => BrowseTab::Sources,
            BrowseTab::Migrate => BrowseTab::Extensions,
        };
        self.set_browse_tab(previous);
    }

    /// Sources in the order they are displayed: pinned, Local, then language groups.
    ///
    /// Selection indexes this vector, so arrow keys follow the visible list instead of
    /// the host's storage order.
    pub fn source_order(&self) -> Vec<SourceRef> {
        let mut order = Vec::new();

        let mut pinned: Vec<usize> = (0..self.remote.sources.len())
            .filter(|index| self.is_source_pinned(&self.remote.sources[*index]))
            .collect();
        sort_sources_by_name(&self.remote.sources, &mut pinned);
        order.extend(pinned.into_iter().map(SourceRef::Remote));

        order.push(SourceRef::Local);

        let mut grouped: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, source) in self.remote.sources.iter().enumerate() {
            if self.is_source_pinned(source) {
                continue;
            }
            grouped.entry(source.lang.clone()).or_default().push(index);
        }
        for (_, mut indices) in grouped {
            sort_sources_by_name(&self.remote.sources, &mut indices);
            order.extend(indices.into_iter().map(SourceRef::Remote));
        }
        order
    }

    pub fn selected_source_ref(&self) -> Option<SourceRef> {
        self.source_order().get(self.browse_selection).copied()
    }

    /// Rendered rows, with a non-selectable header before each group.
    pub fn source_rows(&self) -> Vec<SourceRow> {
        let mut rows = Vec::new();
        let mut group: Option<String> = None;
        for item in self.source_order() {
            let label = match item {
                SourceRef::Local => "Local".to_owned(),
                SourceRef::Remote(index) => {
                    let source = &self.remote.sources[index];
                    if self.is_source_pinned(source) {
                        "Pinned".to_owned()
                    } else {
                        language_label(&source.lang)
                    }
                }
            };
            if group.as_deref() != Some(label.as_str()) {
                rows.push(SourceRow::Header(label.clone()));
                group = Some(label);
            }
            rows.push(match item {
                SourceRef::Local => SourceRow::Local,
                SourceRef::Remote(index) => SourceRow::Remote(index),
            });
        }
        rows
    }

    /// Row index of the currently selected source, for the list highlight.
    pub fn selected_source_row(&self) -> Option<usize> {
        let mut selection = 0;
        for (row, item) in self.source_rows().iter().enumerate() {
            if matches!(item, SourceRow::Header(_)) {
                continue;
            }
            if selection == self.browse_selection {
                return Some(row);
            }
            selection += 1;
        }
        None
    }

    /// Select the source shown at `row` in the rendered Sources list.
    pub fn select_source_row(&mut self, row: usize) {
        let offset = self.browse_list_state.offset();
        if let Some(index) = self.source_index_for_row(offset + row) {
            self.select_browse_index(index);
        }
    }

    /// Map a rendered Sources row to a selection index.
    pub fn source_index_for_row(&self, row: usize) -> Option<usize> {
        let mut selection = 0;
        for (index, item) in self.source_rows().iter().enumerate() {
            if matches!(item, SourceRow::Header(_)) {
                continue;
            }
            if index == row {
                return Some(selection);
            }
            selection += 1;
        }
        None
    }

    pub fn is_source_pinned(&self, source: &RemoteSource) -> bool {
        self.config
            .pinned_sources
            .iter()
            .any(|pinned| pin_key_matches(pinned, source))
    }

    fn toggle_source_pin(&mut self, source: &RemoteSource) {
        if let Some(position) = self
            .config
            .pinned_sources
            .iter()
            .position(|pinned| pin_key_matches(pinned, source))
        {
            self.config.pinned_sources.remove(position);
            self.status = Some(format!("Unpinned {}", source.display_name()));
        } else {
            self.config.pinned_sources.push(source_pin_key(source));
            self.status = Some(format!("Pinned {}", source.display_name()));
        }
    }

    /// Pin or unpin the source selected in the Sources list.
    pub fn toggle_pin_for_selection(&mut self) {
        if self.browse_view != BrowseView::Sources {
            return;
        }
        let Some(SourceRef::Remote(index)) = self.selected_source_ref() else {
            return;
        };
        let Some(source) = self.remote.sources.get(index).cloned() else {
            return;
        };
        self.toggle_source_pin(&source);
        // Pinning moves the source into the Pinned group; keep it selected there.
        self.browse_selection = self
            .source_order()
            .iter()
            .position(|item| *item == SourceRef::Remote(index))
            .unwrap_or(0);
        self.select_browse_entry(self.browse_selection);
        self.persist_settings();
    }

    /// Sources of an extension, resolved against the loaded source list.
    ///
    /// The host reports installed extensions with an empty source language, so match by
    /// package and name instead of trusting the extension row's language.
    fn resolved_extension_sources(&self, extension: &ExtensionInfo) -> Vec<RemoteSource> {
        let mut sources = Vec::new();
        for source in &extension.sources {
            if let Some(remote) = self.remote.sources.iter().find(|remote| {
                remote.pkg.as_deref() == Some(extension.pkg.as_str())
                    && remote.name.eq_ignore_ascii_case(&source.name)
                    && (source.lang.is_empty() || remote.lang == source.lang)
            }) {
                sources.push(remote.clone());
            }
        }
        sources
    }

    pub fn is_extension_pinned(&self, extension: &ExtensionInfo) -> bool {
        let sources = self.resolved_extension_sources(extension);
        !sources.is_empty() && sources.iter().all(|source| self.is_source_pinned(source))
    }

    /// Pin or unpin every source of the selected extension.
    pub fn toggle_selected_extension_pin(&mut self) {
        let Some(extension) = self.selected_extension().cloned() else {
            return;
        };
        if extension.sources.is_empty() {
            return;
        }
        let sources = self.resolved_extension_sources(&extension);
        if sources.is_empty() {
            return;
        }
        let all_pinned = sources.iter().all(|source| self.is_source_pinned(source));
        for source in &sources {
            if all_pinned {
                if let Some(position) = self
                    .config
                    .pinned_sources
                    .iter()
                    .position(|pinned| pin_key_matches(pinned, source))
                {
                    self.config.pinned_sources.remove(position);
                }
            } else if !self.is_source_pinned(source) {
                self.config.pinned_sources.push(source_pin_key(source));
            }
        }
        self.status = Some(if all_pinned {
            format!("Unpinned {} sources", extension.name)
        } else {
            format!("Pinned {} sources", extension.name)
        });
        self.persist_settings();
    }

    pub fn extension_search_start(&mut self) {
        if self.browse_view == BrowseView::Extensions {
            self.extension_search = true;
        }
    }

    pub fn extension_search_stop(&mut self, clear: bool) {
        self.extension_search = false;
        if clear {
            self.extension_query.clear();
        }
        self.browse_selection = 0;
        self.select_browse_entry(0);
    }

    pub fn extension_search_push(&mut self, character: char) {
        self.extension_query.push(character);
        self.reset_extension_selection();
    }

    pub fn extension_search_pop(&mut self) {
        self.extension_query.pop();
        self.reset_extension_selection();
    }

    fn reset_extension_selection(&mut self) {
        self.browse_selection = 0;
        self.select_browse_entry(0);
    }

    pub fn filtered_extension_indices(&self) -> Vec<usize> {
        let query = self.extension_query.trim().to_lowercase();
        self.remote
            .extensions
            .iter()
            .enumerate()
            .filter(|(_, extension)| {
                query.is_empty()
                    || extension.name.to_lowercase().contains(&query)
                    || extension.pkg.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn filtered_extension_count(&self) -> usize {
        if self.extension_query.trim().is_empty() {
            self.remote.extensions.len()
        } else {
            self.filtered_extension_indices().len()
        }
    }

    pub fn load_favorites(&mut self) {
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        match client.favorites() {
            Ok(favorites) => {
                self.remote.favorites = favorites;
                self.remote.favorites_loaded = true;
                // Persist without thumbnails: their proxy URLs contain an ephemeral port.
                let mut persisted = self.remote.favorites.clone();
                for favorite in &mut persisted {
                    favorite.thumbnail_url = None;
                }
                self.config.remote_library = persisted;
                self.persist_settings();
                self.merge_remote_library();
                self.cache_favorite_covers();
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Fill the on-disk cover cache for saved manga in the background.
    ///
    /// The covers must be fetched while the host is running because the proxy URL is
    /// process-local. Keeping the bytes means the library still has covers at the next
    /// start, before the JVM is launched.
    fn cache_favorite_covers(&self) {
        let jobs: Vec<(String, String, String)> = self
            .remote
            .favorites
            .iter()
            .filter_map(|favorite| {
                let path = remote_cover_path(&favorite.source_id, &favorite.url)?;
                if path.exists() {
                    return None;
                }
                Some((
                    favorite.source_id.clone(),
                    favorite.url.clone(),
                    favorite.thumbnail_url.clone()?,
                ))
            })
            .collect();
        if jobs.is_empty() {
            return;
        }

        std::thread::spawn(move || {
            for (source_id, manga_url, url) in jobs {
                let Some(path) = remote_cover_path(&source_id, &manga_url) else {
                    continue;
                };
                if path.exists() {
                    continue;
                }
                let Ok(bytes) = fetch_bytes(&url) else {
                    continue;
                };
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&path, bytes);
            }
        });
    }

    pub fn is_favorite(&self, source_id: &str, url: &str) -> bool {
        self.remote
            .favorites
            .iter()
            .any(|favorite| favorite.source_id == source_id && favorite.url == url)
    }

    pub fn toggle_current_favorite(&mut self) {
        if !self.remote.favorites_loaded {
            self.load_favorites();
        }
        let Some(manga) = self.remote.manga.clone() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };
        if self.is_favorite(&manga.source_id, &manga.url) {
            match client.remove_favorite(&manga.source_id, &manga.url) {
                Ok(()) => {
                    self.remote.status = Some(format!("Removed {} from library", manga.title));
                    self.remote.error = false;
                    self.load_favorites();
                }
                Err(error) => self.set_remote_error(error),
            }
        } else {
            match client.add_favorite(&manga) {
                Ok(()) => {
                    // Cache the cover now: the proxy URL is only valid while the host
                    // runs, and the library should keep showing it after a restart.
                    self.ensure_library_cover(
                        &manga.source_id,
                        &manga.url,
                        manga.thumbnail_url.as_deref(),
                    );
                    self.remote.status = Some(format!("Added {} to library", manga.title));
                    self.remote.error = false;
                    self.load_favorites();
                }
                Err(error) => self.set_remote_error(error),
            }
        }
    }

    fn pinned_sources(&self) -> Vec<RemoteSource> {
        self.remote
            .sources
            .iter()
            .filter(|source| {
                self.config
                    .pinned_sources
                    .iter()
                    .any(|key| pin_key_matches(key, source))
            })
            .cloned()
            .collect()
    }

    /// Search every pinned source and group the results by source.
    pub fn global_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let sources = self.pinned_sources();
        if sources.is_empty() {
            self.remote.status =
                Some("Pin a source first (P) to search across sources.".to_owned());
            self.remote.error = false;
            return;
        }

        let mut groups = Vec::new();
        let mut failures = Vec::new();
        for source in sources {
            match client.search(&source.id, query, 1) {
                Ok(page) => {
                    if !page.mangas.is_empty() {
                        groups.push(GlobalGroup {
                            source,
                            mangas: page.mangas,
                        });
                    }
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        groups.sort_by(|left, right| {
            natural_cmp(&left.source.display_name(), &right.source.display_name())
        });

        let has_groups = !groups.is_empty();
        self.remote.global_query = query.to_owned();
        self.remote.global_groups = groups;
        self.remote.global_group = 0;
        self.remote.global_item = 0;
        self.remote.status = if !has_groups {
            if let Some(error) = failures.first() {
                Some(format!("No results · {error}"))
            } else {
                Some("No results.".to_owned())
            }
        } else if failures.is_empty() {
            None
        } else {
            Some(format!("{} sources failed", failures.len()))
        };
        self.remote.error = !has_groups && !failures.is_empty();
        self.browse_view = BrowseView::GlobalResults;
        self.browse_selection = 0;
    }

    pub fn global_next_group(&mut self) {
        if self.remote.global_groups.is_empty() {
            return;
        }
        self.remote.global_group =
            (self.remote.global_group + 1).min(self.remote.global_groups.len() - 1);
        self.remote.global_item = 0;
    }

    pub fn global_previous_group(&mut self) {
        self.remote.global_group = self.remote.global_group.saturating_sub(1);
        self.remote.global_item = 0;
    }

    pub fn global_next_item(&mut self) {
        let Some(group) = self.remote.global_groups.get(self.remote.global_group) else {
            return;
        };
        if group.mangas.is_empty() {
            return;
        }
        self.remote.global_item = (self.remote.global_item + 1).min(group.mangas.len() - 1);
    }

    pub fn global_previous_item(&mut self) {
        self.remote.global_item = self.remote.global_item.saturating_sub(1);
    }

    pub fn global_open(&mut self) {
        let Some(group) = self.remote.global_groups.get(self.remote.global_group) else {
            return;
        };
        let Some(manga) = group.mangas.get(self.remote.global_item).cloned() else {
            return;
        };
        self.remote.manga = Some(manga);
        self.remote.remote_details_origin = BrowseView::GlobalResults;
        self.load_remote_details();
        self.load_remote_chapters();
        self.browse_view = BrowseView::RemoteDetails;
        self.browse_selection = 0;
        self.select_browse_entry(0);
    }

    /// Connect to the configured host, or start the bundled one.
    ///
    /// Starting the JVM is deferred until remote sources are actually used, so a local-
    /// only session never pays for it.
    fn ensure_host(&mut self) -> bool {
        if self.host_client.is_some() {
            return true;
        }
        if let Some(url) = self
            .config
            .host_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            if validate_host_url(url).is_ok() {
                self.host_client = Some(HostClient::new(url));
                return true;
            }
        }
        self.start_bundled_host();
        self.host_client.is_some()
    }

    fn start_bundled_host(&mut self) {
        self.remote.status = Some("Starting the extension host…".to_owned());
        self.remote.error = false;

        let runtime = match ensure_runtime(self.config.host_jar.as_deref()) {
            Ok(Some(runtime)) => runtime,
            Ok(None) => {
                self.remote.status = Some(
                    "No extension host found. Put miwayomi.jar in the runtime cache or set \
                     host_jar in the config."
                        .to_owned(),
                );
                self.remote.error = true;
                return;
            }
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        let Some(data_dir) = host_data_dir() else {
            self.set_remote_error("Cannot resolve the host data directory.");
            return;
        };
        match HostProcess::start(&runtime, &data_dir) {
            Ok(process) => {
                self.remote.host = Some(process.base_url().to_owned());
                self.host_client = Some(HostClient::new(process.base_url()));
                self.host_process = Some(process);
                self.remote.favorites.clear();
                self.remote.favorites_loaded = false;
                self.remote.status = None;
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    pub fn load_remote_sources(&mut self) {
        if !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        match client.sources() {
            Ok(mut sources) => {
                sources.sort_by(|left, right| {
                    natural_cmp(&left.display_name(), &right.display_name())
                });
                self.remote.sources = sources;
                self.remote.status = None;
                self.remote.error = false;
                if !self.remote.favorites_loaded {
                    self.load_favorites();
                }
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    pub fn load_remote_latest(&mut self) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        match client.latest(&source.id, 1) {
            Ok(result) => {
                self.remote.results = result.mangas;
                self.remote.has_next_page = result.has_next_page;
                self.remote.query.clear();
                self.remote.page = 1;
                self.remote.status = None;
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
        self.browse_view = BrowseView::RemoteResults;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    pub fn search_remote(&mut self, query: &str) {
        self.remote.query = query.trim().to_owned();
        self.remote.page = 1;
        self.load_remote_results(true);
        self.browse_view = BrowseView::RemoteResults;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    fn load_remote_results(&mut self, search: bool) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        let page = self.remote.page.max(1);
        let result = if search && !self.remote.query.is_empty() {
            client.search(&source.id, &self.remote.query, page)
        } else {
            client.popular(&source.id, page)
        };
        match result {
            Ok(result) => {
                self.remote.results = result.mangas;
                self.remote.has_next_page = result.has_next_page;
                self.remote.status = None;
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Refresh the details pane from the source. Search results often omit the
    /// description and tags, so this replaces the list entry with the full metadata.
    fn load_remote_details(&mut self) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(manga) = self.remote.manga.clone() else {
            return;
        };
        if let Ok(details) = client.details(&manga.source_id, &manga.url, manga.memo.as_deref()) {
            self.remote.manga = Some(RemoteManga {
                // Keep the identity we already trust even if the source rewrites the URL.
                source_id: manga.source_id,
                url: manga.url,
                ..details
            });
        }
    }

    fn load_remote_chapters(&mut self) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(manga) = self.remote.manga.clone() else {
            return;
        };
        match client.chapters(&manga.source_id, &manga.url, manga.memo.as_deref()) {
            Ok(mut chapters) => {
                chapters.sort_by(|left, right| {
                    right
                        .chapter_number
                        .partial_cmp(&left.chapter_number)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| natural_cmp(&right.name, &left.name))
                });
                self.remote.chapters = chapters;
                self.remote.status = None;
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    pub fn open_remote_chapter(&mut self, index: usize) {
        let Some(manga) = self.remote.manga.clone() else {
            return;
        };
        let Some(chapter) = self.remote.chapters.get(index).cloned() else {
            return;
        };
        let pages = match self.remote_chapter_pages(&manga, &chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        if pages.is_empty() {
            self.set_remote_error("Chapter has no pages.");
            return;
        }
        let key = remote_progress_key(&manga.source_id, &manga.url, &chapter.url);
        let start_page = self
            .progress
            .entry(&key)
            .map(|entry| entry.page)
            .unwrap_or(0)
            .min(pages.len() - 1);

        self.reader = Some(ReaderState {
            series_title: manga.title,
            chapter_title: chapter.name,
            chapter_path: key,
            pages,
            page_sizes: Vec::new(),
            page_index: start_page,
            protocol: None,
            scroll: None,
            pending_scroll: None,
            error: None,
        });
        self.remote.reader = true;
        self.remote.chapter_index = index;
        self.reader_origin = ReaderOrigin::Remote;
        self.browse_selection = index;
        self.status = None;
        match self.reader_mode {
            ReaderMode::Page => self.load_page(),
            ReaderMode::Scroll => {
                // Show the first page while page sizes are measured in the background.
                self.load_page();
                self.build_scroll(terminal_width());
            }
        }
    }

    pub fn close_remote_reader(&mut self) {
        self.reader_origin = ReaderOrigin::Remote;
        self.close_reader();
    }

    pub fn reader_chapter_index(&self) -> usize {
        match self.reader_origin {
            ReaderOrigin::Remote => self.remote.chapter_index,
            ReaderOrigin::BrowseLocal => self.browse_local_chapter,
            ReaderOrigin::Library => self.chapter_index,
            ReaderOrigin::LibraryRemote => self.library_remote.chapter_index,
        }
    }

    pub fn reader_chapter_count(&self) -> usize {
        match self.reader_origin {
            ReaderOrigin::Remote => self.remote.chapters.len(),
            ReaderOrigin::BrowseLocal => self.browse_local_chapter_count(),
            ReaderOrigin::Library => self.chapter_count(),
            ReaderOrigin::LibraryRemote => self.library_remote_chapter_count(),
        }
    }

    pub fn ensure_remote_tile_cover(&mut self, url: &str) {
        if self.remote.tile_covers.contains_key(url) {
            return;
        }
        let Some(picker) = self.image_picker.as_ref() else {
            return;
        };
        let Ok(bytes) = fetch_bytes(url) else {
            return;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            return;
        };
        if self.remote.tile_covers.len() >= TILE_COVER_CACHE {
            if let Some(oldest) = self.remote.tile_covers.keys().next().cloned() {
                self.remote.tile_covers.remove(&oldest);
            }
        }
        self.remote
            .tile_covers
            .insert(url.to_owned(), picker.new_resize_protocol(image));
    }

    /// Ensure a saved remote manga's cover is loaded, caching the bytes on disk.
    ///
    /// The host's cover URL embeds a port that only lives as long as the host process,
    /// so the image itself is written to the cover cache the first time it is seen.
    /// Returns the cache path when `tile_covers` has a protocol for it.
    pub fn ensure_library_cover(
        &mut self,
        source_id: &str,
        manga_url: &str,
        thumbnail_url: Option<&str>,
    ) -> Option<PathBuf> {
        let path = remote_cover_path(source_id, manga_url)?;
        let key = path.to_string_lossy().into_owned();
        if !self.tile_covers.contains_key(&path) && path.exists() {
            self.ensure_tile_cover(&path);
        }
        if self.tile_covers.contains_key(&path) {
            self.failed_covers.remove(&key);
            return Some(path);
        }
        if self.failed_covers.contains(&key) {
            return None;
        }

        let url = thumbnail_url?;
        let bytes = match fetch_bytes(url) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.failed_covers.insert(key);
                return None;
            }
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &bytes);
        self.ensure_tile_cover(&path);
        if self.tile_covers.contains_key(&path) {
            Some(path)
        } else {
            self.failed_covers.insert(key);
            None
        }
    }

    /// True while a modal should receive printable keys instead of shortcuts.
    pub fn text_mode(&self) -> bool {
        self.picker.is_some() || self.text_input.is_some() || self.extension_search
    }

    pub fn open_text_input(&mut self, action: TextAction, title: &str, value: &str) {
        self.text_input = Some(TextInput {
            action,
            title: title.to_owned(),
            value: value.to_owned(),
        });
    }

    pub fn input_char(&mut self, character: char) {
        if let Some(input) = &mut self.text_input {
            input.value.push(character);
        }
    }

    pub fn input_backspace(&mut self) {
        if let Some(input) = &mut self.text_input {
            input.value.pop();
        }
    }

    pub fn input_cancel(&mut self) {
        self.text_input = None;
    }

    pub fn input_submit(&mut self) {
        let Some(input) = self.text_input.take() else {
            return;
        };
        match input.action {
            TextAction::SearchRemote => self.search_remote(&input.value),
            TextAction::GlobalSearch => self.global_search(&input.value),
            TextAction::AddExtensionStore => self.add_extension_store(&input.value),
            TextAction::SetHostUrl => self.set_host_url(&input.value),
        }
    }

    fn set_host_url(&mut self, url: &str) {
        let url = url.trim();
        if !url.is_empty() {
            if let Err(message) = validate_host_url(url) {
                self.set_remote_error(message);
                self.remote.host_hint = Some(message.to_owned());
                self.open_text_input(TextAction::SetHostUrl, "Extension host URL", url);
                return;
            }
        }
        self.remote.host_hint = None;
        self.config.host_url = (!url.is_empty()).then(|| url.to_owned());
        self.host_process = None;
        self.host_client = self.config.host_url.as_deref().map(HostClient::new);
        self.remote.sources.clear();
        self.remote.results.clear();
        self.remote.manga = None;
        self.remote.chapters.clear();
        self.remote.extensions.clear();
        self.remote.repos.clear();
        self.remote.favorites.clear();
        self.remote.favorites_loaded = false;
        self.remote.status = None;
        self.remote.error = false;
        if self.host_client.is_some() {
            self.load_remote_sources();
        } else {
            self.start_bundled_host();
        }
        self.persist_settings();
        self.browse_view = BrowseView::Sources;
        self.browse_selection = 0;
        self.select_browse_entry(0);
    }

    pub fn open_extensions(&mut self) {
        self.browse_view = BrowseView::Extensions;
        self.browse_selection = 0;
        self.load_extensions();
        self.select_browse_entry(0);
    }

    pub fn open_stores(&mut self) {
        self.browse_view = BrowseView::Stores;
        self.browse_selection = 0;
        if self.remote.repos.is_empty() {
            self.load_extensions();
        }
        self.select_browse_entry(0);
    }

    /// Refresh installed extensions and merge them with every configured repository.
    pub fn load_extensions(&mut self) {
        if !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let installed = match client.installed_extensions() {
            Ok(installed) => installed,
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        let repos = match client.repos() {
            Ok(repos) => repos,
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };

        let mut merged = installed;
        for repo in &repos {
            match client.repo_extensions(repo) {
                Ok(entries) => {
                    for entry in entries {
                        match merged.iter_mut().find(|known| known.pkg == entry.pkg) {
                            Some(known) => {
                                known.repo_url = entry.repo_url;
                                known.apk = entry.apk;
                                known.version = entry.version;
                                known.source_count = known.source_count.max(entry.source_count);
                                if known.lang.is_empty() {
                                    known.lang = entry.lang;
                                }
                            }
                            None => merged.push(entry),
                        }
                    }
                }
                Err(error) => self.set_remote_error(error),
            }
        }
        merged.sort_by(|left, right| {
            right
                .installed
                .cmp(&left.installed)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });

        self.remote.extensions = merged;
        self.remote.repos = repos;
        if self.remote.status.is_none() {
            self.remote.error = false;
        }
    }

    fn toggle_selected_extension(&mut self) {
        let Some(extension) = self.selected_extension().cloned() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };

        if extension.installed {
            match client.uninstall_extension(&extension.pkg) {
                Ok(()) => {
                    let name = extension.name;
                    self.load_extensions();
                    self.load_remote_sources();
                    self.remote.status = Some(format!("Uninstalled {name}"));
                    self.remote.error = false;
                }
                Err(error) => self.set_remote_error(error),
            }
            return;
        }

        if extension.repo_url.is_empty() || extension.apk.is_empty() {
            self.set_remote_error("Add an extension store first (a).");
            return;
        }
        match client.install_extension(&extension.repo_url, &extension.apk) {
            Ok(_) => {
                let name = extension.name;
                let pkg = extension.pkg.clone();
                // The APK often loads zero sources until the published desktop jar is in
                // place and the host restarts. Report what is actually loaded afterwards.
                let swapped = self.install_desktop_jar(&extension.apk);
                if swapped {
                    self.restart_bundled_host();
                }
                self.load_extensions();
                self.load_remote_sources();
                let sources = self.loaded_source_count(&pkg);
                self.remote.status = Some(if sources == 0 {
                    format!("Installed {name} but it exposed no sources")
                } else {
                    format!("Installed {name} ({sources} sources)")
                });
                self.remote.error = sources == 0;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    fn loaded_source_count(&self, pkg: &str) -> usize {
        self.remote
            .sources
            .iter()
            .filter(|source| source.pkg.as_deref() == Some(pkg))
            .count()
    }

    /// Reinstall the selected extension from its repository, which also updates it.
    pub fn update_selected_extension(&mut self) {
        let Some(extension) = self.selected_extension().cloned() else {
            return;
        };
        if !extension.installed || extension.repo_url.is_empty() || extension.apk.is_empty() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        match client.install_extension(&extension.repo_url, &extension.apk) {
            Ok(_) => {
                let name = extension.name;
                let pkg = extension.pkg.clone();
                let swapped = self.install_desktop_jar(&extension.apk);
                if swapped {
                    self.restart_bundled_host();
                }
                self.load_extensions();
                self.load_remote_sources();
                let sources = self.loaded_source_count(&pkg);
                self.remote.status = Some(format!("Reinstalled {name} ({sources} sources)"));
                self.remote.error = sources == 0;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Keiyoushi publishes a desktop JVM jar next to every APK.
    ///
    /// The host's APK path runs `dex2jar`, which produces unverifiable bytecode for some
    /// extensions (MangaDex fails with `Expecting a stackmap frame`). Dropping the
    /// published jar beside the APK and restarting the host makes its loader prefer the
    /// jar. Only the bundled host is patched this way; an external host is left alone.
    fn install_desktop_jar(&self, apk_url: &str) -> bool {
        let Some(process) = self.host_process.as_ref() else {
            return false;
        };
        let Some(url) = desktop_jar_url(apk_url) else {
            return false;
        };
        let Some(name) = url.rsplit('/').next() else {
            return false;
        };
        let Ok(bytes) = fetch_bytes(&url) else {
            return false;
        };
        let extensions = process.data_dir().join("extensions");
        if std::fs::create_dir_all(&extensions).is_err() {
            return false;
        }
        std::fs::write(extensions.join(name), bytes).is_ok()
    }

    fn restart_bundled_host(&mut self) {
        if self.host_process.is_none() {
            return;
        }
        // Dropping the process terminates the JVM; the next start reuses the same data dir.
        self.host_process = None;
        self.host_client = None;
        self.start_bundled_host();
    }

    fn add_extension_store(&mut self, url: &str) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let url = url.trim();
        if url.is_empty() {
            return;
        }
        let mut repos = self.remote.repos.clone();
        if !repos.iter().any(|repo| repo == url) {
            repos.push(url.to_owned());
        }
        match client.set_repos(&repos) {
            Ok(()) => {
                self.load_extensions();
                self.load_remote_sources();
                self.browse_view = BrowseView::Extensions;
                self.browse_selection = 0;
                self.select_browse_entry(0);
                self.remote.status = Some("Extension store added".to_owned());
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    pub fn remove_selected_store(&mut self) {
        let Some(store) = self.selected_store().cloned() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let repos: Vec<String> = self
            .remote
            .repos
            .iter()
            .filter(|repo| **repo != store)
            .cloned()
            .collect();
        match client.set_repos(&repos) {
            Ok(()) => {
                self.load_extensions();
                self.load_remote_sources();
                self.browse_selection = 0;
                self.select_browse_entry(0);
                self.remote.status = Some("Extension store removed".to_owned());
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    fn set_remote_error(&mut self, error: impl std::fmt::Display) {
        self.remote.status = Some(error.to_string());
        self.remote.error = true;
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
        let count = self.browse_item_count();
        if self.browse_view == BrowseView::Chapters {
            self.browse_local_chapter = index;
            self.browse_local_list_state
                .select((count > 0).then_some(index));
        } else {
            self.browse_list_state.select((count > 0).then_some(index));
        }
    }

    /// Open a local series inside Browse. This never mutates the Library tab's view,
    /// category, or selection.
    fn open_browse_series(&mut self, path: &Path) {
        self.browse_local_series = Some(path.to_path_buf());
        self.browse_local_chapter = 0;
        self.browse_local_list_state = ListState::default();
        self.browse_view = BrowseView::Chapters;
        self.browse_selection = 0;
        self.status = None;
        self.select_browse_entry(0);
    }

    fn open_browse_local_chapter(&mut self, index: usize) {
        let Some(chapter) = self
            .selected_browse_local_series()
            .and_then(|series| series.chapters.get(index))
            .cloned()
        else {
            return;
        };
        let series_title = self
            .selected_browse_local_series()
            .map(|series| series.title.clone())
            .unwrap_or_default();
        let pages = match chapter_pages(&chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.status = Some(error.to_string());
                return;
            }
        };
        if pages.is_empty() {
            self.status = Some(format!("{} has no pages", chapter.title));
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
        self.reader_origin = ReaderOrigin::BrowseLocal;
        self.browse_local_chapter = index;
        self.browse_selection = index;
        self.remote.reader = false;
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
            ReaderMode::Scroll => {
                // Show the first page while page sizes are measured in the background.
                self.load_page();
                self.build_scroll(terminal_width());
            }
        }
    }

    pub(crate) fn set_scroll_worker(&mut self, worker: ScrollWorker) {
        worker.set_generation(self.scroll_generation);
        self.scroll_worker = Some(worker);
    }

    pub(crate) fn set_size_worker(&mut self, worker: SizeWorker) {
        self.size_worker = Some(worker);
    }

    pub(crate) fn set_download_worker(&mut self, worker: DownloadWorker) {
        self.download_worker = Some(worker);
    }

    /// Apply finished chapter downloads. Returns whether the UI changed.
    pub fn poll_downloads(&mut self) -> bool {
        let mut changed = false;
        while let Some(result) = self
            .download_worker
            .as_ref()
            .and_then(DownloadWorker::try_result)
        {
            self.downloading = None;
            let (message, error) = match &result.result {
                Ok(dir) => (
                    format!("Downloaded {} to {}", result.label, dir.display()),
                    false,
                ),
                Err(error) => (
                    format!("Download failed for {}: {error}", result.label),
                    true,
                ),
            };
            self.status = Some(message.clone());
            if self.tab == 3 {
                self.remote.status = Some(message);
                self.remote.error = error;
            }
            changed = true;
        }
        changed
    }

    /// Whether the current screen has a remote chapter that can be downloaded.
    pub fn has_downloadable_chapter(&self) -> bool {
        self.active_remote_chapter().is_some()
    }

    /// Download the selected remote chapter to the Mihon-style downloads tree.
    pub fn download_selected_chapter(&mut self) {
        let Some((manga, chapter)) = self.active_remote_chapter() else {
            self.status = Some("Open a remote chapter to download it.".to_owned());
            return;
        };
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        if self.remote.sources.is_empty() {
            self.load_remote_sources();
        }
        let Some(root) = downloads_dir(&self.config) else {
            self.status = Some("Cannot resolve the downloads directory.".to_owned());
            return;
        };
        let source = self.remote_source_name(&manga.source_id);
        if is_downloaded(
            &root,
            &source,
            &manga.title,
            &chapter.name,
            chapter.scanlator.as_deref(),
        ) {
            self.status = Some(format!("{} is already downloaded", chapter.name));
            return;
        }

        let pages = match self.remote_chapter_pages(&manga, &chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        let label = format!("{} · {}", manga.title, chapter.name);
        let Some(worker) = self.download_worker.as_ref() else {
            self.status = Some("The download worker is unavailable.".to_owned());
            return;
        };
        worker.request(DownloadRequest {
            label: label.clone(),
            root,
            source_name: source,
            series_title: manga.title,
            chapter_name: chapter.name,
            scanlator: chapter.scanlator,
            pages,
        });
        self.downloading = Some(label.clone());
        let message = format!("Downloading {label}…");
        self.status = Some(message.clone());
        if self.tab == 3 {
            self.remote.status = Some(message);
            self.remote.error = false;
        }
    }

    /// The remote chapter under the cursor in the reader, Library, or Browse.
    fn active_remote_chapter(&self) -> Option<(RemoteManga, RemoteChapter)> {
        if self.reader.is_some() {
            return match self.reader_origin {
                ReaderOrigin::Remote => {
                    let manga = self.remote.manga.clone()?;
                    let chapter = self.remote.chapters.get(self.remote.chapter_index)?.clone();
                    Some((manga, chapter))
                }
                ReaderOrigin::LibraryRemote => {
                    let manga = self.library_remote.manga.clone()?;
                    let chapter = self
                        .library_remote
                        .chapters
                        .get(self.library_remote.chapter_index)?
                        .clone();
                    Some((manga, chapter))
                }
                ReaderOrigin::Library | ReaderOrigin::BrowseLocal => None,
            };
        }

        if self.tab == 3 && self.browse_view == BrowseView::RemoteDetails {
            let manga = self.remote.manga.clone()?;
            let chapter = self.remote.chapters.get(self.browse_selection)?.clone();
            return Some((manga, chapter));
        }
        if self.tab == 0 && self.view == LibraryView::RemoteDetails {
            let manga = self.library_remote.manga.clone()?;
            let chapter = self
                .library_remote
                .chapters
                .get(self.library_remote.chapter_index)?
                .clone();
            return Some((manga, chapter));
        }
        None
    }

    fn remote_source_name(&self, source_id: &str) -> String {
        self.remote
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map(RemoteSource::display_name)
            .unwrap_or_else(|| source_id.to_owned())
    }

    fn downloaded_chapter_dir(
        &self,
        manga: &RemoteManga,
        chapter: &RemoteChapter,
    ) -> Option<PathBuf> {
        let root = downloads_dir(&self.config)?;
        let source = self.remote_source_name(&manga.source_id);
        let dir = chapter_dir(
            &root,
            &source,
            &manga.title,
            &chapter.name,
            chapter.scanlator.as_deref(),
        );
        dir.is_dir().then_some(dir)
    }

    /// Pages for a remote chapter: the downloaded directory when present, otherwise
    /// the source through the host.
    fn remote_chapter_pages(
        &mut self,
        manga: &RemoteManga,
        chapter: &RemoteChapter,
    ) -> Result<Vec<Page>, String> {
        if let Some(dir) = self.downloaded_chapter_dir(manga, chapter) {
            let local = Chapter {
                path: dir,
                title: chapter.name.clone(),
                number: Some(chapter.chapter_number),
                kind: ChapterKind::Directory,
                scanlator: chapter.scanlator.clone(),
                date: None,
            };
            return chapter_pages(&local).map_err(|error| error.to_string());
        }

        let client = self
            .host_client
            .clone()
            .ok_or_else(|| "the extension host is not running".to_owned())?;
        let urls = client
            .pages(
                &manga.source_id,
                &chapter.url,
                chapter.memo.as_deref(),
                Some(&chapter.name),
            )
            .map_err(|error| error.to_string())?;
        if urls.is_empty() {
            return Err("Chapter has no pages.".to_owned());
        }
        Ok(urls
            .into_iter()
            .enumerate()
            .map(|(index, url)| Page {
                name: format!("{:03}.jpg", index + 1),
                source: PageSource::Remote { url },
            })
            .collect())
    }

    /// Apply any scroll page built by the worker. Returns whether the UI changed.
    pub fn poll_scroll_results(&mut self) -> bool {
        let mut changed = false;

        while let Some(result) = self.size_worker.as_ref().and_then(SizeWorker::try_result) {
            let matches = self.reader.as_ref().is_some_and(|reader| {
                reader.chapter_path == result.chapter
                    && reader.pages.len() == result.sizes.len()
                    && self.size_generation == result.generation
            });
            if !matches {
                continue;
            }
            if let Some(reader) = self.reader.as_mut() {
                reader.page_sizes = result.sizes;
            }
            self.build_scroll(terminal_width());
            changed = true;
        }
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
            scroll_debug("promote blocked");
            return false;
        }
        scroll_debug("promote");

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
                if let Some(series) = self.selected_series() {
                    if let Some(remote) = series.remote.clone() {
                        let title = series.title.clone();
                        self.open_library_remote_series(remote, title);
                    } else {
                        self.view = LibraryView::Chapters;
                        self.chapter_index = 0;
                        self.status = None;
                        let count = self.chapter_count();
                        self.chapter_list_state.select((count > 0).then_some(0));
                    }
                }
            }
            LibraryView::Chapters => self.open_chapter(),
            LibraryView::RemoteDetails => {
                self.open_library_remote_chapter(self.library_remote.chapter_index)
            }
        }
    }

    /// Close the reader and return to the list it was opened from.
    pub fn close_reader(&mut self) {
        if self.reader.is_none() {
            return;
        }
        self.reader = None;
        self.status = None;
        match self.reader_origin {
            ReaderOrigin::Remote => {
                self.remote.reader = false;
                self.browse_view = BrowseView::RemoteDetails;
                self.browse_selection = self.remote.chapter_index;
                self.select_browse_entry(self.browse_selection);
            }
            ReaderOrigin::BrowseLocal => {
                self.browse_view = BrowseView::Chapters;
                self.browse_selection = self.browse_local_chapter;
                self.select_browse_entry(self.browse_selection);
            }
            ReaderOrigin::Library => {
                self.view = LibraryView::Chapters;
            }
            ReaderOrigin::LibraryRemote => {
                self.view = LibraryView::RemoteDetails;
                self.library_remote
                    .list_state
                    .select(Some(self.library_remote.chapter_index));
            }
        }
        self.reader_origin = ReaderOrigin::Library;
    }

    /// Back out of the reader/chapter list, or return to the Library tab.
    pub fn back(&mut self) {
        if self.reader.is_some() {
            self.close_reader();
            return;
        }
        match self.view {
            LibraryView::Chapters => {
                self.view = LibraryView::Series;
                self.chapter_index = 0;
                self.status = None;
                self.chapter_list_state.select(None);
            }
            LibraryView::RemoteDetails => {
                self.view = LibraryView::Series;
                self.library_remote.chapter_index = 0;
                self.status = None;
                self.library_remote.list_state.select(None);
            }
            LibraryView::Series => self.select_tab(0),
        }
    }

    pub fn previous(&mut self) {
        if self.reader.is_some() {
            if self.reader_mode == ReaderMode::Scroll {
                self.scroll_lines(-SCROLL_LINES);
            } else {
                self.change_page(false);
            }
            return;
        }
        match self.view {
            LibraryView::Series => {
                self.selected = self.selected.saturating_sub(self.library_columns.max(1));
                self.status = None;
            }
            LibraryView::Chapters => {
                self.chapter_index = self.chapter_index.saturating_sub(1);
                self.status = None;
            }
            LibraryView::RemoteDetails => {
                self.library_remote.chapter_index =
                    self.library_remote.chapter_index.saturating_sub(1);
                self.library_remote
                    .list_state
                    .select(Some(self.library_remote.chapter_index));
                self.status = None;
            }
        }
    }

    pub fn next(&mut self) {
        if self.reader.is_some() {
            if self.reader_mode == ReaderMode::Scroll {
                self.scroll_lines(SCROLL_LINES);
            } else {
                self.change_page(true);
            }
            return;
        }
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
            LibraryView::RemoteDetails => {
                if let Some(last) = self.library_remote_chapter_count().checked_sub(1) {
                    self.library_remote.chapter_index =
                        (self.library_remote.chapter_index + 1).min(last);
                    self.library_remote
                        .list_state
                        .select(Some(self.library_remote.chapter_index));
                    self.status = None;
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
        if self.reader.is_some() && self.reader_mode == ReaderMode::Scroll {
            let page = self.reader.as_ref().map_or(0, |reader| reader.page_index);
            self.jump_to_page(page + 1);
        } else {
            self.change_page(true);
        }
    }

    pub fn previous_page(&mut self) {
        if self.reader.is_some() && self.reader_mode == ReaderMode::Scroll {
            let (page, aligned) = {
                let Some(reader) = self.reader.as_ref() else {
                    return;
                };
                let Some(scroll) = reader.scroll.as_ref() else {
                    return;
                };
                let page = reader.page_index;
                let current = scroll.offset as i32;
                let start = page_start_row(&scroll.layout, page) as i32;
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
        if self.reader.is_some() && self.reader_mode == ReaderMode::Scroll {
            self.scroll_lines(self.scroll_page_step());
        } else {
            self.change_page(true);
        }
    }

    pub fn previous_screen(&mut self) {
        if self.reader.is_some() && self.reader_mode == ReaderMode::Scroll {
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
        self.host_client = None;
        self.remote.host = None;
        self.remote.host_hint = None;
        if let Some(url) = config
            .host_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            match validate_host_url(url) {
                Ok(()) => {
                    self.host_client = Some(HostClient::new(url));
                    self.remote.host = Some(url.to_owned());
                }
                Err(message) => {
                    self.remote.host_hint = Some(format!("{message} Using the bundled host."));
                }
            }
        }
        config.thumbnail_width = config.thumbnail_width.clamp(8, 28);
        self.zoom = config.reader_zoom.clamp(50, 300);
        self.scroll_cache = ScrollCacheConfig {
            prefetch_behind: config.scroll_prefetch_behind.min(16),
            prefetch_ahead: config.scroll_prefetch_ahead.min(16),
            cache_margin: config.scroll_cache_margin.min(32),
        };
        self.remote.favorites = config.remote_library.clone();
        self.config = config;
        self.merge_remote_library();
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
            SettingId::HostUrl => {
                let current = self.config.host_url.clone().unwrap_or_default();
                self.open_text_input(TextAction::SetHostUrl, "Extension host URL", &current);
            }
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
                if self.reader.is_some() {
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
            SettingId::LibraryPath | SettingId::RescanLibrary | SettingId::HostUrl => return,
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
        if self.reader.is_some() && self.reader_mode == ReaderMode::Scroll {
            self.build_scroll(terminal_width());
        }
    }

    pub fn toggle_reader_mode(&mut self) {
        self.reader_mode = match self.reader_mode {
            ReaderMode::Page => ReaderMode::Scroll,
            ReaderMode::Scroll => ReaderMode::Page,
        };

        if self.reader.is_none() {
            return;
        }

        match self.reader_mode {
            ReaderMode::Page => self.load_page(),
            ReaderMode::Scroll => {
                // Show the first page while page sizes are measured in the background.
                self.load_page();
                self.build_scroll(terminal_width());
            }
        }
    }

    pub fn jump_to_page(&mut self, page: usize) {
        if self.reader.is_none() {
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
                let start = page_start_row(&scroll.layout, page);
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
            LibraryView::RemoteDetails if index < self.library_remote_chapter_count() => {
                self.library_remote.chapter_index = index;
                self.library_remote.list_state.select(Some(index));
                self.status = None;
            }
            _ => {}
        }
    }

    fn open_chapter(&mut self) {
        let Some(chapter) = self.selected_chapter().cloned() else {
            return;
        };
        self.remote.reader = false;
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
        self.reader_origin = ReaderOrigin::Library;
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
            ReaderMode::Scroll => {
                // Show the first page while page sizes are measured in the background.
                self.load_page();
                self.build_scroll(terminal_width());
            }
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
        if self.reader.is_none() {
            return;
        }
        if self.remote.reader {
            if self.remote.chapter_index > 0 {
                self.remote.chapter_index -= 1;
                self.open_remote_chapter(self.remote.chapter_index);
            }
            return;
        }
        if self.chapter_index == 0 {
            return;
        }
        self.chapter_index -= 1;
        self.open_chapter();
    }

    pub fn previous_chapter(&mut self) {
        if self.reader.is_none() {
            return;
        }
        if self.remote.reader {
            if self.remote.chapter_index + 1 < self.remote.chapters.len() {
                self.remote.chapter_index += 1;
                self.open_remote_chapter(self.remote.chapter_index);
            }
            return;
        }
        if self.chapter_index + 1 >= self.chapter_count() {
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
        scroll_debug(&format!("build_scroll width={width}"));
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

        if self
            .reader
            .as_ref()
            .is_some_and(|reader| reader.page_sizes.len() != pages.len())
            && !cfg!(test)
        {
            if let (Some(worker), Some(reader)) = (self.size_worker.as_ref(), self.reader.as_ref())
            {
                self.size_generation = self.size_generation.wrapping_add(1);
                worker.request(SizeRequest {
                    generation: self.size_generation,
                    chapter: reader.chapter_path.clone(),
                    pages,
                });
                return;
            }
        }

        let started = std::time::Instant::now();
        let sizes = match self.page_sizes(&pages) {
            Ok(sizes) => sizes,
            Err(error) => {
                scroll_debug(&format!("page_sizes error: {error}"));
                self.set_reader_error(error);
                return;
            }
        };
        scroll_debug(&format!(
            "page_sizes {} pages in {:?}: {:?}",
            sizes.len(),
            started.elapsed(),
            sizes.iter().take(4).collect::<Vec<_>>()
        ));

        // Scroll mode is drawn as a stack of bounded blocks. Block size matters:
        // ratatui-image's kitty backend can only address 297 rows (its diacritic
        // table), so a taller block scrolled past by more than that draws the wrong
        // scanline; very tall protocols also hit terminal texture limits. The worker
        // caches the decoded page, so splitting into blocks costs only crop+encode.
        const MAX_BLOCK_PIXELS: u32 = 2048;
        const MAX_BLOCK_ROWS: u32 = 256;
        let block_rows = (MAX_BLOCK_PIXELS / font_height).clamp(24, MAX_BLOCK_ROWS);

        let mut layout = Vec::with_capacity(sizes.len());
        let mut total_rows = 0u32;
        for (page_index, (source_width, source_height)) in sizes.into_iter().enumerate() {
            let fit_scale = (f64::from(available_pixel_width) / f64::from(source_width)).min(1.0);
            let scale = (fit_scale * zoom).max(f64::EPSILON);
            let pixel_width = (f64::from(source_width) * scale).round().max(1.0) as u32;
            let rows =
                ((f64::from(source_height) * scale / f64::from(font_height)).ceil() as u32).max(1);
            let cell_width = pixel_width.div_ceil(font_width).max(1);
            if cell_width > u32::from(u16::MAX) {
                self.set_reader_error("A page is too wide for scroll mode.");
                return;
            }

            // Very tall webtoon pages are split into segments. One giant protocol per
            // page is slow to encode and often misrenders; segments stay small.
            let mut consumed = 0u32;
            while consumed < rows {
                let segment_rows = (rows - consumed).min(block_rows);
                let segment_px_height = segment_rows * font_height;
                let top_px = (f64::from(consumed) * f64::from(font_height) / scale).floor() as u32;
                let bottom_px = if consumed + segment_rows >= rows {
                    source_height
                } else {
                    ((f64::from(consumed + segment_rows) * f64::from(font_height) / scale).floor()
                        as u32)
                        .min(source_height)
                };
                let segment_source_y = top_px.min(source_height.saturating_sub(1));
                let segment_source_height = bottom_px.saturating_sub(segment_source_y).max(1);

                layout.push(ScrollPageLayout {
                    start: total_rows,
                    rows: segment_rows as u16,
                    cell_width: cell_width as u16,
                    pixel_width,
                    pixel_height: segment_px_height,
                    page: page_index,
                    source_y: segment_source_y,
                    source_height: segment_source_height,
                });
                total_rows = total_rows.saturating_add(segment_rows);
                consumed += segment_rows;
            }
        }

        let viewport_height = self
            .reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .map_or(10, |scroll| scroll.viewport_height)
            .max(1);
        let page_index = self.reader.as_ref().map_or(0, |reader| reader.page_index);
        let start_row = layout
            .iter()
            .find(|segment| segment.page == page_index)
            .map_or(0, |segment| segment.start);
        let total_rows = total_rows.max(1);
        let max_scroll = total_rows.saturating_sub(u32::from(viewport_height));
        let offset = start_row.min(max_scroll);
        let page_count = layout.len();
        let generation = self.bump_scroll_generation();

        scroll_debug(&format!(
            "layout {} pages rows={} viewport={} offset={}",
            layout.len(),
            total_rows,
            viewport_height,
            offset
        ));
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
                scroll_debug("state -> pending");
                reader.pending_scroll = Some(state);
            } else {
                scroll_debug("state -> scroll");
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
        for chunk in pages.chunks(8) {
            let started = std::time::Instant::now();
            let batch: Vec<Option<(u32, u32)>> = std::thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|page| scope.spawn(move || page_dimensions(page).ok()))
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap_or(None))
                    .collect()
            });
            for (page, size) in chunk.iter().zip(batch) {
                // A bad page falls back to the previous size instead of aborting the
                // whole scroll view.
                let size = size
                    .or_else(|| sizes.last().copied())
                    .unwrap_or((800, 1200));
                scroll_debug(&format!(
                    "dim {} {:?} -> {:?}",
                    page.name,
                    started.elapsed(),
                    size
                ));
                sizes.push(size);
            }
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
                                    let layout = scroll.layout.get(*index).copied()?;
                                    Some((
                                        *index,
                                        reader.pages.get(layout.page)?.clone(),
                                        layout,
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
        let Some(layout) = scroll.layout.get(index).copied() else {
            return;
        };
        let Some(page) = reader.pages.get(layout.page).cloned() else {
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
                .rposition(|segment| segment.start as i32 <= next)
                .map_or(0, |index| scroll.layout[index].page);
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
        // Each tab keeps its own navigation state. Only refresh the remote source list
        // the first time Browse is opened with a host configured.
        if index == 3 && self.tab != 3 && self.host_autostart && self.remote.sources.is_empty() {
            self.load_remote_sources();
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

    pub fn library_remote_chapter_count(&self) -> usize {
        self.library_remote.chapters.len()
    }

    pub fn selected_library_remote_chapter(&self) -> Option<&RemoteChapter> {
        self.library_remote
            .chapters
            .get(self.library_remote.chapter_index)
    }

    /// Open a favorite from the Remote library grid.
    pub fn open_library_remote_series(&mut self, remote: RemoteSeries, title: String) {
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        if self.remote.sources.is_empty() {
            self.load_remote_sources();
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let mut manga = RemoteManga {
            source_id: remote.source_id.clone(),
            url: remote.url.clone(),
            title,
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: remote.thumbnail_url.clone(),
            memo: remote.memo.clone(),
        };
        if let Ok(details) = client.details(&manga.source_id, &manga.url, manga.memo.as_deref()) {
            manga.author = details.author;
            manga.artist = details.artist;
            manga.description = details.description;
            manga.genre = details.genre;
            manga.status = details.status;
            manga.memo = details.memo;
            if manga.thumbnail_url.is_none() {
                manga.thumbnail_url = details.thumbnail_url;
            }
        }
        let chapters = match client.chapters(&manga.source_id, &manga.url, manga.memo.as_deref()) {
            Ok(mut chapters) => {
                chapters.sort_by(|left, right| {
                    right
                        .chapter_number
                        .partial_cmp(&left.chapter_number)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| natural_cmp(&right.name, &left.name))
                });
                chapters
            }
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        if chapters.is_empty() {
            self.status = Some(format!("{} has no chapters", manga.title));
            return;
        }
        self.library_remote.manga = Some(manga);
        self.library_remote.chapters = chapters;
        self.library_remote.chapter_index = 0;
        self.library_remote.list_state = ListState::default();
        self.library_remote.list_state.select(Some(0));
        self.view = LibraryView::RemoteDetails;
        self.status = None;
    }

    /// Fold saved remote manga into the one library grid, under the default category.
    pub fn merge_remote_library(&mut self) {
        self.failed_covers.clear();
        let selected_path = self.selected_series().map(|series| series.path.clone());
        self.library.series.retain(|series| series.remote.is_none());

        let category = self
            .config
            .default_category
            .clone()
            .unwrap_or_else(|| "Default".to_owned());
        if !self.library.categories.iter().any(|name| name == &category) {
            self.library.categories.push(category.clone());
        }
        // `Default` is the system category and sits leftmost, like Mihon.
        self.library.categories.retain(|name| name != "Default");
        self.library.categories.insert(0, "Default".to_owned());

        for favorite in self.remote.favorites.clone() {
            let path = PathBuf::from(format!("remote://{}/{}", favorite.source_id, favorite.url));
            self.library.series.push(Series {
                path,
                remote: Some(RemoteSeries {
                    source_id: favorite.source_id,
                    url: favorite.url,
                    thumbnail_url: favorite.thumbnail_url,
                    memo: None,
                }),
                title: favorite.title,
                author: None,
                artist: None,
                description: String::new(),
                tags: Vec::new(),
                status: None,
                cover: None,
                categories: vec![category.clone()],
                chapters: Vec::new(),
                warnings: Vec::new(),
            });
        }

        self.library
            .series
            .sort_by(|left, right| natural_cmp(&left.title, &right.title));
        if let Some(path) = selected_path {
            if let Some(index) = self
                .library
                .series
                .iter()
                .position(|series| series.path == path)
            {
                self.selected = index;
            }
        }
        self.clamp_selection();
    }

    pub fn open_library_remote_chapter(&mut self, index: usize) {
        let Some(chapter) = self.library_remote.chapters.get(index).cloned() else {
            return;
        };
        let Some(manga) = self.library_remote.manga.clone() else {
            return;
        };
        let pages = match self.remote_chapter_pages(&manga, &chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        if pages.is_empty() {
            self.set_remote_error("Chapter has no pages.");
            return;
        }
        let key = remote_progress_key(&manga.source_id, &manga.url, &chapter.url);
        let start_page = self
            .progress
            .entry(&key)
            .map(|entry| entry.page)
            .unwrap_or(0)
            .min(pages.len() - 1);
        self.reader = Some(ReaderState {
            series_title: manga.title,
            chapter_title: chapter.name,
            chapter_path: key,
            pages,
            page_sizes: Vec::new(),
            page_index: start_page,
            protocol: None,
            scroll: None,
            pending_scroll: None,
            error: None,
        });
        self.reader_origin = ReaderOrigin::LibraryRemote;
        self.library_remote.chapter_index = index;
        self.library_remote.list_state.select(Some(index));
        self.remote.reader = false;
        self.status = None;
        self.record_progress(
            &self
                .reader
                .as_ref()
                .map(|reader| reader.chapter_path.clone())
                .unwrap_or_default(),
            start_page,
            self.reader.as_ref().map_or(0, |reader| reader.pages.len()),
        );
        match self.reader_mode {
            ReaderMode::Page => self.load_page(),
            ReaderMode::Scroll => {
                // Show the first page while page sizes are measured in the background.
                self.load_page();
                self.build_scroll(terminal_width());
            }
        }
    }

    /// Human-readable extension host state for the About page.
    pub fn host_summary(&self) -> String {
        if let Some(process) = &self.host_process {
            format!("running · {}", process.base_url())
        } else if let Some(client) = &self.host_client {
            format!("external · {}", client.base_url())
        } else {
            "stopped".to_owned()
        }
    }

    pub fn host_data_path(&self) -> Option<PathBuf> {
        host_data_dir()
    }

    pub fn host_log_path(&self) -> Option<PathBuf> {
        self.host_process.as_ref().map(HostProcess::log_path)
    }

    /// Stop the sidecar, or reconnect/start it. Exposed on the About page.
    pub fn toggle_host(&mut self) {
        if self.host_process.is_some() || self.host_client.is_some() {
            self.host_process = None;
            self.host_client = None;
            self.remote.sources.clear();
            self.remote.results.clear();
            self.remote.manga = None;
            self.remote.chapters.clear();
            self.remote.favorites.clear();
            self.remote.favorites_loaded = false;
            self.remote.status = None;
            self.status = Some("Extension host stopped.".to_owned());
        } else {
            self.status = None;
            self.remote.status = None;
            self.load_remote_sources();
        }
    }

    /// Rescan the library root, keeping the Library tab's place and the Browse
    /// selection when the underlying series still exists.
    pub fn refresh(&mut self) {
        if self.library.root.as_os_str().is_empty() {
            self.status = Some("No library configured. Set TSUIYOMI_LIBRARY.".to_owned());
            return;
        }

        let previous_series = self.selected_series().map(|series| series.path.clone());
        let previous_view = self.view;
        let previous_chapter = self.chapter_index;
        let previous_category = self.category_name_or_default();

        match scan_library(&self.library.root) {
            Ok(library) => {
                self.library = library;
                self.tile_covers.clear();

                self.category_index = self
                    .library
                    .categories
                    .iter()
                    .position(|name| name == &previous_category)
                    .or_else(|| {
                        self.config.default_category.as_deref().and_then(|name| {
                            self.library.categories.iter().position(|item| item == name)
                        })
                    })
                    .unwrap_or(0);

                let restored = previous_series.and_then(|path| {
                    self.visible_indices()
                        .into_iter()
                        .position(|index| self.library.series[index].path == path)
                });
                if let Some(position) = restored {
                    self.selected = position;
                    self.view = previous_view;
                    self.chapter_index =
                        previous_chapter.min(self.chapter_count().saturating_sub(1));
                    if self.view == LibraryView::Chapters {
                        let count = self.chapter_count();
                        self.chapter_list_state
                            .select((count > 0).then_some(self.chapter_index));
                    }
                } else {
                    self.view = LibraryView::Series;
                    self.selected = 0;
                    self.chapter_index = 0;
                    self.chapter_list_state.select(None);
                }

                // Drop Browse selections whose series disappeared during the scan.
                let browse_series_exists = self.browse_local_series.as_deref().is_none_or(|path| {
                    self.library.series.iter().any(|series| series.path == path)
                });
                if !browse_series_exists {
                    self.browse_local_series = None;
                    if self.browse_view == BrowseView::Chapters {
                        self.browse_view = BrowseView::Local;
                    }
                }
                self.merge_remote_library();
                self.clamp_selection();
                self.clamp_browse_selection();
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

    fn category_name_or_default(&self) -> String {
        self.library
            .categories
            .get(self.category_index)
            .cloned()
            .unwrap_or_default()
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

fn scroll_debug(message: &str) {
    if std::env::var_os("TSUIYOMI_DEBUG_SCROLL").is_none() {
        return;
    }
    use std::io::Write;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/tsuiyomi-scroll.log")
    {
        let _ = writeln!(file, "{millis} {message}");
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

fn page_start_row(layout: &[ScrollPageLayout], page: usize) -> u32 {
    layout
        .iter()
        .find(|segment| segment.page == page)
        .map_or(0, |segment| segment.start)
}

fn page_at_offset(scroll: &ScrollState) -> usize {
    let max_scroll = scroll
        .total_rows
        .saturating_sub(u32::from(scroll.viewport_height));
    if scroll.offset >= max_scroll {
        return scroll.layout.last().map_or(0, |segment| segment.page);
    }
    scroll
        .layout
        .iter()
        .rposition(|segment| segment.start <= scroll.offset)
        .map(|index| scroll.layout[index].page)
        .unwrap_or(0)
}

pub(crate) fn build_scroll_protocol(
    picker: &Picker,
    page: &Page,
    layout: ScrollPageLayout,
) -> Result<SlicedProtocol, String> {
    let started = std::time::Instant::now();
    scroll_debug(&format!(
        "build start {} {}x{} rows={}",
        page.name, layout.pixel_width, layout.pixel_height, layout.rows
    ));
    let bytes = read_page(page).map_err(|error| error.to_string())?;
    let image =
        image::load_from_memory(&bytes).map_err(|error| format!("{}: {error}", page.name))?;
    let result = build_scroll_block(picker, &image, &page.name, layout);
    scroll_debug(&format!(
        "build end {} {:?} ok={}",
        page.name,
        started.elapsed(),
        result.is_ok()
    ));
    result
}

/// Crop and encode one scroll block from an already decoded page.
///
/// The worker keeps decoded pages in a small cache so each block of a long webtoon
/// strip does not download and decode the whole image again.
pub(crate) fn build_scroll_block(
    picker: &Picker,
    image: &image::DynamicImage,
    name: &str,
    layout: ScrollPageLayout,
) -> Result<SlicedProtocol, String> {
    let started = std::time::Instant::now();
    let source_y = layout.source_y.min(image.height().saturating_sub(1));
    let available = image.height().saturating_sub(source_y);
    let height = layout.source_height.min(available).max(1);
    let cropped;
    let image = if source_y > 0 || height < image.height() {
        cropped = image.crop_imm(0, source_y, image.width(), height);
        &cropped
    } else {
        image
    };
    let image = image.resize_exact(
        layout.pixel_width,
        layout.pixel_height,
        image::imageops::FilterType::Triangle,
    );
    let result = SlicedProtocol::new_with_resize(
        picker,
        image,
        Size::new(layout.cell_width, layout.rows),
        Resize::Fit(None),
    )
    .map_err(|error| error.to_string());
    if std::env::var_os("TSUIYOMI_DEBUG_SCROLL").is_some() {
        scroll_debug(&format!(
            "block {} y={} h={} {:?} ok={}",
            name,
            layout.source_y,
            layout.source_height,
            started.elapsed(),
            result.is_ok()
        ));
    }
    result
}

fn in_category(series: &Series, category: Option<&str>) -> bool {
    category.is_none_or(|category| series.categories.iter().any(|item| item == category))
}

fn terminal_width() -> u16 {
    terminal::size()
        .map(|(width, _)| width.max(1))
        .unwrap_or(80)
}

/// Tolerate keys written before the host exposed source languages (`pkg|name|`).
fn pin_key_matches(pinned: &str, source: &RemoteSource) -> bool {
    let mut parts = pinned.split('|');
    let (Some(pkg), Some(name), Some(lang)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    pkg == source.pkg.as_deref().unwrap_or_default()
        && name.eq_ignore_ascii_case(&source.name)
        && (lang.is_empty() || lang == source.lang)
}

fn source_pin_key(source: &RemoteSource) -> String {
    format!(
        "{}|{}|{}",
        source.pkg.as_deref().unwrap_or_default(),
        source.name,
        source.lang
    )
}

fn sort_sources_by_name(sources: &[RemoteSource], indices: &mut [usize]) {
    indices.sort_by(|left, right| {
        natural_cmp(
            &sources[*left].display_name(),
            &sources[*right].display_name(),
        )
    });
}

/// Header label for a source language, matching Mihon's readable group names.
fn language_label(lang: &str) -> String {
    match lang {
        "" => "Other".to_owned(),
        "all" => "All".to_owned(),
        other => other.to_uppercase(),
    }
}

/// A host URL points at the JVM service root, not at an extension repository index.
fn validate_host_url(url: &str) -> Result<(), &'static str> {
    let lower = url.trim().to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("The host URL must start with http:// or https://.");
    }
    if lower.ends_with(".json")
        || lower.ends_with(".pb")
        || lower.ends_with(".apk")
        || lower.contains("/raw/repo/")
        || lower.contains("/index.min")
    {
        return Err("That is an extension repository URL. Add it in Browse → Extensions (e → a).");
    }
    Ok(())
}

/// Map an absolute APK URL to the desktop jar published in the same release directory.
fn desktop_jar_url(apk_url: &str) -> Option<String> {
    let url = apk_url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return None;
    }
    url.strip_suffix(".apk").map(|stem| format!("{stem}.jar"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui_image::picker::Picker;

    use crate::scroll_worker::ScrollWorker;

    use super::*;
    use tsuiyomi_core::ExtensionSource;

    fn series(title: &str, categories: &[&str]) -> Series {
        Series {
            path: title.into(),
            remote: None,
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
    fn repository_urls_are_rejected_as_host_urls() {
        let repo = "https://github.com/keiyoushi/extensions/raw/repo/index.pb";
        assert!(validate_host_url(repo).is_err());
        assert!(validate_host_url("https://example.org/index.min.json").is_err());
        assert!(validate_host_url("https://example.org/x.apk").is_err());
        assert!(validate_host_url("127.0.0.1:4567").is_err());
        assert!(validate_host_url("http://127.0.0.1:4567").is_ok());
        assert!(validate_host_url("https://host.example/").is_ok());

        let mut app = AppState::default();
        app.set_host_url(repo);
        assert!(app.config.host_url.is_none());
        assert!(app.remote.host_hint.is_some());
        // The dialog stays open so the value can be corrected.
        assert_eq!(
            app.text_input.as_ref().map(|input| input.value.as_str()),
            Some(repo)
        );

        let config = Config {
            host_url: Some(repo.to_owned()),
            ..Config::default()
        };
        let mut app = AppState::default();
        app.set_config(config);
        assert!(app.host_client.is_none());
        assert!(app.remote.host_hint.is_some());
    }

    #[test]
    fn desktop_jar_urls_mirror_apk_urls() {
        assert_eq!(
            desktop_jar_url("https://example.org/releases/x-v1.0.apk").as_deref(),
            Some("https://example.org/releases/x-v1.0.jar")
        );
        assert!(desktop_jar_url("x.apk").is_none());
        assert!(desktop_jar_url("https://example.org/x.jar").is_none());
    }

    #[test]
    fn text_input_edits_and_cancels() {
        // The quit dialog must keep its shortcut keys, not switch to text input.
        let quit_dialog = AppState {
            quit_confirm: true,
            ..AppState::default()
        };
        assert!(!quit_dialog.text_mode());

        let mut app = AppState::default();
        app.open_text_input(TextAction::SearchRemote, "Search source", "one");
        assert!(app.text_mode());

        app.input_char(' ');
        app.input_char('p');
        app.input_backspace();
        assert_eq!(app.text_input.as_ref().unwrap().value, "one ");

        app.input_cancel();
        assert!(app.text_input.is_none());
        assert!(!app.text_mode());

        app.open_text_input(TextAction::SearchRemote, "Search source", "berserk");
        app.input_submit();
        assert!(app.text_input.is_none());
        assert_eq!(app.browse_view, BrowseView::RemoteResults);
        assert_eq!(app.remote.query, "berserk");
    }

    #[test]
    fn remote_reader_uses_remote_chapter_navigation_counts() {
        let mut app = AppState {
            reader_origin: ReaderOrigin::Remote,
            ..AppState::default()
        };
        app.remote.reader = true;
        app.remote.chapter_index = 1;
        app.remote.chapters = vec![
            RemoteChapter {
                url: "/ch/2".to_owned(),
                name: "Chapter 2".to_owned(),
                chapter_number: 2.0,
                scanlator: None,
                upload_date: 0,
                memo: None,
            },
            RemoteChapter {
                url: "/ch/1".to_owned(),
                name: "Chapter 1".to_owned(),
                chapter_number: 1.0,
                scanlator: None,
                upload_date: 0,
                memo: None,
            },
        ];

        assert_eq!(app.reader_chapter_index(), 1);
        assert_eq!(app.reader_chapter_count(), 2);
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
            remote: None,
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

        assert!(app.reader.is_some());
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
    fn splits_tall_webtoon_pages_into_small_blocks() {
        let root = tempfile::tempdir().unwrap();
        let image = image::RgbImage::from_pixel(100, 12_000, image::Rgb([30, 40, 50]));
        image.save(root.path().join("1.png")).unwrap();

        let series = Series {
            path: root.path().to_path_buf(),
            remote: None,
            title: "Strip".to_owned(),
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
        app.open();
        app.open();
        app.toggle_reader_mode();

        let scroll = app.reader.as_ref().unwrap().scroll.as_ref().unwrap();
        assert!(scroll.layout.len() > 1, "tall page must be split");
        assert!(scroll.layout.iter().all(|block| block.page == 0));
        assert!(scroll.layout.iter().all(|block| block.rows <= 256));
        for pair in scroll.layout.windows(2) {
            assert_eq!(
                pair[0].start + u32::from(pair[0].rows),
                pair[1].start,
                "blocks must tile without gaps"
            );
            assert_eq!(
                pair[0].source_y + pair[0].source_height,
                pair[1].source_y,
                "source crops must tile without gaps"
            );
        }
        let last = scroll.layout.last().unwrap();
        assert_eq!(last.source_y + last.source_height, 12_000);
        assert_eq!(page_at_offset(scroll), 0);
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
        assert_eq!(app.tab, 3);
        assert_eq!(app.browse_view, BrowseView::Chapters);
        assert_eq!(app.view, LibraryView::Series);
        assert_eq!(
            app.browse_detail_series()
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );
        assert_eq!(
            app.selected_browse_local_series()
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );
    }

    fn remote_app() -> AppState {
        let mut app = AppState::new(library(), None);
        app.host_autostart = false;
        app.remote.sources = vec![
            RemoteSource {
                id: "1".to_owned(),
                name: "Alpha".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.a".to_owned()),
            },
            RemoteSource {
                id: "2".to_owned(),
                name: "Beta".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.b".to_owned()),
            },
            RemoteSource {
                id: "3".to_owned(),
                name: "Gamma".to_owned(),
                lang: "ja".to_owned(),
                pkg: Some("pkg.c".to_owned()),
            },
        ];
        app
    }

    #[test]
    fn opens_a_downloaded_chapter_without_the_host() {
        let root = tempfile::tempdir().unwrap();
        let downloads = root.path().join("downloads");
        let dir = chapter_dir(&downloads, "Alpha (EN)", "Test Manga", "Chapter 1", None);
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbImage::from_pixel(10, 20, image::Rgb([10, 20, 30]))
            .save(dir.join("001.png"))
            .unwrap();

        let mut app = remote_app();
        app.config.downloads_path = Some(downloads);
        app.library_remote.manga = Some(RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/test".to_owned(),
            title: "Test Manga".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        });
        app.library_remote.chapters = vec![RemoteChapter {
            url: "/chapter/1".to_owned(),
            name: "Chapter 1".to_owned(),
            chapter_number: 1.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        }];

        app.open_library_remote_chapter(0);

        let reader = app.reader.as_ref().expect("chapter should open");
        assert_eq!(reader.pages.len(), 1);
        assert!(matches!(reader.pages[0].source, PageSource::File(_)));
    }

    #[test]
    fn sources_are_grouped_by_language_with_pins_first() {
        let mut app = remote_app();
        app.config.pinned_sources = vec!["pkg.c|Gamma|ja".to_owned()];

        let rows = app.source_rows();
        let labels: Vec<String> = rows
            .iter()
            .map(|row| match row {
                SourceRow::Header(label) => format!("[{label}]"),
                SourceRow::Local => "local".to_owned(),
                SourceRow::Remote(index) => app.remote.sources[*index].name.clone(),
            })
            .collect();
        assert_eq!(
            labels,
            vec!["[Pinned]", "Gamma", "[Local]", "local", "[EN]", "Alpha", "Beta"]
        );
        // Row 1 is Gamma, which is remote index 2 and therefore selection index 3.
        assert_eq!(app.source_index_for_row(1), Some(0));
        assert_eq!(app.source_index_for_row(0), None);
        assert_eq!(
            app.source_index_for_row(3),
            Some(1),
            "row 3 is the Local source"
        );
    }

    #[test]
    fn source_arrows_follow_the_displayed_order() {
        let mut app = remote_app();
        app.config.pinned_sources = vec!["pkg.c|Gamma|ja".to_owned()];
        app.browse_view = BrowseView::Sources;
        app.browse_selection = 0;

        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(2)));
        app.browse_next();
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Local));
        app.browse_next();
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(0)));
        app.browse_next();
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(1)));
        app.browse_previous();
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(0)));
        app.browse_previous();
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Local));
        app.browse_previous();
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(2)));
    }

    #[test]
    fn escape_at_a_browse_tab_root_returns_to_library() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::Sources;
        app.browse_parent();
        assert_eq!(app.tab, 0);

        app.tab = 3;
        app.browse_view = BrowseView::Local;
        app.browse_parent();
        assert_eq!(app.tab, 3);
        assert_eq!(app.browse_view, BrowseView::Sources);

        // Extensions and Migrate keep you in Browse.
        for view in [BrowseView::Extensions, BrowseView::Migrate] {
            app.tab = 3;
            app.browse_view = view;
            app.browse_parent();
            assert_eq!(app.tab, 3);
            assert_eq!(app.browse_view, view);
        }
    }

    #[test]
    fn pinning_a_source_persists_and_unpins() {
        let mut app = remote_app();
        app.browse_view = BrowseView::Sources;
        app.browse_selection = 1; // Alpha

        app.toggle_pin_for_selection();
        assert_eq!(app.config.pinned_sources, vec!["pkg.a|Alpha|en"]);
        assert_eq!(
            app.selected_source_ref(),
            Some(SourceRef::Remote(0)),
            "the pinned source stays selected after moving to the group"
        );

        app.toggle_pin_for_selection();
        assert!(app.config.pinned_sources.is_empty());
    }

    #[test]
    fn extension_search_filters_and_extension_pins_all_sources() {
        let mut app = remote_app();
        app.remote.sources.push(RemoteSource {
            id: "4".to_owned(),
            name: "MangaDex".to_owned(),
            lang: "en".to_owned(),
            pkg: Some("pkg.mangadex".to_owned()),
        });
        app.remote.sources.push(RemoteSource {
            id: "5".to_owned(),
            name: "MangaDex".to_owned(),
            lang: "ja".to_owned(),
            pkg: Some("pkg.mangadex".to_owned()),
        });
        app.remote.extensions = vec![
            ExtensionInfo {
                name: "MangaDex".to_owned(),
                pkg: "pkg.mangadex".to_owned(),
                lang: "all".to_owned(),
                version: "1.0".to_owned(),
                apk: "x.apk".to_owned(),
                installed: true,
                nsfw: false,
                source_count: 2,
                sources: vec![
                    ExtensionSource {
                        name: "MangaDex".to_owned(),
                        lang: "en".to_owned(),
                    },
                    ExtensionSource {
                        name: "MangaDex".to_owned(),
                        lang: "ja".to_owned(),
                    },
                ],
                repo_url: "repo".to_owned(),
            },
            ExtensionInfo {
                name: "Comick".to_owned(),
                pkg: "pkg.comick".to_owned(),
                lang: "en".to_owned(),
                version: "1.0".to_owned(),
                apk: "y.apk".to_owned(),
                installed: false,
                nsfw: false,
                source_count: 1,
                sources: Vec::new(),
                repo_url: "repo".to_owned(),
            },
        ];
        app.browse_view = BrowseView::Extensions;
        app.browse_tab = BrowseTab::Extensions;

        app.extension_search_start();
        for character in "dex".chars() {
            app.extension_search_push(character);
        }
        assert!(app.extension_search);
        assert_eq!(app.filtered_extension_count(), 1);
        assert_eq!(
            app.selected_extension().map(|ext| ext.name.as_str()),
            Some("MangaDex")
        );

        app.toggle_selected_extension_pin();
        assert_eq!(
            app.config.pinned_sources,
            vec!["pkg.mangadex|MangaDex|en", "pkg.mangadex|MangaDex|ja"]
        );

        app.extension_search_stop(true);
        assert!(!app.extension_search);
        assert_eq!(app.filtered_extension_count(), 2);
    }

    #[test]
    fn global_search_navigation_moves_between_sources_and_results() {
        let mut app = remote_app();
        let manga = |title: &str| RemoteManga {
            source_id: "1".to_owned(),
            url: format!("/manga/{title}"),
            title: title.to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        app.remote.global_groups = vec![
            GlobalGroup {
                source: app.remote.sources[0].clone(),
                mangas: vec![manga("Alpha"), manga("Beta")],
            },
            GlobalGroup {
                source: app.remote.sources[1].clone(),
                mangas: vec![manga("Gamma")],
            },
        ];

        app.global_next_item();
        assert_eq!(app.remote.global_item, 1);
        app.global_previous_item();
        assert_eq!(app.remote.global_item, 0);
        app.global_next_group();
        assert_eq!(app.remote.global_group, 1);
        assert_eq!(app.remote.global_item, 0);
        app.global_previous_group();
        assert_eq!(app.remote.global_group, 0);

        app.global_open();
        assert_eq!(app.browse_view, BrowseView::RemoteDetails);
        assert_eq!(
            app.remote.manga.as_ref().map(|manga| manga.title.as_str()),
            Some("Alpha")
        );
        assert_eq!(app.remote.remote_details_origin, BrowseView::GlobalResults);
    }

    #[test]
    fn browse_tabs_restore_the_sources_stack() {
        let mut app = remote_app();
        app.browse_view = BrowseView::RemoteResults;

        app.set_browse_tab(BrowseTab::Extensions);
        assert_eq!(app.browse_view, BrowseView::Extensions);

        app.set_browse_tab(BrowseTab::Sources);
        assert_eq!(app.browse_view, BrowseView::RemoteResults);

        app.set_browse_tab(BrowseTab::Migrate);
        assert_eq!(app.browse_view, BrowseView::Migrate);
    }

    #[test]
    fn browse_and_library_keep_independent_navigation() {
        let mut app = AppState::new(library(), None);
        // Do not let the test spawn a JVM.
        app.host_autostart = false;

        // Open the first library series and its chapter list.
        app.open();
        assert_eq!(app.view, LibraryView::Chapters);
        let library_series = app
            .selected_series()
            .map(|series| series.title.clone())
            .unwrap();

        // Browse starts at its own root, not at the Library's chapter list.
        app.select_tab(3);
        assert_eq!(app.view, LibraryView::Chapters);
        assert_eq!(app.browse_view, BrowseView::Sources);

        // Open a different series in Browse.
        app.browse_view = BrowseView::Local;
        let position = app
            .browse_local_indices()
            .into_iter()
            .position(|index| app.library.series[index].title == "Berserk")
            .unwrap();
        app.select_browse_index(position);
        app.browse_open();
        app.browse_open();
        assert_eq!(app.browse_view, BrowseView::Chapters);
        assert_eq!(
            app.selected_browse_local_series()
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );

        // The Library selection did not move.
        assert_eq!(
            app.selected_series().map(|series| series.title.clone()),
            Some(library_series)
        );
        assert_eq!(app.view, LibraryView::Chapters);
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
