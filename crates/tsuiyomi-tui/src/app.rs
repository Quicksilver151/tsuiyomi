use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use crossterm::terminal;
use ratatui::{layout::Size, widgets::ListState};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, sliced::SlicedProtocol, Resize};
use tsuiyomi_core::progress_key as remote_progress_key;
use tsuiyomi_core::{
    chapter::natural_cmp, chapter_dir, chapter_dir_name, chapter_pages, downloads_dir,
    ensure_runtime, ensure_series_cover, fetch_bytes, host_data_dir, is_downloaded,
    legacy_remote_cover_path, load_cached_story, merge_extensions, page_dimensions, read_cover,
    read_page, remote_cover_path, save_cached_story, save_config, scan_library, series_dir_name,
    source_dir_name, story_cache_path, BookmarkStore, CachedStory, Chapter, ChapterKind, Config,
    ExtensionInfo, GlobalSearchScope, HistoryEntry, HistoryStore, HostClient, HostProcess, Library,
    LibrarySort, LocalSourceOrder, Page, PageSource, ProgressStore, ReadingMode, RemoteChapter,
    RemoteFavorite, RemoteManga, RemoteMangaPage, RemoteSeries, RemoteSource, Series,
    SourceDisplayMode, SourceFilter, SourcePreference, SourcePreferenceKind, SourcePreferenceValue,
    SourcePreferences, Theme,
};

use crate::{
    cover_worker::{CoverRequest, CoverWorker},
    download_worker::{DownloadRequest, DownloadWorker},
    host_worker::{HostRequest, HostWorker},
    layout::TAB_COUNT,
    palette,
    scroll_worker::{BuildRequest, BuildResult, ScrollWorker, SizeRequest, SizeWorker},
    search_worker::{GlobalSearchEvent, GlobalSearchWorker},
    story_worker::{StoryRequest, StoryWorker},
};

/// Lines moved per arrow/wheel step in continuous scroll mode.
const SCROLL_LINES: i32 = 5;

/// Maximum size of one scroll block. Keeping blocks around one screen bounds the
/// terminal write each time a new image is transmitted.
const SCROLL_BLOCK_PIXELS: u32 = 1024;
const SCROLL_BLOCK_ROWS_MAX: u32 = 128;
/// Older builds used 2048px blocks; scale prefetch counts so the lookahead distance
/// in rows stays the same after shrinking blocks.
const SCROLL_PREFETCH_SCALE: usize = 2;

/// Upper bound on decoded library tile covers kept in memory.
const TILE_COVER_CACHE: usize = 256;

/// Unique suffix for atomic cover writes; the loader thread and UI can race.
static COVER_TEMP_ID: AtomicU64 = AtomicU64::new(0);
/// Retry a failed cover fetch after this long instead of giving up for the session.
const COVER_RETRY: Duration = Duration::from_secs(10);

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
    /// Source display name from the cache, for offline details.
    pub source_name: Option<String>,
    /// Per-chapter download flags, recomputed once per open/download.
    pub downloaded: Vec<bool>,
}

/// Where the reader was opened from, so closing it returns to the right place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReaderOrigin {
    #[default]
    Library,
    BrowseLocal,
    Remote,
    LibraryRemote,
    History,
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
    SourceFilters,
    SourcePrefs,
    Filters,
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

#[derive(Debug, Clone)]
pub struct GlobalFailure {
    pub source: RemoteSource,
    pub message: String,
}

/// One rendered row in the Sources list. Headers are not selectable.
pub enum SourceRow {
    Header(String),
    Local,
    Remote(usize),
}

pub struct SourcePrefsState {
    pub source: RemoteSource,
    pub preferences: SourcePreferences,
    pub list_state: ListState,
}

pub struct SourcePrefPicker {
    pub key: String,
    pub title: String,
    pub values: Vec<String>,
    pub labels: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterRow {
    Filter { path: Vec<usize>, depth: usize },
    Reset,
    Apply,
}

pub struct FilterPicker {
    pub path: Vec<usize>,
    pub title: String,
    pub values: Vec<String>,
    pub selected: usize,
}

/// One rendered row in the source-management screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceFilterRow {
    Header(&'static str),
    Language {
        lang: String,
        enabled: bool,
        source_count: usize,
    },
    Source {
        index: usize,
        enabled: bool,
    },
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
    SearchLocal,
    GlobalSearch,
    SourcePreference,
    FilterText,
    AddExtensionStore,
    SetHostUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    pub action: TextAction,
    pub title: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RemoteListing {
    #[default]
    Popular,
    Latest,
    Search,
}

pub struct RemoteState {
    pub sources: Vec<RemoteSource>,
    pub source: Option<RemoteSource>,
    pub results: Vec<RemoteManga>,
    pub listing: RemoteListing,
    pub filters: Vec<SourceFilter>,
    pub default_filters: Vec<SourceFilter>,
    pub filters_loaded: bool,
    pub filter_list_state: ListState,
    pub remote_list_state: ListState,
    pub query: String,
    pub page: i32,
    pub has_next_page: bool,
    pub manga: Option<RemoteManga>,
    pub chapters: Vec<RemoteChapter>,
    pub chapter_index: usize,
    /// Per-chapter download flags, recomputed once per open/download.
    pub downloaded: Vec<bool>,
    pub reader: bool,
    pub status: Option<String>,
    pub error: bool,
    pub columns: usize,
    pub grid_offset: usize,
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
    pub global_failures: Vec<GlobalFailure>,
    pub global_total: usize,
    pub global_finished: usize,
    pub global_loading: bool,
    pub global_generation: u64,
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
            listing: RemoteListing::Popular,
            filters: Vec::new(),
            default_filters: Vec::new(),
            filters_loaded: false,
            filter_list_state: ListState::default(),
            remote_list_state: ListState::default(),
            query: String::new(),
            page: 1,
            has_next_page: false,
            manga: None,
            chapters: Vec::new(),
            chapter_index: 0,
            downloaded: Vec::new(),
            reader: false,
            status: None,
            error: false,
            columns: 1,
            grid_offset: 0,
            extensions: Vec::new(),
            repos: Vec::new(),
            host: None,
            host_hint: None,
            favorites: Vec::new(),
            favorites_loaded: false,
            global_query: String::new(),
            global_groups: Vec::new(),
            global_failures: Vec::new(),
            global_total: 0,
            global_finished: 0,
            global_loading: false,
            global_generation: 0,
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

/// A stable position inside the source image, used to keep the viewport in place
/// when zoom or width rebuilds the row layout.
#[derive(Debug, Clone, Copy)]
pub struct ScrollAnchor {
    page: usize,
    /// Vertical position inside the source page, in source pixels.
    source_y: f64,
}

pub struct ScrollState {
    pub generation: u64,
    pub layout: Vec<ScrollPageLayout>,
    pub protocols: Vec<Option<SlicedProtocol>>,
    pub requested: Vec<bool>,
    /// Protocols built but not yet rendered. Rendering a Kitty block copies its
    /// transmission string into the frame buffer, so reveal fresh blocks gradually.
    pub fresh: Vec<bool>,
    /// A fresh visible block was held back this frame; draw again immediately.
    pub deferred_transmits: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionScope {
    LibrarySeries,
    BrowseLocal,
    RemoteResults,
    GlobalResults,
    Sources,
    Extensions,
    Stores,
    LocalChapters,
    RemoteChapters,
    History,
}

#[derive(Debug, Clone)]
pub struct MultiSelect {
    pub scope: SelectionScope,
    pub keys: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct LibraryDeleteItem {
    pub path: PathBuf,
    pub title: String,
    pub remote: Option<RemoteSeries>,
}

#[derive(Debug, Clone)]
pub struct LibraryDeleteDialog {
    pub items: Vec<LibraryDeleteItem>,
    pub remove_from_library: bool,
    pub delete_downloads: bool,
    pub focus: usize,
}

impl LibraryDeleteDialog {
    fn has_remote(&self) -> bool {
        self.items.iter().any(|item| item.remote.is_some())
    }
}

#[derive(Default)]
pub struct AppState {
    pub tab: usize,
    pub fullscreen: bool,
    pub view: LibraryView,
    pub reader_mode: ReaderMode,
    pub library: Library,
    pub library_remote: LibraryRemoteState,
    /// Chapters fetched for saved manga, keyed by source+URL. Reopening a story is
    /// then instant; `r` refreshes it.
    remote_chapter_cache: HashMap<String, (RemoteManga, Vec<RemoteChapter>)>,
    pub category_index: usize,
    pub selected: usize,
    pub chapter_index: usize,
    pub chapter_list_state: ListState,
    pub reader: Option<ReaderState>,
    pub reader_origin: ReaderOrigin,
    pub image_picker: Option<Picker>,
    pub progress: ProgressStore,
    pub bookmarks: BookmarkStore,
    pub history: HistoryStore,
    pub history_selection: usize,
    pub history_list_state: ListState,
    history_dirty: bool,
    progress_dirty: bool,
    progress_changed_at: Option<Instant>,
    /// Worker that decodes/resizes/encodes scroll pages off the UI thread.
    pub(crate) scroll_worker: Option<ScrollWorker>,
    pub(crate) size_worker: Option<SizeWorker>,
    pub(crate) download_worker: Option<DownloadWorker>,
    pub(crate) story_worker: Option<StoryWorker>,
    pub(crate) host_worker: Option<HostWorker>,
    pub(crate) global_search_worker: Option<GlobalSearchWorker>,
    pub(crate) cover_worker: Option<CoverWorker>,
    cover_lookups: HashSet<String>,
    host_starting: bool,
    host_start_epoch: u64,
    pending_browse_sources: bool,
    cover_recovery_at: Option<Instant>,
    /// Story refresh currently running (source+URL key).
    story_refresh: Option<String>,
    /// Stories already refreshed in this session.
    story_refreshed: HashSet<String>,
    /// Stories removed from the library; ignore late background refreshes.
    removed_stories: HashSet<String>,
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
    pub browse_local_query: String,
    pub browse_local_latest: bool,
    pub browse_local_list_state: ListState,
    pub source_filter_list_state: ListState,
    pub source_prefs: Option<SourcePrefsState>,
    pub source_pref_picker: Option<SourcePrefPicker>,
    source_pref_edit: Option<String>,
    pub filter_picker: Option<FilterPicker>,
    filter_edit: Option<Vec<usize>>,
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
    pub failed_covers: HashMap<String, Instant>,
    pub more_section: MoreSection,
    pub more_focus: MoreFocus,
    pub settings_page: SettingsPage,
    pub setting_index: usize,
    pub theme: Theme,
    pub accent: Option<String>,
    pub picker: Option<PickerState>,
    pub quit_confirm: bool,
    /// Pending extension uninstall: package and display name.
    pub uninstall_confirm: Option<Vec<(String, String)>>,
    pub library_delete: Option<LibraryDeleteDialog>,
    pub multi_select: Option<MultiSelect>,
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
            remote_chapter_cache: HashMap::new(),
            category_index: 0,
            selected: 0,
            chapter_index: 0,
            chapter_list_state: ListState::default(),
            reader: None,
            reader_origin: ReaderOrigin::Library,
            image_picker: None,
            progress: ProgressStore::default(),
            bookmarks: BookmarkStore::default(),
            history: HistoryStore::default(),
            history_selection: 0,
            history_list_state: ListState::default(),
            history_dirty: false,
            progress_dirty: false,
            progress_changed_at: None,
            scroll_worker: None,
            size_worker: None,
            download_worker: None,
            story_worker: None,
            host_worker: None,
            global_search_worker: None,
            cover_worker: None,
            cover_lookups: HashSet::new(),
            host_starting: false,
            host_start_epoch: 0,
            pending_browse_sources: false,
            cover_recovery_at: None,
            story_refresh: None,
            story_refreshed: HashSet::new(),
            removed_stories: HashSet::new(),
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
            browse_local_query: String::new(),
            browse_local_latest: false,
            browse_local_list_state: ListState::default(),
            source_filter_list_state: ListState::default(),
            source_prefs: None,
            source_pref_picker: None,
            source_pref_edit: None,
            filter_picker: None,
            filter_edit: None,
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
            failed_covers: HashMap::new(),
            more_section: MoreSection::Settings,
            more_focus: MoreFocus::Content,
            settings_page: SettingsPage::Main,
            setting_index: 0,
            theme: Theme::default(),
            accent: None,
            picker: None,
            quit_confirm: false,
            uninstall_confirm: None,
            library_delete: None,
            multi_select: None,
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
        let Ok(bytes) = read_cover(path) else {
            return;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            return;
        };
        self.insert_tile_cover(path.to_path_buf(), image);
    }

    /// Return a usable cover path for a local series, regenerating it if the file
    /// disappeared since the last scan. Failed attempts are remembered until refresh.
    pub fn ensure_local_series_cover(&mut self, index: usize) -> Option<PathBuf> {
        if let Some(path) = self
            .library
            .series
            .get(index)
            .and_then(|series| series.cover.clone())
            .filter(|path| path.exists())
        {
            return Some(path);
        }
        let key = format!(
            "local:{}",
            self.library
                .series
                .get(index)
                .map(|series| series.path.display().to_string())
                .unwrap_or_default()
        );
        if self.cover_failed(&key) {
            return None;
        }
        let generated = self.library.series.get(index).and_then(ensure_series_cover);
        if let Some(path) = generated {
            if let Some(series) = self.library.series.get_mut(index) {
                series.cover = Some(path.clone());
            }
            self.failed_covers.remove(&key);
            Some(path)
        } else {
            self.mark_cover_failed(key);
            None
        }
    }

    fn cover_failed(&self, key: &str) -> bool {
        self.failed_covers
            .get(key)
            .is_some_and(|at| at.elapsed() < COVER_RETRY)
    }

    fn mark_cover_failed(&mut self, key: String) {
        self.failed_covers.insert(key, Instant::now());
    }

    fn insert_tile_cover(&mut self, path: PathBuf, image: image::DynamicImage) {
        let Some(picker) = self.image_picker.as_ref() else {
            return;
        };
        if self.tile_covers.len() >= TILE_COVER_CACHE {
            if let Some(oldest) = self.tile_covers.keys().next().cloned() {
                self.tile_covers.remove(&oldest);
            }
        }
        let protocol = picker.new_resize_protocol(image);
        self.tile_covers.insert(path, protocol);
    }

    pub fn browse_previous(&mut self) {
        let step = match self.browse_view {
            BrowseView::Local => self.browse_columns.max(1),
            BrowseView::RemoteResults
                if self.config.source_display_mode != SourceDisplayMode::List =>
            {
                self.browse_columns.max(1)
            }
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
                BrowseView::RemoteResults
                    if self.config.source_display_mode != SourceDisplayMode::List =>
                {
                    self.browse_columns.max(1)
                }
                _ => 1,
            };
            self.browse_selection = (self.browse_selection + step).min(count - 1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    pub fn browse_previous_column(&mut self) {
        if self.browse_view == BrowseView::Local
            || (self.browse_view == BrowseView::RemoteResults
                && self.config.source_display_mode != SourceDisplayMode::List)
        {
            self.browse_selection = self.browse_selection.saturating_sub(1);
            self.select_browse_entry(self.browse_selection);
        }
    }

    pub fn browse_next_column(&mut self) {
        if self.browse_view == BrowseView::Local
            || (self.browse_view == BrowseView::RemoteResults
                && self.config.source_display_mode != SourceDisplayMode::List)
        {
            let count = self.browse_item_count();
            if count > 0 {
                self.browse_selection = (self.browse_selection + 1).min(count - 1);
                self.select_browse_entry(self.browse_selection);
            }
        }
    }

    pub fn browse_open(&mut self) {
        // Enter is an action on these panels; keep the selection for batch install
        // and update. Other panels navigate when Enter is pressed.
        if !matches!(
            self.browse_view,
            BrowseView::Extensions | BrowseView::Stores
        ) {
            self.multi_select = None;
        }
        match self.browse_view {
            BrowseView::Sources => match self.selected_source_ref() {
                Some(SourceRef::Local) => {
                    self.record_source_opened(SourceRef::Local);
                    self.browse_view = BrowseView::Local;
                    self.browse_selection = 0;
                    self.select_browse_entry(0);
                }
                Some(SourceRef::Remote(index)) => {
                    let Some(source) = self.remote.sources.get(index).cloned() else {
                        return;
                    };
                    self.record_source_opened(SourceRef::Remote(index));
                    self.remote.source = Some(source);
                    self.remote.query.clear();
                    self.remote.page = 1;
                    self.remote.filters.clear();
                    self.remote.default_filters.clear();
                    self.remote.filters_loaded = false;
                    self.load_remote_filters();
                    self.load_remote_results(RemoteListing::Popular);
                    self.browse_view = BrowseView::RemoteResults;
                    self.browse_selection = 0;
                    self.browse_grid_offset = 0;
                    self.select_browse_entry(0);
                }
                None => {}
            },
            BrowseView::SourceFilters => self.toggle_source_filter_selection(),
            BrowseView::SourcePrefs => self.activate_source_preference(),
            BrowseView::Filters => self.activate_filter_selection(),
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
                let Some(manga) = self.visible_remote_result(self.browse_selection).cloned() else {
                    return;
                };
                let key = remote_story_key(&manga.source_id, &manga.url);
                if let Some((cached, chapters)) = self.remote_chapter_cache.get(&key).cloned() {
                    self.remote.manga = Some(cached);
                    self.remote.chapters = chapters;
                    self.remote.remote_details_origin = BrowseView::RemoteResults;
                    self.refresh_download_marks();
                    self.browse_view = BrowseView::RemoteDetails;
                    self.browse_selection = 0;
                    self.select_browse_entry(0);
                    return;
                }
                self.remote.manga = Some(manga);
                self.remote.remote_details_origin = BrowseView::RemoteResults;
                self.load_remote_details();
                self.load_remote_chapters();
                self.browse_view = BrowseView::RemoteDetails;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::RemoteDetails => self.open_remote_chapter(self.browse_selection),
            BrowseView::Extensions => self.install_selected_extension(),
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
            BrowseView::SourceFilters | BrowseView::SourcePrefs => {
                self.browse_view = BrowseView::Sources;
                self.browse_selection = 0;
                self.browse_list_state = ListState::default();
                self.select_browse_entry(0);
            }
            BrowseView::Filters => {
                self.browse_view = BrowseView::RemoteResults;
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
            BrowseView::SourceFilters => self.load_remote_sources(),
            BrowseView::SourcePrefs => self.reload_source_prefs(),
            BrowseView::Filters => self.load_remote_filters(),
            BrowseView::Local => self.refresh(),
            BrowseView::RemoteResults => {
                self.load_remote_results(self.remote.listing);
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
            BrowseView::Sources => self.source_order().len(),
            BrowseView::SourceFilters => self.source_filter_item_count(),
            BrowseView::SourcePrefs => self
                .source_prefs
                .as_ref()
                .map_or(0, |state| state.preferences.prefs.len()),
            BrowseView::Filters => self.filter_item_count(),
            BrowseView::Local => self.browse_local_indices().len(),
            BrowseView::Chapters => self.browse_local_chapter_count(),
            BrowseView::RemoteResults => self.visible_remote_count(),
            BrowseView::RemoteDetails => self.remote.chapters.len(),
            BrowseView::Extensions => self.filtered_extension_count(),
            BrowseView::Stores => self.remote.repos.len(),
            BrowseView::GlobalResults | BrowseView::Migrate => 0,
        }
    }

    pub fn local_series_count(&self) -> usize {
        self.library
            .series
            .iter()
            .filter(|series| series.remote.is_none())
            .count()
    }

    /// Indices into `library.series` for the Local source, after search and recent
    /// filtering and in the configured order.
    pub fn browse_local_indices(&self) -> Vec<usize> {
        let query = self.browse_local_query.trim().to_lowercase();
        let recent_after =
            std::time::SystemTime::now().checked_sub(Duration::from_secs(7 * 24 * 60 * 60));
        let mut indices: Vec<(usize, Option<std::time::SystemTime>)> = self
            .library
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| series.remote.is_none())
            .filter(|(_, series)| query.is_empty() || series.title.to_lowercase().contains(&query))
            .filter_map(|(index, series)| {
                let modified = std::fs::metadata(&series.path)
                    .and_then(|metadata| metadata.modified())
                    .ok();
                if self.browse_local_latest {
                    let modified = modified?;
                    if recent_after.is_some_and(|cutoff| modified < cutoff) {
                        return None;
                    }
                }
                Some((index, modified))
            })
            .collect();

        let descending = self.config.local_source_order == LocalSourceOrder::TitleDesc;
        if self.browse_local_latest {
            indices.sort_by(|left, right| {
                right.1.cmp(&left.1).then_with(|| {
                    natural_cmp(
                        &self.library.series[left.0].title,
                        &self.library.series[right.0].title,
                    )
                })
            });
        } else {
            indices.sort_by(|left, right| {
                let order = natural_cmp(
                    &self.library.series[left.0].title,
                    &self.library.series[right.0].title,
                );
                if descending {
                    order.reverse()
                } else {
                    order
                }
            });
        }
        indices.into_iter().map(|(index, _)| index).collect()
    }

    /// Apply a title filter to the Local source grid.
    pub fn search_local(&mut self, query: &str) {
        self.browse_local_query = query.trim().to_owned();
        if !self.browse_local_query.is_empty() {
            self.browse_local_latest = false;
        }
        self.browse_view = BrowseView::Local;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    /// Toggle the Local source between all series and recently modified folders.
    pub fn toggle_local_latest(&mut self) {
        self.browse_local_latest = !self.browse_local_latest;
        if self.browse_local_latest {
            self.browse_local_query.clear();
        }
        self.browse_view = BrowseView::Local;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    /// Toggle Local source title ordering.
    pub fn toggle_local_order(&mut self) {
        self.config.local_source_order = match self.config.local_source_order {
            LocalSourceOrder::TitleAsc => LocalSourceOrder::TitleDesc,
            LocalSourceOrder::TitleDesc => LocalSourceOrder::TitleAsc,
        };
        self.persist_settings();
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    /// Cycle remote result layout: comfortable grid, compact grid, list.
    pub fn cycle_source_display_mode(&mut self) {
        self.config.source_display_mode = match self.config.source_display_mode {
            SourceDisplayMode::Comfortable => SourceDisplayMode::Compact,
            SourceDisplayMode::Compact => SourceDisplayMode::List,
            SourceDisplayMode::List => SourceDisplayMode::Comfortable,
        };
        self.persist_settings();
        self.browse_grid_offset = 0;
        self.select_browse_entry(self.browse_selection);
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
        self.multi_select = None;
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

    /// Whether a remote source is visible in Browse and global search.
    pub fn source_enabled(&self, source: &RemoteSource) -> bool {
        if self
            .config
            .disabled_sources
            .iter()
            .any(|disabled| pin_key_matches(disabled, source))
        {
            return false;
        }
        self.language_enabled(&source.lang)
    }

    /// Whether a language is enabled. `None` means all languages.
    pub fn language_enabled(&self, lang: &str) -> bool {
        self.config
            .enabled_languages
            .as_ref()
            .is_none_or(|languages| {
                languages
                    .iter()
                    .any(|enabled| enabled.eq_ignore_ascii_case(lang))
            })
    }

    /// Languages present in the loaded source list, sorted by display name.
    pub fn source_languages(&self) -> Vec<String> {
        let mut languages: Vec<String> = self
            .remote
            .sources
            .iter()
            .map(|source| source.lang.clone())
            .collect();
        languages.sort_by(|left, right| {
            natural_cmp(&language_label(left), &language_label(right)).then_with(|| left.cmp(right))
        });
        languages.dedup();
        languages
    }

    fn last_used_source_ref(&self) -> Option<SourceRef> {
        let key = self.config.last_used_source.as_deref()?;
        if key == "local" {
            return Some(SourceRef::Local);
        }
        self.remote
            .sources
            .iter()
            .position(|source| self.source_enabled(source) && pin_key_matches(key, source))
            .map(SourceRef::Remote)
    }

    /// Sources in the order they are displayed: last used, pinned, Local, then language
    /// groups. Disabled sources and languages are omitted.
    ///
    /// Selection indexes this vector, so arrow keys follow the visible list instead of
    /// the host's storage order.
    pub fn source_order(&self) -> Vec<SourceRef> {
        let last_used = self.last_used_source_ref();
        let mut order: Vec<SourceRef> = last_used.into_iter().collect();

        let mut pinned: Vec<usize> = (0..self.remote.sources.len())
            .filter(|index| {
                let source = &self.remote.sources[*index];
                self.source_enabled(source)
                    && self.is_source_pinned(source)
                    && last_used != Some(SourceRef::Remote(*index))
            })
            .collect();
        sort_sources_by_name(&self.remote.sources, &mut pinned);
        order.extend(pinned.into_iter().map(SourceRef::Remote));

        if last_used != Some(SourceRef::Local) {
            order.push(SourceRef::Local);
        }

        let mut grouped: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, source) in self.remote.sources.iter().enumerate() {
            if !self.source_enabled(source)
                || self.is_source_pinned(source)
                || last_used == Some(SourceRef::Remote(index))
            {
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
        let last_used = self.last_used_source_ref();
        let mut group: Option<String> = None;
        for item in self.source_order() {
            let label = if last_used == Some(item) {
                "Last used".to_owned()
            } else {
                match item {
                    SourceRef::Local => "Local".to_owned(),
                    SourceRef::Remote(index) => {
                        let source = &self.remote.sources[index];
                        if self.is_source_pinned(source) {
                            "Pinned".to_owned()
                        } else {
                            language_label(&source.lang)
                        }
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

    /// Rows for the source-management screen: language toggles, then source toggles.
    pub fn source_filter_rows(&self) -> Vec<SourceFilterRow> {
        let mut rows = vec![SourceFilterRow::Header("Languages")];
        for lang in self.source_languages() {
            let source_count = self
                .remote
                .sources
                .iter()
                .filter(|source| source.lang == lang)
                .count();
            rows.push(SourceFilterRow::Language {
                enabled: self.language_enabled(&lang),
                lang,
                source_count,
            });
        }

        rows.push(SourceFilterRow::Header("Sources"));
        let mut indices: Vec<usize> = (0..self.remote.sources.len())
            .filter(|index| {
                let source = &self.remote.sources[*index];
                self.language_enabled(&source.lang)
            })
            .collect();
        indices.sort_by(|left, right| {
            natural_cmp(
                &language_label(&self.remote.sources[*left].lang),
                &language_label(&self.remote.sources[*right].lang),
            )
            .then_with(|| {
                natural_cmp(
                    &self.remote.sources[*left].display_name(),
                    &self.remote.sources[*right].display_name(),
                )
            })
        });
        rows.extend(indices.into_iter().map(|index| {
            SourceFilterRow::Source {
                index,
                enabled: !self
                    .config
                    .disabled_sources
                    .iter()
                    .any(|disabled| pin_key_matches(disabled, &self.remote.sources[index])),
            }
        }));
        rows
    }

    pub fn source_filter_item_count(&self) -> usize {
        self.source_filter_rows()
            .iter()
            .filter(|row| !matches!(row, SourceFilterRow::Header(_)))
            .count()
    }

    pub fn selected_source_filter_row(&self) -> Option<usize> {
        let mut selection = 0;
        for (row, item) in self.source_filter_rows().iter().enumerate() {
            if matches!(item, SourceFilterRow::Header(_)) {
                continue;
            }
            if selection == self.browse_selection {
                return Some(row);
            }
            selection += 1;
        }
        None
    }

    /// Select the row shown at `row` in the rendered source-management list.
    pub fn select_source_filter_row(&mut self, row: usize) {
        let offset = self.source_filter_list_state.offset();
        if let Some(index) = self.source_filter_index_for_row(offset + row) {
            self.select_browse_index(index);
        }
    }

    fn source_filter_index_for_row(&self, row: usize) -> Option<usize> {
        let mut selection = 0;
        for (index, item) in self.source_filter_rows().iter().enumerate() {
            if matches!(item, SourceFilterRow::Header(_)) {
                continue;
            }
            if index == row {
                return Some(selection);
            }
            selection += 1;
        }
        None
    }

    pub fn open_source_filters(&mut self) {
        self.browse_view = BrowseView::SourceFilters;
        self.browse_selection = 0;
        self.source_filter_list_state = ListState::default();
        self.select_browse_entry(0);
    }

    /// Toggle the selected language or source on the source-management screen.
    pub fn toggle_source_filter_selection(&mut self) {
        let Some(selected_row) = self.selected_source_filter_row() else {
            return;
        };
        let rows = self.source_filter_rows();
        let Some(row) = rows.get(selected_row).cloned() else {
            return;
        };
        let anchor = self.browse_selection;
        match row {
            SourceFilterRow::Header(_) => return,
            SourceFilterRow::Language { lang, .. } => {
                let all = self.source_languages();
                let mut enabled = self
                    .config
                    .enabled_languages
                    .clone()
                    .unwrap_or_else(|| all.clone());
                if let Some(position) = enabled
                    .iter()
                    .position(|candidate| candidate.eq_ignore_ascii_case(&lang))
                {
                    enabled.remove(position);
                } else {
                    enabled.push(lang);
                }
                self.config.enabled_languages =
                    if all.iter().all(|candidate| enabled.contains(candidate)) {
                        None
                    } else {
                        Some(enabled)
                    };
            }
            SourceFilterRow::Source { index, enabled } => {
                let Some(source) = self.remote.sources.get(index).cloned() else {
                    return;
                };
                let key = source_pin_key(&source);
                if enabled {
                    self.config.disabled_sources.push(key);
                } else if let Some(position) = self
                    .config
                    .disabled_sources
                    .iter()
                    .position(|disabled| disabled == &key)
                {
                    self.config.disabled_sources.remove(position);
                }
            }
        }
        self.persist_settings();
        let count = self.source_filter_item_count();
        if count == 0 {
            self.browse_selection = 0;
        } else {
            self.browse_selection = anchor.min(count - 1);
        }
        self.select_browse_entry(self.browse_selection);
    }

    /// Load the selected remote source's configurable preferences.
    pub fn open_source_prefs(&mut self) {
        let Some(SourceRef::Remote(index)) = self.selected_source_ref() else {
            self.remote.status = Some("Select a remote source to configure.".to_owned());
            self.remote.error = false;
            return;
        };
        let Some(source) = self.remote.sources.get(index).cloned() else {
            return;
        };
        self.load_source_prefs(source);
    }

    pub fn reload_source_prefs(&mut self) {
        let Some(state) = self.source_prefs.as_ref() else {
            return;
        };
        let source = state.source.clone();
        self.load_source_prefs(source);
    }

    fn load_source_prefs(&mut self, source: RemoteSource) {
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        match client.source_preferences(&source.id) {
            Ok(preferences) => {
                if !preferences.configurable {
                    self.remote.status = Some(format!(
                        "{} has no source preferences.",
                        source.display_name()
                    ));
                    self.remote.error = false;
                    return;
                }
                if let Some(error) = &preferences.error {
                    self.set_remote_error(format!(
                        "{} preferences: {error}",
                        source.display_name()
                    ));
                    return;
                }
                let mut list_state = ListState::default();
                list_state.select((!preferences.prefs.is_empty()).then_some(0));
                self.source_prefs = Some(SourcePrefsState {
                    source,
                    preferences,
                    list_state,
                });
                self.browse_view = BrowseView::SourcePrefs;
                self.browse_selection = 0;
                self.remote.status = None;
                self.remote.error = false;
                self.select_browse_entry(0);
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    fn selected_source_preference(&self) -> Option<&SourcePreference> {
        let state = self.source_prefs.as_ref()?;
        let index = state.list_state.selected().unwrap_or(self.browse_selection);
        state.preferences.prefs.get(index)
    }

    pub fn activate_source_preference(&mut self) {
        let Some(preference) = self.selected_source_preference().cloned() else {
            return;
        };
        let Some(key) = preference.key.clone() else {
            self.remote.status = Some("This preference has no key.".to_owned());
            self.remote.error = false;
            return;
        };
        match preference.kind {
            SourcePreferenceKind::Switch => {
                self.save_source_preference(
                    key,
                    SourcePreferenceValue::Bool(!preference.switch_value()),
                );
            }
            SourcePreferenceKind::Text => {
                self.source_pref_edit = Some(key);
                self.open_text_input(
                    TextAction::SourcePreference,
                    &preference.title,
                    preference.value.as_deref().unwrap_or(""),
                );
            }
            SourcePreferenceKind::List => {
                if preference.values.is_empty() {
                    self.remote.status = Some("This list preference has no values.".to_owned());
                    self.remote.error = false;
                    return;
                }
                let selected = preference
                    .values
                    .iter()
                    .position(|value| Some(value) == preference.value.as_ref())
                    .unwrap_or(0);
                self.source_pref_picker = Some(SourcePrefPicker {
                    key,
                    title: preference.title,
                    values: preference.values,
                    labels: preference.labels,
                    selected,
                });
            }
            SourcePreferenceKind::Multi => {
                self.remote.status =
                    Some("Multi-select source preferences need a newer host build.".to_owned());
                self.remote.error = false;
            }
            SourcePreferenceKind::Unknown => {
                self.remote.status = Some("Unsupported source preference type.".to_owned());
                self.remote.error = false;
            }
        }
    }

    fn save_source_preference(&mut self, key: String, value: SourcePreferenceValue) {
        let Some(state) = self.source_prefs.as_ref() else {
            return;
        };
        let source_id = state.source.id.clone();
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let mut values = BTreeMap::new();
        values.insert(key.clone(), value.clone());
        match client.set_source_preferences(&source_id, &values) {
            Ok(()) => {
                if let Some(state) = self.source_prefs.as_mut() {
                    if let Some(preference) = state
                        .preferences
                        .prefs
                        .iter_mut()
                        .find(|preference| preference.key.as_deref() == Some(key.as_str()))
                    {
                        preference.value = Some(match value {
                            SourcePreferenceValue::Bool(value) => value.to_string(),
                            SourcePreferenceValue::Text(value) => value,
                        });
                    }
                }
                self.remote.status = Some(format!("Saved {key}"));
                self.remote.error = false;
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    pub fn source_pref_picker_previous(&mut self) {
        if let Some(picker) = self.source_pref_picker.as_mut() {
            picker.selected = picker.selected.saturating_sub(1);
        }
    }

    pub fn source_pref_picker_next(&mut self) {
        if let Some(picker) = self.source_pref_picker.as_mut() {
            if !picker.values.is_empty() {
                picker.selected = (picker.selected + 1).min(picker.values.len() - 1);
            }
        }
    }

    pub fn source_pref_picker_confirm(&mut self) {
        let Some(picker) = self.source_pref_picker.take() else {
            return;
        };
        let Some(value) = picker.values.get(picker.selected).cloned() else {
            return;
        };
        self.save_source_preference(picker.key, SourcePreferenceValue::Text(value));
    }

    pub fn source_pref_picker_cancel(&mut self) {
        self.source_pref_picker = None;
    }

    /// Fetch the selected source's filter tree. Failures leave the list empty so an old
    /// host or a broken extension cannot block browsing.
    pub fn load_remote_filters(&mut self) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        match client.filters(&source.id) {
            Ok(filters) => {
                self.remote.default_filters = filters.clone();
                self.remote.filters = filters;
                self.remote.filters_loaded = true;
            }
            Err(_) => {
                self.remote.default_filters.clear();
                self.remote.filters.clear();
                self.remote.filters_loaded = false;
            }
        }
    }

    pub fn open_filters(&mut self) {
        if self.remote.source.is_none() {
            return;
        }
        if !self.remote.filters_loaded {
            self.load_remote_filters();
        }
        if self.remote.filters.is_empty() {
            self.remote.status = Some("This source has no filters.".to_owned());
            self.remote.error = false;
            return;
        }
        self.browse_view = BrowseView::Filters;
        self.browse_selection = 0;
        self.remote.filter_list_state = ListState::default();
        self.select_browse_entry(0);
    }

    pub fn reset_filters(&mut self) {
        self.remote.filters = self.remote.default_filters.clone();
        self.clamp_browse_selection();
    }

    pub fn apply_filters(&mut self) {
        self.browse_view = BrowseView::RemoteResults;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.remote.page = 1;
        self.remote.listing = RemoteListing::Search;
        self.load_remote_results(RemoteListing::Search);
        self.select_browse_entry(0);
    }

    pub fn filter_rows(&self) -> Vec<FilterRow> {
        let mut rows = Vec::new();
        let mut path = Vec::new();
        collect_filter_rows(&self.remote.filters, &mut path, 0, &mut rows);
        rows.push(FilterRow::Reset);
        rows.push(FilterRow::Apply);
        rows
    }

    pub fn filter_item_count(&self) -> usize {
        self.filter_rows()
            .iter()
            .filter(|row| match row {
                FilterRow::Filter { path, .. } => self
                    .filter_at_path(path)
                    .is_some_and(SourceFilter::is_selectable),
                FilterRow::Reset | FilterRow::Apply => true,
            })
            .count()
    }

    pub(crate) fn filter_at_path(&self, path: &[usize]) -> Option<&SourceFilter> {
        filter_at_path(&self.remote.filters, path)
    }

    fn filter_at_path_mut(&mut self, path: &[usize]) -> Option<&mut SourceFilter> {
        filter_at_path_mut(&mut self.remote.filters, path)
    }

    pub fn selected_filter_row(&self) -> Option<usize> {
        let mut selection = 0;
        for (row, item) in self.filter_rows().iter().enumerate() {
            let selectable = match item {
                FilterRow::Filter { path, .. } => self
                    .filter_at_path(path)
                    .is_some_and(SourceFilter::is_selectable),
                FilterRow::Reset | FilterRow::Apply => true,
            };
            if !selectable {
                continue;
            }
            if selection == self.browse_selection {
                return Some(row);
            }
            selection += 1;
        }
        None
    }

    pub fn select_filter_row(&mut self, row: usize) {
        let offset = self.remote.filter_list_state.offset();
        if let Some(index) = self.filter_index_for_row(offset + row) {
            self.select_browse_index(index);
        }
    }

    fn filter_index_for_row(&self, row: usize) -> Option<usize> {
        let mut selection = 0;
        for (index, item) in self.filter_rows().iter().enumerate() {
            let selectable = match item {
                FilterRow::Filter { path, .. } => self
                    .filter_at_path(path)
                    .is_some_and(SourceFilter::is_selectable),
                FilterRow::Reset | FilterRow::Apply => true,
            };
            if !selectable {
                continue;
            }
            if index == row {
                return Some(selection);
            }
            selection += 1;
        }
        None
    }

    pub fn activate_filter_selection(&mut self) {
        let Some(row) = self
            .selected_filter_row()
            .and_then(|row| self.filter_rows().get(row).cloned())
        else {
            return;
        };
        match row {
            FilterRow::Reset => self.reset_filters(),
            FilterRow::Apply => self.apply_filters(),
            FilterRow::Filter { path, .. } => {
                let Some(filter) = self.filter_at_path(&path).cloned() else {
                    return;
                };
                match filter {
                    SourceFilter::Checkbox { .. } | SourceFilter::Tristate { .. } => {
                        if let Some(filter) = self.filter_at_path_mut(&path) {
                            filter.toggle();
                        }
                    }
                    SourceFilter::Text { name, text, .. } => {
                        self.filter_edit = Some(path);
                        self.open_text_input(TextAction::FilterText, &name, &text);
                    }
                    SourceFilter::Select {
                        name,
                        selected,
                        values,
                        ..
                    } => {
                        self.filter_picker = Some(FilterPicker {
                            path,
                            title: name,
                            values,
                            selected: selected.max(0) as usize,
                        });
                    }
                    SourceFilter::Sort {
                        name,
                        values,
                        index,
                        ..
                    } => {
                        self.filter_picker = Some(FilterPicker {
                            path,
                            title: name,
                            values,
                            selected: index.unwrap_or(0).max(0) as usize,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    /// Apply left/right changes to select, tri-state, and sort filters.
    pub fn adjust_filter_selection(&mut self, delta: i32) {
        let Some(FilterRow::Filter { path, .. }) = self
            .selected_filter_row()
            .and_then(|row| self.filter_rows().get(row).cloned())
        else {
            return;
        };
        if let Some(filter) = self.filter_at_path_mut(&path) {
            filter.adjust(delta);
        }
    }

    pub fn filter_picker_previous(&mut self) {
        if let Some(picker) = self.filter_picker.as_mut() {
            picker.selected = picker.selected.saturating_sub(1);
        }
    }

    pub fn filter_picker_next(&mut self) {
        if let Some(picker) = self.filter_picker.as_mut() {
            if !picker.values.is_empty() {
                picker.selected = (picker.selected + 1).min(picker.values.len() - 1);
            }
        }
    }

    pub fn filter_picker_confirm(&mut self) {
        let Some(picker) = self.filter_picker.take() else {
            return;
        };
        if let Some(filter) = self.filter_at_path_mut(&picker.path) {
            match filter {
                SourceFilter::Select { selected, .. } => {
                    *selected = picker.selected as i32;
                }
                SourceFilter::Sort { index, .. } => {
                    *index = Some(picker.selected as i32);
                }
                _ => {}
            }
        }
    }

    pub fn filter_picker_cancel(&mut self) {
        self.filter_picker = None;
    }

    /// Remember the source the user just opened.
    pub fn record_source_opened(&mut self, source: SourceRef) {
        if self.config.incognito {
            return;
        }
        let key = match source {
            SourceRef::Local => "local".to_owned(),
            SourceRef::Remote(index) => {
                let Some(source) = self.remote.sources.get(index) else {
                    return;
                };
                source_pin_key(source)
            }
        };
        if self.config.last_used_source.as_deref() == Some(key.as_str()) {
            return;
        }
        self.config.last_used_source = Some(key);
        self.persist_settings();
    }

    pub fn is_source_pinned(&self, source: &RemoteSource) -> bool {
        self.config
            .pinned_sources
            .iter()
            .any(|pinned| pin_key_matches(pinned, source))
    }

    /// Pin or unpin the source selected in the Sources list.
    pub fn toggle_pin_for_selection(&mut self) {
        if self.browse_view != BrowseView::Sources {
            return;
        }
        let mut sources: Vec<RemoteSource> = Vec::new();
        if let Some(keys) = self.selected_keys(SelectionScope::Sources) {
            sources = self
                .remote
                .sources
                .iter()
                .filter(|source| keys.contains(&format!("source:{}", source.id)))
                .cloned()
                .collect();
        }
        if sources.is_empty() {
            if let Some(SourceRef::Remote(index)) = self.selected_source_ref() {
                if let Some(source) = self.remote.sources.get(index).cloned() {
                    sources.push(source);
                }
            }
        }
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
        self.remote.status = Some(if all_pinned {
            format!("Unpinned {} sources", sources.len())
        } else {
            format!("Pinned {} sources", sources.len())
        });
        self.persist_settings();
        let anchor = sources[0].id.clone();
        self.browse_selection = self
            .source_order()
            .iter()
            .position(|item| match item {
                SourceRef::Remote(index) => self
                    .remote
                    .sources
                    .get(*index)
                    .is_some_and(|source| source.id == anchor),
                SourceRef::Local => false,
            })
            .unwrap_or(0);
        self.select_browse_entry(self.browse_selection);
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
        let Some(client) = self.host_client.clone() else {
            return;
        };
        // Keep favorites without a stored thumbnail: the details call can supply one.
        let jobs: Vec<(String, String, Option<String>)> = self
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
                    favorite.thumbnail_url.clone(),
                ))
            })
            .collect();
        if jobs.is_empty() {
            return;
        }

        std::thread::spawn(move || {
            for (source_id, manga_url, thumbnail_url) in jobs {
                let Some(path) = remote_cover_path(&source_id, &manga_url) else {
                    continue;
                };
                if path.exists() {
                    continue;
                }
                let url = thumbnail_url.or_else(|| {
                    client
                        .details(&source_id, &manga_url, None)
                        .ok()
                        .and_then(|manga| manga.thumbnail_url)
                });
                let Some(url) = url else {
                    continue;
                };
                let Ok(bytes) = fetch_bytes(&url) else {
                    continue;
                };
                if image::load_from_memory(&bytes).is_err() {
                    continue;
                }
                write_cover_atomically(&path, &bytes);
            }
        });
    }

    pub fn is_favorite(&self, source_id: &str, url: &str) -> bool {
        self.remote
            .favorites
            .iter()
            .any(|favorite| favorite.source_id == source_id && favorite.url == url)
    }

    fn remote_result_visible(&self, manga: &RemoteManga) -> bool {
        !self.config.browse_hide_saved || !self.is_favorite(&manga.source_id, &manga.url)
    }

    pub fn visible_remote_results(&self) -> Vec<RemoteManga> {
        self.remote
            .results
            .iter()
            .filter(|manga| self.remote_result_visible(manga))
            .cloned()
            .collect()
    }

    fn visible_remote_count(&self) -> usize {
        self.remote
            .results
            .iter()
            .filter(|manga| self.remote_result_visible(manga))
            .count()
    }

    fn visible_remote_result(&self, selection: usize) -> Option<&RemoteManga> {
        self.remote
            .results
            .iter()
            .filter(|manga| self.remote_result_visible(manga))
            .nth(selection)
    }

    pub fn toggle_hide_saved_results(&mut self) {
        self.config.browse_hide_saved = !self.config.browse_hide_saved;
        self.persist_settings();
        self.browse_grid_offset = 0;
        self.browse_selection = 0;
        self.select_browse_entry(0);
        self.remote.status = Some(if self.config.browse_hide_saved {
            "Hiding saved manga from results".to_owned()
        } else {
            "Showing saved manga in results".to_owned()
        });
        self.remote.error = false;
    }

    fn selected_browse_mangas(&self) -> Vec<RemoteManga> {
        if let Some(keys) = self
            .selected_keys(SelectionScope::RemoteResults)
            .or_else(|| self.selected_keys(SelectionScope::GlobalResults))
        {
            let mut mangas: Vec<RemoteManga> = self
                .remote
                .results
                .iter()
                .filter(|manga| {
                    keys.contains(&format!("remote:{}\n{}", manga.source_id, manga.url))
                })
                .cloned()
                .collect();
            for group in &self.remote.global_groups {
                for manga in &group.mangas {
                    if keys.contains(&format!("remote:{}\n{}", group.source.id, manga.url)) {
                        mangas.push(manga.clone());
                    }
                }
            }
            if !mangas.is_empty() {
                return mangas;
            }
        }
        match self.browse_view {
            BrowseView::RemoteResults => self
                .remote
                .results
                .get(self.browse_selection)
                .cloned()
                .into_iter()
                .collect(),
            BrowseView::GlobalResults => self
                .remote
                .global_groups
                .get(self.remote.global_group)
                .and_then(|group| group.mangas.get(self.remote.global_item))
                .cloned()
                .into_iter()
                .collect(),
            BrowseView::RemoteDetails => self.remote.manga.clone().into_iter().collect(),
            _ => Vec::new(),
        }
    }

    pub fn toggle_selected_favorites(&mut self) {
        let mangas = self.selected_browse_mangas();
        if mangas.is_empty() {
            return;
        }
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let all_saved = mangas
            .iter()
            .all(|manga| self.is_favorite(&manga.source_id, &manga.url));
        let mut changed = 0usize;
        let mut failed = 0usize;
        for manga in &mangas {
            let result = if all_saved {
                client.remove_favorite(&manga.source_id, &manga.url)
            } else if self.is_favorite(&manga.source_id, &manga.url) {
                Ok(())
            } else {
                client.add_favorite(manga)
            };
            match result {
                Ok(()) => {
                    changed += 1;
                    if !all_saved {
                        self.removed_stories
                            .remove(&remote_story_key(&manga.source_id, &manga.url));
                    }
                }
                Err(error) => {
                    failed += 1;
                    self.set_remote_error(error);
                }
            }
        }
        self.load_favorites();
        self.clamp_browse_selection();
        self.remote.status = Some(if all_saved {
            format!("Removed {changed} manga from the library")
        } else {
            format!("Added {changed} manga to the library")
        });
        self.remote.error = failed > 0;
        self.multi_select = None;
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
                    self.clamp_browse_selection();
                }
                Err(error) => self.set_remote_error(error),
            }
        } else {
            match client.add_favorite(&manga) {
                Ok(()) => {
                    self.removed_stories
                        .remove(&remote_story_key(&manga.source_id, &manga.url));
                    // Cache the cover now: the proxy URL is only valid while the host
                    // runs, and the library should keep showing it after a restart.
                    self.ensure_remote_cover(
                        &manga.source_id,
                        &manga.url,
                        manga.thumbnail_url.as_deref(),
                    );
                    self.remote.status = Some(format!("Added {} to library", manga.title));
                    self.remote.error = false;
                    self.load_favorites();
                    self.clamp_browse_selection();
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
                self.source_enabled(source)
                    && self
                        .config
                        .pinned_sources
                        .iter()
                        .any(|key| pin_key_matches(key, source))
            })
            .cloned()
            .collect()
    }

    /// Sources included in global search.
    pub fn global_search_sources(&self) -> Vec<RemoteSource> {
        match self.config.global_search_scope {
            GlobalSearchScope::Pinned => self.pinned_sources(),
            GlobalSearchScope::All => self
                .remote
                .sources
                .iter()
                .filter(|source| self.source_enabled(source))
                .cloned()
                .collect(),
        }
    }

    /// Toggle between pinned-only and all-source global search.
    pub fn toggle_global_search_scope(&mut self) {
        self.config.global_search_scope = match self.config.global_search_scope {
            GlobalSearchScope::Pinned => GlobalSearchScope::All,
            GlobalSearchScope::All => GlobalSearchScope::Pinned,
        };
        self.persist_settings();
        let label = match self.config.global_search_scope {
            GlobalSearchScope::Pinned => "pinned",
            GlobalSearchScope::All => "all enabled",
        };
        self.remote.status = Some(format!("Global search: {label} sources"));
        self.remote.error = false;
        if !self.remote.global_query.is_empty() && self.host_client.is_some() {
            let query = self.remote.global_query.clone();
            self.global_search(&query);
        }
    }

    /// Search every selected source in the background and stream results by source.
    pub fn global_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let sources = self.global_search_sources();
        if sources.is_empty() {
            self.remote.status = Some(match self.config.global_search_scope {
                GlobalSearchScope::Pinned => {
                    "Pin a source first (P) or switch to all sources with A.".to_owned()
                }
                GlobalSearchScope::All => "No enabled sources to search.".to_owned(),
            });
            self.remote.error = false;
            return;
        }

        let generation = self.remote.global_generation.wrapping_add(1);
        self.remote.global_generation = generation;
        self.remote.global_query = query.to_owned();
        self.remote.global_groups.clear();
        self.remote.global_failures.clear();
        self.remote.global_total = sources.len();
        self.remote.global_finished = 0;
        self.remote.global_loading = true;
        self.remote.global_group = 0;
        self.remote.global_item = 0;
        self.cover_lookups.clear();
        self.failed_covers.clear();
        self.browse_view = BrowseView::GlobalResults;
        self.browse_selection = 0;

        if let Some(worker) = self.global_search_worker.as_ref() {
            worker.request(generation, query.to_owned(), sources, client);
        } else {
            // Tests and headless callers do not install the worker.
            for source in sources {
                let event = match client.search(&source.id, query, 1) {
                    Ok(page) => GlobalSearchEvent::Group {
                        generation,
                        source,
                        mangas: page.mangas,
                    },
                    Err(error) => GlobalSearchEvent::Error {
                        generation,
                        source,
                        message: error.to_string(),
                    },
                };
                self.apply_global_search_event(event);
            }
            self.apply_global_search_event(GlobalSearchEvent::Finished { generation });
        }
        self.update_global_search_status();
    }

    /// Apply one streamed global-search event. Returns whether the UI changed.
    fn apply_global_search_event(&mut self, event: GlobalSearchEvent) -> bool {
        match event {
            GlobalSearchEvent::Group {
                generation,
                source,
                mangas,
            } if generation == self.remote.global_generation => {
                if !mangas.is_empty() {
                    self.remote
                        .global_groups
                        .push(GlobalGroup { source, mangas });
                    self.remote.global_groups.sort_by(|left, right| {
                        natural_cmp(&left.source.display_name(), &right.source.display_name())
                    });
                }
                self.remote.global_finished += 1;
                self.update_global_search_status();
                true
            }
            GlobalSearchEvent::Error {
                generation,
                source,
                message,
            } if generation == self.remote.global_generation => {
                self.remote
                    .global_failures
                    .push(GlobalFailure { source, message });
                self.remote.global_failures.sort_by(|left, right| {
                    natural_cmp(&left.source.display_name(), &right.source.display_name())
                });
                self.remote.global_finished += 1;
                self.update_global_search_status();
                true
            }
            GlobalSearchEvent::Finished { generation }
                if generation == self.remote.global_generation =>
            {
                self.remote.global_loading = false;
                self.update_global_search_status();
                true
            }
            _ => false,
        }
    }

    fn update_global_search_status(&mut self) {
        let failures = self.remote.global_failures.len();
        self.remote.status = if self.remote.global_loading {
            let suffix = if failures > 0 {
                format!(" · {failures} failed")
            } else {
                String::new()
            };
            self.remote.error = false;
            Some(format!(
                "Searching · {}/{} sources{suffix}",
                self.remote.global_finished, self.remote.global_total
            ))
        } else if self.remote.global_groups.is_empty() {
            if let Some(failure) = self.remote.global_failures.first() {
                self.remote.error = true;
                Some(format!("No results · {}", failure.message))
            } else {
                self.remote.error = false;
                Some("No results.".to_owned())
            }
        } else if failures > 0 {
            self.remote.error = false;
            Some(format!("{failures} sources failed"))
        } else {
            self.remote.error = false;
            None
        };
    }

    /// Poll the background global search. Returns whether the UI changed.
    pub fn poll_global_search(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self
            .global_search_worker
            .as_ref()
            .and_then(GlobalSearchWorker::try_event)
        {
            if self.apply_global_search_event(event) {
                changed = true;
            }
        }
        if changed {
            if self.remote.global_groups.is_empty() {
                self.remote.global_group = 0;
                self.remote.global_item = 0;
            } else {
                self.remote.global_group = self
                    .remote
                    .global_group
                    .min(self.remote.global_groups.len() - 1);
                let items = self.remote.global_groups[self.remote.global_group]
                    .mangas
                    .len();
                self.remote.global_item = self.remote.global_item.min(items.saturating_sub(1));
            }
        }
        changed
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
        self.multi_select = None;
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
        if self.host_starting {
            self.remote.status = Some("The extension host is still starting…".to_owned());
            return false;
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
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        if !source.supports_latest {
            self.remote.status = Some(format!("{} has no latest listing", source.display_name()));
            self.remote.error = false;
            return;
        }
        self.remote.query.clear();
        self.load_remote_results(RemoteListing::Latest);
        self.browse_view = BrowseView::RemoteResults;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    pub fn search_remote(&mut self, query: &str) {
        self.remote.query = query.trim().to_owned();
        self.load_remote_results(RemoteListing::Search);
        self.browse_view = BrowseView::RemoteResults;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    fn fetch_remote_page(
        &self,
        client: &HostClient,
        source: &RemoteSource,
        page: i32,
    ) -> Result<RemoteMangaPage, tsuiyomi_core::CoreError> {
        match self.remote.listing {
            RemoteListing::Popular => client.popular(&source.id, page),
            RemoteListing::Latest => client.latest(&source.id, page),
            RemoteListing::Search => {
                if self.remote.filters.is_empty() {
                    client.search(&source.id, &self.remote.query, page)
                } else {
                    client.search_filtered(
                        &source.id,
                        &self.remote.query,
                        page,
                        &self.remote.filters,
                    )
                }
            }
        }
    }

    fn load_remote_results(&mut self, listing: RemoteListing) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        self.remote.listing = listing;
        self.remote.page = 1;
        match self.fetch_remote_page(&client, &source, 1) {
            Ok(result) => {
                self.remote.results = result.mangas;
                self.remote.has_next_page = result.has_next_page;
                self.remote.status = None;
                self.remote.error = false;
                self.cover_lookups.clear();
                self.failed_covers.clear();
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Append the next result page without moving the current selection.
    pub fn load_next_remote_page(&mut self) {
        if !self.remote.has_next_page {
            self.remote.status = Some("End of results.".to_owned());
            self.remote.error = false;
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        let next_page = self.remote.page.saturating_add(1).max(2);
        match self.fetch_remote_page(&client, &source, next_page) {
            Ok(result) => {
                self.append_remote_results(result.mangas);
                self.remote.page = next_page;
                self.remote.has_next_page = result.has_next_page;
                self.remote.status = None;
                self.remote.error = false;
                self.refresh_download_marks();
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    fn append_remote_results(&mut self, mangas: Vec<RemoteManga>) {
        let mut known: HashSet<String> = self
            .remote
            .results
            .iter()
            .map(|manga| manga.url.clone())
            .collect();
        for manga in mangas {
            if known.insert(manga.url.clone()) {
                self.remote.results.push(manga);
            }
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
        if let Ok(mut details) = client.details(&manga.source_id, &manga.url, manga.memo.as_deref())
        {
            if details.thumbnail_url.is_none() {
                // Many sources return a cover in search results but omit it from
                // details. Never erase a thumbnail that already worked.
                details.thumbnail_url = manga.thumbnail_url.clone();
            }
            self.remote.manga = Some(RemoteManga {
                // Keep the identity we already trust even if the source rewrites the URL.
                source_id: manga.source_id,
                url: manga.url,
                ..details
            });
            self.update_result_thumbnail();
        }
    }

    /// Copy details gained from a manga page back into the result grids, so returning
    /// to a list shows the cover instead of the placeholder.
    fn update_result_thumbnail(&mut self) {
        let Some(manga) = self.remote.manga.clone() else {
            return;
        };
        for result in &mut self.remote.results {
            if result.source_id == manga.source_id && result.url == manga.url {
                Self::merge_result_details(result, &manga);
            }
        }
        for group in &mut self.remote.global_groups {
            if group.source.id != manga.source_id {
                continue;
            }
            for result in &mut group.mangas {
                if result.url == manga.url {
                    Self::merge_result_details(result, &manga);
                }
            }
        }
    }

    fn merge_result_details(result: &mut RemoteManga, manga: &RemoteManga) {
        if manga.thumbnail_url.is_some() {
            result.thumbnail_url = manga.thumbnail_url.clone();
        }
        if manga.author.is_some() {
            result.author = manga.author.clone();
        }
        if manga.artist.is_some() {
            result.artist = manga.artist.clone();
        }
        if manga.description.is_some() {
            result.description = manga.description.clone();
        }
        if !manga.genre.is_empty() {
            result.genre = manga.genre.clone();
        }
        if manga.memo.is_some() {
            result.memo = manga.memo.clone();
        }
        result.status = manga.status.clone();
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
                if let Some(manga) = self.remote.manga.clone() {
                    self.cache_story(manga, self.remote.chapters.clone());
                }
                self.refresh_download_marks();
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
        if self.downloaded_chapter_dir(&manga, &chapter).is_none()
            && self.host_client.is_none()
            && !self.ensure_host()
        {
            return;
        }
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
            ReaderOrigin::History => 0,
        }
    }

    pub fn reader_chapter_count(&self) -> usize {
        match self.reader_origin {
            ReaderOrigin::Remote => self.remote.chapters.len(),
            ReaderOrigin::BrowseLocal => self.browse_local_chapter_count(),
            ReaderOrigin::Library => self.chapter_count(),
            ReaderOrigin::LibraryRemote => self.library_remote_chapter_count(),
            ReaderOrigin::History => 1,
        }
    }

    /// Ensure a remote manga's cover is loaded, caching the bytes on disk.
    ///
    /// Used for both Library and Browse. The host's cover URL embeds a port that only
    /// lives as long as the host process, so the image itself is written to the cover
    /// cache the first time it is seen and reused forever afterwards.
    pub fn ensure_remote_cover(
        &mut self,
        source_id: &str,
        manga_url: &str,
        thumbnail_url: Option<&str>,
    ) -> Option<PathBuf> {
        let path = remote_cover_path(source_id, manga_url)?;
        let key = path.to_string_lossy().into_owned();
        if !self.tile_covers.contains_key(&path) && path.exists() {
            self.ensure_tile_cover(&path);
            if !self.tile_covers.contains_key(&path) {
                // Corrupt or unreadable cache entry; let the source replace it.
                let _ = std::fs::remove_file(&path);
            }
        }
        if !self.tile_covers.contains_key(&path)
            && migrate_legacy_cover(&path, source_id, manga_url)
        {
            self.ensure_tile_cover(&path);
        }
        if self.tile_covers.contains_key(&path) {
            self.failed_covers.remove(&key);
            return Some(path);
        }
        if self.cover_failed(&key) {
            return None;
        }

        let url = thumbnail_url?;
        let bytes = match fetch_bytes(url) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.mark_cover_failed(key);
                return None;
            }
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            self.mark_cover_failed(key);
            return None;
        };
        write_cover_atomically(&path, &bytes);
        self.insert_tile_cover(path.clone(), image);
        if self.tile_covers.contains_key(&path) {
            Some(path)
        } else {
            self.mark_cover_failed(key);
            None
        }
    }

    /// Explicitly refetch missing covers for saved source manga.
    pub fn refresh_library_covers(&mut self) {
        self.cover_lookups.clear();
        self.failed_covers.clear();
        self.cover_recovery_at = None;
        if self.config.remote_library.is_empty() && self.remote.favorites.is_empty() {
            self.status = Some("No saved source manga.".to_owned());
            return;
        }
        if self.host_client.is_none() {
            self.status = Some("Starting the host to refresh covers…".to_owned());
            self.begin_host_start(true);
            return;
        }
        self.status = Some("Refreshing library covers…".to_owned());
        self.load_favorites();
    }

    /// Restore missing library covers once per session by starting the host in the
    /// background. Covers live in the data directory, but old cache cleanups or a
    /// corrupt file can still leave gaps.
    pub fn maybe_recover_covers(&mut self) {
        if self.config.remote_library.is_empty() {
            return;
        }
        let now = Instant::now();
        if self
            .cover_recovery_at
            .is_some_and(|at| now.duration_since(at) < Duration::from_secs(15))
        {
            return;
        }
        let missing = self.config.remote_library.iter().any(|favorite| {
            let current = remote_cover_path(&favorite.source_id, &favorite.url)
                .is_some_and(|path| path.exists());
            let legacy = legacy_remote_cover_path(&favorite.source_id, &favorite.url)
                .is_some_and(|path| path.exists());
            !current && !legacy
        });
        if !missing {
            return;
        }
        self.cover_recovery_at = Some(now);
        if self.host_client.is_some() {
            if self.remote.favorites_loaded {
                self.cache_favorite_covers();
            } else {
                self.load_favorites();
            }
        } else if self.host_autostart && !self.host_starting {
            self.begin_host_start(true);
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
        self.source_pref_edit = None;
        self.filter_edit = None;
    }

    pub fn input_submit(&mut self) {
        let Some(input) = self.text_input.take() else {
            return;
        };
        match input.action {
            TextAction::SearchRemote => self.search_remote(&input.value),
            TextAction::SearchLocal => self.search_local(&input.value),
            TextAction::GlobalSearch => self.global_search(&input.value),
            TextAction::SourcePreference => {
                if let Some(key) = self.source_pref_edit.take() {
                    self.save_source_preference(key, SourcePreferenceValue::Text(input.value));
                }
            }
            TextAction::FilterText => {
                if let Some(path) = self.filter_edit.take() {
                    if let Some(SourceFilter::Text { text, .. }) = self.filter_at_path_mut(&path) {
                        *text = input.value;
                    }
                }
            }
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

        let mut entries = Vec::new();
        for repo in &repos {
            match client.repo_extensions(repo) {
                Ok(available) => entries.extend(available),
                Err(error) => self.set_remote_error(error),
            }
        }
        self.remote.extensions = merge_extensions(installed, entries);
        self.remote.repos = repos;
        if self.remote.status.is_none() {
            self.remote.error = false;
        }
    }

    fn selected_extensions(&self) -> Vec<ExtensionInfo> {
        if let Some(keys) = self.selected_keys(SelectionScope::Extensions) {
            let selected: Vec<_> = self
                .remote
                .extensions
                .iter()
                .filter(|extension| keys.contains(&format!("ext:{}", extension.pkg)))
                .cloned()
                .collect();
            if !selected.is_empty() {
                return selected;
            }
        }
        self.selected_extension().cloned().into_iter().collect()
    }

    pub fn install_selected_extension(&mut self) {
        let extensions: Vec<ExtensionInfo> = self
            .selected_extensions()
            .into_iter()
            .filter(|extension| !extension.installed)
            .collect();
        if extensions.is_empty() {
            self.remote.status = Some("Selected extensions are already installed".to_owned());
            self.remote.error = false;
            return;
        }
        self.install_extensions(extensions, "Installed");
    }

    fn install_extensions(&mut self, extensions: Vec<ExtensionInfo>, verb: &str) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let mut installed = 0usize;
        let mut failed = 0usize;
        let mut swapped = false;
        for extension in &extensions {
            if extension.repo_url.is_empty() || extension.apk.is_empty() {
                failed += 1;
                continue;
            }
            match client.install_extension(&extension.repo_url, &extension.apk) {
                Ok(_) => {
                    installed += 1;
                    if self.install_desktop_jar(&extension.apk) {
                        swapped = true;
                    }
                }
                Err(_) => failed += 1,
            }
        }
        if swapped {
            self.restart_bundled_host();
        }
        self.load_extensions();
        self.load_remote_sources();
        let sources: usize = extensions
            .iter()
            .map(|extension| self.loaded_source_count(&extension.pkg))
            .sum();
        self.remote.status = Some(if failed == 0 {
            format!("{verb} {installed} extensions ({sources} sources)")
        } else {
            format!("{verb} {installed} extensions · {failed} failed")
        });
        self.remote.error = failed > 0 || installed == 0;
        self.multi_select = None;
    }

    pub fn uninstall_selected_extension(&mut self) {
        let extensions: Vec<ExtensionInfo> = self
            .selected_extensions()
            .into_iter()
            .filter(|extension| extension.installed)
            .collect();
        if extensions.is_empty() {
            self.remote.status = Some("Selected extensions are not installed".to_owned());
            self.remote.error = false;
            return;
        }
        let items: Vec<_> = extensions
            .iter()
            .map(|extension| (extension.pkg.clone(), extension.name.clone()))
            .collect();
        let count = items.len();
        self.uninstall_confirm = Some(items);
        self.remote.status = Some(if count == 1 {
            format!("Press y to uninstall {}, Esc cancels", extensions[0].name)
        } else {
            format!("Press y to uninstall {count} extensions, Esc cancels")
        });
        self.remote.error = false;
    }

    pub fn confirm_uninstall_extension(&mut self) {
        let Some(items) = self.uninstall_confirm.take() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let mut removed = 0usize;
        let mut failed = 0usize;
        for (pkg, _) in &items {
            match client.uninstall_extension(pkg) {
                Ok(()) => removed += 1,
                Err(_) => {
                    failed += 1;
                    break;
                }
            }
        }
        self.load_extensions();
        self.load_remote_sources();
        self.remote.status = Some(if failed == 0 {
            format!("Uninstalled {removed} extensions")
        } else {
            format!("Uninstalled {removed} extensions · {failed} failed")
        });
        self.remote.error = failed > 0;
        self.multi_select = None;
    }

    pub fn cancel_uninstall_extension(&mut self) {
        if self.uninstall_confirm.take().is_some() {
            self.remote.status = Some("Uninstall cancelled".to_owned());
            self.remote.error = false;
        }
    }

    pub fn extension_update_count(&self) -> usize {
        self.remote
            .extensions
            .iter()
            .filter(|extension| extension.update_available())
            .count()
    }

    pub fn update_selected_extension(&mut self) {
        let extensions: Vec<ExtensionInfo> = self
            .selected_extensions()
            .into_iter()
            .filter(|extension| {
                extension.update_available()
                    && !extension.repo_url.is_empty()
                    && !extension.apk.is_empty()
            })
            .collect();
        if extensions.is_empty() {
            self.remote.status = Some("Selected extensions are up to date".to_owned());
            self.remote.error = false;
            return;
        }
        self.install_extensions(extensions, "Updated");
    }

    pub fn update_all_extensions(&mut self) {
        let extensions: Vec<ExtensionInfo> = self
            .remote
            .extensions
            .iter()
            .filter(|extension| {
                extension.update_available()
                    && !extension.repo_url.is_empty()
                    && !extension.apk.is_empty()
            })
            .cloned()
            .collect();
        if extensions.is_empty() {
            self.remote.status = Some("No extension updates available".to_owned());
            self.remote.error = false;
            return;
        }
        self.install_extensions(extensions, "Updated");
    }

    pub fn toggle_selected_extension_pin(&mut self) {
        let extensions = self.selected_extensions();
        let sources: Vec<_> = extensions
            .iter()
            .flat_map(|extension| self.resolved_extension_sources(extension))
            .collect();
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
        self.remote.status = Some(if all_pinned {
            format!("Unpinned {} sources", sources.len())
        } else {
            format!("Pinned {} sources", sources.len())
        });
        self.persist_settings();
    }

    fn loaded_source_count(&self, pkg: &str) -> usize {
        self.remote
            .sources
            .iter()
            .filter(|source| source.pkg.as_deref() == Some(pkg))
            .count()
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
        let mut stores: Vec<String> = Vec::new();
        if let Some(keys) = self.selected_keys(SelectionScope::Stores) {
            stores = keys
                .iter()
                .filter_map(|key| key.strip_prefix("store:").map(str::to_owned))
                .collect();
        }
        if stores.is_empty() {
            if let Some(store) = self.selected_store().cloned() {
                stores.push(store);
            }
        }
        if stores.is_empty() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let repos: Vec<String> = self
            .remote
            .repos
            .iter()
            .filter(|repo| !stores.contains(repo))
            .cloned()
            .collect();
        match client.set_repos(&repos) {
            Ok(()) => {
                self.load_extensions();
                self.load_remote_sources();
                self.browse_selection = 0;
                self.select_browse_entry(0);
                self.remote.status = Some(format!(
                    "Removed {} extension {}",
                    stores.len(),
                    if stores.len() == 1 { "store" } else { "stores" }
                ));
                self.remote.error = false;
                self.multi_select = None;
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
        } else if self.browse_view == BrowseView::SourceFilters {
            self.source_filter_list_state
                .select((count > 0).then_some(index));
        } else if self.browse_view == BrowseView::SourcePrefs {
            if let Some(state) = self.source_prefs.as_mut() {
                state.list_state.select((count > 0).then_some(index));
            }
        } else if self.browse_view == BrowseView::Filters {
            self.remote
                .filter_list_state
                .select((count > 0).then_some(index));
        } else if self.browse_view == BrowseView::RemoteResults
            && self.config.source_display_mode == SourceDisplayMode::List
        {
            self.remote
                .remote_list_state
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

    pub(crate) fn set_story_worker(&mut self, worker: StoryWorker) {
        self.story_worker = Some(worker);
    }

    pub(crate) fn set_host_worker(&mut self, worker: HostWorker) {
        self.host_worker = Some(worker);
    }

    pub(crate) fn set_global_search_worker(&mut self, worker: GlobalSearchWorker) {
        self.global_search_worker = Some(worker);
    }

    pub(crate) fn set_cover_worker(&mut self, worker: CoverWorker) {
        self.cover_worker = Some(worker);
    }

    /// Ask the source's details page for a thumbnail the result/list did not include.
    pub fn request_series_thumbnail(&mut self, source_id: &str, url: &str, memo: Option<&str>) {
        let key = remote_story_key(source_id, url);
        if self.cover_lookups.contains(&key) {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(worker) = self.cover_worker.as_ref() else {
            return;
        };
        worker.request(CoverRequest {
            source_id: source_id.to_owned(),
            url: url.to_owned(),
            memo: memo.map(str::to_owned),
            client,
        });
        self.cover_lookups.insert(key);
    }

    /// Apply thumbnail lookups that finished in the background.
    pub fn poll_cover_lookups(&mut self) -> bool {
        let mut changed = false;
        while let Some(result) = self.cover_worker.as_ref().and_then(CoverWorker::try_result) {
            let Some(thumbnail) = result.thumbnail_url else {
                continue;
            };
            self.apply_thumbnail(&result.source_id, &result.url, &thumbnail);
            changed = true;
        }
        changed
    }

    fn apply_thumbnail(&mut self, source_id: &str, url: &str, thumbnail: &str) {
        for result in &mut self.remote.results {
            if result.source_id == source_id && result.url == url {
                result.thumbnail_url = Some(thumbnail.to_owned());
            }
        }
        for group in &mut self.remote.global_groups {
            if group.source.id != source_id {
                continue;
            }
            for result in &mut group.mangas {
                if result.url == url {
                    result.thumbnail_url = Some(thumbnail.to_owned());
                }
            }
        }
        if self
            .remote
            .manga
            .as_ref()
            .is_some_and(|manga| manga.source_id == source_id && manga.url == url)
        {
            if let Some(manga) = self.remote.manga.as_mut() {
                manga.thumbnail_url = Some(thumbnail.to_owned());
            }
        }
        if self
            .library_remote
            .manga
            .as_ref()
            .is_some_and(|manga| manga.source_id == source_id && manga.url == url)
        {
            if let Some(manga) = self.library_remote.manga.as_mut() {
                manga.thumbnail_url = Some(thumbnail.to_owned());
            }
        }
        for favorite in &mut self.remote.favorites {
            if favorite.source_id == source_id && favorite.url == url {
                favorite.thumbnail_url = Some(thumbnail.to_owned());
            }
        }
        for series in &mut self.library.series {
            if let Some(remote) = &mut series.remote {
                if remote.source_id == source_id && remote.url == url {
                    remote.thumbnail_url = Some(thumbnail.to_owned());
                }
            }
        }
    }

    /// Apply a finished background host start. Returns whether the UI changed.
    pub fn poll_host_start(&mut self) -> bool {
        let Some(result) = self.host_worker.as_ref().and_then(HostWorker::try_result) else {
            return false;
        };
        if result.epoch != self.host_start_epoch {
            return false;
        }
        self.host_starting = false;
        match result.result {
            Ok(process) => {
                self.remote.host = Some(process.base_url().to_owned());
                self.host_client = Some(HostClient::new(process.base_url()));
                self.host_process = Some(process);
                self.remote.status = None;
                self.remote.error = false;
                if self.pending_browse_sources {
                    self.pending_browse_sources = false;
                    self.load_remote_sources();
                }
            }
            Err(error) => {
                self.pending_browse_sources = false;
                self.set_remote_error(error);
            }
        }
        true
    }

    /// Connect to an external host immediately, or start the bundled one in the
    /// background so the UI can draw before the JVM is ready.
    fn begin_host_start(&mut self, then_load_sources: bool) {
        if self.host_client.is_some() {
            if then_load_sources {
                self.load_remote_sources();
            }
            return;
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
                if then_load_sources {
                    self.load_remote_sources();
                }
                return;
            }
        }
        if self.host_starting {
            self.pending_browse_sources |= then_load_sources;
            return;
        }
        let Some(worker) = self.host_worker.as_ref() else {
            if then_load_sources {
                self.load_remote_sources();
            } else {
                let _ = self.ensure_host();
            }
            return;
        };
        self.host_starting = true;
        self.pending_browse_sources = then_load_sources;
        self.remote.status = Some("Starting the extension host…".to_owned());
        self.remote.error = false;
        worker.request(HostRequest {
            host_jar: self.config.host_jar.clone(),
            epoch: self.host_start_epoch,
        });
    }

    /// Refresh a story's details and chapters in the background, when the host is
    /// already running. This must never block on starting the JVM.
    fn refresh_current_library_story(&mut self) {
        let Some(worker) = self.story_worker.as_ref() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(manga) = self.library_remote.manga.clone() else {
            return;
        };
        let key = remote_story_key(&manga.source_id, &manga.url);
        if self.story_refresh.as_deref() == Some(key.as_str())
            || self.story_refreshed.contains(&key)
        {
            return;
        }
        worker.request(StoryRequest {
            key: key.clone(),
            manga,
            client,
        });
        self.story_refresh = Some(key.clone());
        self.story_refreshed.insert(key);
    }

    /// Apply background story refreshes. Returns whether the UI changed.
    pub fn poll_story_refresh(&mut self) -> bool {
        let mut changed = false;
        while let Some(result) = self.story_worker.as_ref().and_then(StoryWorker::try_result) {
            if self.story_refresh.as_deref() == Some(result.key.as_str()) {
                self.story_refresh = None;
            }
            if result.chapters.is_empty() || self.removed_stories.contains(&result.key) {
                continue;
            }
            self.cache_story(result.manga.clone(), result.chapters.clone());

            let library_key = self
                .library_remote
                .manga
                .as_ref()
                .map(|manga| remote_story_key(&manga.source_id, &manga.url));
            if library_key.as_deref() == Some(result.key.as_str()) {
                let selected = self
                    .library_remote
                    .chapters
                    .get(self.library_remote.chapter_index)
                    .map(|chapter| chapter.url.clone());
                if self.library_remote.source_name.is_none() {
                    self.library_remote.source_name =
                        self.remote_source_display_name(&result.manga.source_id);
                }
                self.library_remote.manga = Some(result.manga.clone());
                self.library_remote.chapters = result.chapters.clone();
                self.library_remote.chapter_index = selected
                    .and_then(|url| {
                        self.library_remote
                            .chapters
                            .iter()
                            .position(|chapter| chapter.url == url)
                    })
                    .unwrap_or(0)
                    .min(self.library_remote.chapters.len().saturating_sub(1));
                self.library_remote
                    .list_state
                    .select(Some(self.library_remote.chapter_index));
                self.library_remote.downloaded = self
                    .library_remote
                    .manga
                    .as_ref()
                    .map(|manga| self.mark_downloaded(manga, &self.library_remote.chapters))
                    .unwrap_or_default();
                changed = true;
            }

            let browse_key = self
                .remote
                .manga
                .as_ref()
                .map(|manga| remote_story_key(&manga.source_id, &manga.url));
            if browse_key.as_deref() == Some(result.key.as_str()) {
                let selected = self
                    .remote
                    .chapters
                    .get(self.browse_selection)
                    .map(|chapter| chapter.url.clone());
                self.remote.manga = Some(result.manga);
                self.remote.chapters = result.chapters;
                self.browse_selection = selected
                    .and_then(|url| {
                        self.remote
                            .chapters
                            .iter()
                            .position(|chapter| chapter.url == url)
                    })
                    .unwrap_or(0)
                    .min(self.remote.chapters.len().saturating_sub(1));
                self.browse_list_state.select(Some(self.browse_selection));
                self.remote.downloaded = self
                    .remote
                    .manga
                    .as_ref()
                    .map(|manga| self.mark_downloaded(manga, &self.remote.chapters))
                    .unwrap_or_default();
                changed = true;
            }
        }
        changed
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

    /// Whether the scroll view still has a freshly built block to reveal.
    pub fn needs_scroll_redraw(&self) -> bool {
        self.reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .is_some_and(|scroll| scroll.deferred_transmits)
    }

    /// Whether the current screen has a remote chapter that can be downloaded.
    pub fn has_downloadable_chapter(&self) -> bool {
        self.active_remote_chapter().is_some()
    }

    /// Download the selected remote chapter to the Mihon-style downloads tree.
    pub fn download_selected_chapter(&mut self) {
        let Some((manga, chapters)) = self.selected_remote_chapters() else {
            self.status = Some("Open a remote chapter to download it.".to_owned());
            return;
        };
        if chapters.is_empty() {
            self.status = Some("No remote chapters selected.".to_owned());
            return;
        }
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
        let mut queued = 0usize;
        let mut already = 0usize;
        let mut failed = 0usize;
        for chapter in &chapters {
            if is_downloaded(
                &root,
                &source,
                &manga.title,
                &chapter.name,
                chapter.scanlator.as_deref(),
            ) {
                already += 1;
                continue;
            }
            let pages = match self.remote_chapter_pages(&manga, chapter) {
                Ok(pages) => pages,
                Err(_) => {
                    failed += 1;
                    continue;
                }
            };
            let Some(worker) = self.download_worker.as_ref() else {
                self.status = Some("The download worker is unavailable.".to_owned());
                return;
            };
            worker.request(DownloadRequest {
                label: format!("{} · {}", manga.title, chapter.name),
                root: root.clone(),
                source_name: source.clone(),
                series_title: manga.title.clone(),
                chapter_name: chapter.name.clone(),
                scanlator: chapter.scanlator.clone(),
                pages,
            });
            queued += 1;
        }

        if queued == 0 {
            self.status = Some(if failed > 0 {
                format!("Could not download {failed} chapters")
            } else if already > 0 {
                "Selected chapters are already downloaded".to_owned()
            } else {
                "Nothing to download".to_owned()
            });
            return;
        }
        let message = format!(
            "Downloading {queued} {}…",
            if queued == 1 { "chapter" } else { "chapters" }
        );
        self.downloading = Some(message.clone());
        self.status = Some(message.clone());
        if self.tab == 3 {
            self.remote.status = Some(message);
            self.remote.error = false;
        }
        self.multi_select = None;
    }

    /// The remote story and chapter list under the cursor.
    fn active_remote_story(&self) -> Option<(RemoteManga, Vec<RemoteChapter>, usize)> {
        if self.reader.is_some() {
            return match self.reader_origin {
                ReaderOrigin::Remote => Some((
                    self.remote.manga.clone()?,
                    self.remote.chapters.clone(),
                    self.remote.chapter_index,
                )),
                ReaderOrigin::LibraryRemote => Some((
                    self.library_remote.manga.clone()?,
                    self.library_remote.chapters.clone(),
                    self.library_remote.chapter_index,
                )),
                ReaderOrigin::Library | ReaderOrigin::BrowseLocal | ReaderOrigin::History => None,
            };
        }
        if self.tab == 3 && self.browse_view == BrowseView::RemoteDetails {
            return Some((
                self.remote.manga.clone()?,
                self.remote.chapters.clone(),
                self.browse_selection,
            ));
        }
        if self.tab == 0 && self.view == LibraryView::RemoteDetails {
            return Some((
                self.library_remote.manga.clone()?,
                self.library_remote.chapters.clone(),
                self.library_remote.chapter_index,
            ));
        }
        None
    }

    fn active_remote_chapter(&self) -> Option<(RemoteManga, RemoteChapter)> {
        let (manga, chapters, index) = self.active_remote_story()?;
        let chapter = chapters.get(index)?.clone();
        Some((manga, chapter))
    }

    fn selected_remote_chapters(&self) -> Option<(RemoteManga, Vec<RemoteChapter>)> {
        let (manga, chapters, index) = self.active_remote_story()?;
        if let Some(keys) = self.selected_keys(SelectionScope::RemoteChapters) {
            let selected: Vec<_> = chapters
                .iter()
                .filter(|chapter| {
                    keys.contains(&format!(
                        "rchap:{}\n{}\n{}",
                        manga.source_id, manga.url, chapter.url
                    ))
                })
                .cloned()
                .collect();
            if !selected.is_empty() {
                return Some((manga, selected));
            }
        }
        Some((manga, chapters.get(index).cloned().into_iter().collect()))
    }

    fn remote_source_name(&self, source_id: &str) -> String {
        self.remote_source_display_name(source_id)
            .unwrap_or_else(|| source_id.to_owned())
    }

    fn remote_source_display_name(&self, source_id: &str) -> Option<String> {
        self.remote
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map(RemoteSource::display_name)
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

    pub fn set_bookmarks(&mut self, bookmarks: BookmarkStore) {
        self.bookmarks = bookmarks;
    }

    pub fn set_history(&mut self, history: HistoryStore) {
        self.history = history;
    }

    /// Whether the current screen has a chapter that can be bookmarked.
    pub fn has_bookmarkable_chapter(&self) -> bool {
        self.active_chapter_key().is_some()
    }

    pub fn toggle_bookmark(&mut self) {
        let mut keys: Vec<PathBuf> = Vec::new();
        if self.reader.is_none() {
            if let Some(selected) = self.selected_bookmark_keys() {
                keys = selected;
            }
        }
        if keys.is_empty() {
            if let Some(key) = self.active_chapter_key() {
                keys.push(key);
            }
        }
        if keys.is_empty() {
            self.status = Some("Open a chapter to bookmark it.".to_owned());
            return;
        }

        let all_bookmarked = keys.iter().all(|key| self.bookmarks.is_bookmarked(key));
        let target = !all_bookmarked;
        for key in &keys {
            self.bookmarks.set(key, target);
        }
        self.status = Some(match self.bookmarks.save() {
            Ok(()) if target => format!(
                "Bookmarked {} {}",
                keys.len(),
                if keys.len() == 1 {
                    "chapter"
                } else {
                    "chapters"
                }
            ),
            Ok(()) => format!(
                "Removed bookmarks from {} {}",
                keys.len(),
                if keys.len() == 1 {
                    "chapter"
                } else {
                    "chapters"
                }
            ),
            Err(error) => format!("Could not save bookmark: {error}"),
        });
        if self.tab == 3 {
            self.remote.status = self.status.clone();
            self.remote.error = false;
        }
        self.multi_select = None;
    }

    fn selected_bookmark_keys(&self) -> Option<Vec<PathBuf>> {
        if let Some(keys) = self.selected_keys(SelectionScope::LocalChapters) {
            let chapters = if self.tab == 0 && self.view == LibraryView::Chapters {
                self.selected_series()
                    .map(|series| series.chapters.clone())
                    .unwrap_or_default()
            } else if self.tab == 3 && self.browse_view == BrowseView::Chapters {
                self.selected_browse_local_series()
                    .map(|series| series.chapters.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let selected: Vec<PathBuf> = chapters
                .iter()
                .filter(|chapter| keys.contains(&format!("chapter:{}", chapter.path.display())))
                .map(|chapter| chapter.path.clone())
                .collect();
            if !selected.is_empty() {
                return Some(selected);
            }
        }
        if let Some(keys) = self.selected_keys(SelectionScope::RemoteChapters) {
            let (manga, chapters, _) = self.active_remote_story()?;
            let selected: Vec<PathBuf> = chapters
                .iter()
                .filter(|chapter| {
                    keys.contains(&format!(
                        "rchap:{}\n{}\n{}",
                        manga.source_id, manga.url, chapter.url
                    ))
                })
                .map(|chapter| remote_progress_key(&manga.source_id, &manga.url, &chapter.url))
                .collect();
            if !selected.is_empty() {
                return Some(selected);
            }
        }
        None
    }

    /// Stable key for the chapter under the cursor in the reader or a story screen.    /// Stable key for the chapter under the cursor in the reader or a story screen.
    fn active_chapter_key(&self) -> Option<PathBuf> {
        if let Some(reader) = &self.reader {
            return Some(reader.chapter_path.clone());
        }

        if self.tab == 0 {
            match self.view {
                LibraryView::Chapters => {
                    return self.selected_chapter().map(|chapter| chapter.path.clone());
                }
                LibraryView::RemoteDetails => {
                    let manga = self.library_remote.manga.as_ref()?;
                    let chapter = self
                        .library_remote
                        .chapters
                        .get(self.library_remote.chapter_index)?;
                    return Some(remote_progress_key(
                        &manga.source_id,
                        &manga.url,
                        &chapter.url,
                    ));
                }
                LibraryView::Series => {}
            }
        }

        if self.tab == 3 {
            match self.browse_view {
                BrowseView::Chapters => {
                    let series = self.selected_browse_local_series()?;
                    let chapter = series.chapters.get(self.browse_local_chapter)?;
                    return Some(chapter.path.clone());
                }
                BrowseView::RemoteDetails => {
                    let manga = self.remote.manga.as_ref()?;
                    let chapter = self.remote.chapters.get(self.browse_selection)?;
                    return Some(remote_progress_key(
                        &manga.source_id,
                        &manga.url,
                        &chapter.url,
                    ));
                }
                _ => {}
            }
        }
        None
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
        self.multi_select = None;
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
        self.flush_progress();
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
            ReaderOrigin::History => {
                self.tab = 2;
                self.history_selection = self
                    .history_selection
                    .min(self.history.recent().len().saturating_sub(1));
                self.history_list_state.select(Some(self.history_selection));
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
        if self.reader.is_none() && self.tab == 2 {
            self.history_previous();
            return;
        }
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
        if self.reader.is_none() && self.tab == 2 {
            self.history_next();
            return;
        }
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
        self.apply_hidden_series();
        self.clamp_selection();
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

    /// Remove local series listed in config from the scanned library. Files stay.
    fn apply_hidden_series(&mut self) {
        let hidden = self.config.hidden_series.clone();
        let root = self.library.root.clone();
        self.library.series.retain(|series| {
            if series.remote.is_some() {
                return true;
            }
            let key = series
                .path
                .strip_prefix(&root)
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|_| series.path.clone());
            !hidden.iter().any(|entry| entry == &key)
        });
    }

    /// Open the delete dialog for the selected library series.
    pub fn open_library_delete_dialog(&mut self) {
        let mut items: Vec<LibraryDeleteItem> =
            if let Some(keys) = self.selected_keys(SelectionScope::LibrarySeries) {
                self.library
                    .series
                    .iter()
                    .filter(|series| keys.contains(&Self::series_selection_key(series)))
                    .map(|series| LibraryDeleteItem {
                        path: series.path.clone(),
                        title: series.title.clone(),
                        remote: series.remote.clone(),
                    })
                    .collect()
            } else {
                Vec::new()
            };
        if items.is_empty() {
            let Some(series) = self.selected_series() else {
                return;
            };
            items.push(LibraryDeleteItem {
                path: series.path.clone(),
                title: series.title.clone(),
                remote: series.remote.clone(),
            });
        }
        self.library_delete = Some(LibraryDeleteDialog {
            items,
            remove_from_library: true,
            delete_downloads: false,
            focus: 0,
        });
    }

    pub fn library_delete_next(&mut self) {
        if let Some(dialog) = self.library_delete.as_mut() {
            let options = if dialog.has_remote() { 2 } else { 1 };
            dialog.focus = (dialog.focus + 1) % options;
        }
    }

    pub fn library_delete_previous(&mut self) {
        if let Some(dialog) = self.library_delete.as_mut() {
            let options = if dialog.has_remote() { 2 } else { 1 };
            dialog.focus = (dialog.focus + options - 1) % options;
        }
    }

    pub fn library_delete_toggle(&mut self) {
        if let Some(dialog) = self.library_delete.as_mut() {
            if dialog.focus == 0 {
                dialog.remove_from_library = !dialog.remove_from_library;
            } else {
                dialog.delete_downloads = !dialog.delete_downloads;
            }
        }
    }

    pub fn cancel_library_delete(&mut self) {
        self.library_delete = None;
    }

    pub fn confirm_library_delete(&mut self) {
        let Some(dialog) = self.library_delete.clone() else {
            return;
        };
        if !dialog.remove_from_library && !dialog.delete_downloads {
            self.status = Some("Select at least one option.".to_owned());
            return;
        }

        let remotes: Vec<_> = dialog
            .items
            .iter()
            .filter_map(|item| {
                item.remote
                    .clone()
                    .map(|remote| (remote, item.title.clone()))
            })
            .collect();
        let mut removed = 0usize;
        if dialog.remove_from_library {
            if !remotes.is_empty() {
                let series: Vec<_> = remotes.iter().map(|(remote, _)| remote.clone()).collect();
                if !self.remove_remote_library_items(&series) {
                    return;
                }
                removed += remotes.len();
            }
            let mut hid_local = false;
            for item in &dialog.items {
                if item.remote.is_none() {
                    self.hide_local_path(&item.path);
                    removed += 1;
                    hid_local = true;
                }
            }
            if hid_local {
                self.persist_settings();
                self.apply_hidden_series();
            }
        }

        let mut downloads = 0usize;
        if dialog.delete_downloads {
            for (remote, title) in &remotes {
                match self.delete_series_downloads(remote, title) {
                    Ok(true) => downloads += 1,
                    Ok(false) => {}
                    Err(error) => {
                        self.status = Some(format!("Could not delete downloads: {error}"));
                    }
                }
            }
            self.refresh_download_marks();
        }

        if downloads > 0 {
            self.status = Some(format!("Deleted downloads for {downloads} series"));
        } else if removed > 0 {
            self.status = Some(format!(
                "Removed {removed} {} from the library",
                if removed == 1 { "item" } else { "items" }
            ));
        }
        self.multi_select = None;
        self.library_delete = None;
        self.clamp_selection();
        self.clamp_browse_selection();
    }

    fn hide_local_path(&mut self, path: &Path) {
        let key = path
            .strip_prefix(&self.library.root)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf());
        if !self.config.hidden_series.iter().any(|entry| entry == &key) {
            self.config.hidden_series.push(key);
        }
        self.library.series.retain(|series| series.path != path);
    }

    fn remove_remote_library_items(&mut self, items: &[RemoteSeries]) -> bool {
        if items.is_empty() {
            return true;
        }
        if self.host_client.is_none() && !self.ensure_host() {
            return false;
        }
        let Some(client) = self.host_client.clone() else {
            return false;
        };
        let mut removed = Vec::new();
        for remote in items {
            match client.remove_favorite(&remote.source_id, &remote.url) {
                Ok(()) => removed.push(remote.clone()),
                Err(error) => {
                    self.set_remote_error(error);
                    break;
                }
            }
        }
        for remote in &removed {
            self.remote.favorites.retain(|favorite| {
                !(favorite.source_id == remote.source_id && favorite.url == remote.url)
            });
            self.config.remote_library.retain(|favorite| {
                !(favorite.source_id == remote.source_id && favorite.url == remote.url)
            });
            let key = remote_story_key(&remote.source_id, &remote.url);
            self.remote_chapter_cache.remove(&key);
            self.story_refreshed.remove(&key);
            if self.story_refresh.as_deref() == Some(key.as_str()) {
                self.story_refresh = None;
            }
            if let Some(path) = remote_cover_path(&remote.source_id, &remote.url) {
                let _ = std::fs::remove_file(path);
            }
            if let Some(path) = story_cache_path(&remote.source_id, &remote.url) {
                let _ = std::fs::remove_file(path);
            }
            if self
                .library_remote
                .manga
                .as_ref()
                .is_some_and(|manga| manga.source_id == remote.source_id && manga.url == remote.url)
            {
                self.library_remote = LibraryRemoteState::default();
                if self.view == LibraryView::RemoteDetails {
                    self.view = LibraryView::Series;
                }
            }
            self.removed_stories.insert(key);
        }
        self.persist_settings();
        self.merge_remote_library();
        removed.len() == items.len()
    }

    fn delete_series_downloads(&self, remote: &RemoteSeries, title: &str) -> Result<bool, String> {
        let root = downloads_dir(&self.config)
            .ok_or_else(|| "cannot resolve the downloads directory".to_owned())?;
        let source = self.remote_source_name(&remote.source_id);
        let series_name = series_dir_name(title);
        let mut series_dir = root.join(source_dir_name(&source)).join(&series_name);
        if !series_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join(&series_name);
                    if candidate.is_dir() {
                        series_dir = candidate;
                        break;
                    }
                }
            }
        }
        if !series_dir.is_dir() {
            return Ok(false);
        }
        std::fs::remove_dir_all(&series_dir).map_err(|error| error.to_string())?;
        Ok(true)
    }

    /// Stable key for a series in multi-select.
    pub fn series_selection_key(series: &Series) -> String {
        match &series.remote {
            Some(remote) => format!("remote:{}\n{}", remote.source_id, remote.url),
            None => format!("local:{}", series.path.display()),
        }
    }

    fn remote_selection_key(source_id: &str, url: &str) -> String {
        format!("remote:{source_id}\n{url}")
    }

    fn local_chapter_selection_key(chapter: &Chapter) -> String {
        format!("chapter:{}", chapter.path.display())
    }

    fn remote_chapter_selection_key(source_id: &str, manga_url: &str, chapter_url: &str) -> String {
        format!("rchap:{source_id}\n{manga_url}\n{chapter_url}")
    }

    fn current_selection_target(&self) -> Option<(SelectionScope, String)> {
        if self.reader.is_some() {
            return None;
        }
        if self.tab == 0 {
            return match self.view {
                LibraryView::Series => {
                    let series = self.selected_series()?;
                    Some((
                        SelectionScope::LibrarySeries,
                        Self::series_selection_key(series),
                    ))
                }
                LibraryView::Chapters => {
                    let chapter = self.selected_chapter()?;
                    Some((
                        SelectionScope::LocalChapters,
                        Self::local_chapter_selection_key(chapter),
                    ))
                }
                LibraryView::RemoteDetails => {
                    let manga = self.library_remote.manga.as_ref()?;
                    let chapter = self
                        .library_remote
                        .chapters
                        .get(self.library_remote.chapter_index)?;
                    Some((
                        SelectionScope::RemoteChapters,
                        Self::remote_chapter_selection_key(
                            &manga.source_id,
                            &manga.url,
                            &chapter.url,
                        ),
                    ))
                }
            };
        }
        if self.tab == 2 {
            let recent = self.history.recent();
            let (key, _) = recent.get(self.history_selection)?;
            return Some((SelectionScope::History, format!("history:{key}")));
        }
        if self.tab != 3 {
            return None;
        }
        match self.browse_view {
            BrowseView::Local => {
                let series = self.selected_browse_series()?;
                Some((
                    SelectionScope::BrowseLocal,
                    Self::series_selection_key(series),
                ))
            }
            BrowseView::RemoteResults => {
                let manga = self.visible_remote_result(self.browse_selection)?;
                Some((
                    SelectionScope::RemoteResults,
                    Self::remote_selection_key(&manga.source_id, &manga.url),
                ))
            }
            BrowseView::GlobalResults => {
                let group = self.remote.global_groups.get(self.remote.global_group)?;
                let manga = group.mangas.get(self.remote.global_item)?;
                Some((
                    SelectionScope::GlobalResults,
                    Self::remote_selection_key(&group.source.id, &manga.url),
                ))
            }
            BrowseView::RemoteDetails => {
                let manga = self.remote.manga.as_ref()?;
                let chapter = self.remote.chapters.get(self.browse_selection)?;
                Some((
                    SelectionScope::RemoteChapters,
                    Self::remote_chapter_selection_key(&manga.source_id, &manga.url, &chapter.url),
                ))
            }
            BrowseView::Chapters => {
                let chapter = self
                    .selected_browse_local_series()?
                    .chapters
                    .get(self.browse_local_chapter)?;
                Some((
                    SelectionScope::LocalChapters,
                    Self::local_chapter_selection_key(chapter),
                ))
            }
            BrowseView::Sources => match self.selected_source_ref()? {
                SourceRef::Local => Some((SelectionScope::Sources, "source:local".to_owned())),
                SourceRef::Remote(index) => {
                    let source = self.remote.sources.get(index)?;
                    Some((SelectionScope::Sources, format!("source:{}", source.id)))
                }
            },
            BrowseView::Extensions => {
                let extension = self.selected_extension()?;
                Some((SelectionScope::Extensions, format!("ext:{}", extension.pkg)))
            }
            BrowseView::Stores => {
                let store = self.selected_store()?;
                Some((SelectionScope::Stores, format!("store:{store}")))
            }
            BrowseView::Migrate
            | BrowseView::SourceFilters
            | BrowseView::SourcePrefs
            | BrowseView::Filters => None,
        }
    }

    pub fn supports_multi_select(&self) -> bool {
        self.current_selection_target().is_some()
    }

    pub fn toggle_multi_select(&mut self) {
        let Some((scope, key)) = self.current_selection_target() else {
            return;
        };
        match self.multi_select.as_mut() {
            Some(selection) if selection.scope == scope => {
                if !selection.keys.remove(&key) {
                    selection.keys.insert(key);
                }
                if selection.keys.is_empty() {
                    self.multi_select = None;
                }
            }
            _ => {
                self.multi_select = Some(MultiSelect {
                    scope,
                    keys: std::iter::once(key).collect(),
                });
            }
        }
    }

    pub fn clear_multi_select(&mut self) {
        self.multi_select = None;
    }

    pub fn is_multi_selected(&self, scope: SelectionScope, key: &str) -> bool {
        self.multi_select
            .as_ref()
            .is_some_and(|selection| selection.scope == scope && selection.keys.contains(key))
    }

    pub fn selected_keys(&self, scope: SelectionScope) -> Option<&HashSet<String>> {
        self.multi_select
            .as_ref()
            .filter(|selection| selection.scope == scope)
            .map(|selection| &selection.keys)
    }

    pub fn history_count(&self) -> usize {
        self.history.recent().len()
    }

    fn clamp_history_selection(&mut self) {
        let count = self.history_count();
        self.history_selection = self.history_selection.min(count.saturating_sub(1));
        self.history_list_state
            .select((count > 0).then_some(self.history_selection));
    }

    pub fn history_next(&mut self) {
        if let Some(last) = self.history_count().checked_sub(1) {
            self.history_selection = (self.history_selection + 1).min(last);
            self.history_list_state.select(Some(self.history_selection));
        }
    }

    pub fn history_previous(&mut self) {
        self.history_selection = self.history_selection.saturating_sub(1);
        self.history_list_state.select(Some(self.history_selection));
    }

    pub fn delete_history_selection(&mut self) {
        let mut keys: Vec<String> = Vec::new();
        if let Some(selected) = self.selected_keys(SelectionScope::History) {
            keys = selected
                .iter()
                .filter_map(|key| key.strip_prefix("history:").map(str::to_owned))
                .collect();
        }
        if keys.is_empty() {
            if let Some((key, _)) = self.history.recent().get(self.history_selection) {
                keys.push((*key).clone());
            }
        }
        if keys.is_empty() {
            return;
        }
        let mut removed = 0usize;
        for key in keys {
            if self.history.remove(&key) {
                removed += 1;
            }
        }
        let _ = self.history.save();
        self.history_dirty = false;
        self.multi_select = None;
        self.clamp_history_selection();
        self.status = Some(format!(
            "Removed {removed} history {}",
            if removed == 1 { "entry" } else { "entries" }
        ));
    }

    pub fn open_history_entry(&mut self) {
        let Some((key, entry)) = self
            .history
            .recent()
            .get(self.history_selection)
            .map(|(key, entry)| ((*key).clone(), (*entry).clone()))
        else {
            return;
        };
        let start_page = entry.page.min(entry.pages.saturating_sub(1));

        if let Some(local) = &entry.local {
            let chapter = self
                .library
                .series
                .iter()
                .find(|series| series.path == local.series_path)
                .and_then(|series| {
                    series
                        .chapters
                        .iter()
                        .find(|chapter| chapter.path == local.chapter_path)
                        .cloned()
                });
            let Some(chapter) = chapter else {
                self.status = Some("That local chapter is no longer in the library.".to_owned());
                return;
            };
            let pages = match chapter_pages(&chapter) {
                Ok(pages) => pages,
                Err(error) => {
                    self.status = Some(error.to_string());
                    return;
                }
            };
            self.start_history_reader(
                entry.series_title.clone(),
                entry.chapter_title.clone(),
                chapter.path.clone(),
                pages,
                start_page,
            );
            return;
        }

        if let Some(remote) = &entry.remote {
            if self.host_client.is_none() && !self.ensure_host() {
                return;
            }
            let pages = match self.remote_chapter_pages(&remote.manga, &remote.chapter) {
                Ok(pages) => pages,
                Err(error) => {
                    self.set_remote_error(error);
                    return;
                }
            };
            self.start_history_reader(
                entry.series_title.clone(),
                entry.chapter_title.clone(),
                PathBuf::from(key),
                pages,
                start_page,
            );
        }
    }

    fn start_history_reader(
        &mut self,
        series_title: String,
        chapter_title: String,
        chapter_path: PathBuf,
        pages: Vec<Page>,
        start_page: usize,
    ) {
        if pages.is_empty() {
            self.status = Some("Chapter has no pages.".to_owned());
            return;
        }
        let start_page = start_page.min(pages.len() - 1);
        self.reader = Some(ReaderState {
            series_title,
            chapter_title,
            chapter_path: chapter_path.clone(),
            pages,
            page_sizes: Vec::new(),
            page_index: start_page,
            protocol: None,
            scroll: None,
            pending_scroll: None,
            error: None,
        });
        self.reader_origin = ReaderOrigin::History;
        self.tab = 2;
        self.status = None;
        self.record_progress(
            &chapter_path,
            start_page,
            self.reader.as_ref().map_or(0, |reader| reader.pages.len()),
        );
        match self.reader_mode {
            ReaderMode::Page => self.load_page(),
            ReaderMode::Scroll => {
                self.load_page();
                self.build_scroll(terminal_width());
            }
        }
    }

    /// Persist reading progress unless Incognito mode is on.
    fn record_progress(&mut self, chapter: &std::path::Path, page: usize, pages: usize) {
        if self.config.incognito {
            return;
        }
        self.progress.update(chapter, page, pages);
        self.record_history(chapter, page, pages);
        self.progress_dirty = true;
        self.progress_changed_at = Some(Instant::now());
    }

    fn record_history(&mut self, chapter: &std::path::Path, page: usize, pages: usize) {
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        let series_title = reader.series_title.clone();
        let chapter_title = reader.chapter_title.clone();
        let key = chapter.to_string_lossy().into_owned();

        let entry = match self.reader_origin {
            ReaderOrigin::Library | ReaderOrigin::BrowseLocal => {
                let chapter_path = chapter.to_path_buf();
                let series = self.library.series.iter().find(|series| {
                    series
                        .chapters
                        .iter()
                        .any(|known| known.path == chapter_path)
                });
                if let Some(series) = series {
                    HistoryEntry::local(
                        series_title,
                        chapter_title,
                        series.path.clone(),
                        chapter_path,
                        series.cover.clone(),
                        page,
                        pages,
                    )
                } else if let Some(mut existing) = self.history.get(&key).cloned() {
                    existing.page = page;
                    existing.pages = pages;
                    existing.last_read = 0;
                    existing
                } else {
                    return;
                }
            }
            ReaderOrigin::Remote => {
                let Some(manga) = self.remote.manga.clone() else {
                    return;
                };
                let Some(remote_chapter) =
                    self.remote.chapters.get(self.remote.chapter_index).cloned()
                else {
                    return;
                };
                let source = self.remote_source_name(&manga.source_id);
                HistoryEntry::remote(
                    series_title,
                    chapter_title,
                    source,
                    manga,
                    remote_chapter,
                    page,
                    pages,
                )
            }
            ReaderOrigin::LibraryRemote => {
                let Some(manga) = self.library_remote.manga.clone() else {
                    return;
                };
                let Some(remote_chapter) = self
                    .library_remote
                    .chapters
                    .get(self.library_remote.chapter_index)
                    .cloned()
                else {
                    return;
                };
                let source = self.remote_source_name(&manga.source_id);
                HistoryEntry::remote(
                    series_title,
                    chapter_title,
                    source,
                    manga,
                    remote_chapter,
                    page,
                    pages,
                )
            }
            ReaderOrigin::History => {
                let Some(mut existing) = self.history.get(&key).cloned() else {
                    return;
                };
                existing.page = page;
                existing.pages = pages;
                existing.last_read = 0;
                existing
            }
        };

        self.history.record(key, entry);
        self.history_dirty = true;
    }

    /// Whether the in-memory progress has been idle long enough to write.
    pub fn progress_save_due(&self) -> bool {
        (self.progress_dirty || self.history_dirty)
            && self
                .progress_changed_at
                .is_some_and(|changed| changed.elapsed() >= Duration::from_millis(500))
    }

    /// Write pending progress immediately. Safe to call when nothing is dirty.
    pub fn flush_progress(&mut self) {
        if self.progress_dirty {
            let _ = self.progress.save();
            self.progress_dirty = false;
            self.progress_changed_at = None;
        }
        if self.history_dirty {
            let _ = self.history.save();
            self.history_dirty = false;
        }
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
                // The old scroll layout belongs to wherever page mode was before.
                if let Some(reader) = self.reader.as_mut() {
                    reader.scroll = None;
                    reader.pending_scroll = None;
                }
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
            self.sync_pending_offset();
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
        if self.reader_origin == ReaderOrigin::History {
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
        if self.reader_origin == ReaderOrigin::History {
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
        // scanline. Smaller blocks also cap the bytes copied and written to the
        // terminal when a block is first shown; the worker caches the decoded page,
        // so splitting into blocks costs only crop+encode.
        let block_rows = (SCROLL_BLOCK_PIXELS / font_height).clamp(16, SCROLL_BLOCK_ROWS_MAX);

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

        let anchor = self.reader.as_ref().and_then(|reader| {
            reader
                .scroll
                .as_ref()
                .and_then(|scroll| scroll_anchor(scroll, scroll.offset))
        });
        let viewport_height = self
            .reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref().or(reader.pending_scroll.as_ref()))
            .map_or(10, |scroll| scroll.viewport_height)
            .max(1);
        let page_index = self.reader.as_ref().map_or(0, |reader| reader.page_index);
        let start_row = layout
            .iter()
            .find(|segment| segment.page == page_index)
            .map_or(0, |segment| segment.start);
        let total_rows = total_rows.max(1);
        let max_scroll = total_rows.saturating_sub(u32::from(viewport_height));
        let offset = anchor
            .and_then(|anchor| offset_for_anchor(&layout, anchor))
            .unwrap_or(start_row)
            .min(max_scroll);
        let page_count = layout.len();
        let generation = self.bump_scroll_generation();

        scroll_debug(&format!(
            "layout {} pages rows={} viewport={} offset={} anchor={:?}",
            layout.len(),
            total_rows,
            viewport_height,
            offset,
            anchor
        ));
        if let Some(reader) = self.reader.as_mut() {
            reader.error = None;
            let state = ScrollState {
                generation,
                layout,
                protocols: (0..page_count).map(|_| None).collect(),
                requested: vec![false; page_count],
                fresh: vec![false; page_count],
                deferred_transmits: false,
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
        let (load_start, load_end, first, last, page, pending, generation) = {
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
            let load_start = first.saturating_sub(
                self.scroll_cache
                    .prefetch_behind
                    .saturating_mul(SCROLL_PREFETCH_SCALE),
            );
            let load_end = (last
                + self
                    .scroll_cache
                    .prefetch_ahead
                    .saturating_mul(SCROLL_PREFETCH_SCALE))
            .min(scroll.layout.len().saturating_sub(1));
            let page = page_at_offset(scroll);
            let generation = scroll.generation;

            (load_start, load_end, first, last, page, pending, generation)
        };

        if !pending {
            if let Some(reader) = self.reader.as_mut() {
                reader.page_index = page;
            }
        }

        if let Some(worker) = self.scroll_worker.as_ref() {
            worker.set_window(generation, load_start, load_end);
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
                    // A request outside the window is dropped by the worker; clear its
                    // flag so scrolling back can request it again.
                    for (index, requested) in scroll.requested.iter_mut().enumerate() {
                        if (index < load_start || index > load_end) && *requested {
                            *requested = false;
                        }
                    }
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
                let margin = self
                    .scroll_cache
                    .cache_margin
                    .saturating_mul(SCROLL_PREFETCH_SCALE);
                let keep_start = first.saturating_sub(margin);
                let keep_end = last + margin;
                for (index, protocol) in scroll.protocols.iter_mut().enumerate() {
                    if index < keep_start || index > keep_end {
                        *protocol = None;
                        if let Some(fresh) = scroll.fresh.get_mut(index) {
                            *fresh = false;
                        }
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
                        if let Some(fresh) = scroll.fresh.get_mut(index) {
                            *fresh = true;
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
        self.sync_pending_offset();
    }

    fn scroll_page_step(&self) -> i32 {
        self.reader
            .as_ref()
            .and_then(|reader| reader.scroll.as_ref())
            .map_or(10, |scroll| {
                i32::from(scroll.viewport_height.saturating_sub(2).max(1))
            })
    }

    /// Keep the not-yet-visible layout at the same source position as the old one.
    fn sync_pending_offset(&mut self) {
        let anchor = self.reader.as_ref().and_then(|reader| {
            reader
                .scroll
                .as_ref()
                .and_then(|scroll| scroll_anchor(scroll, scroll.offset))
        });
        let Some(anchor) = anchor else {
            return;
        };
        let Some(reader) = self.reader.as_mut() else {
            return;
        };
        let Some(pending) = reader.pending_scroll.as_mut() else {
            return;
        };
        let max_scroll = pending
            .total_rows
            .saturating_sub(u32::from(pending.viewport_height));
        if let Some(offset) = offset_for_anchor(&pending.layout, anchor) {
            pending.offset = offset.min(max_scroll);
        }
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
        if index != self.tab {
            self.multi_select = None;
        }
        // Each tab keeps its own navigation state. Only refresh the remote source list
        // the first time Browse is opened with a host configured.
        if index == 3 && self.tab != 3 && self.host_autostart && self.remote.sources.is_empty() {
            // Do not freeze the UI while the JVM starts. The Sources screen renders
            // immediately and shows a status row until the host answers.
            self.begin_host_start(true);
        }
        if index == 0 {
            self.maybe_recover_covers();
        }
        if index == 2 {
            self.clamp_history_selection();
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
        let same_open =
            self.library_remote.manga.as_ref().is_some_and(|manga| {
                manga.source_id == remote.source_id && manga.url == remote.url
            });
        if !same_open {
            self.library_remote.chapter_index = 0;
        }
        let key = remote_story_key(&remote.source_id, &remote.url);
        if let Some((manga, chapters)) = self.remote_chapter_cache.get(&key).cloned() {
            let mut manga = manga;
            if remote.thumbnail_url.is_some() {
                manga.thumbnail_url = remote.thumbnail_url;
            }
            let source = self
                .remote_source_display_name(&remote.source_id)
                .or_else(|| {
                    if same_open {
                        self.library_remote.source_name.clone()
                    } else {
                        None
                    }
                });
            self.show_library_remote_story(manga, chapters, source);
            self.refresh_current_library_story();
            return;
        }
        if let Some(story) = load_cached_story(&remote.source_id, &remote.url) {
            let mut manga = story.manga;
            // Cached thumbnail URLs contain the old host process's port.
            manga.thumbnail_url = remote.thumbnail_url;
            let source = story.source_name.clone();
            self.remote_chapter_cache
                .insert(key, (manga.clone(), story.chapters.clone()));
            self.show_library_remote_story(manga, story.chapters, source);
            self.refresh_current_library_story();
            return;
        }
        let already_loaded =
            self.library_remote.manga.as_ref().is_some_and(|manga| {
                manga.source_id == remote.source_id && manga.url == remote.url
            }) && !self.library_remote.chapters.is_empty();
        if already_loaded {
            self.goto_library_remote_story();
            return;
        }
        self.fetch_library_remote_series(remote, title);
    }

    /// Store a story in memory and on disk after a fetch.
    fn cache_story(&mut self, manga: RemoteManga, chapters: Vec<RemoteChapter>) {
        if chapters.is_empty() {
            return;
        }
        let key = remote_story_key(&manga.source_id, &manga.url);
        if self.removed_stories.contains(&key) {
            return;
        }
        if self.remote_chapter_cache.len() >= 24 {
            self.remote_chapter_cache.clear();
        }
        self.remote_chapter_cache
            .insert(key, (manga.clone(), chapters.clone()));
        let source_name = self
            .remote
            .sources
            .iter()
            .find(|source| source.id == manga.source_id)
            .map(RemoteSource::display_name);
        let mut story = CachedStory::new(manga, chapters);
        story.source_name = source_name;
        let _ = save_cached_story(&story);
    }

    fn goto_library_remote_story(&mut self) {
        self.view = LibraryView::RemoteDetails;
        self.status = None;
        let selected = self
            .library_remote
            .chapter_index
            .min(self.library_remote.chapters.len().saturating_sub(1));
        self.library_remote.list_state.select(Some(selected));
    }

    fn show_library_remote_story(
        &mut self,
        manga: RemoteManga,
        chapters: Vec<RemoteChapter>,
        source_name: Option<String>,
    ) {
        let selected = self
            .library_remote
            .chapter_index
            .min(chapters.len().saturating_sub(1));
        self.library_remote.manga = Some(manga);
        self.library_remote.chapters = chapters;
        self.library_remote.source_name = source_name;
        self.library_remote.chapter_index = selected;
        self.library_remote.list_state = ListState::default();
        self.library_remote.list_state.select(Some(selected));
        self.library_remote.downloaded = self
            .library_remote
            .manga
            .as_ref()
            .map(|manga| self.mark_downloaded(manga, &self.library_remote.chapters))
            .unwrap_or_default();
        self.view = LibraryView::RemoteDetails;
        self.status = None;
    }

    /// Reload metadata and chapters for the saved manga that is already open.
    pub fn reload_library_remote_chapters(&mut self) {
        let Some(manga) = self.library_remote.manga.clone() else {
            return;
        };
        let remote = RemoteSeries {
            source_id: manga.source_id.clone(),
            url: manga.url.clone(),
            thumbnail_url: manga.thumbnail_url.clone(),
            memo: manga.memo.clone(),
        };
        self.fetch_library_remote_series(remote, manga.title.clone());
    }

    fn fetch_library_remote_series(&mut self, remote: RemoteSeries, title: String) {
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
        self.story_refreshed
            .insert(remote_story_key(&manga.source_id, &manga.url));
        self.cache_story(manga.clone(), chapters.clone());
        let source = self.remote_source_display_name(&manga.source_id);
        self.show_library_remote_story(manga, chapters, source);
    }

    /// Whether each chapter of a remote manga has a completed download directory.
    fn mark_downloaded(&self, manga: &RemoteManga, chapters: &[RemoteChapter]) -> Vec<bool> {
        let Some(root) = downloads_dir(&self.config) else {
            return vec![false; chapters.len()];
        };
        let source = self.remote_source_name(&manga.source_id);
        let series_name = series_dir_name(&manga.title);
        let mut series_dir = root.join(source_dir_name(&source)).join(&series_name);
        if !series_dir.is_dir() {
            // The source list is not loaded yet (for example when a cached story is
            // opened before the host starts). Find the series under whichever source
            // directory already has it.
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join(&series_name);
                    if candidate.is_dir() {
                        series_dir = candidate;
                        break;
                    }
                }
            }
        }
        let mut names = HashSet::new();
        if let Ok(entries) = std::fs::read_dir(&series_dir) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    names.insert(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        chapters
            .iter()
            .map(|chapter| {
                names.contains(&chapter_dir_name(
                    &chapter.name,
                    chapter.scanlator.as_deref(),
                ))
            })
            .collect()
    }

    /// Recompute download markers for the open Library and Browse manga.
    pub fn refresh_download_marks(&mut self) {
        if let Some(marks) = self
            .library_remote
            .manga
            .as_ref()
            .map(|manga| self.mark_downloaded(manga, &self.library_remote.chapters))
        {
            self.library_remote.downloaded = marks;
        }
        if let Some(marks) = self
            .remote
            .manga
            .as_ref()
            .map(|manga| self.mark_downloaded(manga, &self.remote.chapters))
        {
            self.remote.downloaded = marks;
        }
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

        // Before the host loads favorites, `remote.favorites` can be empty even though
        // saved manga are mirrored in the config. Prefer the persisted list then;
        // otherwise a delete/refresh in that window would drop every remote item from
        // the grid until the next launch.
        let favorites = if self.remote.favorites_loaded {
            self.remote.favorites.clone()
        } else {
            self.config.remote_library.clone()
        };
        for favorite in favorites {
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
        if self.downloaded_chapter_dir(&manga, &chapter).is_none()
            && self.host_client.is_none()
            && !self.ensure_host()
        {
            return;
        }
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
            self.host_start_epoch = self.host_start_epoch.wrapping_add(1);
            self.host_starting = false;
            self.pending_browse_sources = false;
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
                self.apply_hidden_series();

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
                if self.tab == 0 && self.view == LibraryView::Series {
                    self.maybe_recover_covers();
                }
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
            if let Some(fresh) = scroll.fresh.get_mut(result.index) {
                *fresh = true;
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

pub(crate) fn scroll_debug(message: &str) {
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

fn scroll_anchor(scroll: &ScrollState, offset: u32) -> Option<ScrollAnchor> {
    let index = scroll
        .layout
        .iter()
        .rposition(|segment| segment.start <= offset)?;
    let segment = scroll.layout.get(index)?;
    let within = f64::from(offset.saturating_sub(segment.start));
    let fraction = (within / f64::from(segment.rows.max(1))).clamp(0.0, 1.0);
    Some(ScrollAnchor {
        page: segment.page,
        source_y: f64::from(segment.source_y) + fraction * f64::from(segment.source_height),
    })
}

fn offset_for_anchor(layout: &[ScrollPageLayout], anchor: ScrollAnchor) -> Option<u32> {
    let mut last = None;
    for segment in layout.iter().filter(|segment| segment.page == anchor.page) {
        last = Some(segment);
        let top = f64::from(segment.source_y);
        let bottom = top + f64::from(segment.source_height);
        if anchor.source_y < bottom {
            let fraction =
                ((anchor.source_y - top) / f64::from(segment.source_height.max(1))).clamp(0.0, 1.0);
            return Some(
                (f64::from(segment.start) + fraction * f64::from(segment.rows)).round() as u32,
            );
        }
    }
    last.map(|segment| segment.start + u32::from(segment.rows).saturating_sub(1))
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

fn write_cover_atomically(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let id = COVER_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("img.{id}.tmp"));
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

fn migrate_legacy_cover(path: &Path, source_id: &str, manga_url: &str) -> bool {
    let Some(legacy) = legacy_remote_cover_path(source_id, manga_url) else {
        return false;
    };
    if !legacy.exists() {
        return false;
    }
    let Ok(bytes) = std::fs::read(&legacy) else {
        return false;
    };
    if image::load_from_memory(&bytes).is_err() {
        let _ = std::fs::remove_file(&legacy);
        return false;
    }
    write_cover_atomically(path, &bytes);
    let _ = std::fs::remove_file(legacy);
    true
}

fn remote_story_key(source_id: &str, url: &str) -> String {
    format!("{source_id}\n{url}")
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

fn collect_filter_rows(
    filters: &[SourceFilter],
    path: &mut Vec<usize>,
    depth: usize,
    rows: &mut Vec<FilterRow>,
) {
    for (index, filter) in filters.iter().enumerate() {
        path.push(index);
        rows.push(FilterRow::Filter {
            path: path.clone(),
            depth,
        });
        if let SourceFilter::Group { children, .. } = filter {
            collect_filter_rows(children, path, depth + 1, rows);
        }
        path.pop();
    }
}

fn filter_at_path<'a>(filters: &'a [SourceFilter], path: &[usize]) -> Option<&'a SourceFilter> {
    let (first, rest) = path.split_first()?;
    let filter = filters.get(*first)?;
    if rest.is_empty() {
        Some(filter)
    } else if let SourceFilter::Group { children, .. } = filter {
        filter_at_path(children, rest)
    } else {
        None
    }
}

fn filter_at_path_mut<'a>(
    filters: &'a mut [SourceFilter],
    path: &[usize],
) -> Option<&'a mut SourceFilter> {
    let (first, rest) = path.split_first()?;
    let filter = filters.get_mut(*first)?;
    if rest.is_empty() {
        Some(filter)
    } else if let SourceFilter::Group { children, .. } = filter {
        filter_at_path_mut(children, rest)
    } else {
        None
    }
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
    fn toggles_a_bookmark_for_the_selected_chapter() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.open();

        let key = app.active_chapter_key().expect("chapter selected");
        app.toggle_bookmark();
        assert!(app.bookmarks.is_bookmarked(&key));
        app.toggle_bookmark();
        assert!(!app.bookmarks.is_bookmarked(&key));
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
    fn zooming_keeps_the_same_spot_in_a_long_page() {
        let root = tempfile::tempdir().unwrap();
        let image = image::RgbImage::from_pixel(100, 12_000, image::Rgb([40, 50, 60]));
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
                kind: ChapterKind::Directory,
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
        assert!(scroll.layout.len() > 2);
        let offset = scroll.layout[2].start + 7;
        app.reader.as_mut().unwrap().scroll.as_mut().unwrap().offset = offset;
        let before = scroll_anchor(
            app.reader.as_ref().unwrap().scroll.as_ref().unwrap(),
            offset,
        )
        .unwrap();

        app.zoom_in();

        let pending = app
            .reader
            .as_ref()
            .unwrap()
            .pending_scroll
            .as_ref()
            .unwrap();
        let after = scroll_anchor(pending, pending.offset).unwrap();
        assert_eq!(before.page, 0);
        assert_eq!(after.page, 0);
        assert!(
            (before.source_y - after.source_y).abs() < 64.0,
            "{before:?} -> {after:?}"
        );
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
                supports_latest: true,
            },
            RemoteSource {
                id: "2".to_owned(),
                name: "Beta".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.b".to_owned()),
                supports_latest: true,
            },
            RemoteSource {
                id: "3".to_owned(),
                name: "Gamma".to_owned(),
                lang: "ja".to_owned(),
                pkg: Some("pkg.c".to_owned()),
                supports_latest: true,
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
    fn last_used_source_leads_the_list() {
        let mut app = remote_app();
        app.config.last_used_source = Some("pkg.b|Beta|en".to_owned());

        let labels: Vec<String> = app
            .source_rows()
            .iter()
            .map(|row| match row {
                SourceRow::Header(label) => format!("[{label}]"),
                SourceRow::Local => "local".to_owned(),
                SourceRow::Remote(index) => app.remote.sources[*index].name.clone(),
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                "[Last used]",
                "Beta",
                "[Local]",
                "local",
                "[EN]",
                "Alpha",
                "[JA]",
                "Gamma"
            ]
        );
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(1)));
    }

    #[test]
    fn disabled_sources_and_languages_are_hidden() {
        let mut app = remote_app();
        app.config.disabled_sources = vec!["pkg.b|Beta|en".to_owned()];
        app.config.enabled_languages = Some(vec!["en".to_owned()]);

        assert!(!app.source_enabled(&app.remote.sources[1].clone()));
        assert!(!app.source_enabled(&app.remote.sources[2].clone()));
        assert_eq!(
            app.source_order(),
            vec![SourceRef::Local, SourceRef::Remote(0)]
        );
        assert_eq!(app.browse_item_count(), 2);
    }

    #[test]
    fn applying_filters_returns_to_results() {
        let mut app = remote_app();
        app.remote.source = Some(app.remote.sources[0].clone());
        app.remote.filters = vec![SourceFilter::Checkbox {
            name: "Completed".to_owned(),
            checked: true,
        }];
        app.remote.filters_loaded = true;
        app.browse_view = BrowseView::Filters;
        app.browse_selection = app.filter_item_count() - 1;

        app.activate_filter_selection();

        assert_eq!(app.browse_view, BrowseView::RemoteResults);
        assert_eq!(app.remote.listing, RemoteListing::Search);
        assert_eq!(app.remote.page, 1);
    }

    #[test]
    fn display_mode_cycles_and_saved_results_can_be_hidden() {
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
        app.remote.results = vec![manga("Alpha"), manga("Beta")];
        app.remote.favorites = vec![RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/Alpha".to_owned(),
            title: "Alpha".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
        }];
        app.browse_view = BrowseView::RemoteResults;

        assert_eq!(
            app.config.source_display_mode,
            SourceDisplayMode::Comfortable
        );
        app.cycle_source_display_mode();
        assert_eq!(app.config.source_display_mode, SourceDisplayMode::Compact);
        app.cycle_source_display_mode();
        assert_eq!(app.config.source_display_mode, SourceDisplayMode::List);
        app.cycle_source_display_mode();
        assert_eq!(
            app.config.source_display_mode,
            SourceDisplayMode::Comfortable
        );

        assert_eq!(app.browse_item_count(), 2);
        app.toggle_hide_saved_results();
        assert!(app.config.browse_hide_saved);
        assert_eq!(app.browse_item_count(), 1);
        assert_eq!(app.visible_remote_results()[0].title, "Beta");
        app.toggle_hide_saved_results();
        assert_eq!(app.browse_item_count(), 2);
    }

    #[test]
    fn filter_rows_toggle_and_reset_values() {
        let mut app = remote_app();
        app.remote.filters = vec![
            SourceFilter::Header {
                name: "Tags".to_owned(),
            },
            SourceFilter::Checkbox {
                name: "Completed".to_owned(),
                checked: false,
            },
            SourceFilter::Group {
                name: "More".to_owned(),
                children: vec![SourceFilter::Tristate {
                    name: "Adult".to_owned(),
                    tri_state: 0,
                }],
            },
            SourceFilter::Select {
                name: "Order".to_owned(),
                selected: 0,
                values: vec!["Popular".to_owned(), "Latest".to_owned()],
            },
        ];
        app.remote.default_filters = app.remote.filters.clone();
        app.remote.filters_loaded = true;
        app.browse_view = BrowseView::Filters;

        // Header is skipped; selection 0 is the checkbox, then Group, TriState, Select,
        // Reset, Apply.
        assert_eq!(app.filter_item_count(), 6);
        app.browse_selection = 0;
        app.activate_filter_selection();
        assert!(matches!(
            app.remote.filters[1],
            SourceFilter::Checkbox { checked: true, .. }
        ));

        app.browse_selection = 3;
        app.adjust_filter_selection(1);
        assert!(matches!(
            app.remote.filters[3],
            SourceFilter::Select { selected: 1, .. }
        ));

        app.reset_filters();
        assert!(matches!(
            app.remote.filters[1],
            SourceFilter::Checkbox { checked: false, .. }
        ));
    }

    #[test]
    fn source_filter_screen_toggles_languages_and_sources() {
        let mut app = remote_app();
        app.open_source_filters();
        assert_eq!(app.browse_view, BrowseView::SourceFilters);

        // Languages are the first two selectable rows: EN then JA.
        app.browse_selection = 1;
        app.toggle_source_filter_selection();
        assert!(!app.language_enabled("ja"));
        assert_eq!(
            app.source_order(),
            vec![SourceRef::Local, SourceRef::Remote(0), SourceRef::Remote(1)]
        );

        // Toggling it back restores the default "all languages" state.
        app.browse_selection = 1;
        app.toggle_source_filter_selection();
        assert!(app.language_enabled("ja"));
        assert_eq!(app.config.enabled_languages, None);

        // Sources follow the language rows: Alpha, Beta, Gamma.
        app.browse_selection = 3;
        app.toggle_source_filter_selection();
        assert!(!app.source_enabled(&app.remote.sources[1].clone()));
        assert!(!app.source_order().contains(&SourceRef::Remote(1)));

        app.browse_selection = 3;
        app.toggle_source_filter_selection();
        assert!(app.source_enabled(&app.remote.sources[1].clone()));
    }

    #[test]
    fn local_source_search_latest_and_order() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("Azumanga")).unwrap();
        std::fs::create_dir_all(root.path().join("Berserk")).unwrap();
        let mut library = library();
        for series in &mut library.series {
            series.path = root.path().join(&series.title);
        }
        let mut app = AppState::new(library, None);
        app.browse_view = BrowseView::Local;

        app.search_local("ber");
        let titles: Vec<&str> = app
            .browse_local_indices()
            .into_iter()
            .map(|index| app.library.series[index].title.as_str())
            .collect();
        assert_eq!(titles, ["Berserk"]);
        assert_eq!(app.browse_item_count(), 1);

        app.search_local("");
        assert_eq!(app.browse_item_count(), 2);

        app.toggle_local_order();
        let titles: Vec<&str> = app
            .browse_local_indices()
            .into_iter()
            .map(|index| app.library.series[index].title.as_str())
            .collect();
        assert_eq!(titles, ["Berserk", "Azumanga"]);
        assert_eq!(
            app.config.local_source_order,
            tsuiyomi_core::LocalSourceOrder::TitleDesc
        );

        app.toggle_local_latest();
        assert!(app.browse_local_latest);
        assert!(app.browse_local_query.is_empty());
        assert_eq!(app.browse_item_count(), 2);
    }

    #[test]
    fn appending_remote_pages_dedupes_by_url() {
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
        app.remote.results = vec![manga("Alpha")];
        app.append_remote_results(vec![manga("Alpha"), manga("Beta")]);
        let titles: Vec<&str> = app
            .remote
            .results
            .iter()
            .map(|manga| manga.title.as_str())
            .collect();
        assert_eq!(titles, ["Alpha", "Beta"]);
    }

    #[test]
    fn unsupported_sources_do_not_load_latest() {
        let mut app = remote_app();
        app.remote.sources[0].supports_latest = false;
        app.remote.source = Some(app.remote.sources[0].clone());

        app.load_remote_latest();

        assert_eq!(
            app.remote.status.as_deref(),
            Some("Alpha (EN) has no latest listing")
        );
        assert_eq!(app.browse_view, BrowseView::Sources);
    }

    #[test]
    fn opening_a_source_records_it_as_last_used() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::Sources;
        app.browse_selection = 1;
        assert_eq!(app.selected_source_ref(), Some(SourceRef::Remote(0)));

        app.browse_open();

        assert_eq!(
            app.config.last_used_source.as_deref(),
            Some("pkg.a|Alpha|en")
        );
    }

    #[test]
    fn incognito_does_not_record_last_used_source() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::Sources;
        app.config.incognito = true;
        app.browse_selection = 1;

        app.browse_open();

        assert_eq!(app.config.last_used_source, None);
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
    fn updating_details_never_clears_a_working_thumbnail() {
        let mut result = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/test".to_owned(),
            title: "Test".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: Some("http://127.0.0.1/search-cover.webp".to_owned()),
            memo: None,
        };
        let details = RemoteManga {
            thumbnail_url: None,
            description: Some("Details".to_owned()),
            ..result.clone()
        };

        AppState::merge_result_details(&mut result, &details);

        assert_eq!(
            result.thumbnail_url.as_deref(),
            Some("http://127.0.0.1/search-cover.webp")
        );
        assert_eq!(result.description.as_deref(), Some("Details"));
    }

    #[test]
    fn details_gained_later_update_result_thumbnails() {
        let mut app = remote_app();
        let mut result = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/test".to_owned(),
            title: "Test".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        app.remote.results = vec![result.clone()];
        result.thumbnail_url = Some("http://127.0.0.1/cover.webp".to_owned());
        result.description = Some("Loaded from details".to_owned());
        app.remote.manga = Some(result);

        app.update_result_thumbnail();

        assert_eq!(
            app.remote.results[0].thumbnail_url.as_deref(),
            Some("http://127.0.0.1/cover.webp")
        );
        assert_eq!(
            app.remote.results[0].description.as_deref(),
            Some("Loaded from details")
        );
    }

    #[test]
    fn installed_extensions_ask_before_uninstalling() {
        let mut app = remote_app();
        app.host_autostart = false;
        app.host_client = Some(HostClient::new("http://127.0.0.1:9"));
        app.tab = 3;
        app.browse_view = BrowseView::Extensions;
        app.browse_selection = 0;
        app.remote.extensions = vec![ExtensionInfo {
            name: "Asura Scans".to_owned(),
            pkg: "pkg.asura".to_owned(),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: "asura.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: "Asura Scans".to_owned(),
                lang: "en".to_owned(),
            }],
            repo_url: "https://example.org/index.min.json".to_owned(),
            available_version: Some("1.1".to_owned()),
        }];

        app.install_selected_extension();
        assert!(app.uninstall_confirm.is_none());
        assert!(app
            .remote
            .status
            .as_deref()
            .is_some_and(|status| status.contains("already installed")));

        app.uninstall_selected_extension();

        assert_eq!(
            app.uninstall_confirm,
            Some(vec![("pkg.asura".to_owned(), "Asura Scans".to_owned())])
        );
        app.cancel_uninstall_extension();
        assert!(app.uninstall_confirm.is_none());
    }

    fn local_reader_app() -> (AppState, tempfile::TempDir) {
        let root = tempfile::tempdir().unwrap();
        image::RgbImage::from_pixel(10, 20, image::Rgb([30, 40, 50]))
            .save(root.path().join("001.png"))
            .unwrap();
        let series = Series {
            path: root.path().to_path_buf(),
            remote: None,
            title: "History Series".to_owned(),
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
                kind: ChapterKind::Directory,
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
        app.set_config(Config::default());
        app.open();
        app.open();
        (app, root)
    }

    #[test]
    fn reading_records_history_and_can_resume_it() {
        let (mut app, _root) = local_reader_app();

        assert_eq!(app.history_count(), 1);
        let (title, entry) = {
            let recent = app.history.recent();
            let (_, entry) = recent[0];
            (entry.series_title.clone(), entry.clone())
        };
        assert_eq!(title, "History Series");
        assert!(entry.local.is_some());

        app.back();
        app.tab = 2;
        app.clamp_history_selection();
        app.open_history_entry();

        assert!(app.reader.is_some());
        assert_eq!(app.reader_origin, ReaderOrigin::History);
        assert_eq!(app.tab, 2);
    }

    #[test]
    fn history_entries_can_be_deleted() {
        let (mut app, _root) = local_reader_app();
        assert_eq!(app.history_count(), 1);

        app.tab = 2;
        app.history_selection = 0;
        app.delete_history_selection();

        assert_eq!(app.history_count(), 0);
    }

    #[test]
    fn multi_select_bookmarks_chapters() {
        let mut app = AppState::new(library(), None);
        app.set_config(Config::default());
        app.select_index(1);
        app.open();

        app.chapter_index = 0;
        app.toggle_multi_select();
        app.chapter_index = 1;
        app.toggle_multi_select();
        assert_eq!(app.multi_select.as_ref().map(|s| s.keys.len()), Some(2));

        app.toggle_bookmark();

        let chapters = app
            .selected_series()
            .map(|series| series.chapters.clone())
            .unwrap_or_default();
        assert_eq!(chapters.len(), 2);
        assert!(chapters
            .iter()
            .all(|chapter| app.bookmarks.is_bookmarked(&chapter.path)));
        assert!(app.multi_select.is_none());
    }

    #[test]
    fn multi_select_batches_library_delete() {
        let mut app = AppState::new(library(), None);
        app.set_config(Config::default());
        app.select_index(0);
        app.toggle_multi_select();
        app.select_index(1);
        app.toggle_multi_select();

        assert_eq!(app.multi_select.as_ref().map(|s| s.keys.len()), Some(2));

        app.open_library_delete_dialog();
        assert_eq!(
            app.library_delete.as_ref().map(|dialog| dialog.items.len()),
            Some(2)
        );
        app.cancel_library_delete();
        app.clear_multi_select();
    }

    #[test]
    fn deleting_a_local_series_hides_it_without_removing_files() {
        let mut app = AppState::new(library(), None);
        app.set_config(Config::default());
        app.select_index(1);
        assert_eq!(
            app.selected_series().map(|series| series.title.as_str()),
            Some("Berserk")
        );

        app.open_library_delete_dialog();
        app.confirm_library_delete();

        assert!(app
            .library
            .series
            .iter()
            .all(|series| series.title != "Berserk"));
        assert!(app
            .config
            .hidden_series
            .iter()
            .any(|path| path == Path::new("Berserk")));
    }

    #[test]
    fn deleting_downloads_removes_the_series_directory() {
        let root = tempfile::tempdir().unwrap();
        let downloads = root.path().join("downloads");
        let chapter = downloads
            .join("Asura Scans (EN)")
            .join("Overgeared")
            .join("Chapter 1");
        std::fs::create_dir_all(&chapter).unwrap();
        std::fs::write(chapter.join("001.jpg"), b"x").unwrap();

        let mut app = AppState::new(library(), None);
        app.set_config(Config::default());
        app.config.downloads_path = Some(downloads.clone());
        app.remote.sources = vec![RemoteSource {
            id: "1".to_owned(),
            name: "Asura Scans".to_owned(),
            lang: "en".to_owned(),
            pkg: None,
            supports_latest: true,
        }];
        let remote = RemoteSeries {
            source_id: "1".to_owned(),
            url: "/series/overgeared".to_owned(),
            thumbnail_url: None,
            memo: None,
        };

        assert!(app.delete_series_downloads(&remote, "Overgeared").unwrap());
        assert!(!downloads.join("Asura Scans (EN)/Overgeared").exists());
    }

    #[test]
    fn local_browse_only_lists_local_series() {
        let mut app = AppState::new(library(), None);
        app.remote.favorites_loaded = true;
        app.remote.favorites = vec![RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/remote".to_owned(),
            title: "Remote Only".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
        }];
        app.merge_remote_library();
        app.browse_view = BrowseView::Local;

        let titles: Vec<&str> = app
            .browse_local_indices()
            .into_iter()
            .map(|index| app.library.series[index].title.as_str())
            .collect();

        assert_eq!(titles, ["Azumanga", "Berserk"]);
        assert_eq!(app.local_series_count(), 2);
        assert_eq!(app.browse_item_count(), 2);
    }

    #[test]
    fn merging_before_favorites_load_keeps_persisted_remote_items() {
        let mut app = AppState::new(library(), None);
        app.remote.favorites.clear();
        app.remote.favorites_loaded = false;
        app.config.remote_library = vec![
            RemoteFavorite {
                source_id: "1".to_owned(),
                url: "/manga/a".to_owned(),
                title: "Alpha Remote".to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
            },
            RemoteFavorite {
                source_id: "2".to_owned(),
                url: "/manga/b".to_owned(),
                title: "Beta Remote".to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
            },
        ];

        app.merge_remote_library();

        let titles: Vec<&str> = app
            .library
            .series
            .iter()
            .filter(|series| series.remote.is_some())
            .map(|series| series.title.as_str())
            .collect();
        assert_eq!(titles, ["Alpha Remote", "Beta Remote"]);
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
    fn extension_update_count_and_update_all_noop_when_current() {
        let mut app = remote_app();
        app.remote.extensions = vec![ExtensionInfo {
            name: "Asura Scans".to_owned(),
            pkg: "pkg.asura".to_owned(),
            lang: "en".to_owned(),
            version: "1.2".to_owned(),
            apk: "asura.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 1,
            sources: Vec::new(),
            repo_url: "https://example.org/index.min.json".to_owned(),
            available_version: Some("1.2".to_owned()),
        }];
        assert_eq!(app.extension_update_count(), 0);

        app.update_all_extensions();
        assert_eq!(
            app.remote.status.as_deref(),
            Some("No extension updates available")
        );

        app.remote.extensions[0].available_version = Some("1.3".to_owned());
        assert_eq!(app.extension_update_count(), 1);
    }

    #[test]
    fn extension_search_filters_and_extension_pins_all_sources() {
        let mut app = remote_app();
        app.remote.sources.push(RemoteSource {
            id: "4".to_owned(),
            name: "MangaDex".to_owned(),
            lang: "en".to_owned(),
            pkg: Some("pkg.mangadex".to_owned()),
            supports_latest: true,
        });
        app.remote.sources.push(RemoteSource {
            id: "5".to_owned(),
            name: "MangaDex".to_owned(),
            lang: "ja".to_owned(),
            pkg: Some("pkg.mangadex".to_owned()),
            supports_latest: true,
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
                available_version: None,
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
                available_version: None,
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
    fn global_search_scope_toggles_between_pinned_and_all_enabled_sources() {
        let mut app = remote_app();
        app.config.pinned_sources = vec!["pkg.a|Alpha|en".to_owned()];
        app.config.disabled_sources = vec!["pkg.c|Gamma|ja".to_owned()];

        assert_eq!(app.config.global_search_scope, GlobalSearchScope::Pinned);
        let sources = app.global_search_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "Alpha");

        app.toggle_global_search_scope();
        assert_eq!(app.config.global_search_scope, GlobalSearchScope::All);
        let names: Vec<String> = app
            .global_search_sources()
            .iter()
            .map(|source| source.name.clone())
            .collect();
        assert_eq!(names, ["Alpha", "Beta"]);
        assert_eq!(
            app.remote.status.as_deref(),
            Some("Global search: all enabled sources")
        );

        app.toggle_global_search_scope();
        assert_eq!(app.config.global_search_scope, GlobalSearchScope::Pinned);
    }

    #[test]
    fn global_search_events_stream_progress_and_ignore_stale_results() {
        let mut app = remote_app();
        let manga = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/Alpha".to_owned(),
            title: "Alpha".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        app.remote.global_generation = 7;
        app.remote.global_total = 2;
        app.remote.global_loading = true;
        app.remote.global_query = "alpha".to_owned();

        assert!(app.apply_global_search_event(GlobalSearchEvent::Group {
            generation: 7,
            source: app.remote.sources[0].clone(),
            mangas: vec![manga],
        }));
        assert_eq!(app.remote.global_finished, 1);
        assert_eq!(app.remote.global_groups.len(), 1);
        assert_eq!(
            app.remote.status.as_deref(),
            Some("Searching · 1/2 sources")
        );

        assert!(app.apply_global_search_event(GlobalSearchEvent::Error {
            generation: 7,
            source: app.remote.sources[1].clone(),
            message: "timeout".to_owned(),
        }));
        assert_eq!(app.remote.global_finished, 2);
        assert_eq!(
            app.remote.status.as_deref(),
            Some("Searching · 2/2 sources · 1 failed")
        );

        assert!(!app.apply_global_search_event(GlobalSearchEvent::Group {
            generation: 6,
            source: app.remote.sources[1].clone(),
            mangas: Vec::new(),
        }));

        assert!(app.apply_global_search_event(GlobalSearchEvent::Finished { generation: 7 }));
        assert!(!app.remote.global_loading);
        assert_eq!(app.remote.status.as_deref(), Some("1 sources failed"));
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
