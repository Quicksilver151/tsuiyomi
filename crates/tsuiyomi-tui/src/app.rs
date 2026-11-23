use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crossterm::terminal;
use ratatui::{layout::Size, widgets::ListState};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    sliced::SlicedProtocol,
    Resize,
};
use tsuiyomi_core::progress_key as remote_progress_key;
use tsuiyomi_core::{
    best_match, chapter::natural_cmp, chapter_dir, chapter_dir_name, chapter_pages, deep_queries,
    downloads_dir, ensure_runtime, ensure_series_cover, extension_fingerprint, fetch_bytes,
    host_data_dir, import_series, is_downloaded, latest_chapter_number, legacy_remote_cover_path,
    load_cached_story, merge_extensions, missing_chapter_count, move_series_downloads,
    normalized_title, page_dimensions, parse_mihon_backup, progress_prefix, read_cover, read_page,
    read_state, remote_cover_path, save_cached_story, save_config, scan_library, search_anilist,
    series_dir_name, source_dir_name, story_cache_path, title_similarity, today_key,
    update_anilist_progress, update_mal_progress, visible_chapters, write_comic_info,
    write_mihon_backup, BackupCategory, BackupChapter, BackupHistory, BackupManga, BackupSource,
    BookmarkStore, CachedStory, Chapter, ChapterDisplay, ChapterKind, ChapterMeta, ChapterSettings,
    ChapterSort, ComicInfo, Config, ExtensionInfo, GlobalSearchScope, HistoryEntry, HistoryStore,
    HostClient, HostProcess, InstallJob, Library, LibrarySort, LocalSourceOrder, MigrationMatch,
    MihonBackup, NotLoadedExtension, Page, PageSource, ProgressStore, ReadState, ReadingMode,
    ReadingStats, RemoteChapter, RemoteFavorite, RemoteManga, RemoteMangaPage, RemoteReadState,
    RemoteSeries, RemoteSource, RemoteUpdate, Series, SourceDisplayMode, SourceFilter,
    SourcePreference, SourcePreferenceKind, SourcePreferenceValue, SourcePreferences, Theme,
    TrackerLink, TrackerMedia, TriState, UpdateEntry, UpdateStore,
};

use crate::{
    cover_cache::{fit_cover_image, image_bytes, TileCoverCache},
    cover_worker::{CoverRequest, CoverWorker},
    download_worker::{DownloadRequest, DownloadWorker},
    extensions_worker::{ExtensionsRequest, ExtensionsSnapshot, ExtensionsWorker},
    favorites_worker::{FavoritesMutation, FavoritesRequest, FavoritesWorker},
    filters_worker::{FiltersRequest, FiltersWorker},
    host_worker::{HostRequest, HostWorker},
    layout::TAB_COUNT,
    migration_worker::{MigrationRequest, MigrationWorker},
    palette,
    prefs_worker::{PrefsRequest, PrefsWorker},
    result_worker::{ResultListing, ResultRequest, ResultWorker},
    scan_worker::{ScanRequest, ScanWorker},
    scroll_worker::{BuildRequest, BuildResult, ScrollWorker, SizeRequest, SizeWorker},
    search_worker::{GlobalSearchEvent, GlobalSearchWorker},
    source_worker::{SourceRequest, SourceWorker},
    story_worker::{StoryRequest, StoryWorker},
    tracker_worker::{TrackerEvent, TrackerRequest, TrackerWorker},
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

/// Unique suffix for atomic cover writes; the loader thread and UI can race.
static COVER_TEMP_ID: AtomicU64 = AtomicU64::new(0);
/// Retry a failed cover fetch after this long instead of giving up for the session.
const COVER_RETRY: Duration = Duration::from_secs(10);
/// Reading time stops accumulating after this long without input, so leaving the
/// reader open while idle does not inflate stats.
const READING_IDLE_CUTOFF: Duration = Duration::from_secs(120);

/// Page-turn direction derived from Mihon's reading mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadingDirection {
    #[default]
    LeftToRight,
    RightToLeft,
    /// Mihon's paged-vertical mode: pages turn up and down instead of sideways.
    Vertical,
}

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
    Updates,
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
    ExtensionDetails,
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
    pub error: Option<String>,
    pub loading: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionRow {
    Header(String),
    NotLoaded(usize),
    Extension(usize),
}

#[derive(Debug, Clone)]
pub struct ExtensionJobState {
    pub job_id: String,
    pub pkg: String,
    pub name: String,
    pub verb: String,
    pub apk: String,
    pub state: String,
    pub progress: f64,
    pub error: Option<String>,
}

impl ExtensionJobState {
    pub fn label(&self) -> String {
        match self.state.as_str() {
            "pending" => "pending".to_owned(),
            "downloading" => format!("downloading {:.0}%", self.progress * 100.0),
            "installing" => "installing".to_owned(),
            "cancelled" => "cancelled".to_owned(),
            "error" => "error".to_owned(),
            state => state.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionDetailRow {
    Source(usize),
    EnableAll,
    DisableAll,
    Incognito,
    ClearCookies,
    Uninstall,
}

pub struct ExtensionDetailsState {
    pub extension: ExtensionInfo,
    pub sources: Vec<RemoteSource>,
    pub list_state: ListState,
    pub fingerprint: Option<String>,
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
    RenameStore,
    SetHostUrl,
    SetFlareSolverr,
    SetDownloadsPath,
    AddCategory,
    RenameCategory,
    ImportBackup,
    ExportBackup,
    ImportSeries,
    MigrationSearch,
    StoryNotes,
    SetCustomCover,
    TrackerSearch,
    AniListToken,
    MalClientId,
    MalToken,
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
    /// A source result page is being fetched.
    pub loading: bool,
    /// Installed files the host could not load.
    pub not_loaded: Vec<NotLoadedExtension>,
    /// Async install/update jobs keyed by package name.
    pub extension_jobs: BTreeMap<String, ExtensionJobState>,
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
            loading: false,
            not_loaded: Vec::new(),
            extension_jobs: BTreeMap::new(),
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
    ReaderFit,
    PrefetchBehind,
    PrefetchAhead,
    CacheMargin,
    LibraryPath,
    DefaultCategory,
    LibrarySort,
    UpdateInterval,
    RescanLibrary,
    AniListToken,
    MalClientId,
    MalToken,
    Theme,
    Accent,
    Fullscreen,
    ThumbnailSize,
    Incognito,
    HostUrl,
    ShowNsfw,
    FlareSolverr,
    DownloadsPath,
    ImportBackup,
    ExportBackup,
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

const LIBRARY_ROWS: [MoreRow; 8] = [
    MoreRow::Divider("Storage"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Setting(SettingId::RescanLibrary),
    MoreRow::Divider("Display"),
    MoreRow::Setting(SettingId::LibrarySort),
    MoreRow::Setting(SettingId::DefaultCategory),
    MoreRow::Divider("Updates"),
    MoreRow::Setting(SettingId::UpdateInterval),
];

const READER_ROWS: [MoreRow; 8] = [
    MoreRow::Divider("Reading"),
    MoreRow::Setting(SettingId::ReadingMode),
    MoreRow::Setting(SettingId::ReaderFit),
    MoreRow::Divider("Continuous scroll"),
    MoreRow::Setting(SettingId::Zoom),
    MoreRow::Setting(SettingId::PrefetchBehind),
    MoreRow::Setting(SettingId::PrefetchAhead),
    MoreRow::Setting(SettingId::CacheMargin),
];

const DOWNLOADS_ROWS: [MoreRow; 4] = [
    MoreRow::Divider("Storage"),
    MoreRow::Setting(SettingId::DownloadsPath),
    MoreRow::Note("TSUIYOMI_DOWNLOADS overrides this for one session."),
    MoreRow::Note("Local library chapters are read directly from disk."),
];

const TRACKING_ROWS: [MoreRow; 7] = [
    MoreRow::Divider("AniList"),
    MoreRow::Setting(SettingId::AniListToken),
    MoreRow::Divider("MyAnimeList"),
    MoreRow::Setting(SettingId::MalClientId),
    MoreRow::Setting(SettingId::MalToken),
    MoreRow::Note("Paste tracker credentials to enable search and progress sync."),
    MoreRow::Note("Tokens are stored in the config file in plain text."),
];

const BROWSE_ROWS: [MoreRow; 7] = [
    MoreRow::Divider("Sources"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Setting(SettingId::HostUrl),
    MoreRow::Divider("Extensions"),
    MoreRow::Setting(SettingId::ShowNsfw),
    MoreRow::Note("Local source lists series directories in the library folder."),
    MoreRow::Note("Extension host runs APKs. Add repositories in Browse → Extensions."),
];

const DATA_ROWS: [MoreRow; 6] = [
    MoreRow::Divider("Storage"),
    MoreRow::Setting(SettingId::LibraryPath),
    MoreRow::Divider("Backups"),
    MoreRow::Setting(SettingId::ImportBackup),
    MoreRow::Setting(SettingId::ExportBackup),
    MoreRow::Note("Backups use Mihon's .tachibk / .proto.gz format."),
];

const SECURITY_ROWS: [MoreRow; 3] = [
    MoreRow::Divider("Privacy"),
    MoreRow::Setting(SettingId::Incognito),
    MoreRow::Note("Incognito stops writing reading progress."),
];

const ADVANCED_ROWS: [MoreRow; 5] = [
    MoreRow::Divider("Network"),
    MoreRow::Setting(SettingId::FlareSolverr),
    MoreRow::Divider("Diagnostics"),
    MoreRow::Note("Log level comes from TSUIYOMI_LOG."),
    MoreRow::Note("Refresh scans synchronously and may block briefly."),
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
            Self::Downloads => "Folder · queue",
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HistorySection {
    #[default]
    History,
    Stats,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MigrateScreen {
    #[default]
    Sources,
    Favorites,
    Matches,
}

#[derive(Debug, Default)]
pub struct MigrateState {
    pub screen: MigrateScreen,
    pub matching: bool,
    pub applying: bool,
    pub source_id: Option<String>,
    pub favorites: Vec<RemoteFavorite>,
    pub matches: Vec<MigrationMatch>,
    pub list_state: ListState,
    pub hide_unmatched: bool,
    pub hide_without_updates: bool,
    pub prioritize_chapters: bool,
    apply_replace: bool,
    apply_queue: Vec<usize>,
    apply_position: usize,
    apply_done: usize,
    apply_failed: usize,
    apply_downloads_moved: usize,
    apply_downloads_failed: usize,
    apply_categories: Vec<(String, String, Vec<String>)>,
}

/// Rows in the chapter settings modal. Scanlator rows follow the fixed settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterSettingRow {
    Downloaded,
    Unread,
    Bookmarked,
    Sort,
    Direction,
    Display,
    DownloadNewChapters,
    Reset,
    Scanlator(usize),
}

#[derive(Debug, Default)]
pub struct ChapterSettingsModal {
    pub selected: usize,
    pub list_state: ListState,
}

/// In-memory state for a chapter in the download queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Queued,
    Downloading,
    Done,
    Error,
    Cancelled,
}

pub struct DownloadTask {
    pub key: String,
    request: DownloadRequest,
}

#[derive(Default)]
pub struct DownloadQueue {
    pub pending: VecDeque<DownloadTask>,
    pub active: Option<(String, String, Arc<AtomicBool>)>,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelling: bool,
}

impl DownloadQueue {
    pub fn running(&self) -> bool {
        self.active.is_some() || !self.pending.is_empty()
    }

    pub fn pending_labels(&self) -> Vec<&str> {
        self.pending
            .iter()
            .map(|task| task.request.label.as_str())
            .collect()
    }

    pub fn label(&self) -> Option<String> {
        let total = self.total;
        if total == 0 {
            return None;
        }
        if self.cancelling {
            return Some(format!("cancelling · {}/{}", self.completed, total));
        }
        if self.running() {
            return Some(format!(
                "downloading · {}/{}",
                (self.completed + self.failed).min(total),
                total
            ));
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadMenuChoice {
    Selected,
    Next(usize),
    Unread,
    Bookmarked,
    Cancel,
}

#[derive(Debug, Default)]
pub struct DownloadMenu {
    pub selected: usize,
}

#[derive(Debug, Default)]
pub struct CategoryPicker {
    pub selected: usize,
    pub chosen: HashSet<String>,
    pub local: bool,
}

pub struct SaveCategories {
    pub manga: RemoteManga,
    pub selected: usize,
    pub chosen: HashSet<String>,
}

#[derive(Debug, Default)]
pub struct TagPicker {
    pub tags: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Default)]
pub struct TrackerPicker {
    pub results: Vec<TrackerMedia>,
    pub selected: usize,
}

#[derive(Debug, Default)]
pub struct MigrationSourcePicker {
    pub selected: usize,
    pub chosen: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MigrationSearchResult {
    pub source: RemoteSource,
    pub manga: RemoteManga,
    pub score: f64,
}

#[derive(Debug, Default)]
pub struct MigrationSearchState {
    pub results: Vec<MigrationSearchResult>,
    pub selected: usize,
}

pub struct HistoryDeleteDialog {
    pub keys: Vec<String>,
    pub series_keys: Vec<String>,
    pub all_series: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedSeries {
    pub source: String,
    pub series: String,
    pub chapters: usize,
    pub bytes: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct StorageSnapshot {
    pub library_bytes: u64,
    pub downloads_bytes: u64,
    pub host_bytes: u64,
    pub series: usize,
    pub extensions: usize,
}

#[derive(Clone)]
pub struct ScanContext {
    previous_series: Option<PathBuf>,
    previous_view: LibraryView,
    previous_chapter: usize,
    previous_category: String,
}

pub struct ReadingSession {
    last_activity: Instant,
    accumulated_secs: u64,
    chapter_key: String,
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
    Migrate,
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
pub struct DuplicateDialog {
    pub manga: RemoteManga,
    pub duplicates: Vec<RemoteFavorite>,
    pub selected: usize,
}

#[derive(Clone)]
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
    pub reading_direction: ReadingDirection,
    pub library: Library,
    pub library_remote: LibraryRemoteState,
    /// Chapters fetched for saved manga, keyed by source+URL. Reopening a story is
    /// then instant; `r` refreshes it.
    remote_chapter_cache: HashMap<String, (RemoteManga, Vec<RemoteChapter>)>,
    pub category_index: usize,
    pub category_manage_index: usize,
    pub selected: usize,
    pub library_query: String,
    pub library_search: bool,
    pub chapter_index: usize,
    pub chapter_list_state: ListState,
    pub reader: Option<ReaderState>,
    pub reader_origin: ReaderOrigin,
    pub page_pan: (f32, f32),
    pub image_picker: Option<Picker>,
    pub progress: ProgressStore,
    pub bookmarks: BookmarkStore,
    pub history: HistoryStore,
    pub reading_stats: ReadingStats,
    reading_session: Option<ReadingSession>,
    pub history_selection: usize,
    pub history_section: HistorySection,
    pub history_query: String,
    pub history_search: bool,
    pub history_scroll: usize,
    pub history_stats_scroll: usize,
    pub history_list_state: ListState,
    pub history_delete: Option<HistoryDeleteDialog>,
    pub history_clear_confirm: bool,
    pub updates: UpdateStore,
    pub updates_selection: usize,
    pub story_updated_at: Option<u64>,
    auto_update_checked_at: Option<Instant>,
    auto_update_inflight: HashSet<String>,
    pub migrate: MigrateState,
    pub chapter_settings_modal: Option<ChapterSettingsModal>,
    history_dirty: bool,
    progress_dirty: bool,
    progress_changed_at: Option<Instant>,
    /// Worker that decodes/resizes/encodes scroll pages off the UI thread.
    pub(crate) scroll_worker: Option<ScrollWorker>,
    pub(crate) size_worker: Option<SizeWorker>,
    pub(crate) download_worker: Option<DownloadWorker>,
    pub(crate) result_worker: Option<ResultWorker>,
    result_generation: u64,
    pub(crate) source_worker: Option<SourceWorker>,
    source_generation: u64,
    pub(crate) favorites_worker: Option<FavoritesWorker>,
    favorites_generation: u64,
    pub(crate) filters_worker: Option<FiltersWorker>,
    filters_generation: u64,
    pub(crate) prefs_worker: Option<PrefsWorker>,
    prefs_generation: u64,
    pub(crate) extensions_worker: Option<ExtensionsWorker>,
    extensions_generation: u64,
    pub(crate) migration_worker: Option<MigrationWorker>,
    migration_generation: u64,
    pub(crate) scan_worker: Option<ScanWorker>,
    scan_generation: u64,
    scan_context: Option<ScanContext>,
    pub storage_snapshot: Option<StorageSnapshot>,
    pub storage_clear_downloads_confirm: bool,
    pub downloaded_series: Vec<DownloadedSeries>,
    pub downloads_selection: usize,
    pub downloads_loaded: bool,
    pub downloads_delete_confirm: bool,
    pending_states: BTreeMap<String, RemoteReadState>,
    pending_favorite_categories: BTreeMap<String, Vec<String>>,
    host_states_loaded: bool,
    state_dirty: bool,
    pub(crate) story_worker: Option<StoryWorker>,
    pub(crate) tracker_worker: Option<TrackerWorker>,
    tracker_generation: u64,
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
    source_prefs_pending: Option<RemoteSource>,
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
    pub(crate) tile_covers: TileCoverCache,
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
    pub duplicate_dialog: Option<DuplicateDialog>,
    pub extension_details: Option<ExtensionDetailsState>,
    pub remove_favorite: Option<RemoteManga>,
    pub multi_select: Option<MultiSelect>,
    /// Chapter download currently running, if any.
    pub download_queue: DownloadQueue,
    pub download_states: BTreeMap<String, DownloadState>,
    pub download_menu: Option<DownloadMenu>,
    pub download_delete_confirm: bool,
    pub category_picker: Option<CategoryPicker>,
    pub save_categories: Option<SaveCategories>,
    pub tag_picker: Option<TagPicker>,
    pub tracker_picker: Option<TrackerPicker>,
    pub tracker_unlink_confirm: bool,
    pub migration_source_picker: Option<MigrationSourcePicker>,
    pub migration_search: Option<MigrationSearchState>,
    pub cover_overlay: bool,
    pub status: Option<String>,
}

impl AppState {
    pub fn new(library: Library, status: Option<String>) -> Self {
        Self {
            tab: 0,
            fullscreen: false,
            view: LibraryView::Series,
            reader_mode: ReaderMode::Page,
            reading_direction: ReadingDirection::LeftToRight,
            library,
            library_remote: LibraryRemoteState::default(),
            remote_chapter_cache: HashMap::new(),
            category_index: 0,
            category_manage_index: 0,
            selected: 0,
            library_query: String::new(),
            library_search: false,
            chapter_index: 0,
            chapter_list_state: ListState::default(),
            reader: None,
            reader_origin: ReaderOrigin::Library,
            page_pan: (0.0, 0.0),
            image_picker: None,
            progress: ProgressStore::default(),
            bookmarks: BookmarkStore::default(),
            history: HistoryStore::default(),
            reading_stats: ReadingStats::default(),
            reading_session: None,
            history_selection: 0,
            history_section: HistorySection::History,
            history_query: String::new(),
            history_search: false,
            history_scroll: 0,
            history_stats_scroll: 0,
            history_list_state: ListState::default(),
            history_delete: None,
            history_clear_confirm: false,
            updates: UpdateStore::default(),
            updates_selection: 0,
            story_updated_at: None,
            auto_update_checked_at: None,
            auto_update_inflight: HashSet::new(),
            migrate: MigrateState::default(),
            chapter_settings_modal: None,
            history_dirty: false,
            progress_dirty: false,
            progress_changed_at: None,
            scroll_worker: None,
            size_worker: None,
            download_worker: None,
            result_worker: None,
            result_generation: 0,
            source_worker: None,
            source_generation: 0,
            favorites_worker: None,
            favorites_generation: 0,
            filters_worker: None,
            filters_generation: 0,
            prefs_worker: None,
            prefs_generation: 0,
            extensions_worker: None,
            extensions_generation: 0,
            migration_worker: None,
            migration_generation: 0,
            scan_worker: None,
            scan_generation: 0,
            scan_context: None,
            storage_snapshot: None,
            storage_clear_downloads_confirm: false,
            downloaded_series: Vec::new(),
            downloads_selection: 0,
            downloads_loaded: false,
            downloads_delete_confirm: false,
            pending_states: BTreeMap::new(),
            pending_favorite_categories: BTreeMap::new(),
            host_states_loaded: false,
            state_dirty: false,
            story_worker: None,
            tracker_worker: None,
            tracker_generation: 0,
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
            source_prefs_pending: None,
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
            tile_covers: TileCoverCache::default(),
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
            duplicate_dialog: None,
            extension_details: None,
            remove_favorite: None,
            multi_select: None,
            download_queue: DownloadQueue::default(),
            download_states: BTreeMap::new(),
            download_menu: None,
            download_delete_confirm: false,
            category_picker: None,
            save_categories: None,
            tag_picker: None,
            tracker_picker: None,
            tracker_unlink_confirm: false,
            migration_source_picker: None,
            migration_search: None,
            cover_overlay: false,
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
        let image = fit_cover_image(image);
        let bytes = image_bytes(&image);
        let protocol = picker.new_resize_protocol(image);
        self.tile_covers.insert(path, protocol, bytes);
    }

    pub fn browse_previous(&mut self) {
        if matches!(
            self.browse_view,
            BrowseView::Chapters | BrowseView::RemoteDetails
        ) {
            let position = self.story_chapter_position();
            self.select_story_position(position.saturating_sub(1));
            return;
        }
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
        if matches!(
            self.browse_view,
            BrowseView::Chapters | BrowseView::RemoteDetails
        ) {
            let position = self.story_chapter_position();
            let count = self.story_chapter_indices().len();
            if count > 0 {
                self.select_story_position((position + 1).min(count - 1));
            }
            return;
        }
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
            self.maybe_load_next_remote_page();
        }
    }

    /// Fetch the next result page when the cursor reaches the last visible result.
    fn maybe_load_next_remote_page(&mut self) {
        if self.browse_view != BrowseView::RemoteResults || !self.remote.has_next_page {
            return;
        }
        let count = self.visible_remote_count();
        if count > 0 && self.browse_selection + 1 >= count {
            self.load_next_remote_page();
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
                self.maybe_load_next_remote_page();
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
                    self.load_remote_filters_async();
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
                self.open_remote_story_async(manga, BrowseView::RemoteResults);
            }
            BrowseView::RemoteDetails => self.open_remote_chapter(self.browse_selection),
            BrowseView::Extensions => self.install_selected_extension(),
            BrowseView::ExtensionDetails => self.activate_extension_detail_selection(),
            BrowseView::Migrate => self.migrate_open(),
            BrowseView::GlobalResults => self.global_open(),
            BrowseView::Stores => {}
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
            BrowseView::ExtensionDetails => {
                self.browse_view = BrowseView::Extensions;
                self.browse_selection = 0;
                self.browse_list_state = ListState::default();
                self.select_browse_entry(0);
            }
            BrowseView::Stores => {
                self.browse_view = BrowseView::Extensions;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::RemoteResults if !self.remote.query.trim().is_empty() => {
                self.remote.query.clear();
                self.remote.listing = RemoteListing::Popular;
                self.remote.page = 1;
                self.browse_selection = 0;
                self.load_remote_results(RemoteListing::Popular);
                self.select_browse_entry(0);
            }
            BrowseView::Local if !self.browse_local_query.trim().is_empty() => {
                self.browse_local_query.clear();
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::Local | BrowseView::RemoteResults => {
                self.browse_view = BrowseView::Sources;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            BrowseView::Migrate => self.migrate_back(),
            // Esc leaves Browse only from the Sources root. Extensions stays.
            BrowseView::Sources => self.select_tab(0),
            BrowseView::Extensions => {}
        }
    }

    /// Refresh the active Browse list.
    pub fn refresh_browse(&mut self) {
        match self.browse_view {
            BrowseView::Sources => self.load_remote_sources_async(),
            BrowseView::SourceFilters => self.load_remote_sources_async(),
            BrowseView::SourcePrefs => self.reload_source_prefs(),
            BrowseView::Filters => self.load_remote_filters(),
            BrowseView::Local => self.refresh_async(),
            BrowseView::RemoteResults => {
                self.load_remote_results(self.remote.listing);
            }
            BrowseView::RemoteDetails => {
                self.load_remote_details();
                self.load_remote_chapters_with(true);
                self.status = Some("Refreshed details and chapters".to_owned());
            }
            BrowseView::Extensions | BrowseView::Stores => self.load_extensions_async(),
            BrowseView::ExtensionDetails => self.load_extensions_async(),
            BrowseView::Chapters => self.refresh_async(),
            BrowseView::Migrate => self.migrate_refresh(),
            BrowseView::GlobalResults => self.retry_global_search(),
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
            BrowseView::Chapters => self.story_chapter_indices().len(),
            BrowseView::RemoteResults => self.visible_remote_count(),
            BrowseView::RemoteDetails => self.story_chapter_indices().len(),
            BrowseView::Extensions => self.filtered_extension_count(),
            BrowseView::ExtensionDetails => self.extension_detail_rows().len(),
            BrowseView::Stores => self.remote.repos.len(),
            BrowseView::Migrate => self.migrate_item_count(),
            BrowseView::GlobalResults => 0,
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
        match self.extension_selection_rows().get(self.browse_selection)? {
            ExtensionRow::Extension(index) => self.remote.extensions.get(*index),
            _ => None,
        }
    }

    pub fn selected_store(&self) -> Option<&String> {
        self.remote.repos.get(self.browse_selection)
    }

    pub fn store_label(&self, url: &str) -> String {
        self.config
            .store_names
            .get(url)
            .filter(|name| !name.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| {
                url.split("://")
                    .nth(1)
                    .unwrap_or(url)
                    .split('/')
                    .next()
                    .unwrap_or(url)
                    .to_owned()
            })
    }

    pub fn open_rename_store(&mut self) {
        if !(self.tab == 3 && self.browse_view == BrowseView::Stores) {
            return;
        }
        let Some(store) = self.selected_store().cloned() else {
            return;
        };
        let current = self.store_label(&store);
        self.open_text_input(TextAction::RenameStore, "Store name", &current);
    }

    fn save_store_name(&mut self, value: &str) {
        let Some(store) = self.selected_store().cloned() else {
            return;
        };
        let value = value.trim();
        if value.is_empty() {
            self.config.store_names.remove(&store);
        } else {
            self.config
                .store_names
                .insert(store.clone(), value.to_owned());
        }
        self.persist_settings();
        self.remote.status = Some(format!("Store name: {}", self.store_label(&store)));
        self.remote.error = false;
    }

    pub fn migrate_sources(&self) -> Vec<(RemoteSource, usize)> {
        let mut grouped: BTreeMap<String, (RemoteSource, usize)> = BTreeMap::new();
        for favorite in &self.config.remote_library {
            let source = self
                .remote
                .sources
                .iter()
                .find(|source| source.id == favorite.source_id)
                .cloned()
                .unwrap_or_else(|| RemoteSource {
                    id: favorite.source_id.clone(),
                    name: "Unknown source".to_owned(),
                    lang: String::new(),
                    pkg: None,
                    supports_latest: true,
                });
            let entry = grouped
                .entry(favorite.source_id.clone())
                .or_insert_with(|| (source, 0));
            entry.1 += 1;
        }
        let mut sources: Vec<_> = grouped.into_values().collect();
        sources.sort_by(|left, right| natural_cmp(&left.0.display_name(), &right.0.display_name()));
        sources
    }

    pub fn migrate_favorites(&self) -> Vec<RemoteFavorite> {
        let Some(source_id) = self.migrate.source_id.as_deref() else {
            return Vec::new();
        };
        let mut favorites: Vec<_> = self
            .config
            .remote_library
            .iter()
            .filter(|favorite| favorite.source_id == source_id)
            .cloned()
            .collect();
        favorites.sort_by(|left, right| natural_cmp(&left.title, &right.title));
        favorites
    }

    /// Match indices in display order, honoring the hide/prioritize options.
    pub fn migration_visible_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .migrate
            .matches
            .iter()
            .enumerate()
            .filter(|(_, matched)| {
                (!self.migrate.hide_unmatched || matched.target.is_some())
                    && (!self.migrate.hide_without_updates || migration_has_updates(matched))
            })
            .map(|(index, _)| index)
            .collect();
        if self.migrate.prioritize_chapters {
            indices.sort_by(|left, right| {
                let left_match = &self.migrate.matches[*left];
                let right_match = &self.migrate.matches[*right];
                latest_chapter_number(&right_match.target_chapters)
                    .total_cmp(&latest_chapter_number(&left_match.target_chapters))
                    .then_with(|| left_match.old.title.cmp(&right_match.old.title))
            });
        }
        indices
    }

    fn selected_migration_index(&self) -> Option<usize> {
        self.migration_visible_indices()
            .get(self.browse_selection)
            .copied()
    }

    pub fn toggle_migration_hide_unmatched(&mut self) {
        if self.migrate.screen != MigrateScreen::Matches {
            return;
        }
        self.migrate.hide_unmatched = !self.migrate.hide_unmatched;
        self.browse_selection = 0;
        self.select_browse_entry(0);
        self.remote.status = Some(if self.migrate.hide_unmatched {
            "Hiding unmatched manga".to_owned()
        } else {
            "Showing all matches".to_owned()
        });
        self.remote.error = false;
    }

    pub fn toggle_migration_prioritize(&mut self) {
        if !matches!(
            self.migrate.screen,
            MigrateScreen::Favorites | MigrateScreen::Matches
        ) {
            return;
        }
        self.migrate.prioritize_chapters = !self.migrate.prioritize_chapters;
        self.browse_selection = 0;
        self.select_browse_entry(0);
        self.remote.status = Some(if self.migrate.prioritize_chapters {
            "Prioritizing sources by latest chapter".to_owned()
        } else {
            "Title matching order restored".to_owned()
        });
        self.remote.error = false;
    }

    pub fn toggle_migration_hide_without_updates(&mut self) {
        if self.migrate.screen != MigrateScreen::Matches {
            return;
        }
        self.migrate.hide_without_updates = !self.migrate.hide_without_updates;
        self.browse_selection = 0;
        self.select_browse_entry(0);
        self.remote.status = Some(if self.migrate.hide_without_updates {
            "Hiding matches without newer chapters".to_owned()
        } else {
            "Showing matches without newer chapters".to_owned()
        });
        self.remote.error = false;
    }

    pub(crate) fn migrate_item_count(&self) -> usize {
        match self.migrate.screen {
            MigrateScreen::Sources => self.migrate_sources().len(),
            MigrateScreen::Favorites => self.migrate.favorites.len(),
            MigrateScreen::Matches => self.migration_visible_indices().len(),
        }
    }

    pub fn migrate_open(&mut self) {
        match self.migrate.screen {
            MigrateScreen::Sources => {
                let Some((source, _)) = self.migrate_sources().get(self.browse_selection).cloned()
                else {
                    return;
                };
                self.migrate.source_id = Some(source.id);
                self.migrate.favorites = self.migrate_favorites();
                self.migrate.screen = MigrateScreen::Favorites;
                self.browse_selection = 0;
                self.migrate.list_state = ListState::default();
                self.select_browse_entry(0);
            }
            MigrateScreen::Favorites => self.toggle_multi_select(),
            MigrateScreen::Matches => self.open_migration_search(),
        }
    }

    pub fn migrate_back(&mut self) {
        if self.migrate.matching {
            self.migration_generation = self.migration_generation.wrapping_add(1);
            self.migrate.matching = false;
        }
        match self.migrate.screen {
            MigrateScreen::Matches => {
                self.migrate.screen = MigrateScreen::Favorites;
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            MigrateScreen::Favorites => {
                self.migrate.screen = MigrateScreen::Sources;
                self.migrate.favorites.clear();
                self.browse_selection = 0;
                self.select_browse_entry(0);
            }
            MigrateScreen::Sources => {}
        }
    }

    pub fn migrate_refresh(&mut self) {
        self.migrate.favorites = self.migrate_favorites();
        self.clamp_browse_selection();
    }

    fn selected_migration_favorites(&self) -> Vec<RemoteFavorite> {
        let mut favorites = Vec::new();
        if let Some(keys) = self.selected_keys(SelectionScope::Migrate) {
            favorites = self
                .migrate
                .favorites
                .iter()
                .filter(|favorite| {
                    keys.contains(&format!("migrate:{}\n{}", favorite.source_id, favorite.url))
                })
                .cloned()
                .collect();
        }
        if favorites.is_empty() {
            if let Some(favorite) = self.migrate.favorites.get(self.browse_selection).cloned() {
                favorites.push(favorite);
            }
        }
        favorites
    }

    /// Enabled sources selected for migration. Empty configuration means all enabled.
    pub fn migration_targets(&self) -> Vec<RemoteSource> {
        if self.config.migration_sources.is_empty() {
            return self
                .remote
                .sources
                .iter()
                .filter(|source| self.source_enabled(source))
                .cloned()
                .collect();
        }
        self.config
            .migration_sources
            .iter()
            .filter_map(|key| {
                self.remote
                    .sources
                    .iter()
                    .find(|source| pin_key_matches(key, source) && self.source_enabled(source))
                    .cloned()
            })
            .collect()
    }

    /// Enabled sources in picker order: chosen sources first, then the rest by name.
    pub fn migration_source_list(&self) -> Vec<RemoteSource> {
        let mut ordered: Vec<RemoteSource> = self
            .migration_source_picker
            .as_ref()
            .map(|picker| {
                picker
                    .chosen
                    .iter()
                    .filter_map(|key| {
                        self.remote
                            .sources
                            .iter()
                            .find(|source| {
                                pin_key_matches(key, source) && self.source_enabled(source)
                            })
                            .cloned()
                    })
                    .collect()
            })
            .unwrap_or_default();
        for source in self
            .remote
            .sources
            .iter()
            .filter(|source| self.source_enabled(source))
        {
            if !ordered.iter().any(|known| known.id == source.id) {
                ordered.push(source.clone());
            }
        }
        ordered
    }

    pub fn open_migration_sources(&mut self) {
        if self.tab != 3 || self.browse_view != BrowseView::Migrate {
            return;
        }
        let chosen = self.config.migration_sources.clone();
        self.migration_source_picker = Some(MigrationSourcePicker {
            selected: 0,
            chosen,
        });
    }

    pub fn migration_sources_previous(&mut self) {
        if let Some(picker) = self.migration_source_picker.as_mut() {
            picker.selected = picker.selected.saturating_sub(1);
        }
    }

    pub fn migration_sources_next(&mut self) {
        let count = self.migration_source_list().len();
        if let Some(picker) = self.migration_source_picker.as_mut() {
            if count > 0 {
                picker.selected = (picker.selected + 1).min(count - 1);
            }
        }
    }

    pub fn migration_source_selected(&self, source: &RemoteSource) -> bool {
        self.migration_source_picker
            .as_ref()
            .is_some_and(|picker| picker.chosen.iter().any(|key| pin_key_matches(key, source)))
    }

    /// Move the selected target source up (`delta < 0`) or down in the chosen order.
    pub fn migration_sources_move(&mut self, delta: i32) {
        let list = self.migration_source_list();
        let selected = self
            .migration_source_picker
            .as_ref()
            .map_or(0, |picker| picker.selected);
        let Some(source) = list.get(selected) else {
            return;
        };
        let key = source_pin_key(source);
        let Some(picker) = self.migration_source_picker.as_mut() else {
            return;
        };
        let Some(position) = picker.chosen.iter().position(|candidate| candidate == &key) else {
            return;
        };
        let Some(target) = position
            .checked_add_signed(delta as isize)
            .filter(|target| *target < picker.chosen.len())
        else {
            return;
        };
        picker.chosen.swap(position, target);
        picker.selected = picker.selected.saturating_add_signed(delta as isize);
    }

    pub fn migration_sources_toggle(&mut self) {
        let Some(source) = self
            .migration_source_list()
            .get(
                self.migration_source_picker
                    .as_ref()
                    .map_or(0, |picker| picker.selected),
            )
            .cloned()
        else {
            return;
        };
        let count = self.migration_source_list().len();
        let Some(picker) = self.migration_source_picker.as_mut() else {
            return;
        };
        let key = source_pin_key(&source);
        if let Some(position) = picker.chosen.iter().position(|candidate| candidate == &key) {
            picker.chosen.remove(position);
            picker.selected = picker.selected.min(count.saturating_sub(1));
        } else {
            picker.chosen.push(key);
        }
    }

    pub fn migration_sources_all(&mut self, all: bool) {
        let keys: Vec<String> = if all {
            self.remote
                .sources
                .iter()
                .filter(|source| self.source_enabled(source))
                .map(source_pin_key)
                .collect()
        } else {
            Vec::new()
        };
        if let Some(picker) = self.migration_source_picker.as_mut() {
            picker.chosen = keys;
        }
    }

    pub fn cancel_migration_sources(&mut self) {
        self.migration_source_picker = None;
    }

    pub fn apply_migration_sources(&mut self) {
        let Some(picker) = self.migration_source_picker.take() else {
            return;
        };
        self.config.migration_sources = picker.chosen;
        self.persist_settings();
        self.remote.status = Some(if self.config.migration_sources.is_empty() {
            "Migration uses all enabled sources".to_owned()
        } else {
            format!(
                "Migration uses {} sources",
                self.config.migration_sources.len()
            )
        });
        self.remote.error = false;
    }

    pub fn migrate_start_matching(&mut self) {
        if self.migrate.matching {
            self.remote.status = Some("Matching is already running…".to_owned());
            self.remote.error = false;
            return;
        }
        if self.migrate.screen == MigrateScreen::Sources {
            self.migrate_open();
        }
        if self.migrate.screen != MigrateScreen::Favorites {
            return;
        }
        let favorites = self.selected_migration_favorites();
        if favorites.is_empty() {
            self.remote.status = Some("Select at least one manga to migrate.".to_owned());
            self.remote.error = false;
            return;
        }
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let targets = self.migration_targets();
        if targets.is_empty() {
            self.remote.status = Some("No enabled target sources.".to_owned());
            self.remote.error = false;
            return;
        }

        if let Some(worker) = self.migration_worker.as_ref() {
            self.migration_generation = self.migration_generation.wrapping_add(1);
            self.migrate.matching = true;
            let count = favorites.len();
            self.remote.status = Some(format!("Matching {count} manga…"));
            self.remote.error = false;
            worker.request(MigrationRequest {
                generation: self.migration_generation,
                client,
                favorites,
                targets,
                prioritize_chapters: self.migrate.prioritize_chapters,
            });
            return;
        }

        let mut matches = Vec::new();
        for favorite in favorites {
            let old_chapters = load_cached_story(&favorite.source_id, &favorite.url)
                .map(|story| story.chapters)
                .or_else(|| {
                    client
                        .chapters(&favorite.source_id, &favorite.url, None)
                        .ok()
                })
                .unwrap_or_default();
            let mut best: Option<(RemoteSource, RemoteManga, f64)> = None;
            for source in &targets {
                if source.id == favorite.source_id {
                    continue;
                }
                let mut source_best: Option<(RemoteManga, f64)> = None;
                let mut queries = vec![favorite.title.clone()];
                queries.extend(deep_queries(&favorite.title));
                queries.dedup();
                for query in queries {
                    let Ok(page) = client.search(&source.id, &query, 1) else {
                        continue;
                    };
                    let candidates: Vec<(usize, String)> = page
                        .mangas
                        .iter()
                        .enumerate()
                        .map(|(index, manga)| (index, manga.title.clone()))
                        .collect();
                    if let Some((index, score)) = best_match(&favorite.title, candidates) {
                        let candidate = page.mangas[index].clone();
                        if source_best.as_ref().is_none_or(|(_, known)| score > *known) {
                            source_best = Some((candidate, score));
                        }
                        if score >= 0.95 {
                            break;
                        }
                    }
                }
                if let Some((manga, score)) = source_best {
                    if best.as_ref().is_none_or(|(_, _, known)| score > *known) {
                        best = Some((source.clone(), manga, score));
                    }
                }
            }

            let mut matched = MigrationMatch {
                old: favorite.clone(),
                old_chapters,
                target: None,
                target_source: None,
                target_chapters: Vec::new(),
                score: 0.0,
                error: Some("No match found".to_owned()),
                skipped: false,
            };
            if let Some((source, target, score)) = best {
                match client.chapters(&source.id, &target.url, target.memo.as_deref()) {
                    Ok(chapters) if !chapters.is_empty() => {
                        matched.target = Some(target);
                        matched.target_source = Some(source);
                        matched.target_chapters = chapters;
                        matched.score = score;
                        matched.error = None;
                    }
                    Ok(_) => matched.error = Some("Target has no chapters".to_owned()),
                    Err(error) => matched.error = Some(error.to_string()),
                }
            }
            matches.push(matched);
        }

        self.migrate.matches = matches;
        self.migrate.screen = MigrateScreen::Matches;
        self.browse_selection = 0;
        self.migrate.list_state = ListState::default();
        self.select_browse_entry(0);
        let found = self
            .migrate
            .matches
            .iter()
            .filter(|matched| matched.target.is_some())
            .count();
        self.remote.status = Some(format!(
            "Matched {found}/{} selected manga",
            self.migrate.matches.len()
        ));
        self.remote.error = found == 0;
    }

    /// Apply completed background migration matching. Returns whether the UI changed.
    pub fn poll_migration_worker(&mut self) -> bool {
        let Some(event) = self
            .migration_worker
            .as_ref()
            .and_then(MigrationWorker::try_result)
        else {
            return false;
        };
        if event.generation != self.migration_generation {
            return false;
        }
        self.migrate.matching = false;
        self.migrate.matches = event.matches;
        self.migrate.screen = MigrateScreen::Matches;
        self.browse_selection = 0;
        self.migrate.list_state = ListState::default();
        self.select_browse_entry(0);
        let found = self
            .migrate
            .matches
            .iter()
            .filter(|matched| matched.target.is_some())
            .count();
        self.remote.status = Some(format!(
            "Matched {found}/{} selected manga",
            self.migrate.matches.len()
        ));
        self.remote.error = found == 0;
        true
    }

    pub fn open_migration_search(&mut self) {
        if self.tab != 3
            || self.browse_view != BrowseView::Migrate
            || self.migrate.screen != MigrateScreen::Matches
        {
            return;
        }
        let Some(index) = self.selected_migration_index() else {
            return;
        };
        let Some(title) = self
            .migrate
            .matches
            .get(index)
            .map(|matched| matched.old.title.clone())
        else {
            return;
        };
        self.open_text_input(TextAction::MigrationSearch, "Search target", &title);
    }

    fn run_migration_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        let Some(index) = self.selected_migration_index() else {
            return;
        };
        let Some(old) = self
            .migrate
            .matches
            .get(index)
            .map(|matched| matched.old.clone())
        else {
            return;
        };
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let mut results = Vec::new();
        for source in self.migration_targets() {
            if source.id == old.source_id {
                continue;
            }
            let Ok(page) = client.search(&source.id, query, 1) else {
                continue;
            };
            for manga in page.mangas {
                results.push(MigrationSearchResult {
                    score: title_similarity(&old.title, &manga.title),
                    source: source.clone(),
                    manga,
                });
            }
        }
        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(50);
        if results.is_empty() {
            self.remote.status = Some("No manual targets found.".to_owned());
            self.remote.error = true;
            return;
        }
        self.remote.status = Some(format!("{} manual targets", results.len()));
        self.remote.error = false;
        self.migration_search = Some(MigrationSearchState {
            results,
            selected: 0,
        });
    }

    pub fn migration_search_previous(&mut self) {
        if let Some(state) = self.migration_search.as_mut() {
            state.selected = state.selected.saturating_sub(1);
        }
    }

    pub fn migration_search_next(&mut self) {
        if let Some(state) = self.migration_search.as_mut() {
            if !state.results.is_empty() {
                state.selected = (state.selected + 1).min(state.results.len() - 1);
            }
        }
    }

    pub fn cancel_migration_search(&mut self) {
        self.migration_search = None;
    }

    pub fn confirm_migration_search(&mut self) {
        let Some(state) = self.migration_search.take() else {
            return;
        };
        let Some(result) = state.results.get(state.selected).cloned() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let chapters = client.chapters(
            &result.source.id,
            &result.manga.url,
            result.manga.memo.as_deref(),
        );
        let (message, error) = match chapters {
            Ok(chapters) if !chapters.is_empty() => {
                let Some(index) = self.selected_migration_index() else {
                    return;
                };
                let Some(matched) = self.migrate.matches.get_mut(index) else {
                    return;
                };
                matched.target = Some(result.manga.clone());
                matched.target_source = Some(result.source.clone());
                matched.target_chapters = chapters;
                matched.score = result.score;
                matched.error = None;
                matched.skipped = false;
                (
                    format!(
                        "Target set to {} ({})",
                        result.manga.title,
                        result.source.display_name()
                    ),
                    false,
                )
            }
            Ok(_) => ("Target has no chapters.".to_owned(), true),
            Err(error) => (error.to_string(), true),
        };
        self.remote.status = Some(message);
        self.remote.error = error;
    }

    pub fn migrate_toggle_skip(&mut self) {
        if self.migrate.screen != MigrateScreen::Matches {
            return;
        }
        let Some(index) = self.selected_migration_index() else {
            return;
        };
        if let Some(matched) = self.migrate.matches.get_mut(index) {
            matched.skipped = !matched.skipped;
            let skipped = matched.skipped;
            let title = matched.old.title.clone();
            self.remote.status = Some(if skipped {
                format!("Skipping {title}")
            } else {
                format!("Including {title}")
            });
            self.remote.error = false;
        }
    }

    pub fn migrate_apply(&mut self, replace: bool) {
        if self.migrate.matching
            || self.migrate.applying
            || self.migrate.screen != MigrateScreen::Matches
        {
            return;
        }
        let queue: Vec<usize> = self
            .migrate
            .matches
            .iter()
            .enumerate()
            .filter(|(_, matched)| matched.target.is_some() && !matched.skipped)
            .map(|(index, _)| index)
            .collect();
        if queue.is_empty() {
            self.remote.status = Some("No matched manga to migrate.".to_owned());
            self.remote.error = true;
            return;
        }
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        if self.host_client.is_none() {
            return;
        }

        self.migrate.applying = true;
        self.migrate.apply_replace = replace;
        self.migrate.apply_queue = queue;
        self.migrate.apply_position = 0;
        self.migrate.apply_done = 0;
        self.migrate.apply_failed = 0;
        self.migrate.apply_downloads_moved = 0;
        self.migrate.apply_downloads_failed = 0;
        self.migrate.apply_categories.clear();
        self.remote.error = false;
        self.update_migration_apply_status();
        // Do the first item now so the status changes as soon as the key is pressed.
        self.migrate_apply_step();
    }

    /// Process one matched manga per event-loop tick. This keeps progress visible and
    /// lets Esc cancel between items instead of freezing until every host call finishes.
    pub fn migrate_apply_step(&mut self) {
        if !self.migrate.applying {
            return;
        }
        if self.migrate.apply_position >= self.migrate.apply_queue.len() {
            self.finish_migration_apply(false);
            return;
        }

        let index = self.migrate.apply_queue[self.migrate.apply_position];
        self.migrate.apply_position += 1;
        let Some(matched) = self.migrate.matches.get(index).cloned() else {
            self.migrate.apply_failed += 1;
            self.update_migration_apply_status();
            return;
        };
        let Some(target) = matched.target.clone() else {
            self.migrate.apply_failed += 1;
            self.update_migration_apply_status();
            return;
        };
        let Some(target_source) = matched.target_source.clone() else {
            self.migrate.apply_failed += 1;
            self.update_migration_apply_status();
            return;
        };
        let Some(client) = self.host_client.clone() else {
            self.migrate.apply_failed += 1;
            self.update_migration_apply_status();
            return;
        };

        let old_categories = self
            .config
            .remote_library
            .iter()
            .find(|favorite| {
                favorite.source_id == matched.old.source_id && favorite.url == matched.old.url
            })
            .map(|favorite| favorite.categories.clone())
            .unwrap_or_default();
        if !self.is_favorite(&target.source_id, &target.url) {
            match client.add_favorite(&target) {
                Ok(()) => self.migrate.apply_categories.push((
                    target.source_id.clone(),
                    target.url.clone(),
                    old_categories,
                )),
                Err(_) => {
                    self.migrate.apply_failed += 1;
                    self.update_migration_apply_status();
                    return;
                }
            }
        }

        self.copy_migration_reading_state(&matched, self.migrate.apply_replace);
        if self.migrate.apply_replace {
            if let Some(root) = downloads_dir(&self.config) {
                let old_source = self
                    .remote_source_display_name(&matched.old.source_id)
                    .unwrap_or_else(|| matched.old.source_id.clone());
                match move_series_downloads(
                    &root,
                    &old_source,
                    &matched.old.title,
                    &target_source.display_name(),
                    &target.title,
                ) {
                    Ok(true) => self.migrate.apply_downloads_moved += 1,
                    Ok(false) => {}
                    Err(_) => self.migrate.apply_downloads_failed += 1,
                }
            }
            let _ = client.remove_favorite(&matched.old.source_id, &matched.old.url);
        }
        self.migrate.apply_done += 1;
        self.update_migration_apply_status();
    }

    fn update_migration_apply_status(&mut self) {
        let done = self.migrate.apply_done;
        let total = self.migrate.apply_queue.len();
        let mut status = format!(
            "{} {done}/{total} manga…",
            if self.migrate.apply_replace {
                "Migrating"
            } else {
                "Copying"
            }
        );
        if self.migrate.apply_failed > 0 {
            status.push_str(&format!(" · {} failed", self.migrate.apply_failed));
        }
        self.remote.status = Some(status);
    }

    /// Stop before the next item. Completed items stay migrated and are persisted.
    pub fn cancel_migration_apply(&mut self) {
        if self.migrate.applying {
            self.finish_migration_apply(true);
        }
    }

    fn finish_migration_apply(&mut self, cancelled: bool) {
        if !self.migrate.applying {
            return;
        }
        let replace = self.migrate.apply_replace;
        let done = self.migrate.apply_done;
        let failed = self.migrate.apply_failed;
        let total = self.migrate.apply_queue.len();
        let downloads_moved = self.migrate.apply_downloads_moved;
        let downloads_failed = self.migrate.apply_downloads_failed;
        let categories = std::mem::take(&mut self.migrate.apply_categories);
        self.migrate.applying = false;
        self.migrate.apply_queue.clear();
        self.migrate.apply_position = 0;

        self.load_favorites();
        for (source_id, url, categories) in categories {
            if categories.is_empty() {
                continue;
            }
            if let Some(favorite) = self
                .config
                .remote_library
                .iter_mut()
                .find(|favorite| favorite.source_id == source_id && favorite.url == url)
            {
                favorite.categories = categories;
            }
        }
        self.persist_settings();
        self.merge_remote_library();
        let _ = self.history.save();
        let _ = self.progress.save();
        let _ = self.bookmarks.save();
        self.multi_select = None;
        self.migrate.screen = MigrateScreen::Sources;
        self.migrate.source_id = None;
        self.migrate.favorites.clear();
        self.migrate.matches.clear();
        self.browse_selection = 0;
        self.select_browse_entry(0);

        let mut status = if cancelled {
            format!("Migration cancelled · {done}/{total} manga")
        } else {
            format!(
                "{} {done} manga",
                if replace { "Migrated" } else { "Copied" }
            )
        };
        if failed > 0 {
            status.push_str(&format!(" · {failed} failed"));
        }
        if downloads_moved > 0 {
            status.push_str(&format!(" · {downloads_moved} download sets moved"));
        }
        if downloads_failed > 0 {
            status.push_str(&format!(" · {downloads_failed} download moves failed"));
        }
        self.remote.status = Some(status);
        self.remote.error = !cancelled && (failed > 0 || done == 0);
    }

    fn copy_migration_reading_state(&mut self, matched: &MigrationMatch, replace: bool) {
        let Some(target) = &matched.target else {
            return;
        };
        let old_series = format!("remote:{}\n{}", matched.old.source_id, matched.old.url);
        let old_entries: Vec<(String, HistoryEntry)> = self
            .history
            .recent()
            .into_iter()
            .filter(|(_, entry)| entry.series_key() == old_series)
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();

        let target_by_number: BTreeMap<u32, &RemoteChapter> = matched
            .target_chapters
            .iter()
            .filter(|chapter| chapter.chapter_number >= 0.0)
            .map(|chapter| (chapter.chapter_number.to_bits(), chapter))
            .collect();
        let target_by_name: BTreeMap<&str, &RemoteChapter> = matched
            .target_chapters
            .iter()
            .map(|chapter| (chapter.name.as_str(), chapter))
            .collect();

        for (old_key, entry) in old_entries {
            let Some(old_remote) = &entry.remote else {
                continue;
            };
            let old_chapter = &old_remote.chapter;
            let target_chapter = target_by_number
                .get(&old_chapter.chapter_number.to_bits())
                .copied()
                .or_else(|| target_by_name.get(old_chapter.name.as_str()).copied());
            let Some(target_chapter) = target_chapter else {
                continue;
            };
            let new_key = remote_progress_key(&target.source_id, &target.url, &target_chapter.url);
            if let Some(progress) = self.progress.entry(Path::new(&old_key)) {
                self.progress
                    .update(&new_key, progress.page, progress.pages);
            }
            if self.bookmarks.is_bookmarked(Path::new(&old_key)) {
                self.bookmarks.set(&new_key, true);
            }
            let source_name = matched
                .target_source
                .as_ref()
                .map(RemoteSource::display_name)
                .unwrap_or_else(|| entry.source.clone());
            let new_entry = HistoryEntry::remote(
                target.title.clone(),
                target_chapter.name.clone(),
                source_name,
                target.clone(),
                target_chapter.clone(),
                entry.page,
                entry.pages,
            );
            let mut new_entry = new_entry;
            if replace {
                new_entry.read_duration_secs = entry.read_duration_secs;
                new_entry.daily_duration_secs = entry.daily_duration_secs.clone();
            }
            self.history
                .record(new_key.to_string_lossy().into_owned(), new_entry);
        }

        if replace {
            self.history.remove_series(&old_series);
            for chapter in &matched.old_chapters {
                let key =
                    remote_progress_key(&matched.old.source_id, &matched.old.url, &chapter.url);
                self.progress.remove(&key);
                self.bookmarks.set(&key, false);
            }
        }
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
        self.prefs_generation = self.prefs_generation.wrapping_add(1);
        if let Some(worker) = self.prefs_worker.as_ref() {
            self.source_prefs = None;
            self.source_prefs_pending = Some(source.clone());
            self.browse_view = BrowseView::SourcePrefs;
            self.browse_selection = 0;
            self.remote.status = Some(format!("Loading {} preferences…", source.display_name()));
            self.remote.error = false;
            worker.request(PrefsRequest {
                generation: self.prefs_generation,
                source_id: source.id,
                client,
            });
            return;
        }
        match client.source_preferences(&source.id) {
            Ok(preferences) => self.apply_source_preferences(source, preferences),
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Apply completed background preference loads. Returns whether the UI changed.
    pub fn poll_prefs_worker(&mut self) -> bool {
        let Some(event) = self.prefs_worker.as_ref().and_then(PrefsWorker::try_result) else {
            return false;
        };
        if event.generation != self.prefs_generation {
            return false;
        }
        let Some(source) = self.source_prefs_pending.take() else {
            return false;
        };
        if source.id != event.source_id {
            return false;
        }
        match event.result {
            Ok(preferences) => self.apply_source_preferences(source, preferences),
            Err(error) => {
                self.source_prefs = None;
                self.remote.status = Some(error);
                self.remote.error = true;
            }
        }
        true
    }

    fn apply_source_preferences(&mut self, source: RemoteSource, preferences: SourcePreferences) {
        if !preferences.configurable {
            self.source_prefs = None;
            self.remote.status = Some(format!(
                "{} has no source preferences.",
                source.display_name()
            ));
            self.remote.error = false;
            self.browse_view = BrowseView::Sources;
            return;
        }
        if let Some(error) = &preferences.error {
            self.source_prefs = None;
            self.set_remote_error(format!("{} preferences: {error}", source.display_name()));
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
    /// Preload source filters without blocking the source/results transition.
    pub fn load_remote_filters_async(&mut self) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        self.filters_generation = self.filters_generation.wrapping_add(1);
        if let Some(worker) = self.filters_worker.as_ref() {
            worker.request(FiltersRequest {
                generation: self.filters_generation,
                source_id: source.id,
                client,
            });
            return;
        }
        self.load_remote_filters();
    }

    /// Apply completed background filter loads. Returns whether the UI changed.
    pub fn poll_filters_worker(&mut self) -> bool {
        let Some(event) = self
            .filters_worker
            .as_ref()
            .and_then(FiltersWorker::try_result)
        else {
            return false;
        };
        if event.generation != self.filters_generation
            || self.remote.source.as_ref().map(|source| source.id.as_str())
                != Some(event.source_id.as_str())
        {
            return false;
        }
        match event.result {
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
        true
    }

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

    fn matching_extension_indices(&self) -> Vec<usize> {
        let terms: Vec<String> = self
            .extension_query
            .split(',')
            .map(|term| term.trim().to_lowercase())
            .filter(|term| !term.is_empty())
            .collect();
        let languages = self.config.enabled_languages.as_ref();
        self.remote
            .extensions
            .iter()
            .enumerate()
            .filter(|(_, extension)| {
                if extension.nsfw && !self.config.show_nsfw_extensions {
                    return false;
                }
                if !extension.installed
                    && languages.is_some_and(|languages| {
                        !extension.lang.is_empty() && !languages.contains(&extension.lang)
                    })
                {
                    return false;
                }
                terms.is_empty()
                    || terms.iter().all(|term| {
                        extension.name.to_lowercase().contains(term)
                            || extension.pkg.to_lowercase().contains(term)
                            || extension.lang.to_lowercase().contains(term)
                            || extension.sources.iter().any(|source| {
                                source.name.to_lowercase().contains(term)
                                    || source.lang.to_lowercase().contains(term)
                                    || source.id.to_lowercase().contains(term)
                                    || source.base_url.to_lowercase().contains(term)
                            })
                    })
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn matching_not_loaded_indices(&self) -> Vec<usize> {
        let terms: Vec<String> = self
            .extension_query
            .split(',')
            .map(|term| term.trim().to_lowercase())
            .filter(|term| !term.is_empty())
            .collect();
        self.remote
            .not_loaded
            .iter()
            .enumerate()
            .filter(|(_, extension)| {
                terms.is_empty()
                    || terms.iter().all(|term| {
                        extension.name.to_lowercase().contains(term)
                            || extension.pkg.to_lowercase().contains(term)
                            || extension.file.to_lowercase().contains(term)
                            || extension.reason.to_lowercase().contains(term)
                    })
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Grouped extension rows: Updates, Installed, then available extensions by language.
    pub fn extension_rows(&self) -> Vec<ExtensionRow> {
        let matching = self.matching_extension_indices();
        let mut updates = Vec::new();
        let mut installed = Vec::new();
        let mut available = Vec::new();
        for index in matching {
            let extension = &self.remote.extensions[index];
            if extension.update_available() {
                updates.push(index);
            } else if extension.installed {
                installed.push(index);
            } else {
                available.push(index);
            }
        }
        for list in [&mut updates, &mut installed] {
            list.sort_by(|left, right| {
                natural_cmp(
                    &self.remote.extensions[*left].name,
                    &self.remote.extensions[*right].name,
                )
            });
        }
        available.sort_by(|left, right| {
            natural_cmp(
                &language_label(&self.remote.extensions[*left].lang),
                &language_label(&self.remote.extensions[*right].lang),
            )
            .then_with(|| {
                natural_cmp(
                    &self.remote.extensions[*left].name,
                    &self.remote.extensions[*right].name,
                )
            })
        });

        let mut rows = Vec::new();
        if !updates.is_empty() {
            rows.push(ExtensionRow::Header("Updates".to_owned()));
            rows.extend(updates.into_iter().map(ExtensionRow::Extension));
        }
        let not_loaded = self.matching_not_loaded_indices();
        if !not_loaded.is_empty() {
            rows.push(ExtensionRow::Header("Not loaded".to_owned()));
            rows.extend(not_loaded.into_iter().map(ExtensionRow::NotLoaded));
        }
        if !installed.is_empty() {
            rows.push(ExtensionRow::Header("Installed".to_owned()));
            rows.extend(installed.into_iter().map(ExtensionRow::Extension));
        }
        let mut current_lang: Option<String> = None;
        for index in available {
            let lang = self.remote.extensions[index].lang.clone();
            if current_lang.as_deref() != Some(lang.as_str()) {
                rows.push(ExtensionRow::Header(language_label(&lang)));
                current_lang = Some(lang);
            }
            rows.push(ExtensionRow::Extension(index));
        }
        rows
    }

    pub fn filtered_extension_indices(&self) -> Vec<usize> {
        self.extension_rows()
            .into_iter()
            .filter_map(|row| match row {
                ExtensionRow::Extension(index) => Some(index),
                ExtensionRow::NotLoaded(_) | ExtensionRow::Header(_) => None,
            })
            .collect()
    }

    pub fn filtered_extension_count(&self) -> usize {
        self.extension_selection_rows().len()
    }

    fn extension_selection_rows(&self) -> Vec<ExtensionRow> {
        self.extension_rows()
            .into_iter()
            .filter(|row| !matches!(row, ExtensionRow::Header(_)))
            .collect()
    }

    pub fn selected_not_loaded(&self) -> Option<&NotLoadedExtension> {
        match self.extension_selection_rows().get(self.browse_selection)? {
            ExtensionRow::NotLoaded(index) => self.remote.not_loaded.get(*index),
            _ => None,
        }
    }

    pub fn selected_extension_row(&self) -> Option<usize> {
        let mut selection = 0;
        for (row, item) in self.extension_rows().iter().enumerate() {
            if matches!(item, ExtensionRow::Header(_)) {
                continue;
            }
            if selection == self.browse_selection {
                return Some(row);
            }
            selection += 1;
        }
        None
    }

    pub fn select_extension_row(&mut self, row: usize) {
        let offset = self.browse_list_state.offset();
        if let Some(index) = self.extension_index_for_row(offset + row) {
            self.select_browse_index(index);
        }
    }

    fn extension_index_for_row(&self, row: usize) -> Option<usize> {
        let mut selection = 0;
        for (index, item) in self.extension_rows().iter().enumerate() {
            if matches!(item, ExtensionRow::Header(_)) {
                continue;
            }
            if index == row {
                return Some(selection);
            }
            selection += 1;
        }
        None
    }

    pub fn load_favorites(&mut self) {
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        match client.favorites() {
            Ok(favorites) => self.apply_favorites(favorites),
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Load favorites without blocking when a worker is available.
    pub fn load_favorites_async(&mut self) {
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        self.favorites_generation = self.favorites_generation.wrapping_add(1);
        if let Some(worker) = self.favorites_worker.as_ref() {
            worker.request(FavoritesRequest {
                generation: self.favorites_generation,
                client,
                mutation: None,
                label: None,
                added: Vec::new(),
            });
            return;
        }
        self.load_favorites();
    }

    fn enqueue_favorites_mutation(
        &mut self,
        mutation: FavoritesMutation,
        label: String,
        added: Vec<RemoteManga>,
    ) -> bool {
        if self.host_client.is_none() && !self.ensure_host() {
            return false;
        }
        let (Some(worker), Some(client)) =
            (self.favorites_worker.as_ref(), self.host_client.clone())
        else {
            self.remote.status =
                Some("The extension host is starting; try again shortly.".to_owned());
            self.remote.error = false;
            return false;
        };
        self.favorites_generation = self.favorites_generation.wrapping_add(1);
        worker.request(FavoritesRequest {
            generation: self.favorites_generation,
            client,
            mutation: Some(mutation),
            label: Some(label),
            added,
        });
        true
    }

    /// Apply a completed background favorites load. Returns whether the UI changed.
    pub fn poll_favorites_worker(&mut self) -> bool {
        let Some(event) = self
            .favorites_worker
            .as_ref()
            .and_then(FavoritesWorker::try_result)
        else {
            return false;
        };
        if event.generation != self.favorites_generation {
            return false;
        }
        match event.result {
            Ok(favorites) => {
                self.apply_favorites(favorites);
                if let Some(label) = event.label {
                    self.remote.status = Some(label);
                    self.remote.error = false;
                }
            }
            Err(error) => {
                for manga in &event.added {
                    self.pending_favorite_categories
                        .remove(&remote_story_key(&manga.source_id, &manga.url));
                }
                self.remote.status = Some(error);
                self.remote.error = true;
            }
        }
        true
    }

    fn apply_favorites(&mut self, mut favorites: Vec<RemoteFavorite>) {
        let existing: std::collections::HashMap<(String, String), Vec<String>> = self
            .config
            .remote_library
            .iter()
            .map(|favorite| {
                (
                    (favorite.source_id.clone(), favorite.url.clone()),
                    favorite.categories.clone(),
                )
            })
            .collect();
        let default_category = self.favorite_category();
        for favorite in &mut favorites {
            let key = remote_story_key(&favorite.source_id, &favorite.url);
            if let Some(categories) = self.pending_favorite_categories.remove(&key) {
                favorite.categories = categories;
                continue;
            }
            if favorite.categories.is_empty() {
                if let Some(categories) =
                    existing.get(&(favorite.source_id.clone(), favorite.url.clone()))
                {
                    favorite.categories = categories.clone();
                } else {
                    favorite.categories = vec![default_category.clone()];
                }
            }
        }
        for favorite in &favorites {
            self.removed_stories
                .remove(&remote_story_key(&favorite.source_id, &favorite.url));
        }
        self.remote.favorites = favorites;
        self.remote.favorites_loaded = true;
        // Persist without thumbnails: their proxy URLs contain an ephemeral port.
        let mut persisted = self.remote.favorites.clone();
        for favorite in &mut persisted {
            favorite.thumbnail_url = None;
        }
        self.config.remote_library = persisted;
        self.persist_settings();
        if self.updates.retain_favorites(&self.config.remote_library) > 0 {
            let _ = self.updates.save();
        }
        self.merge_remote_library();
        self.cache_favorite_covers();
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
                .visible_remote_result(self.browse_selection)
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
        if mangas.len() == 1 {
            self.request_save_manga(mangas[0].clone());
            return;
        }
        let all_saved = mangas
            .iter()
            .all(|manga| self.is_favorite(&manga.source_id, &manga.url));
        let (mutation, added, label) = if all_saved {
            let count = mangas.len();
            (
                FavoritesMutation::RemoveMany(
                    mangas
                        .iter()
                        .map(|manga| (manga.source_id.clone(), manga.url.clone()))
                        .collect(),
                ),
                Vec::new(),
                format!("Removing {count} manga from the library…"),
            )
        } else {
            let added: Vec<RemoteManga> = mangas
                .iter()
                .filter(|manga| !self.is_favorite(&manga.source_id, &manga.url))
                .cloned()
                .collect();
            let count = added.len();
            (
                FavoritesMutation::AddMany(added.clone()),
                added,
                format!("Adding {count} manga to the library…"),
            )
        };
        if self.enqueue_favorites_mutation(mutation, label, added) {
            self.multi_select = None;
        }
    }

    pub fn toggle_current_favorite(&mut self) {
        if let Some(manga) = self.remote.manga.clone() {
            self.request_save_manga(manga);
        }
    }

    /// Save a remote manga, or ask before removing it or adding a duplicate.
    fn request_save_manga(&mut self, manga: RemoteManga) {
        if self.is_favorite(&manga.source_id, &manga.url) {
            self.remove_favorite = Some(manga);
            return;
        }
        let normalized = normalized_title(&manga.title);
        let duplicates: Vec<RemoteFavorite> = self
            .remote
            .favorites
            .iter()
            .filter(|favorite| normalized_title(&favorite.title) == normalized)
            .filter(|favorite| favorite.source_id != manga.source_id || favorite.url != manga.url)
            .cloned()
            .collect();
        if duplicates.is_empty() {
            self.open_save_categories(manga);
        } else {
            self.duplicate_dialog = Some(DuplicateDialog {
                manga,
                duplicates,
                selected: 0,
            });
        }
    }

    fn open_save_categories(&mut self, manga: RemoteManga) {
        let default = self.favorite_category();
        let mut chosen = HashSet::new();
        chosen.insert(default);
        self.save_categories = Some(SaveCategories {
            manga,
            selected: 0,
            chosen,
        });
    }

    pub fn save_categories_previous(&mut self) {
        if let Some(dialog) = self.save_categories.as_mut() {
            dialog.selected = dialog.selected.saturating_sub(1);
        }
    }

    pub fn save_categories_next(&mut self) {
        let count = self.library.categories.len();
        if let Some(dialog) = self.save_categories.as_mut() {
            if count > 0 {
                dialog.selected = (dialog.selected + 1).min(count - 1);
            }
        }
    }

    pub fn save_categories_toggle(&mut self) {
        let Some(dialog) = self.save_categories.as_mut() else {
            return;
        };
        let Some(category) = self.library.categories.get(dialog.selected).cloned() else {
            return;
        };
        if !dialog.chosen.remove(&category) {
            dialog.chosen.insert(category);
        }
    }

    pub fn cancel_save_categories(&mut self) {
        self.save_categories = None;
    }

    pub fn confirm_save_categories(&mut self) {
        let Some(dialog) = self.save_categories.take() else {
            return;
        };
        let mut categories: Vec<String> = dialog.chosen.into_iter().collect();
        categories.sort();
        let manga = dialog.manga.clone();
        let key = remote_story_key(&manga.source_id, &manga.url);
        self.pending_favorite_categories
            .insert(key.clone(), categories);
        if !self.add_manga_to_library(manga) {
            self.pending_favorite_categories.remove(&key);
        }
    }

    pub fn add_manga_to_library(&mut self, manga: RemoteManga) -> bool {
        let label = format!("Added {} to library", manga.title);
        self.enqueue_favorites_mutation(FavoritesMutation::Add(manga.clone()), label, vec![manga])
    }

    pub fn duplicate_previous(&mut self) {
        if let Some(dialog) = self.duplicate_dialog.as_mut() {
            dialog.selected = dialog.selected.saturating_sub(1);
        }
    }

    pub fn duplicate_next(&mut self) {
        if let Some(dialog) = self.duplicate_dialog.as_mut() {
            if !dialog.duplicates.is_empty() {
                dialog.selected = (dialog.selected + 1).min(dialog.duplicates.len() - 1);
            }
        }
    }

    pub fn duplicate_add_anyway(&mut self) {
        let Some(dialog) = self.duplicate_dialog.take() else {
            return;
        };
        self.add_manga_to_library(dialog.manga);
    }

    pub fn duplicate_open(&mut self) {
        let Some(dialog) = self.duplicate_dialog.take() else {
            return;
        };
        let Some(favorite) = dialog.duplicates.get(dialog.selected).cloned() else {
            return;
        };
        let source = self
            .remote
            .sources
            .iter()
            .find(|source| source.id == favorite.source_id)
            .cloned();
        if source.is_some() {
            self.remote.source = source;
        }
        self.remote.manga = Some(RemoteManga {
            source_id: favorite.source_id,
            url: favorite.url,
            title: favorite.title,
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: favorite.thumbnail_url,
            memo: None,
        });
        self.remote.remote_details_origin = match self.browse_view {
            BrowseView::RemoteResults | BrowseView::GlobalResults => self.browse_view,
            _ => BrowseView::RemoteResults,
        };
        self.browse_view = BrowseView::RemoteDetails;
        self.browse_selection = 0;
        self.load_remote_details();
        self.load_remote_chapters();
        self.select_browse_entry(0);
    }

    pub fn duplicate_cancel(&mut self) {
        self.duplicate_dialog = None;
    }

    pub fn confirm_remove_favorite(&mut self) {
        let Some(manga) = self.remove_favorite.take() else {
            return;
        };
        self.enqueue_favorites_mutation(
            FavoritesMutation::Remove {
                source_id: manga.source_id.clone(),
                url: manga.url.clone(),
            },
            format!("Removed {} from library", manga.title),
            Vec::new(),
        );
    }

    pub fn cancel_remove_favorite(&mut self) {
        self.remove_favorite = None;
    }

    fn favorite_category(&self) -> String {
        self.config
            .default_category
            .clone()
            .filter(|category| {
                self.library
                    .categories
                    .iter()
                    .any(|known| known == category)
            })
            .unwrap_or_else(|| "Default".to_owned())
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

    pub fn toggle_global_search_only_results(&mut self) {
        self.config.global_search_only_results = !self.config.global_search_only_results;
        self.persist_settings();
        self.remote.status = Some(if self.config.global_search_only_results {
            "Showing only sources with results".to_owned()
        } else {
            "Showing empty and failed sources".to_owned()
        });
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
        self.remote.global_groups = sources
            .iter()
            .map(|source| GlobalGroup {
                source: source.clone(),
                mangas: Vec::new(),
                error: None,
                loading: true,
            })
            .collect();
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
                if let Some(group) = self
                    .remote
                    .global_groups
                    .iter_mut()
                    .find(|group| group.source.id == source.id)
                {
                    group.mangas = mangas;
                    group.error = None;
                    group.loading = false;
                } else if !mangas.is_empty() || !self.config.global_search_only_results {
                    self.remote.global_groups.push(GlobalGroup {
                        source,
                        mangas,
                        error: None,
                        loading: false,
                    });
                }
                if self.config.global_search_only_results {
                    self.remote
                        .global_groups
                        .retain(|group| group.loading || !group.mangas.is_empty());
                }
                self.sort_global_groups();
                self.remote.global_finished += 1;
                self.update_global_search_status();
                true
            }
            GlobalSearchEvent::Error {
                generation,
                source,
                message,
            } if generation == self.remote.global_generation => {
                if let Some(group) = self
                    .remote
                    .global_groups
                    .iter_mut()
                    .find(|group| group.source.id == source.id)
                {
                    group.mangas.clear();
                    group.error = Some(message.clone());
                    group.loading = false;
                } else if !self.config.global_search_only_results {
                    self.remote.global_groups.push(GlobalGroup {
                        source: source.clone(),
                        mangas: Vec::new(),
                        error: Some(message.clone()),
                        loading: false,
                    });
                }
                if self.config.global_search_only_results {
                    self.remote
                        .global_groups
                        .retain(|group| group.loading || group.error.is_none());
                }
                self.sort_global_groups();
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
                for group in &mut self.remote.global_groups {
                    group.loading = false;
                }
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

    /// Pinned sources first, normal results next, empty/error groups last.
    fn sort_global_groups(&mut self) {
        let pinned = self.config.pinned_sources.clone();
        self.remote
            .global_groups
            .sort_by(|left, right| global_group_cmp(left, right, &pinned));
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

    pub fn retry_global_search(&mut self) {
        let query = self.remote.global_query.clone();
        if query.trim().is_empty() {
            self.remote.status = Some("Run a global search first (/)".to_owned());
            self.remote.error = false;
            return;
        }
        self.global_search(&query);
    }

    /// Open the selected global-search source with the global query filled in.
    pub fn global_open_group(&mut self) {
        let Some(group) = self
            .remote
            .global_groups
            .get(self.remote.global_group)
            .cloned()
        else {
            return;
        };
        let query = self.remote.global_query.clone();
        self.record_source_opened_source(&group.source);
        self.remote.source = Some(group.source);
        self.remote.query = query;
        self.remote.listing = RemoteListing::Search;
        self.remote.page = 1;
        self.remote.filters.clear();
        self.remote.default_filters.clear();
        self.remote.filters_loaded = false;
        self.load_remote_filters();
        self.load_remote_results(RemoteListing::Search);
        self.browse_view = BrowseView::RemoteResults;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
    }

    fn record_source_opened_source(&mut self, source: &RemoteSource) {
        if self.config.incognito {
            return;
        }
        let key = source_pin_key(source);
        if self.config.last_used_source.as_deref() != Some(key.as_str()) {
            self.config.last_used_source = Some(key);
            self.persist_settings();
        }
    }

    /// Base URL for the selected source, resolved from loaded extension metadata.
    pub fn selected_source_base_url(&self) -> Option<String> {
        let source_id = if self.browse_view == BrowseView::ExtensionDetails {
            self.extension_details
                .as_ref()
                .and_then(|state| state.sources.get(self.browse_selection))
                .map(|source| source.id.clone())
        } else {
            self.selected_source_ref().and_then(|source| match source {
                SourceRef::Local => None,
                SourceRef::Remote(index) => self
                    .remote
                    .sources
                    .get(index)
                    .map(|source| source.id.clone()),
            })
        }?;
        self.remote
            .extensions
            .iter()
            .flat_map(|extension| extension.sources.iter())
            .find(|source| source.id == source_id && !source.base_url.is_empty())
            .map(|source| source.base_url.clone())
    }

    pub fn open_selected_source_home(&mut self) {
        if self.browse_view == BrowseView::Sources
            && matches!(self.selected_source_ref(), Some(SourceRef::Local))
        {
            let path = self.library.root.display().to_string();
            match open_external_target(&path) {
                Ok(()) => {
                    self.remote.status = Some(format!("Opened {path}"));
                    self.remote.error = false;
                }
                Err(error) => {
                    self.remote.status = Some(format!("Could not open {path}: {error}"));
                    self.remote.error = true;
                }
            }
            return;
        }
        let Some(url) = self.selected_source_base_url() else {
            self.remote.status =
                Some("Source URL is unavailable; open the extension details instead.".to_owned());
            self.remote.error = true;
            return;
        };
        match open_external_target(&url) {
            Ok(()) => {
                self.remote.status = Some(format!("Opened {url}"));
                self.remote.error = false;
            }
            Err(error) => {
                self.remote.status = Some(format!("Could not open {url}: {error}"));
                self.remote.error = true;
            }
        }
    }

    pub fn copy_store_url(&mut self) {
        let Some(store) = self.selected_store().cloned() else {
            return;
        };
        match write_osc52_clipboard(&store) {
            Ok(()) => self.remote.status = Some(format!("Copied {store}")),
            Err(error) => self.remote.status = Some(format!("Could not copy: {error}")),
        }
        self.remote.error = false;
    }

    pub fn open_store_url(&mut self) {
        let Some(store) = self.selected_store().cloned() else {
            return;
        };
        if !(store.starts_with("http://") || store.starts_with("https://")) {
            self.remote.status = Some("This store URL is not an absolute web URL.".to_owned());
            self.remote.error = true;
            return;
        }
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match std::process::Command::new(opener).arg(&store).spawn() {
            Ok(_) => self.remote.status = Some(format!("Opened {store}")),
            Err(error) => self.remote.status = Some(format!("Could not open {store}: {error}")),
        }
        self.remote.error = false;
    }

    pub fn global_open(&mut self) {
        self.multi_select = None;
        let Some(group) = self.remote.global_groups.get(self.remote.global_group) else {
            return;
        };
        let Some(manga) = group.mangas.get(self.remote.global_item).cloned() else {
            return;
        };
        self.open_remote_story_async(manga, BrowseView::GlobalResults);
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
        match HostProcess::start(&runtime, &data_dir, self.config.flaresolverr_url.as_deref()) {
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
                self.load_remote_states();
            }
            Err(error) => self.set_remote_error(error),
        }
    }

    /// Load the host source list without blocking when a worker is available.
    pub fn load_remote_sources_async(&mut self) {
        if !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        self.source_generation = self.source_generation.wrapping_add(1);
        if let Some(worker) = self.source_worker.as_ref() {
            self.remote.loading = true;
            self.remote.status = Some("Loading sources…".to_owned());
            self.remote.error = false;
            worker.request(SourceRequest {
                generation: self.source_generation,
                client,
            });
            return;
        }
        self.load_remote_sources();
    }

    /// Apply a completed background source-list load. Returns whether the UI changed.
    pub fn poll_source_worker(&mut self) -> bool {
        let Some(event) = self
            .source_worker
            .as_ref()
            .and_then(SourceWorker::try_result)
        else {
            return false;
        };
        if event.generation != self.source_generation {
            return false;
        }
        self.remote.loading = false;
        match event.result {
            Ok(mut sources) => {
                sources.sort_by(|left, right| {
                    natural_cmp(&left.display_name(), &right.display_name())
                });
                self.remote.sources = sources;
                self.remote.status = None;
                self.remote.error = false;
                if !self.remote.favorites_loaded {
                    self.load_favorites_async();
                }
                self.load_remote_states();
            }
            Err(error) => {
                self.remote.status = Some(error);
                self.remote.error = true;
            }
        }
        self.clamp_browse_selection();
        true
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
        self.remote.results.clear();
        self.remote.has_next_page = false;
        self.cover_lookups.clear();
        self.failed_covers.clear();
        self.result_generation = self.result_generation.wrapping_add(1);
        if let Some(worker) = self.result_worker.as_ref() {
            self.remote.loading = true;
            self.remote.status = Some("Loading results…".to_owned());
            self.remote.error = false;
            worker.request(ResultRequest {
                generation: self.result_generation,
                client,
                source,
                listing: match listing {
                    RemoteListing::Popular => ResultListing::Popular,
                    RemoteListing::Latest => ResultListing::Latest,
                    RemoteListing::Search => ResultListing::Search,
                },
                query: self.remote.query.clone(),
                filters: self.remote.filters.clone(),
                page: 1,
                append: false,
            });
            return;
        }
        match self.fetch_remote_page(&client, &source, 1) {
            Ok(result) => {
                self.remote.results = result.mangas;
                self.remote.has_next_page = result.has_next_page;
                self.remote.status = None;
                self.remote.error = false;
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
        if self.remote.loading {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(source) = self.remote.source.clone() else {
            return;
        };
        let next_page = self.remote.page.saturating_add(1).max(2);
        self.result_generation = self.result_generation.wrapping_add(1);
        if let Some(worker) = self.result_worker.as_ref() {
            self.remote.loading = true;
            self.remote.status = Some(format!("Loading page {next_page}…"));
            worker.request(ResultRequest {
                generation: self.result_generation,
                client,
                source,
                listing: match self.remote.listing {
                    RemoteListing::Popular => ResultListing::Popular,
                    RemoteListing::Latest => ResultListing::Latest,
                    RemoteListing::Search => ResultListing::Search,
                },
                query: self.remote.query.clone(),
                filters: self.remote.filters.clone(),
                page: next_page,
                append: true,
            });
            return;
        }
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

    /// Apply a completed background result page. Returns whether the UI changed.
    pub fn poll_result_worker(&mut self) -> bool {
        let mut changed = false;
        while let Some(event) = self
            .result_worker
            .as_ref()
            .and_then(ResultWorker::try_result)
        {
            if event.generation != self.result_generation {
                continue;
            }
            self.remote.loading = false;
            changed = true;
            match event.result {
                Ok(page) => {
                    if event.append {
                        self.append_remote_results(page.mangas);
                    } else {
                        self.remote.results = page.mangas;
                    }
                    self.remote.page = event.page;
                    self.remote.has_next_page = page.has_next_page;
                    self.remote.status = None;
                    self.remote.error = false;
                    self.refresh_download_marks();
                }
                Err(error) => {
                    self.remote.status = Some(error);
                    self.remote.error = true;
                }
            }
        }
        changed
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
    /// Open a remote story immediately and fetch details/chapters on the story worker.
    fn open_remote_story_async(&mut self, manga: RemoteManga, origin: BrowseView) {
        let key = remote_story_key(&manga.source_id, &manga.url);
        self.remote.manga = Some(manga.clone());
        self.remote.chapters.clear();
        self.remote.downloaded.clear();
        self.story_updated_at = None;
        self.remote.remote_details_origin = origin;
        self.browse_view = BrowseView::RemoteDetails;
        self.browse_selection = 0;
        self.select_browse_entry(0);
        self.remote.status = Some("Loading details…".to_owned());
        self.remote.error = false;
        if let (Some(worker), Some(client)) = (self.story_worker.as_ref(), self.host_client.clone())
        {
            worker.request(StoryRequest {
                key: key.clone(),
                manga,
                client,
            });
            self.story_refresh = Some(key.clone());
            self.story_refreshed.insert(key);
            return;
        }
        self.load_remote_details();
        self.load_remote_chapters();
        self.remote.status = None;
    }

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
        self.load_remote_chapters_with(false);
    }

    fn load_remote_chapters_with(&mut self, auto_download: bool) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let Some(manga) = self.remote.manga.clone() else {
            return;
        };
        let old_urls: HashSet<String> = if auto_download {
            self.remote
                .chapters
                .iter()
                .map(|chapter| chapter.url.clone())
                .collect()
        } else {
            HashSet::new()
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
                if let Some(manga) = self.remote.manga.clone() {
                    self.auto_download_new_chapters(
                        &manga,
                        &old_urls,
                        &self.remote.chapters.clone(),
                    );
                }
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
            ReaderOrigin::History | ReaderOrigin::Updates => 0,
        }
    }

    pub fn reader_chapter_count(&self) -> usize {
        match self.reader_origin {
            ReaderOrigin::Remote => self.remote.chapters.len(),
            ReaderOrigin::BrowseLocal => self.browse_local_chapter_count(),
            ReaderOrigin::Library => self.chapter_count(),
            ReaderOrigin::LibraryRemote => self.library_remote_chapter_count(),
            ReaderOrigin::History | ReaderOrigin::Updates => 1,
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
        self.picker.is_some()
            || self.text_input.is_some()
            || self.extension_search
            || self.history_search
            || self.library_search
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
            TextAction::RenameStore => self.save_store_name(&input.value),
            TextAction::SetHostUrl => self.set_host_url(&input.value),
            TextAction::SetFlareSolverr => self.set_flaresolverr_url(&input.value),
            TextAction::SetDownloadsPath => self.set_downloads_path(&input.value),
            TextAction::AddCategory => self.add_category(&input.value),
            TextAction::RenameCategory => self.rename_category(&input.value),
            TextAction::ImportBackup => self.import_backup(&input.value),
            TextAction::ExportBackup => self.export_backup(&input.value),
            TextAction::ImportSeries => self.import_series_folder(&input.value),
            TextAction::MigrationSearch => self.run_migration_search(&input.value),
            TextAction::StoryNotes => self.save_story_notes(&input.value),
            TextAction::SetCustomCover => self.set_custom_cover(&input.value),
            TextAction::TrackerSearch => self.run_tracker_search(&input.value),
            TextAction::AniListToken => self.set_anilist_token(&input.value),
            TextAction::MalClientId => self.set_mal_client_id(&input.value),
            TextAction::MalToken => self.set_mal_token(&input.value),
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
        self.load_extensions_async();
        self.select_browse_entry(0);
    }

    pub fn open_stores(&mut self) {
        self.browse_view = BrowseView::Stores;
        self.browse_selection = 0;
        if self.remote.repos.is_empty() {
            self.load_extensions_async();
        }
        self.select_browse_entry(0);
    }

    /// Load extensions without blocking when the worker is available.
    pub fn load_extensions_async(&mut self) {
        if !self.ensure_host() {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        self.extensions_generation = self.extensions_generation.wrapping_add(1);
        if let Some(worker) = self.extensions_worker.as_ref() {
            self.remote.status = Some("Loading extensions…".to_owned());
            self.remote.error = false;
            worker.request(ExtensionsRequest {
                generation: self.extensions_generation,
                client,
            });
            return;
        }
        self.load_extensions();
    }

    /// Apply a completed background extension-list load. Returns whether the UI changed.
    pub fn poll_extensions_worker(&mut self) -> bool {
        let Some(event) = self
            .extensions_worker
            .as_ref()
            .and_then(ExtensionsWorker::try_result)
        else {
            return false;
        };
        if event.generation != self.extensions_generation {
            return false;
        }
        match event.result {
            Ok(snapshot) => self.apply_extensions_snapshot(snapshot),
            Err(error) => {
                self.remote.status = Some(error);
                self.remote.error = true;
            }
        }
        self.clamp_browse_selection();
        true
    }

    fn apply_extensions_snapshot(&mut self, snapshot: ExtensionsSnapshot) {
        self.remote.extensions = merge_extensions(snapshot.installed, snapshot.entries);
        self.remote.not_loaded = snapshot.not_loaded;
        self.remote.repos = snapshot.repos;
        if snapshot.repo_errors.is_empty() {
            self.remote.status = None;
            self.remote.error = false;
        } else {
            self.remote.status = Some(format!(
                "{} repositories failed to load",
                snapshot.repo_errors.len()
            ));
            self.remote.error = true;
        }
        if let Some(state) = self.extension_details.as_mut() {
            if let Some(updated) = self
                .remote
                .extensions
                .iter()
                .find(|extension| extension.pkg == state.extension.pkg)
                .cloned()
            {
                state.extension = updated;
            }
        }
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
        self.remote.not_loaded = client.not_loaded_extensions().unwrap_or_default();
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
        if let Some(extension) = self.selected_not_loaded() {
            self.remote.status = Some(format!(
                "{} · {}: delete the failed file with x before reinstalling",
                extension.reason, extension.message
            ));
            self.remote.error = true;
            return;
        }
        if let Some(job) = self
            .selected_extension()
            .and_then(|extension| self.remote.extension_jobs.get(&extension.pkg))
        {
            let pkg = job.pkg.clone();
            let label = job.label();
            let name = job.name.clone();
            if !matches!(job.state.as_str(), "error" | "cancelled") {
                self.remote.status = Some(format!("{name} is {label}"));
                self.remote.error = false;
                return;
            }
            self.remote.extension_jobs.remove(&pkg);
        }
        let extensions: Vec<ExtensionInfo> = self
            .selected_extensions()
            .into_iter()
            .filter(|extension| {
                !extension.installed && !self.remote.extension_jobs.contains_key(&extension.pkg)
            })
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
        let mut started = 0usize;
        let mut fallback = Vec::new();
        for extension in extensions {
            if extension.repo_url.is_empty() || extension.apk.is_empty() {
                continue;
            }
            match client.install_extension_async(&extension.repo_url, &extension.apk) {
                Ok(job) if !job.job_id.is_empty() => {
                    self.remote.extension_jobs.insert(
                        extension.pkg.clone(),
                        ExtensionJobState {
                            job_id: job.job_id,
                            pkg: extension.pkg,
                            name: extension.name,
                            verb: verb.to_owned(),
                            apk: extension.apk,
                            state: job.state,
                            progress: job.progress,
                            error: job.error,
                        },
                    );
                    started += 1;
                }
                _ => fallback.push(extension),
            }
        }
        if !fallback.is_empty() {
            self.install_extensions_blocking(fallback, verb);
        }
        if started > 0 {
            self.remote.status = Some(format!(
                "Installing {started} {}…",
                if started == 1 {
                    "extension"
                } else {
                    "extensions"
                }
            ));
            self.remote.error = false;
            self.multi_select = None;
        }
    }

    fn install_extensions_blocking(&mut self, extensions: Vec<ExtensionInfo>, verb: &str) {
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

    /// Poll async install/update jobs and refresh extension/source lists when any finish.
    pub fn poll_extension_jobs(&mut self) -> bool {
        if self.remote.extension_jobs.is_empty() {
            return false;
        }
        let Some(client) = self.host_client.clone() else {
            return false;
        };
        let ids: Vec<(String, String)> = self
            .remote
            .extension_jobs
            .iter()
            .map(|(pkg, job)| (pkg.clone(), job.job_id.clone()))
            .collect();
        let mut changed = false;
        let mut finished: Vec<(String, InstallJob)> = Vec::new();
        for (pkg, job_id) in ids {
            match client.install_job(&job_id) {
                Ok(job) => {
                    if let Some(state) = self.remote.extension_jobs.get_mut(&pkg) {
                        if state.state != job.state || (state.progress - job.progress).abs() > 0.001
                        {
                            state.state = job.state.clone();
                            state.progress = job.progress;
                            state.error = job.error.clone();
                            changed = true;
                        }
                    }
                    if job.finished() {
                        finished.push((pkg, job));
                    }
                }
                Err(error) => {
                    if let Some(state) = self.remote.extension_jobs.get_mut(&pkg) {
                        if state.state != "error" {
                            state.state = "error".to_owned();
                            state.error = Some(error.to_string());
                            changed = true;
                        }
                    }
                }
            }
        }
        if finished.is_empty() {
            return changed;
        }

        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut cancelled = 0usize;
        let mut swapped = false;
        for (pkg, job) in finished {
            let Some(state) = self.remote.extension_jobs.remove(&pkg) else {
                continue;
            };
            match job.state.as_str() {
                "done" => {
                    completed += 1;
                    if self.install_desktop_jar(&state.apk) {
                        swapped = true;
                    }
                }
                "cancelled" => cancelled += 1,
                _ => {
                    failed += 1;
                    self.remote.extension_jobs.insert(
                        pkg,
                        ExtensionJobState {
                            state: "error".to_owned(),
                            error: job.error.clone().or(state.error),
                            ..state
                        },
                    );
                }
            }
        }
        if completed > 0 {
            if swapped {
                self.restart_bundled_host();
            }
            self.load_extensions();
            self.load_remote_sources();
        }
        let parts = [
            (completed, "installed"),
            (failed, "failed"),
            (cancelled, "cancelled"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, label)| format!("{count} {label}"))
        .collect::<Vec<_>>();
        if !parts.is_empty() {
            self.remote.status = Some(parts.join(" · "));
            self.remote.error = failed > 0;
        }
        true
    }

    pub fn cancel_extension_jobs(&mut self) {
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let jobs: Vec<(String, String)> = self
            .remote
            .extension_jobs
            .iter()
            .filter(|(_, job)| !job.state.eq_ignore_ascii_case("error"))
            .map(|(pkg, job)| (pkg.clone(), job.job_id.clone()))
            .collect();
        if jobs.is_empty() {
            self.remote.status = Some("No extension installs are running".to_owned());
            self.remote.error = false;
            return;
        }
        let mut cancelled = 0usize;
        for (pkg, job_id) in jobs {
            if client.cancel_install_job(&job_id).is_ok() {
                cancelled += 1;
                if let Some(job) = self.remote.extension_jobs.get_mut(&pkg) {
                    job.state = "cancelled".to_owned();
                    job.error = Some("cancelled".to_owned());
                }
            }
        }
        self.remote.status = Some(format!("Cancelled {cancelled} extension installs"));
        self.remote.error = cancelled == 0;
    }

    pub fn uninstall_selected_extension(&mut self) {
        if let Some(extension) = self.selected_not_loaded().cloned() {
            self.uninstall_confirm = Some(vec![(
                format!("not-loaded:{}", extension.file),
                extension.name.clone(),
            )]);
            self.remote.status = Some(format!("Press y to delete {}, Esc cancels", extension.file));
            self.remote.error = false;
            return;
        }
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
            let result = if let Some(file) = pkg.strip_prefix("not-loaded:") {
                client.delete_not_loaded(file)
            } else {
                client.uninstall_extension(pkg)
            };
            match result {
                Ok(()) => removed += 1,
                Err(_) => {
                    failed += 1;
                    break;
                }
            }
        }
        self.load_extensions();
        self.load_remote_sources();
        if let Some(details) = self.extension_details.as_ref() {
            if items.iter().any(|(pkg, _)| pkg == &details.extension.pkg) {
                self.extension_details = None;
                self.browse_view = BrowseView::Extensions;
                self.browse_selection = 0;
            }
        }
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

    pub fn open_extension_details(&mut self) {
        if let Some(extension) = self.selected_not_loaded() {
            self.remote.status = Some(format!(
                "{} could not load ({}) · {}",
                extension.name, extension.reason, extension.message
            ));
            self.remote.error = true;
            return;
        }
        let Some(extension) = self.selected_extension().cloned() else {
            return;
        };
        if !extension.installed {
            self.remote.status = Some("Install the extension to open its details.".to_owned());
            self.remote.error = false;
            return;
        }
        let sources = self.resolved_extension_sources(&extension);
        let fingerprint = extension_fingerprint(&extension.apk);
        self.extension_details = Some(ExtensionDetailsState {
            extension,
            sources,
            list_state: ListState::default(),
            fingerprint,
        });
        self.browse_view = BrowseView::ExtensionDetails;
        self.browse_selection = 0;
        self.select_browse_entry(0);
    }

    pub fn toggle_extension_trust(&mut self) {
        let Some(state) = self.extension_details.as_ref() else {
            return;
        };
        let extension = state.extension.clone();
        let Some(fingerprint) = state.fingerprint.clone() else {
            self.remote.status = Some("Cannot fingerprint this extension file.".to_owned());
            self.remote.error = true;
            return;
        };
        let trusted = self
            .config
            .trusted_extensions
            .get(&extension.pkg)
            .is_some_and(|known| known == &fingerprint);
        if trusted {
            self.config.trusted_extensions.remove(&extension.pkg);
            self.remote.status = Some(format!("No longer trusting {}", extension.name));
        } else {
            self.config
                .trusted_extensions
                .insert(extension.pkg.clone(), fingerprint);
            self.remote.status = Some(format!("Trusted {}", extension.name));
        }
        self.persist_settings();
        self.remote.error = false;
    }

    pub fn extension_detail_rows(&self) -> Vec<ExtensionDetailRow> {
        let mut rows: Vec<ExtensionDetailRow> = self
            .extension_details
            .as_ref()
            .map(|state| {
                (0..state.sources.len())
                    .map(ExtensionDetailRow::Source)
                    .collect()
            })
            .unwrap_or_default();
        rows.push(ExtensionDetailRow::EnableAll);
        rows.push(ExtensionDetailRow::DisableAll);
        rows.push(ExtensionDetailRow::Incognito);
        rows.push(ExtensionDetailRow::ClearCookies);
        rows.push(ExtensionDetailRow::Uninstall);
        rows
    }

    pub fn selected_extension_detail_row(&self) -> Option<usize> {
        Some(self.browse_selection)
    }

    pub fn select_extension_detail_row(&mut self, row: usize) {
        let offset = self
            .extension_details
            .as_ref()
            .map_or(0, |state| state.list_state.offset());
        self.select_browse_index(offset + row);
    }

    fn toggle_extension_source(&mut self, index: usize) {
        let Some(source) = self
            .extension_details
            .as_ref()
            .and_then(|state| state.sources.get(index))
            .cloned()
        else {
            return;
        };
        let key = source_pin_key(&source);
        if let Some(position) = self
            .config
            .disabled_sources
            .iter()
            .position(|disabled| disabled == &key)
        {
            self.config.disabled_sources.remove(position);
        } else {
            self.config.disabled_sources.push(key);
        }
        self.persist_settings();
    }

    fn set_extension_sources_enabled(&mut self, enabled: bool) {
        let Some(state) = self.extension_details.as_ref() else {
            return;
        };
        let keys: Vec<String> = state.sources.iter().map(source_pin_key).collect();
        if enabled {
            self.config
                .disabled_sources
                .retain(|disabled| !keys.contains(disabled));
        } else {
            for key in keys {
                if !self.config.disabled_sources.contains(&key) {
                    self.config.disabled_sources.push(key);
                }
            }
        }
        self.persist_settings();
    }

    pub fn activate_extension_detail_selection(&mut self) {
        let Some(row) = self
            .extension_detail_rows()
            .get(self.browse_selection)
            .cloned()
        else {
            return;
        };
        match row {
            ExtensionDetailRow::Source(index) => self.toggle_extension_source(index),
            ExtensionDetailRow::EnableAll => self.set_extension_sources_enabled(true),
            ExtensionDetailRow::DisableAll => self.set_extension_sources_enabled(false),
            ExtensionDetailRow::Incognito => {
                let Some(state) = self.extension_details.as_ref() else {
                    return;
                };
                let pkg = state.extension.pkg.clone();
                let enabled = self.config.incognito_extensions.contains(&pkg);
                if enabled {
                    self.config
                        .incognito_extensions
                        .retain(|candidate| candidate != &pkg);
                } else {
                    self.config.incognito_extensions.push(pkg);
                }
                self.persist_settings();
            }
            ExtensionDetailRow::ClearCookies => self.clear_extension_cookies(),
            ExtensionDetailRow::Uninstall => {
                let Some(state) = self.extension_details.as_ref() else {
                    return;
                };
                self.uninstall_confirm = Some(vec![(
                    state.extension.pkg.clone(),
                    state.extension.name.clone(),
                )]);
                self.remote.status = Some(format!(
                    "Press y to uninstall {}, Esc cancels",
                    state.extension.name
                ));
                self.remote.error = false;
            }
        }
    }

    pub fn open_source_prefs_for_selected(&mut self) {
        let Some(source) = self
            .extension_details
            .as_ref()
            .and_then(|state| state.sources.get(self.browse_selection).cloned())
        else {
            self.remote.status = Some("Select a source to configure.".to_owned());
            self.remote.error = false;
            return;
        };
        self.load_source_prefs(source);
    }

    fn clear_extension_cookies(&mut self) {
        let Some(state) = self.extension_details.as_ref() else {
            return;
        };
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let sources = state.sources.clone();
        let mut cleared = 0usize;
        let mut failed = 0usize;
        for source in &sources {
            match client.clear_source_cookies(&source.id) {
                Ok(count) => cleared += count,
                Err(_) => failed += 1,
            }
        }
        self.remote.status = Some(if failed == 0 {
            format!("Cleared {cleared} cookies")
        } else {
            format!("Cleared {cleared} cookies · {failed} sources failed")
        });
        self.remote.error = failed > 0;
    }

    pub fn update_selected_extension(&mut self) {
        let extensions: Vec<ExtensionInfo> = self
            .selected_extensions()
            .into_iter()
            .filter(|extension| {
                extension.update_available()
                    && !extension.repo_url.is_empty()
                    && !extension.apk.is_empty()
                    && !self.remote.extension_jobs.contains_key(&extension.pkg)
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
                    && !self.remote.extension_jobs.contains_key(&extension.pkg)
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

    fn export_backup(&mut self, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            self.status = Some("Enter an export path.".to_owned());
            return;
        }
        if self.config.remote_library.is_empty()
            && !self
                .library
                .series
                .iter()
                .any(|series| series.remote.is_none())
        {
            self.status = Some("No library manga to export.".to_owned());
            return;
        }
        let mut path = PathBuf::from(value);
        if path.extension().is_none() {
            path.set_extension("tachibk");
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && std::fs::create_dir_all(parent).is_err() {
                self.status = Some("Cannot create the backup directory.".to_owned());
                return;
            }
        }

        let category_names: Vec<String> = self
            .library
            .categories
            .iter()
            .filter(|name| name.as_str() != "Default")
            .cloned()
            .collect();
        let category_ids: BTreeMap<String, i64> = category_names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index as i64 + 1))
            .collect();
        let categories: Vec<BackupCategory> = category_names
            .iter()
            .enumerate()
            .map(|(index, name)| BackupCategory {
                id: index as i64 + 1,
                name: name.clone(),
            })
            .collect();
        let mut sources: BTreeMap<i64, BackupSource> = BTreeMap::new();
        let mut mangas = Vec::new();
        let mut chapter_count = 0usize;
        let mut history_count = 0usize;

        for favorite in &self.config.remote_library {
            let Ok(source_id) = favorite.source_id.parse::<i64>() else {
                continue;
            };
            let story = load_cached_story(&favorite.source_id, &favorite.url);
            let (details, story_chapters) = story
                .map(|story| (Some(story.manga), story.chapters))
                .unwrap_or((None, Vec::new()));
            let source_name = self
                .remote
                .sources
                .iter()
                .find(|source| source.id == favorite.source_id)
                .map(|source| source.name.clone())
                .unwrap_or_else(|| favorite.source_id.clone());
            sources.insert(
                source_id,
                BackupSource {
                    name: source_name,
                    source_id,
                },
            );
            let key = remote_story_key(&favorite.source_id, &favorite.url);
            let mut history_by_chapter: BTreeMap<String, Vec<BackupHistory>> = BTreeMap::new();
            for (_, entry) in self.history.recent() {
                if entry.series_key() != key {
                    continue;
                }
                let Some(remote) = &entry.remote else {
                    continue;
                };
                history_by_chapter
                    .entry(remote.chapter.url.clone())
                    .or_default()
                    .push(BackupHistory {
                        url: remote.chapter.url.clone(),
                        last_read: entry.last_read.saturating_mul(1000) as i64,
                        read_duration: entry.read_duration_secs as i64,
                    });
                history_count += 1;
            }
            let mut chapters = Vec::new();
            for chapter in &story_chapters {
                let progress =
                    remote_progress_key(&favorite.source_id, &favorite.url, &chapter.url);
                let entry = self.progress.entry(&progress);
                let read = entry.is_some_and(|entry| entry.page + 1 >= entry.pages);
                let last_page = entry.map(|entry| entry.page as i64).unwrap_or(0);
                let bookmark = self.bookmarks.is_bookmarked(&progress);
                if entry.is_some() || bookmark {
                    chapter_count += 1;
                }
                chapters.push(BackupChapter {
                    url: chapter.url.clone(),
                    name: chapter.name.clone(),
                    scanlator: chapter.scanlator.clone(),
                    read,
                    bookmark,
                    last_page_read: last_page,
                    date_upload: chapter.upload_date,
                    chapter_number: chapter.chapter_number,
                });
            }
            let history = chapters
                .iter()
                .flat_map(|chapter| history_by_chapter.remove(&chapter.url).unwrap_or_default())
                .collect();
            let categories_for_manga: Vec<i64> = favorite
                .categories
                .iter()
                .filter_map(|name| category_ids.get(name).copied())
                .collect();
            mangas.push(BackupManga {
                source_id,
                url: favorite.url.clone(),
                title: favorite.title.clone(),
                author: details.as_ref().and_then(|manga| manga.author.clone()),
                artist: details.as_ref().and_then(|manga| manga.artist.clone()),
                description: details.as_ref().and_then(|manga| manga.description.clone()),
                genre: details
                    .as_ref()
                    .map(|manga| manga.genre.clone())
                    .unwrap_or_default(),
                thumbnail_url: None,
                chapters,
                category_ids: categories_for_manga,
                favorite: true,
                notes: self
                    .config
                    .story_notes
                    .get(&key)
                    .cloned()
                    .unwrap_or_default(),
                history,
                date_added: self
                    .config
                    .series_added_at
                    .get(&remote_series_identity(&favorite.source_id, &favorite.url))
                    .copied()
                    .unwrap_or(0) as i64,
                favorite_modified_at: None,
            });
        }

        for series in self
            .library
            .series
            .iter()
            .filter(|series| series.remote.is_none())
        {
            let source_id = 0i64;
            sources.entry(source_id).or_insert(BackupSource {
                name: "Local source".to_owned(),
                source_id,
            });
            let key = format!("local:{}", series.path.display());
            let mut history_by_chapter: BTreeMap<String, Vec<BackupHistory>> = BTreeMap::new();
            for (_, entry) in self.history.recent() {
                if entry.series_key() != key {
                    continue;
                }
                let Some(local) = &entry.local else {
                    continue;
                };
                let url = local
                    .chapter_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&entry.chapter_title)
                    .to_owned();
                history_by_chapter
                    .entry(url.clone())
                    .or_default()
                    .push(BackupHistory {
                        url,
                        last_read: entry.last_read.saturating_mul(1000) as i64,
                        read_duration: entry.read_duration_secs as i64,
                    });
                history_count += 1;
            }
            let mut chapters = Vec::new();
            for chapter in &series.chapters {
                let entry = self.progress.entry(&chapter.path);
                let read = entry.is_some_and(|entry| entry.page + 1 >= entry.pages);
                let last_page = entry.map(|entry| entry.page as i64).unwrap_or(0);
                let bookmark = self.bookmarks.is_bookmarked(&chapter.path);
                if entry.is_some() || bookmark {
                    chapter_count += 1;
                }
                let url = chapter
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&chapter.title)
                    .to_owned();
                chapters.push(BackupChapter {
                    url,
                    name: chapter.title.clone(),
                    scanlator: chapter.scanlator.clone(),
                    read,
                    bookmark,
                    last_page_read: last_page,
                    date_upload: 0,
                    chapter_number: chapter.number.unwrap_or(0.0),
                });
            }
            let history = chapters
                .iter()
                .flat_map(|chapter| history_by_chapter.remove(&chapter.url).unwrap_or_default())
                .collect();
            let categories_for_manga: Vec<i64> = series
                .categories
                .iter()
                .filter_map(|name| category_ids.get(name).copied())
                .collect();
            mangas.push(BackupManga {
                source_id,
                url: series
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&series.title)
                    .to_owned(),
                title: series.title.clone(),
                author: series.author.clone(),
                artist: series.artist.clone(),
                description: (!series.description.is_empty()).then(|| series.description.clone()),
                genre: series.tags.clone(),
                thumbnail_url: None,
                chapters,
                category_ids: categories_for_manga,
                favorite: true,
                notes: self
                    .config
                    .story_notes
                    .get(&key)
                    .cloned()
                    .unwrap_or_default(),
                history,
                date_added: self.config.series_added_at.get(&key).copied().unwrap_or(0) as i64,
                favorite_modified_at: None,
            });
        }

        let backup = MihonBackup {
            mangas,
            categories,
            sources: sources.into_values().collect(),
        };
        match write_mihon_backup(&path, &backup) {
            Ok(()) => {
                self.status = Some(format!(
                    "Exported {} manga · {chapter_count} chapter states · {history_count} history to {}",
                    backup.mangas.len(),
                    path.display()
                ));
                self.remote.error = false;
            }
            Err(error) => {
                self.status = Some(format!("Could not export backup: {error}"));
                self.remote.error = true;
            }
        }
    }

    /// Ask for a folder to copy into the library, the TUI equivalent of Mihon's
    /// local-source import picker.
    pub fn open_import_series(&mut self) {
        if self.tab == 0 && self.view == LibraryView::Series {
            self.open_text_input(TextAction::ImportSeries, "Import series folder", "");
        }
    }

    fn import_series_folder(&mut self, value: &str) {
        let source = PathBuf::from(value.trim());
        if source.as_os_str().is_empty() {
            self.status = Some("Enter the folder to import.".to_owned());
            return;
        }
        match import_series(&source, &self.library.root) {
            Ok(destination) => {
                let name = destination
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.refresh_async();
                self.status = Some(format!("Imported {name}"));
            }
            Err(error) => self.status = Some(error.to_string()),
        }
    }

    /// Whether a backup entry belongs to Mihon's Local source.
    fn is_local_backup_manga(source_id: i64, source_name: Option<&String>) -> bool {
        source_id == 0 || source_name.is_some_and(|name| normalized_title(name) == "localsource")
    }

    /// Restore progress, bookmarks, history, notes, and categories for one local entry.
    ///
    /// Returns `(progress, bookmarks, history, categories_changed)` when a library
    /// series matches, or `None` when it is not in the library.
    fn import_local_backup_manga(
        &mut self,
        manga: &BackupManga,
        category_names: &std::collections::HashMap<i64, String>,
    ) -> Option<(usize, usize, usize, bool)> {
        let index = self.library.series.iter().position(|series| {
            series.remote.is_none()
                && (series.path.file_name().and_then(|name| name.to_str())
                    == Some(manga.url.as_str())
                    || series.title == manga.title)
        })?;
        let series = self.library.series[index].clone();
        if let Some(added) = backup_added_at(manga) {
            self.config
                .series_added_at
                .entry(format!("local:{}", series.path.display()))
                .or_insert(added);
        }

        let categories: Vec<String> = manga
            .category_ids
            .iter()
            .filter_map(|id| category_names.get(id).cloned())
            .collect();
        let mut categories_changed = false;
        if !categories.is_empty() {
            for name in &categories {
                if !self.library.categories.iter().any(|known| known == name) {
                    self.library.categories.push(name.clone());
                }
            }
            self.library.series[index].categories = categories;
            categories_changed = true;
        }

        let story_key = format!("local:{}", series.path.display());
        if !manga.notes.trim().is_empty() {
            self.config
                .story_notes
                .insert(story_key, manga.notes.clone());
        }

        let chapter_index = |url: &str, name: &str| {
            series.chapters.iter().position(|chapter| {
                chapter.path.file_name().and_then(|file| file.to_str()) == Some(url)
                    || (!name.is_empty() && chapter.title == name)
            })
        };
        let mut progress = 0usize;
        let mut bookmarks = 0usize;
        let mut history = 0usize;
        for chapter in &manga.chapters {
            let Some(index) = chapter_index(&chapter.url, &chapter.name) else {
                continue;
            };
            let local_chapter = &series.chapters[index];
            let page = chapter.last_page_read.max(0) as usize;
            if chapter.read {
                self.progress
                    .update(&local_chapter.path, page, page.saturating_add(1));
                progress += 1;
            } else if page > 0 {
                self.progress
                    .update(&local_chapter.path, page, page.saturating_add(2));
                progress += 1;
            }
            if chapter.bookmark {
                self.bookmarks.set(&local_chapter.path, true);
                bookmarks += 1;
            }
        }

        let chapters: std::collections::HashMap<&str, &tsuiyomi_core::BackupChapter> = manga
            .chapters
            .iter()
            .map(|chapter| (chapter.url.as_str(), chapter))
            .collect();
        for entry in &manga.history {
            let Some(chapter) = chapters.get(entry.url.as_str()).copied() else {
                continue;
            };
            let Some(index) = chapter_index(&entry.url, &chapter.name) else {
                continue;
            };
            let local_chapter = &series.chapters[index];
            let page = chapter.last_page_read.max(0) as usize;
            let mut history_entry = HistoryEntry::local(
                series.title.clone(),
                local_chapter.title.clone(),
                series.path.clone(),
                local_chapter.path.clone(),
                series.cover.clone(),
                page,
                page.saturating_add(1),
            );
            history_entry.last_read = unix_seconds(entry.last_read.max(0) as u64);
            history_entry.read_duration_secs = entry.read_duration.max(0) as u64;
            self.history.record(
                local_chapter.path.to_string_lossy().into_owned(),
                history_entry,
            );
            history += 1;
        }

        Some((progress, bookmarks, history, categories_changed))
    }

    fn import_backup(&mut self, value: &str) {
        let path = PathBuf::from(value.trim());
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.status = Some(format!("Cannot read {}: {error}", path.display()));
                return;
            }
        };
        let backup = match parse_mihon_backup(&bytes) {
            Ok(backup) => backup,
            Err(error) => {
                self.status = Some(error.to_string());
                return;
            }
        };
        if backup.mangas.is_empty() {
            self.status = Some("Backup contains no manga.".to_owned());
            return;
        }

        let source_names: std::collections::HashMap<i64, String> = backup
            .sources
            .iter()
            .map(|source| (source.source_id, source.name.clone()))
            .collect();
        let category_names: std::collections::HashMap<i64, String> = backup
            .categories
            .iter()
            .map(|category| (category.id, category.name.clone()))
            .collect();
        let remote_mangas: Vec<&BackupManga> = backup
            .mangas
            .iter()
            .filter(|manga| {
                !Self::is_local_backup_manga(manga.source_id, source_names.get(&manga.source_id))
            })
            .collect();

        if !remote_mangas.is_empty() {
            if self.host_client.is_none() && !self.ensure_host() {
                return;
            }
            if self.remote.sources.is_empty() {
                self.load_remote_sources();
            }
            if self.host_client.is_none() {
                return;
            }
        }

        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut progress_count = 0usize;
        let mut bookmark_count = 0usize;
        let mut history_count = 0usize;
        let mut local_categories_changed = false;
        let mut remote_imported = 0usize;
        let mut categories_to_apply: Vec<(String, Vec<String>)> = Vec::new();

        // Local entries do not need the extension host. Restore them first so a
        // local-only backup works with no JVM installed.
        for manga in &backup.mangas {
            if !Self::is_local_backup_manga(manga.source_id, source_names.get(&manga.source_id)) {
                continue;
            }
            match self.import_local_backup_manga(manga, &category_names) {
                Some((progress, bookmarks, history, changed)) => {
                    progress_count += progress;
                    bookmark_count += bookmarks;
                    history_count += history;
                    local_categories_changed |= changed;
                    imported += 1;
                }
                None => skipped += 1,
            }
        }

        if remote_mangas.is_empty() {
            self.finish_backup_import(
                imported,
                skipped,
                failed,
                progress_count,
                bookmark_count,
                history_count,
                local_categories_changed,
                remote_imported,
                categories_to_apply,
            );
            return;
        }

        let Some(client) = self.host_client.clone() else {
            return;
        };
        for manga in remote_mangas {
            let source = if let Some(name) = source_names.get(&manga.source_id) {
                let normalized = normalized_title(name);
                self.remote.sources.iter().find(|source| {
                    normalized_title(&source.name) == normalized
                        || normalized_title(&source.display_name()) == normalized
                })
            } else {
                self.remote
                    .sources
                    .iter()
                    .find(|source| source.id == manga.source_id.to_string())
            }
            .cloned();
            let Some(source) = source else {
                skipped += 1;
                continue;
            };
            let remote_manga = RemoteManga {
                source_id: source.id.clone(),
                url: manga.url.clone(),
                title: manga.title.clone(),
                author: manga.author.clone(),
                artist: manga.artist.clone(),
                description: manga.description.clone(),
                genre: manga.genre.clone(),
                status: "unknown".to_owned(),
                thumbnail_url: manga.thumbnail_url.clone(),
                memo: None,
            };
            let story_key = remote_story_key(&source.id, &manga.url);
            if let Some(added) = backup_added_at(manga) {
                self.config
                    .series_added_at
                    .entry(remote_series_identity(&source.id, &manga.url))
                    .or_insert(added);
            }
            if manga.favorite
                && !self.is_favorite(&source.id, &manga.url)
                && client.add_favorite(&remote_manga).is_err()
            {
                failed += 1;
            }
            let categories: Vec<String> = manga
                .category_ids
                .iter()
                .filter_map(|id| category_names.get(id).cloned())
                .collect();
            if !categories.is_empty() {
                categories_to_apply.push((story_key.clone(), categories));
            }
            if !manga.notes.trim().is_empty() {
                self.config
                    .story_notes
                    .insert(story_key.clone(), manga.notes.clone());
            }

            let chapters: std::collections::HashMap<&str, &tsuiyomi_core::BackupChapter> = manga
                .chapters
                .iter()
                .map(|chapter| (chapter.url.as_str(), chapter))
                .collect();
            for chapter in &manga.chapters {
                if chapter.url.is_empty() {
                    continue;
                }
                let key = remote_progress_key(&source.id, &manga.url, &chapter.url);
                let last = chapter.last_page_read.max(0) as usize;
                if chapter.read {
                    self.progress.update(&key, last, last.saturating_add(1));
                    progress_count += 1;
                } else if last > 0 {
                    self.progress.update(&key, last, last.saturating_add(2));
                    progress_count += 1;
                }
                if chapter.bookmark {
                    self.bookmarks.set(&key, true);
                    bookmark_count += 1;
                }
                if chapter.read || last > 0 || chapter.bookmark {
                    self.pending_states.insert(
                        Self::state_key(&RemoteReadState {
                            source_id: source.id.clone(),
                            manga_url: manga.url.clone(),
                            chapter_url: chapter.url.clone(),
                            page: last,
                            pages: 0,
                            read: chapter.read,
                            bookmark: chapter.bookmark,
                        }),
                        RemoteReadState {
                            source_id: source.id.clone(),
                            manga_url: manga.url.clone(),
                            chapter_url: chapter.url.clone(),
                            page: last,
                            pages: 0,
                            read: chapter.read,
                            bookmark: chapter.bookmark,
                        },
                    );
                    self.state_dirty = true;
                }
            }
            for history in &manga.history {
                let Some(chapter) = chapters.get(history.url.as_str()).copied() else {
                    continue;
                };
                let pages = chapter.last_page_read.max(0) as usize + 1;
                let page = chapter.last_page_read.max(0) as usize;
                let remote_chapter = RemoteChapter {
                    url: chapter.url.clone(),
                    name: if chapter.name.is_empty() {
                        "Chapter".to_owned()
                    } else {
                        chapter.name.clone()
                    },
                    chapter_number: chapter.chapter_number,
                    scanlator: chapter.scanlator.clone(),
                    upload_date: chapter.date_upload,
                    memo: None,
                };
                let key = remote_progress_key(&source.id, &manga.url, &chapter.url);
                let mut entry = HistoryEntry::remote(
                    manga.title.clone(),
                    remote_chapter.name.clone(),
                    source.display_name(),
                    remote_manga.clone(),
                    remote_chapter,
                    page,
                    pages,
                );
                entry.last_read = unix_seconds(history.last_read.max(0) as u64);
                entry.read_duration_secs = history.read_duration.max(0) as u64;
                self.history
                    .record(key.to_string_lossy().into_owned(), entry);
                history_count += 1;
            }
            remote_imported += 1;
            imported += 1;
        }

        self.finish_backup_import(
            imported,
            skipped,
            failed,
            progress_count,
            bookmark_count,
            history_count,
            local_categories_changed,
            remote_imported,
            categories_to_apply,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_backup_import(
        &mut self,
        imported: usize,
        skipped: usize,
        failed: usize,
        progress_count: usize,
        bookmark_count: usize,
        history_count: usize,
        local_categories_changed: bool,
        remote_imported: usize,
        categories_to_apply: Vec<(String, Vec<String>)>,
    ) {
        if imported > 0 {
            if remote_imported > 0 {
                self.load_favorites();
                for (key, categories) in categories_to_apply {
                    if let Some(favorite) = self.config.remote_library.iter_mut().find(|favorite| {
                        remote_story_key(&favorite.source_id, &favorite.url) == key
                    }) {
                        let mut known: std::collections::BTreeSet<String> =
                            favorite.categories.iter().cloned().collect();
                        known.extend(categories);
                        favorite.categories = known.into_iter().collect();
                    }
                }
                self.merge_remote_library();
            }
            if local_categories_changed {
                self.save_library_categories();
            }
            let _ = self.progress.save();
            let _ = self.bookmarks.save();
            let _ = self.history.save();
            self.progress_dirty = false;
            self.history_dirty = false;
            self.persist_settings();
        }
        let mut parts = vec![format!("Imported {imported} manga")];
        if progress_count > 0 {
            parts.push(format!("{progress_count} chapter states"));
        }
        if bookmark_count > 0 {
            parts.push(format!("{bookmark_count} bookmarks"));
        }
        if history_count > 0 {
            parts.push(format!("{history_count} history entries"));
        }
        if skipped > 0 {
            parts.push(format!("{skipped} unmatched manga"));
        }
        if failed > 0 {
            parts.push(format!("{failed} add failures"));
        }
        self.status = Some(parts.join(" · "));
        self.remote.error = failed > 0;
    }

    fn set_downloads_path(&mut self, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            self.config.downloads_path = None;
            self.status = Some("Downloads use the default folder".to_owned());
        } else {
            let path = PathBuf::from(value);
            if std::fs::create_dir_all(&path).is_err() {
                self.status = Some(format!("Cannot create {}", path.display()));
                return;
            }
            self.config.downloads_path = Some(path.clone());
            self.status = Some(format!("Downloads folder: {}", path.display()));
        }
        self.persist_settings();
    }

    fn set_flaresolverr_url(&mut self, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            self.config.flaresolverr_url = None;
        } else if value.starts_with("http://") || value.starts_with("https://") {
            self.config.flaresolverr_url = Some(value.to_owned());
        } else {
            self.status = Some("FlareSolverr URL must start with http:// or https://".to_owned());
            return;
        }
        self.persist_settings();
        self.status = Some(if self.config.flaresolverr_url.is_some() {
            "FlareSolverr configured".to_owned()
        } else {
            "FlareSolverr disabled".to_owned()
        });
        if self.host_process.is_some() {
            self.restart_bundled_host();
        }
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
        if matches!(
            self.browse_view,
            BrowseView::Chapters | BrowseView::RemoteDetails
        ) {
            let indices = self.story_chapter_indices();
            if let Some(source) = indices.get(index).copied() {
                self.set_story_chapter_cursor(source);
                self.sync_story_list_state();
            }
            return;
        }
        if index < self.browse_item_count() {
            self.browse_selection = index;
            self.select_browse_entry(index);
            self.maybe_load_next_remote_page();
        }
    }

    fn clamp_browse_selection(&mut self) {
        if matches!(
            self.browse_view,
            BrowseView::Chapters | BrowseView::RemoteDetails
        ) {
            let indices = self.story_chapter_indices();
            if indices.is_empty() {
                self.sync_story_list_state();
            } else {
                let cursor = self.story_chapter_cursor().unwrap_or(0);
                if !indices.contains(&cursor) {
                    self.set_story_chapter_cursor(indices[0]);
                }
                self.sync_story_list_state();
            }
            return;
        }
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
            let indices = self.story_chapter_indices();
            if !indices.is_empty() && !indices.contains(&index) {
                self.browse_local_chapter = indices[0];
            }
            let position = self.story_chapter_position();
            let count = indices.len();
            self.browse_local_list_state
                .select((count > 0).then_some(position.min(count.saturating_sub(1))));
        } else if self.browse_view == BrowseView::RemoteDetails {
            self.browse_selection = index;
            let indices = self.story_chapter_indices();
            if !indices.is_empty() && !indices.contains(&index) {
                self.browse_selection = indices[0];
            }
            let position = self.story_chapter_position();
            let count = indices.len();
            self.browse_list_state
                .select((count > 0).then_some(position.min(count.saturating_sub(1))));
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
        } else if self.browse_view == BrowseView::ExtensionDetails {
            if let Some(state) = self.extension_details.as_mut() {
                state.list_state.select((count > 0).then_some(index));
            }
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
        self.clamp_story_cursor();
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

    pub(crate) fn set_result_worker(&mut self, worker: ResultWorker) {
        self.result_worker = Some(worker);
    }

    pub(crate) fn set_source_worker(&mut self, worker: SourceWorker) {
        self.source_worker = Some(worker);
    }

    pub(crate) fn set_favorites_worker(&mut self, worker: FavoritesWorker) {
        self.favorites_worker = Some(worker);
    }

    pub(crate) fn set_filters_worker(&mut self, worker: FiltersWorker) {
        self.filters_worker = Some(worker);
    }

    pub(crate) fn set_prefs_worker(&mut self, worker: PrefsWorker) {
        self.prefs_worker = Some(worker);
    }

    pub(crate) fn set_extensions_worker(&mut self, worker: ExtensionsWorker) {
        self.extensions_worker = Some(worker);
    }

    pub(crate) fn set_migration_worker(&mut self, worker: MigrationWorker) {
        self.migration_worker = Some(worker);
    }

    pub(crate) fn set_scan_worker(&mut self, worker: ScanWorker) {
        self.scan_worker = Some(worker);
    }

    pub(crate) fn set_story_worker(&mut self, worker: StoryWorker) {
        self.story_worker = Some(worker);
    }

    pub(crate) fn set_tracker_worker(&mut self, worker: TrackerWorker) {
        self.tracker_worker = Some(worker);
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
                    self.load_remote_sources_async();
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
            flaresolverr_url: self.config.flaresolverr_url.clone(),
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
    /// Refresh saved-source caches whose last check is older than the configured
    /// interval. Runs at most once a minute and only when the host is already up.
    pub fn maybe_auto_update_library(&mut self) {
        let hours = self.config.update_interval_hours;
        if hours == 0 {
            return;
        }
        let now = Instant::now();
        if self
            .auto_update_checked_at
            .is_some_and(|checked| now.duration_since(checked) < Duration::from_secs(60))
        {
            return;
        }
        self.auto_update_checked_at = Some(now);
        let (Some(worker), Some(client)) = (self.story_worker.as_ref(), self.host_client.clone())
        else {
            return;
        };
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let interval = hours.saturating_mul(3_600);
        let favorites = self.config.remote_library.clone();
        let mut queued = 0usize;
        for favorite in favorites {
            if queued >= 10 {
                break;
            }
            let key = remote_story_key(&favorite.source_id, &favorite.url);
            if self.auto_update_inflight.contains(&key)
                || self.story_refresh.as_deref() == Some(key.as_str())
            {
                continue;
            }
            let due = story_cache_path(&favorite.source_id, &favorite.url)
                .and_then(|path| std::fs::metadata(path).ok())
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|modified| modified.as_secs())
                .is_none_or(|updated| now_unix.saturating_sub(updated) >= interval);
            if !due {
                continue;
            }
            self.auto_update_inflight.insert(key.clone());
            worker.request(StoryRequest {
                key,
                manga: RemoteManga {
                    source_id: favorite.source_id,
                    url: favorite.url,
                    title: favorite.title,
                    author: None,
                    artist: None,
                    description: None,
                    genre: Vec::new(),
                    status: "unknown".to_owned(),
                    thumbnail_url: favorite.thumbnail_url,
                    memo: None,
                },
                client: client.clone(),
            });
            queued += 1;
        }
    }

    /// Refresh every saved-source story now, using the same worker as the interval
    /// scheduler. Existing in-flight requests are skipped.
    pub fn update_library_now(&mut self) {
        if self.config.remote_library.is_empty() {
            self.status = Some("No saved source manga to update.".to_owned());
            return;
        }
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        let (Some(worker), Some(client)) = (self.story_worker.as_ref(), self.host_client.clone())
        else {
            self.status = Some("The extension host is starting; try again shortly.".to_owned());
            return;
        };
        let favorites = self.config.remote_library.clone();
        let mut queued = 0usize;
        for favorite in favorites {
            let key = remote_story_key(&favorite.source_id, &favorite.url);
            if self.auto_update_inflight.contains(&key)
                || self.story_refresh.as_deref() == Some(key.as_str())
            {
                continue;
            }
            self.auto_update_inflight.insert(key.clone());
            worker.request(StoryRequest {
                key,
                manga: RemoteManga {
                    source_id: favorite.source_id,
                    url: favorite.url,
                    title: favorite.title,
                    author: None,
                    artist: None,
                    description: None,
                    genre: Vec::new(),
                    status: "unknown".to_owned(),
                    thumbnail_url: favorite.thumbnail_url,
                    memo: None,
                },
                client: client.clone(),
            });
            queued += 1;
        }
        self.status = Some(if queued == 0 {
            "Saved stories are already updating.".to_owned()
        } else {
            format!("Updating {queued} saved stories…")
        });
    }

    pub fn next_story_update_at(&self) -> Option<u64> {
        if self.config.update_interval_hours == 0 {
            return None;
        }
        let updated = self.story_updated_at?;
        let manga = self.remote_story_manga()?;
        if !self.is_favorite(&manga.source_id, &manga.url) {
            return None;
        }
        Some(updated.saturating_add(self.config.update_interval_hours.saturating_mul(3_600)))
    }

    pub fn poll_story_refresh(&mut self) -> bool {
        let mut changed = false;
        while let Some(result) = self.story_worker.as_ref().and_then(StoryWorker::try_result) {
            self.auto_update_inflight.remove(&result.key);
            if self.story_refresh.as_deref() == Some(result.key.as_str()) {
                self.story_refresh = None;
            }
            if let Some(error) = &result.error {
                if self.remote.manga.as_ref().is_some_and(|manga| {
                    remote_story_key(&manga.source_id, &manga.url) == result.key
                }) {
                    self.remote.status = Some(error.clone());
                    self.remote.error = true;
                }
                self.status = Some(error.clone());
                continue;
            }
            if result.chapters.is_empty() || self.removed_stories.contains(&result.key) {
                continue;
            }
            let previous = load_cached_story(&result.manga.source_id, &result.manga.url)
                .map(|story| story.chapters)
                .unwrap_or_default();
            if !previous.is_empty() {
                let known: HashSet<&str> = previous
                    .iter()
                    .map(|chapter| chapter.url.as_str())
                    .collect();
                let now = current_unix_seconds();
                let source_name = self.remote_source_display_name(&result.manga.source_id);
                let mut added = 0usize;
                for chapter in &result.chapters {
                    if known.contains(chapter.url.as_str()) {
                        continue;
                    }
                    if self.updates.record_remote(
                        &result.manga.source_id,
                        source_name.as_deref(),
                        &result.manga.url,
                        &result.manga.title,
                        chapter,
                        now,
                    ) {
                        added += 1;
                    }
                }
                if added > 0 {
                    let _ = self.updates.save();
                    self.status = Some(format!("{added} new chapters from saved sources"));
                }
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
                if self.remote.status.as_deref() == Some("Loading details…") {
                    self.remote.status = None;
                }
                changed = true;
            }
        }
        changed
    }

    /// Apply finished chapter downloads and start the next queued chapter.
    /// Returns whether the UI changed.
    pub fn poll_downloads(&mut self) -> bool {
        let mut changed = false;
        while let Some(result) = self
            .download_worker
            .as_ref()
            .and_then(DownloadWorker::try_result)
        {
            let active = self.download_queue.active.take();
            let worker_cancelled =
                matches!(&result.result, Err(error) if error == "operation cancelled");
            if let Some((key, _, _)) = &active {
                let state = if result.result.is_ok() {
                    self.download_queue.completed += 1;
                    DownloadState::Done
                } else if worker_cancelled {
                    DownloadState::Cancelled
                } else {
                    self.download_queue.failed += 1;
                    DownloadState::Error
                };
                self.download_states.insert(key.clone(), state);
            }
            let cancelled = self.download_queue.cancelling || worker_cancelled;
            if cancelled {
                self.download_queue.pending.clear();
                self.download_queue.cancelling = false;
            }
            let (message, error) = match (&result.result, cancelled) {
                (_, true) => ("Download queue cancelled".to_owned(), false),
                (Ok(dir), false) => (
                    format!("Downloaded {} to {}", result.label, dir.display()),
                    false,
                ),
                (Err(error), false) => (
                    format!("Download failed for {}: {error}", result.label),
                    true,
                ),
            };
            self.status = Some(message.clone());
            if self.tab == 3 {
                self.remote.status = Some(message);
                self.remote.error = error;
            }
            self.refresh_download_marks();
            self.pump_download_queue();
            changed = true;
        }
        changed
    }

    fn pump_download_queue(&mut self) {
        if self.download_queue.active.is_some()
            || self.download_queue.cancelling
            || self.download_queue.pending.is_empty()
        {
            return;
        }
        let Some(worker) = self.download_worker.as_ref() else {
            self.download_queue.pending.clear();
            self.status = Some("The download worker is unavailable.".to_owned());
            return;
        };
        if let Some(task) = self.download_queue.pending.pop_front() {
            self.download_states
                .insert(task.key.clone(), DownloadState::Downloading);
            let cancel = task.request.cancel.clone();
            self.download_queue.active = Some((task.key, task.request.label.clone(), cancel));
            worker.request(task.request);
        }
    }

    fn enqueue_downloads(&mut self, tasks: Vec<DownloadTask>) {
        if tasks.is_empty() {
            return;
        }
        if !self.download_queue.running() {
            self.download_queue = DownloadQueue::default();
            self.download_states
                .retain(|_, state| *state == DownloadState::Done);
        }
        for task in tasks {
            self.download_states
                .insert(task.key.clone(), DownloadState::Queued);
            self.download_queue.pending.push_back(task);
        }
        self.download_queue.total = self.download_queue.pending.len()
            + self.download_queue.completed
            + self.download_queue.failed
            + usize::from(self.download_queue.active.is_some());
        self.pump_download_queue();
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

    pub fn open_download_menu(&mut self) {
        if !self.has_downloadable_chapter() {
            self.status = Some("Open a remote chapter to download it.".to_owned());
            return;
        }
        self.download_menu = Some(DownloadMenu::default());
    }

    pub fn close_download_menu(&mut self) {
        self.download_menu = None;
    }

    pub fn download_menu_previous(&mut self) {
        if let Some(menu) = self.download_menu.as_mut() {
            menu.selected = menu.selected.saturating_sub(1);
        }
    }

    pub fn download_menu_next(&mut self) {
        let count = self.download_menu_rows().len();
        if let Some(menu) = self.download_menu.as_mut() {
            if count > 0 {
                menu.selected = (menu.selected + 1).min(count - 1);
            }
        }
    }

    pub fn download_menu_rows(&self) -> Vec<(String, DownloadMenuChoice)> {
        let mut rows = vec![(
            "Download selected chapters".to_owned(),
            DownloadMenuChoice::Selected,
        )];
        for count in [1usize, 5, 10, 25] {
            rows.push((
                format!(
                    "Download next {count} chapter{}",
                    if count == 1 { "" } else { "s" }
                ),
                DownloadMenuChoice::Next(count),
            ));
        }
        rows.push((
            "Download unread chapters".to_owned(),
            DownloadMenuChoice::Unread,
        ));
        rows.push((
            "Download bookmarked chapters".to_owned(),
            DownloadMenuChoice::Bookmarked,
        ));
        if self.download_queue.running() {
            rows.push((
                "Cancel the download queue".to_owned(),
                DownloadMenuChoice::Cancel,
            ));
        }
        rows
    }

    pub fn download_menu_confirm(&mut self) {
        let Some(menu) = self.download_menu.as_ref() else {
            return;
        };
        let rows = self.download_menu_rows();
        let Some((_, choice)) = rows.get(menu.selected).cloned() else {
            return;
        };
        self.download_menu = None;
        match choice {
            DownloadMenuChoice::Selected => self.download_selected_chapter(),
            DownloadMenuChoice::Next(count) => self.download_next(count),
            DownloadMenuChoice::Unread => self.download_unread(),
            DownloadMenuChoice::Bookmarked => self.download_bookmarked(),
            DownloadMenuChoice::Cancel => self.cancel_downloads(),
        }
    }

    /// Download the selected remote chapters to the Mihon-style downloads tree.
    pub fn download_selected_chapter(&mut self) {
        let Some((manga, chapters)) = self.selected_remote_chapters() else {
            self.status = Some("Open a remote chapter to download it.".to_owned());
            return;
        };
        if chapters.is_empty() {
            self.status = Some("No remote chapters selected.".to_owned());
            return;
        }
        self.enqueue_chapters(&manga, chapters);
    }

    pub fn download_next(&mut self, count: usize) {
        let indices = self.story_chapter_indices();
        let position = self.story_chapter_position();
        let selected: Vec<usize> = indices.into_iter().skip(position).take(count).collect();
        self.download_indices(selected);
    }

    pub fn download_unread(&mut self) {
        let selected: Vec<usize> = self
            .story_chapter_indices()
            .into_iter()
            .filter(|index| self.story_chapter_read_state(*index) != ReadState::Read)
            .collect();
        self.download_indices(selected);
    }

    pub fn download_bookmarked(&mut self) {
        let Some((source_id, manga_url)) = self.remote_story_identity() else {
            return;
        };
        let selected: Vec<usize> = self
            .story_chapter_indices()
            .into_iter()
            .filter(|index| {
                self.story_remote_chapters()
                    .and_then(|chapters| chapters.get(*index))
                    .is_some_and(|chapter| {
                        let key = remote_progress_key(&source_id, &manga_url, &chapter.url);
                        self.bookmarks.is_bookmarked(&key)
                    })
            })
            .collect();
        self.download_indices(selected);
    }

    fn remote_story_identity(&self) -> Option<(String, String)> {
        self.remote_story_manga()
            .map(|manga| (manga.source_id.clone(), manga.url.clone()))
    }

    fn download_indices(&mut self, indices: Vec<usize>) {
        let Some(manga) = self.remote_story_manga().cloned() else {
            self.status = Some("Downloads are only available for remote chapters.".to_owned());
            return;
        };
        let chapters: Vec<RemoteChapter> = self
            .story_remote_chapters()
            .map(|chapters| {
                indices
                    .iter()
                    .filter_map(|index| chapters.get(*index).cloned())
                    .collect()
            })
            .unwrap_or_default();
        self.enqueue_chapters(&manga, chapters);
    }

    fn auto_download_new_chapters(
        &mut self,
        manga: &RemoteManga,
        old_urls: &HashSet<String>,
        chapters: &[RemoteChapter],
    ) {
        if old_urls.is_empty() {
            return;
        }
        let key = remote_story_key(&manga.source_id, &manga.url);
        let settings = self
            .config
            .chapter_settings
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.config.chapter_defaults.clone());
        if !settings.download_new_chapters || self.host_client.is_none() {
            return;
        }
        let new_chapters: Vec<RemoteChapter> = chapters
            .iter()
            .filter(|chapter| !old_urls.contains(&chapter.url))
            .cloned()
            .collect();
        if new_chapters.is_empty() {
            return;
        }
        self.enqueue_chapters(manga, new_chapters);
        if self.download_queue.running() {
            self.status = Some("Queued newly discovered chapters".to_owned());
        }
    }

    fn enqueue_chapters(&mut self, manga: &RemoteManga, chapters: Vec<RemoteChapter>) {
        if chapters.is_empty() {
            self.status = Some("No chapters to download.".to_owned());
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
        if self.download_worker.is_none() {
            self.status = Some("The download worker is unavailable.".to_owned());
            return;
        }
        let source = self.remote_source_name(&manga.source_id);
        let queued_keys: HashSet<String> = self
            .download_queue
            .pending
            .iter()
            .map(|task| task.key.clone())
            .chain(
                self.download_queue
                    .active
                    .iter()
                    .map(|(key, _, _)| key.clone()),
            )
            .collect();
        let mut tasks = Vec::new();
        let mut already = 0usize;
        let mut failed = 0usize;
        for chapter in chapters {
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
            let key = remote_progress_key(&manga.source_id, &manga.url, &chapter.url)
                .to_string_lossy()
                .into_owned();
            if queued_keys.contains(&key) {
                already += 1;
                continue;
            }
            let pages = match self.remote_chapter_pages(manga, &chapter) {
                Ok(pages) => pages,
                Err(_) => {
                    failed += 1;
                    continue;
                }
            };
            tasks.push(DownloadTask {
                key,
                request: DownloadRequest {
                    label: format!("{} · {}", manga.title, chapter.name),
                    root: root.clone(),
                    source_name: source.clone(),
                    series_title: manga.title.clone(),
                    chapter_name: chapter.name.clone(),
                    scanlator: chapter.scanlator.clone(),
                    pages,
                    cancel: Arc::new(AtomicBool::new(false)),
                },
            });
        }

        if tasks.is_empty() {
            self.status = Some(if failed > 0 {
                format!("Could not download {failed} chapters")
            } else if already > 0 {
                "Selected chapters are already downloaded".to_owned()
            } else {
                "Nothing to download".to_owned()
            });
            return;
        }
        let count = tasks.len();
        self.enqueue_downloads(tasks);
        let message = format!(
            "Downloading {count} {}…",
            if count == 1 { "chapter" } else { "chapters" }
        );
        self.status = Some(message.clone());
        if self.tab == 3 {
            self.remote.status = Some(message);
            self.remote.error = false;
        }
        self.multi_select = None;
    }

    pub fn cancel_downloads(&mut self) {
        if !self.download_queue.running() {
            self.status = Some("No downloads are running.".to_owned());
            return;
        }
        for task in self.download_queue.pending.drain(..) {
            self.download_states.remove(&task.key);
        }
        if let Some((_, _, cancel)) = &self.download_queue.active {
            cancel.store(true, Ordering::Relaxed);
            self.download_queue.cancelling = true;
            self.status = Some("Cancelling the download queue…".to_owned());
        } else {
            self.download_queue = DownloadQueue::default();
            self.status = Some("Download queue cancelled".to_owned());
        }
    }

    pub fn open_download_delete_confirm(&mut self) {
        if !self.has_downloadable_chapter() {
            self.status = Some("No downloaded chapters are selected.".to_owned());
            return;
        }
        self.download_delete_confirm = true;
    }

    pub fn cancel_download_delete(&mut self) {
        self.download_delete_confirm = false;
    }

    pub fn confirm_download_delete(&mut self) {
        self.download_delete_confirm = false;
        let Some((manga, chapters)) = self.selected_remote_chapters() else {
            self.status = Some("No downloaded chapters are selected.".to_owned());
            return;
        };
        let mut removed = 0usize;
        for chapter in &chapters {
            let key = remote_progress_key(&manga.source_id, &manga.url, &chapter.url)
                .to_string_lossy()
                .into_owned();
            if let Some(dir) = self.downloaded_chapter_dir(&manga, chapter) {
                if std::fs::remove_dir_all(&dir).is_ok() {
                    removed += 1;
                }
                self.download_states.remove(&key);
            }
        }
        self.refresh_download_marks();
        self.multi_select = None;
        self.status = Some(if removed > 0 {
            format!(
                "Deleted {removed} downloaded {}",
                if removed == 1 { "chapter" } else { "chapters" }
            )
        } else {
            "Selected chapters are not downloaded".to_owned()
        });
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
                ReaderOrigin::Library
                | ReaderOrigin::BrowseLocal
                | ReaderOrigin::History
                | ReaderOrigin::Updates => None,
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
        self.reading_stats = self.history.reading_stats();
    }

    pub fn set_updates(&mut self, updates: UpdateStore) {
        self.updates = updates;
    }

    /// Compare the current library against the last snapshot and persist new chapters.
    pub fn observe_updates(&mut self) {
        if let Err(error) = self.updates.observe_now(&self.library) {
            self.status = Some(format!("Could not save updates: {error}"));
        }
        self.updates_selection = self
            .updates_selection
            .min(self.updates.entries().len().saturating_sub(1));
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
        if let Some((manga, chapters, _)) = self.active_remote_story() {
            for chapter in chapters {
                let key = remote_progress_key(&manga.source_id, &manga.url, &chapter.url);
                if !keys.contains(&key) {
                    continue;
                }
                let page = self
                    .progress
                    .entry(&key)
                    .map(|entry| entry.page)
                    .unwrap_or(0);
                let pages = self
                    .progress
                    .entry(&key)
                    .map(|entry| entry.pages)
                    .unwrap_or(0);
                self.queue_remote_state(&manga, &chapter, page, pages);
            }
        }
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
        let query = self.library_query.trim().to_lowercase();
        let mut indices: Vec<usize> = self
            .library
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| in_category(series, category))
            .filter(|(_, series)| query.is_empty() || series.title.to_lowercase().contains(&query))
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
            LibrarySort::ChapterCountDesc => indices.sort_by(|left, right| {
                self.library.series[*right]
                    .chapters
                    .len()
                    .cmp(&self.library.series[*left].chapters.len())
                    .then_with(|| {
                        self.library.series[*left]
                            .title
                            .cmp(&self.library.series[*right].title)
                    })
            }),
            LibrarySort::LatestChapterDesc => indices.sort_by(|left, right| {
                let latest = |index: usize| {
                    self.library.series[index]
                        .chapters
                        .iter()
                        .filter_map(|chapter| {
                            chapter.date.map(|date| (date.year, date.month, date.day))
                        })
                        .max()
                };
                latest(*right).cmp(&latest(*left)).then_with(|| {
                    self.library.series[*left]
                        .title
                        .cmp(&self.library.series[*right].title)
                })
            }),
            LibrarySort::UnreadDesc => indices.sort_by(|left, right| {
                let unread = |index: usize| {
                    self.library.series[index]
                        .chapters
                        .iter()
                        .filter(|chapter| {
                            !matches!(
                                read_state(
                                    self.progress
                                        .entry(&chapter.path)
                                        .map(|entry| (entry.page, entry.pages))
                                ),
                                ReadState::Read
                            )
                        })
                        .count()
                };
                unread(*right).cmp(&unread(*left)).then_with(|| {
                    self.library.series[*left]
                        .title
                        .cmp(&self.library.series[*right].title)
                })
            }),
            LibrarySort::LastReadDesc => {
                let last_read: BTreeMap<String, u64> = self
                    .history
                    .latest_per_series()
                    .into_iter()
                    .map(|(_, entry)| (entry.series_key(), entry.last_read))
                    .collect();
                indices.sort_by(|left, right| {
                    let when = |index: usize| {
                        last_read
                            .get(&series_identity(&self.library.series[index]))
                            .copied()
                            .unwrap_or(0)
                    };
                    when(*right).cmp(&when(*left)).then_with(|| {
                        self.library.series[*left]
                            .title
                            .cmp(&self.library.series[*right].title)
                    })
                });
            }
            LibrarySort::DateAddedDesc => indices.sort_by(|left, right| {
                let added = |index: usize| {
                    let series = &self.library.series[index];
                    self.config
                        .series_added_at
                        .get(&series_identity(series))
                        .copied()
                        .or_else(|| filesystem_added_at(series))
                        .unwrap_or(0)
                };
                added(*right).cmp(&added(*left)).then_with(|| {
                    self.library.series[*left]
                        .title
                        .cmp(&self.library.series[*right].title)
                })
            }),
        }
        indices
    }

    pub fn library_filter_active(&self) -> bool {
        self.library_search || !self.library_query.trim().is_empty()
    }

    pub fn library_search_start(&mut self) {
        if self.tab == 0 && self.view == LibraryView::Series {
            self.library_search = true;
        }
    }

    pub fn library_search_stop(&mut self, clear: bool) {
        self.library_search = false;
        if clear {
            self.library_query.clear();
            self.selected = 0;
            self.library_grid_offset = 0;
        }
    }

    pub fn library_search_push(&mut self, character: char) {
        self.library_query.push(character);
        self.selected = 0;
        self.library_grid_offset = 0;
    }

    pub fn library_search_pop(&mut self) {
        self.library_query.pop();
        self.selected = 0;
        self.library_grid_offset = 0;
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

    /// Whether the shared story screen is currently visible.
    pub fn story_screen_active(&self) -> bool {
        self.reader.is_none()
            && ((self.tab == 0
                && matches!(
                    self.view,
                    LibraryView::Chapters | LibraryView::RemoteDetails
                ))
                || (self.tab == 3
                    && matches!(
                        self.browse_view,
                        BrowseView::Chapters | BrowseView::RemoteDetails
                    )))
    }

    /// Stable key for per-story chapter settings, matching history's series key.
    pub fn current_story_key(&self) -> Option<String> {
        if self.tab == 0 {
            return match self.view {
                LibraryView::Chapters => self
                    .selected_series()
                    .map(|series| format!("local:{}", series.path.display())),
                LibraryView::RemoteDetails => self
                    .library_remote
                    .manga
                    .as_ref()
                    .map(|manga| remote_story_key(&manga.source_id, &manga.url)),
                LibraryView::Series => None,
            };
        }
        if self.tab == 3 {
            return match self.browse_view {
                BrowseView::Chapters => self
                    .selected_browse_local_series()
                    .map(|series| format!("local:{}", series.path.display())),
                BrowseView::RemoteDetails => self
                    .remote
                    .manga
                    .as_ref()
                    .map(|manga| remote_story_key(&manga.source_id, &manga.url)),
                _ => None,
            };
        }
        None
    }

    /// Settings for the open story, falling back to the global defaults.
    pub fn chapter_settings(&self) -> ChapterSettings {
        self.current_story_key()
            .and_then(|key| self.config.chapter_settings.get(&key).cloned())
            .unwrap_or_else(|| self.config.chapter_defaults.clone())
    }

    fn update_chapter_settings(&mut self, update: impl FnOnce(&mut ChapterSettings)) {
        let Some(key) = self.current_story_key() else {
            return;
        };
        let mut settings = self.chapter_settings();
        update(&mut settings);
        self.config.chapter_settings.insert(key, settings);
        self.persist_settings();
    }

    /// Scanlators present in the open story, for the settings modal.
    pub fn chapter_scanlators(&self) -> Vec<String> {
        let mut scanlators = std::collections::BTreeSet::new();
        if self.tab == 0 && self.view == LibraryView::Chapters {
            if let Some(series) = self.selected_series() {
                for chapter in &series.chapters {
                    if let Some(scanlator) = chapter.scanlator.as_deref() {
                        if !scanlator.is_empty() {
                            scanlators.insert(scanlator.to_owned());
                        }
                    }
                }
            }
        } else if let Some(chapters) = self.story_remote_chapters() {
            for chapter in chapters {
                if let Some(scanlator) = chapter.scanlator.as_deref() {
                    if !scanlator.is_empty() {
                        scanlators.insert(scanlator.to_owned());
                    }
                }
            }
        }
        scanlators.into_iter().collect()
    }

    pub fn chapter_setting_rows(&self) -> Vec<ChapterSettingRow> {
        let mut rows = vec![
            ChapterSettingRow::Downloaded,
            ChapterSettingRow::Unread,
            ChapterSettingRow::Bookmarked,
            ChapterSettingRow::Sort,
            ChapterSettingRow::Direction,
            ChapterSettingRow::Display,
            ChapterSettingRow::DownloadNewChapters,
            ChapterSettingRow::Reset,
        ];
        rows.extend((0..self.chapter_scanlators().len()).map(ChapterSettingRow::Scanlator));
        rows
    }

    /// Source indices after applying the story's filters and sort.
    pub fn story_chapter_indices(&self) -> Vec<usize> {
        let settings = self.chapter_settings();
        if self.tab == 0 {
            match self.view {
                LibraryView::Chapters => {
                    let Some(series) = self.selected_series() else {
                        return Vec::new();
                    };
                    let metas = local_chapter_metas(series, &self.progress, &self.bookmarks);
                    return visible_chapters(&metas, &settings);
                }
                LibraryView::RemoteDetails => {
                    let Some(manga) = self.library_remote.manga.as_ref() else {
                        return Vec::new();
                    };
                    let metas = remote_chapter_metas(
                        &manga.source_id,
                        &manga.url,
                        &self.library_remote.chapters,
                        &self.library_remote.downloaded,
                        &self.progress,
                        &self.bookmarks,
                    );
                    return visible_chapters(&metas, &settings);
                }
                LibraryView::Series => return Vec::new(),
            }
        }
        if self.tab == 3 {
            match self.browse_view {
                BrowseView::Chapters => {
                    let Some(series) = self.selected_browse_local_series() else {
                        return Vec::new();
                    };
                    let metas = local_chapter_metas(series, &self.progress, &self.bookmarks);
                    return visible_chapters(&metas, &settings);
                }
                BrowseView::RemoteDetails => {
                    let Some(manga) = self.remote.manga.as_ref() else {
                        return Vec::new();
                    };
                    let metas = remote_chapter_metas(
                        &manga.source_id,
                        &manga.url,
                        &self.remote.chapters,
                        &self.remote.downloaded,
                        &self.progress,
                        &self.bookmarks,
                    );
                    return visible_chapters(&metas, &settings);
                }
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }

    fn story_remote_chapters(&self) -> Option<&[RemoteChapter]> {
        if self.tab == 0 && self.view == LibraryView::RemoteDetails {
            return Some(&self.library_remote.chapters);
        }
        if self.tab == 3 && self.browse_view == BrowseView::RemoteDetails {
            return Some(&self.remote.chapters);
        }
        None
    }

    /// Raw source index of the chapter cursor.
    pub fn story_chapter_cursor(&self) -> Option<usize> {
        if self.tab == 0 {
            return match self.view {
                LibraryView::Chapters => Some(self.chapter_index),
                LibraryView::RemoteDetails => Some(self.library_remote.chapter_index),
                LibraryView::Series => None,
            };
        }
        if self.tab == 3 {
            return match self.browse_view {
                BrowseView::Chapters => Some(self.browse_local_chapter),
                BrowseView::RemoteDetails => Some(self.browse_selection),
                _ => None,
            };
        }
        None
    }

    fn set_story_chapter_cursor(&mut self, index: usize) {
        if self.tab == 0 {
            match self.view {
                LibraryView::Chapters => self.chapter_index = index,
                LibraryView::RemoteDetails => self.library_remote.chapter_index = index,
                LibraryView::Series => {}
            }
        } else if self.tab == 3 {
            match self.browse_view {
                BrowseView::Chapters => self.browse_local_chapter = index,
                BrowseView::RemoteDetails => self.browse_selection = index,
                _ => {}
            }
        }
    }

    /// Position of the cursor in the filtered chapter list.
    pub fn story_chapter_position(&self) -> usize {
        let indices = self.story_chapter_indices();
        let cursor = self.story_chapter_cursor().unwrap_or(0);
        indices
            .iter()
            .position(|index| *index == cursor)
            .unwrap_or(0)
    }

    /// Move the cursor to a position in the filtered chapter list.
    pub fn select_story_position(&mut self, position: usize) {
        let indices = self.story_chapter_indices();
        if let Some(index) = indices.get(position).copied() {
            self.set_story_chapter_cursor(index);
            self.status = None;
        }
        self.sync_story_list_state();
    }

    fn sync_story_list_state(&mut self) {
        let position = self.story_chapter_position();
        let count = self.story_chapter_indices().len();
        let selected = (count > 0).then_some(position.min(count.saturating_sub(1)));
        if self.tab == 0 {
            match self.view {
                LibraryView::Chapters => self.chapter_list_state.select(selected),
                LibraryView::RemoteDetails => self.library_remote.list_state.select(selected),
                LibraryView::Series => {}
            }
        } else if self.tab == 3 {
            match self.browse_view {
                BrowseView::Chapters => self.browse_local_list_state.select(selected),
                BrowseView::RemoteDetails => self.browse_list_state.select(selected),
                _ => {}
            }
        }
    }

    pub fn story_chapter_total(&self) -> usize {
        if self.tab == 0 {
            return match self.view {
                LibraryView::Chapters => self.chapter_count(),
                LibraryView::RemoteDetails => self.library_remote.chapters.len(),
                LibraryView::Series => 0,
            };
        }
        if self.tab == 3 {
            return match self.browse_view {
                BrowseView::Chapters => self.story_chapter_indices().len(),
                BrowseView::RemoteDetails => self.story_chapter_indices().len(),
                _ => 0,
            };
        }
        0
    }

    pub fn story_missing_chapters(&self) -> usize {
        if self.tab == 0 && self.view == LibraryView::Chapters {
            if let Some(series) = self.selected_series() {
                return missing_chapter_count(series.chapters.iter().map(|chapter| chapter.number));
            }
        }
        if let Some(chapters) = self.story_remote_chapters() {
            return missing_chapter_count(
                chapters.iter().map(|chapter| {
                    (chapter.chapter_number >= 0.0).then_some(chapter.chapter_number)
                }),
            );
        }
        0
    }

    /// Raw source indices selected by the cursor or the multi-selection.
    fn story_selected_indices(&self) -> Vec<usize> {
        let local_scope = (self.tab == 0 && self.view == LibraryView::Chapters)
            || (self.tab == 3 && self.browse_view == BrowseView::Chapters);
        let scope = if local_scope {
            SelectionScope::LocalChapters
        } else {
            SelectionScope::RemoteChapters
        };
        if let Some(keys) = self.selected_keys(scope) {
            let mut indices = Vec::new();
            if local_scope {
                let chapters = if self.tab == 0 {
                    self.selected_series()
                        .map(|series| series.chapters.as_slice())
                        .unwrap_or_default()
                } else {
                    self.selected_browse_local_series()
                        .map(|series| series.chapters.as_slice())
                        .unwrap_or_default()
                };
                for (index, chapter) in chapters.iter().enumerate() {
                    if keys.contains(&format!("chapter:{}", chapter.path.display())) {
                        indices.push(index);
                    }
                }
            } else if let (Some(manga), Some(chapters)) =
                (self.remote_story_manga(), self.story_remote_chapters())
            {
                for (index, chapter) in chapters.iter().enumerate() {
                    if keys.contains(&format!(
                        "rchap:{}\n{}\n{}",
                        manga.source_id, manga.url, chapter.url
                    )) {
                        indices.push(index);
                    }
                }
            }
            if !indices.is_empty() {
                return indices;
            }
        }
        self.story_chapter_cursor().into_iter().collect()
    }

    fn remote_story_manga(&self) -> Option<&RemoteManga> {
        if self.tab == 0 && self.view == LibraryView::RemoteDetails {
            return self.library_remote.manga.as_ref();
        }
        if self.tab == 3 && self.browse_view == BrowseView::RemoteDetails {
            return self.remote.manga.as_ref();
        }
        None
    }

    /// Mark selected chapters read or unread. Read state lives in progress, so this
    /// updates the same store the reader writes and history/stat views observe.
    pub fn mark_selected_read(&mut self, read: bool) {
        let indices = self.story_selected_indices();
        if indices.is_empty() {
            return;
        }
        for index in &indices {
            self.mark_story_chapter_read(*index, read);
        }
        let _ = self.progress.save();
        self.multi_select = None;
        self.sync_story_list_state();
        self.status = Some(format!(
            "Marked {} chapter{} {}",
            indices.len(),
            if indices.len() == 1 { "" } else { "s" },
            if read { "read" } else { "unread" }
        ));
    }

    /// Mark every chapter after the cursor in display order as read.
    pub fn mark_previous_read(&mut self) {
        let indices = self.story_chapter_indices();
        let position = self.story_chapter_position();
        let selected: Vec<usize> = indices
            .iter()
            .skip(position.saturating_add(1))
            .copied()
            .collect();
        if selected.is_empty() {
            self.status = Some("No previous chapters to mark.".to_owned());
            return;
        }
        for index in &selected {
            self.mark_story_chapter_read(*index, true);
        }
        let _ = self.progress.save();
        self.sync_story_list_state();
        self.status = Some(format!("Marked {} previous chapters read", selected.len()));
    }

    /// Open the in-progress chapter, else the first unread chapter, in display order.
    /// Open the next in-progress/unread chapter for the selected Library series.
    pub fn continue_selected_series(&mut self) {
        if self.tab != 0 || self.view != LibraryView::Series || self.selected_series().is_none() {
            return;
        }
        self.view = LibraryView::Chapters;
        self.chapter_index = 0;
        self.continue_reading();
    }

    pub fn continue_reading(&mut self) {
        let indices = self.story_chapter_indices();
        if indices.is_empty() {
            self.status = Some("This story has no chapters.".to_owned());
            return;
        }
        let target = indices
            .iter()
            .copied()
            .find(|index| self.story_chapter_read_state(*index) == ReadState::InProgress)
            .or_else(|| {
                indices
                    .iter()
                    .copied()
                    .find(|index| self.story_chapter_read_state(*index) == ReadState::Unread)
            })
            .unwrap_or(indices[0]);
        self.set_story_chapter_cursor(target);
        self.sync_story_list_state();
        if self.tab == 0 {
            match self.view {
                LibraryView::Chapters => self.open_chapter(),
                LibraryView::RemoteDetails => self.open_library_remote_chapter(target),
                LibraryView::Series => {}
            }
        } else if self.tab == 3 {
            match self.browse_view {
                BrowseView::Chapters => self.open_browse_local_chapter(target),
                BrowseView::RemoteDetails => self.open_remote_chapter(target),
                _ => {}
            }
        }
    }

    fn story_chapter_read_state(&self, source_index: usize) -> ReadState {
        if self.tab == 0 && self.view == LibraryView::Chapters {
            if let Some(chapter) = self
                .selected_series()
                .and_then(|series| series.chapters.get(source_index))
            {
                return read_state(
                    self.progress
                        .entry(&chapter.path)
                        .map(|entry| (entry.page, entry.pages)),
                );
            }
            return ReadState::Unread;
        }
        if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            if let Some(chapter) = self
                .selected_browse_local_series()
                .and_then(|series| series.chapters.get(source_index))
            {
                return read_state(
                    self.progress
                        .entry(&chapter.path)
                        .map(|entry| (entry.page, entry.pages)),
                );
            }
            return ReadState::Unread;
        }
        let Some(manga) = self.remote_story_manga() else {
            return ReadState::Unread;
        };
        let Some(chapter_url) = self
            .story_remote_chapters()
            .and_then(|chapters| chapters.get(source_index))
            .map(|chapter| chapter.url.as_str())
        else {
            return ReadState::Unread;
        };
        let key = remote_progress_key(&manga.source_id, &manga.url, chapter_url);
        read_state(
            self.progress
                .entry(&key)
                .map(|entry| (entry.page, entry.pages)),
        )
    }

    fn mark_story_chapter_read(&mut self, source_index: usize, read: bool) {
        if self.tab == 0 && self.view == LibraryView::Chapters {
            let chapter = self
                .selected_series()
                .and_then(|series| series.chapters.get(source_index))
                .cloned();
            if let Some(chapter) = chapter {
                self.set_local_chapter_read(&chapter, read);
            }
            return;
        }
        if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            let chapter = self
                .selected_browse_local_series()
                .and_then(|series| series.chapters.get(source_index))
                .cloned();
            if let Some(chapter) = chapter {
                self.set_local_chapter_read(&chapter, read);
            }
            return;
        }
        let Some((source_id, manga_url)) = self
            .remote_story_manga()
            .map(|manga| (manga.source_id.clone(), manga.url.clone()))
        else {
            return;
        };
        let Some(chapter_url) = self
            .story_remote_chapters()
            .and_then(|chapters| chapters.get(source_index))
            .map(|chapter| chapter.url.clone())
        else {
            return;
        };
        let key = remote_progress_key(&source_id, &manga_url, &chapter_url);
        if read {
            let pages = self
                .progress
                .entry(&key)
                .map(|entry| entry.pages)
                .filter(|pages| *pages > 0)
                .unwrap_or(1);
            self.progress.update(&key, pages.saturating_sub(1), pages);
            let page = pages.saturating_sub(1);
            if let (Some(manga), Some(chapter)) = (
                self.remote_story_manga().cloned(),
                self.story_remote_chapters()
                    .and_then(|chapters| chapters.get(source_index))
                    .cloned(),
            ) {
                self.queue_remote_state(&manga, &chapter, page, pages);
            }
        } else {
            self.progress.remove(&key);
            if let (Some(manga), Some(chapter)) = (
                self.remote_story_manga().cloned(),
                self.story_remote_chapters()
                    .and_then(|chapters| chapters.get(source_index))
                    .cloned(),
            ) {
                self.queue_remote_state(&manga, &chapter, 0, 0);
            }
        }
    }

    fn set_local_chapter_read(&mut self, chapter: &Chapter, read: bool) {
        if read {
            let pages = chapter_pages(chapter)
                .ok()
                .map(|pages| pages.len())
                .filter(|pages| *pages > 0)
                .unwrap_or(1);
            self.progress
                .update(&chapter.path, pages.saturating_sub(1), pages);
        } else {
            self.progress.remove(&chapter.path);
        }
    }

    pub fn chapter_settings_adjust_sort(&mut self, delta: i32) {
        self.update_chapter_settings(|settings| {
            settings.sort = if delta < 0 {
                match settings.sort {
                    ChapterSort::Source => ChapterSort::Alphabet,
                    ChapterSort::Number => ChapterSort::Source,
                    ChapterSort::UploadDate => ChapterSort::Number,
                    ChapterSort::Alphabet => ChapterSort::UploadDate,
                }
            } else {
                settings.sort.next()
            };
        });
        self.clamp_story_cursor();
    }

    pub fn cycle_chapter_display(&mut self) {
        self.update_chapter_settings(|settings| settings.display = settings.display.next());
    }

    /// Copy the story path or source URL through OSC 52, the clipboard sequence
    /// supported by most terminals without an extra dependency.
    pub fn copy_story_reference(&mut self) {
        let Some(value) = self.story_reference() else {
            return;
        };
        match write_osc52_clipboard(&value) {
            Ok(()) => self.status = Some(format!("Copied {value}")),
            Err(error) => self.status = Some(format!("Could not copy: {error}")),
        }
    }

    /// Open a local series folder or an absolute source URL in the system handler.
    pub fn open_story_source(&mut self) {
        let Some(value) = self.story_reference() else {
            return;
        };
        let is_url = value.starts_with("http://") || value.starts_with("https://");
        if !is_url && !Path::new(&value).exists() {
            self.status =
                Some("This source returns a relative URL; copy it with C instead.".to_owned());
            return;
        }
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match std::process::Command::new(opener).arg(&value).spawn() {
            Ok(_) => self.status = Some(format!("Opened {value}")),
            Err(error) => self.status = Some(format!("Could not open {value}: {error}")),
        }
    }

    fn story_reference(&self) -> Option<String> {
        if let Some(manga) = self.remote_story_manga() {
            if manga.url.starts_with("http://") || manga.url.starts_with("https://") {
                return Some(manga.url.clone());
            }
            if let Some(base) = self.source_base_url(&manga.source_id) {
                return Some(join_source_url(base, &manga.url));
            }
            return Some(manga.url.clone());
        }
        if self.tab == 0 && self.view == LibraryView::Chapters {
            return self
                .selected_series()
                .map(|series| series.path.display().to_string());
        }
        if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            return self
                .selected_browse_local_series()
                .map(|series| series.path.display().to_string());
        }
        None
    }

    fn source_base_url(&self, source_id: &str) -> Option<&str> {
        self.remote
            .extensions
            .iter()
            .flat_map(|extension| &extension.sources)
            .find(|source| source.id == source_id)
            .map(|source| source.base_url.as_str())
            .filter(|base| !base.trim().is_empty())
    }

    pub fn toggle_cover_overlay(&mut self) {
        self.cover_overlay = !self.cover_overlay;
    }

    pub fn edit_custom_cover(&mut self) {
        if self.tab == 0 && self.view == LibraryView::Chapters
            || self.tab == 3 && self.browse_view == BrowseView::Chapters
        {
            self.open_text_input(TextAction::SetCustomCover, "Custom cover path", "");
        } else if self.remote_story_manga().is_some() {
            let saved = self
                .remote_story_manga()
                .is_some_and(|manga| self.is_favorite(&manga.source_id, &manga.url));
            if !saved {
                self.status = Some("Save this manga before setting a custom cover.".to_owned());
                return;
            }
            self.open_text_input(TextAction::SetCustomCover, "Custom cover path", "");
        } else {
            self.status = Some("Open a story to set its cover.".to_owned());
        }
    }

    fn set_custom_cover(&mut self, value: &str) {
        let value = value.trim();
        if let Some(manga) = self.remote_story_manga().cloned() {
            let Some(target) = remote_cover_path(&manga.source_id, &manga.url) else {
                self.status = Some("Cannot resolve the cover cache path.".to_owned());
                return;
            };
            if value.is_empty() {
                let _ = std::fs::remove_file(&target);
                self.tile_covers.clear();
                self.failed_covers.clear();
                self.status = Some(format!("Removed custom cover for {}", manga.title));
                return;
            }
            let path = PathBuf::from(value);
            if !path.is_file() {
                self.status = Some(format!("Cover file not found: {}", path.display()));
                return;
            }
            if let Some(parent) = target.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    self.status = Some("Cannot create the cover cache directory.".to_owned());
                    return;
                }
            }
            match std::fs::copy(&path, &target) {
                Ok(_) => {
                    self.tile_covers.clear();
                    self.failed_covers
                        .remove(&target.to_string_lossy().into_owned());
                    self.status = Some(format!("Set custom cover for {}", manga.title));
                }
                Err(error) => self.status = Some(format!("Could not set cover: {error}")),
            }
            return;
        }

        if value.is_empty() {
            self.status = Some("Enter a cover image path.".to_owned());
            return;
        }
        let path = PathBuf::from(value);
        if !path.is_file() {
            self.status = Some(format!("Cover file not found: {}", path.display()));
            return;
        }
        let series_path = if self.tab == 0 && self.view == LibraryView::Chapters {
            self.selected_series().map(|series| series.path.clone())
        } else if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            self.selected_browse_local_series()
                .map(|series| series.path.clone())
        } else {
            None
        };
        let Some(series_path) = series_path else {
            return;
        };
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("jpg")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif") {
            self.status = Some("Use a jpg, png, webp, or gif image.".to_owned());
            return;
        }
        let target = series_path.join(format!("cover.{extension}"));
        match std::fs::copy(&path, &target) {
            Ok(_) => {
                if let Some(series) = self
                    .library
                    .series
                    .iter_mut()
                    .find(|series| series.path == series_path)
                {
                    series.cover = Some(target.clone());
                }
                self.tile_covers.clear();
                self.status = Some(format!("Set cover {}", target.display()));
            }
            Err(error) => self.status = Some(format!("Could not set cover: {error}")),
        }
    }

    /// Cover file for the open story, if one exists.
    pub fn current_story_cover(&mut self) -> Option<PathBuf> {
        if self.tab == 0 && self.view == LibraryView::Chapters {
            return self
                .selected_series()
                .and_then(|series| series.cover.clone());
        }
        if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            return self
                .selected_browse_local_series()
                .and_then(|series| series.cover.clone());
        }
        let (source_id, url, thumbnail) = self.remote_story_manga().map(|manga| {
            (
                manga.source_id.clone(),
                manga.url.clone(),
                manga.thumbnail_url.clone(),
            )
        })?;
        self.ensure_remote_cover(&source_id, &url, thumbnail.as_deref())
    }

    pub fn edit_story_notes(&mut self) {
        let Some(key) = self.current_story_key() else {
            return;
        };
        let value = self
            .config
            .story_notes
            .get(&key)
            .cloned()
            .unwrap_or_default();
        self.open_text_input(TextAction::StoryNotes, "Story notes", &value);
    }

    fn save_story_notes(&mut self, value: &str) {
        let Some(key) = self.current_story_key() else {
            return;
        };
        let value = value.trim().to_owned();
        if value.is_empty() {
            self.config.story_notes.remove(&key);
        } else {
            self.config.story_notes.insert(key, value);
        }
        self.persist_settings();
        self.status = Some("Saved story notes".to_owned());
    }

    /// Tags for the open story, whether it is local or remote.
    fn current_story_tags(&self) -> Vec<String> {
        if self.tab == 0 && self.view == LibraryView::Chapters {
            self.selected_series()
                .map(|series| series.tags.clone())
                .unwrap_or_default()
        } else if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            self.selected_browse_local_series()
                .map(|series| series.tags.clone())
                .unwrap_or_default()
        } else {
            self.remote_story_manga()
                .map(|manga| manga.genre.clone())
                .unwrap_or_default()
        }
    }

    /// Open the cached cover in the system image viewer. This is the terminal
    /// equivalent of Mihon's cover share action; there is no Android share sheet.
    pub fn share_story_cover(&mut self) {
        let Some(path) = self.current_story_cover() else {
            self.status = Some("No cover is available for this story.".to_owned());
            return;
        };
        let target = path.to_string_lossy().into_owned();
        match open_external_target(&target) {
            Ok(()) => self.status = Some(format!("Opened cover {target}")),
            Err(error) => self.status = Some(format!("Could not open cover: {error}")),
        }
    }

    /// Copy the open story's tags through OSC 52, one per line.
    pub fn copy_story_tags(&mut self) {
        let tags = self.current_story_tags();
        if tags.is_empty() {
            self.status = Some("This story has no tags.".to_owned());
            return;
        }
        let text = tags.join(", ");
        match write_osc52_clipboard(&text) {
            Ok(()) => self.status = Some(format!("Copied {} tags", tags.len())),
            Err(error) => self.status = Some(format!("Could not copy: {error}")),
        }
    }

    /// Write a Mihon-readable `ComicInfo.xml` for the open local series.
    ///
    /// Existing files are never overwritten: hand-edited metadata and fields
    /// tsuiyomi does not model would be lost.
    pub fn export_story_metadata(&mut self) {
        let Some(series) = self.current_local_series() else {
            self.status = Some("Metadata export is only available for local series.".to_owned());
            return;
        };
        let path = series.path.join("ComicInfo.xml");
        if path.exists() {
            self.status = Some(format!(
                "{} already exists; not overwriting.",
                path.display()
            ));
            return;
        }
        let info = ComicInfo {
            title: Some(series.title.clone()),
            series: Some(series.title.clone()),
            summary: (!series.description.trim().is_empty()).then(|| series.description.clone()),
            writer: series.author.clone(),
            penciller: series.artist.clone(),
            genre: (!series.tags.is_empty()).then(|| series.tags.join(", ")),
            publishing_status: series.status.clone(),
            ..ComicInfo::default()
        };
        match write_comic_info(&path, &info) {
            Ok(()) => self.status = Some(format!("Wrote {}", path.display())),
            Err(error) => self.status = Some(format!("Could not write metadata: {error}")),
        }
    }

    fn current_local_series(&self) -> Option<&Series> {
        if self.tab == 0 && self.view == LibraryView::Chapters {
            self.selected_series()
        } else if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            self.selected_browse_local_series()
        } else {
            None
        }
    }

    fn tracker_service_label(service: &str) -> &'static str {
        if service == "mal" {
            "MyAnimeList"
        } else {
            "AniList"
        }
    }

    fn current_story_title(&self) -> Option<String> {
        if let Some(manga) = self.remote_story_manga() {
            return Some(manga.title.clone());
        }
        if self.tab == 0 && self.view == LibraryView::Chapters {
            return self.selected_series().map(|series| series.title.clone());
        }
        if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            return self
                .selected_browse_local_series()
                .map(|series| series.title.clone());
        }
        None
    }

    fn current_read_chapter_count(&self) -> u32 {
        self.story_chapter_indices()
            .into_iter()
            .filter(|index| self.story_chapter_read_state(*index) == ReadState::Read)
            .count() as u32
    }

    fn set_tracker_message(&mut self, message: String, error: bool) {
        self.status = Some(message.clone());
        self.remote.status = Some(message);
        self.remote.error = error;
    }

    pub fn open_tracker(&mut self) {
        if !self.story_screen_active() {
            return;
        }
        let Some(key) = self.current_story_key() else {
            return;
        };
        if let Some(link) = self.config.tracking.get(&key).cloned() {
            self.tracker_unlink_confirm = true;
            self.status = Some(format!(
                "Unlink {} {}? Enter/y confirm · Esc cancel",
                Self::tracker_service_label(&link.service),
                link.title
            ));
            return;
        }
        let title = self.current_story_title().unwrap_or_default();
        self.open_text_input(TextAction::TrackerSearch, "Search AniList", &title);
    }

    fn run_tracker_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        if let Some(worker) = self.tracker_worker.as_ref() {
            self.tracker_generation = self.tracker_generation.wrapping_add(1);
            worker.request(TrackerRequest::Search {
                generation: self.tracker_generation,
                query: query.to_owned(),
                mal_client_id: self.config.mal_client_id.clone(),
            });
            self.set_tracker_message("Searching AniList…".to_owned(), false);
            return;
        }
        match search_anilist(query) {
            Ok(results) => self.show_tracker_results(results),
            Err(error) => self.set_tracker_error(error.to_string()),
        }
    }

    fn show_tracker_results(&mut self, results: Vec<TrackerMedia>) {
        if results.is_empty() {
            self.set_tracker_message("No AniList manga found.".to_owned(), true);
            return;
        }
        self.set_tracker_message(format!("{} AniList results", results.len()), false);
        self.tracker_picker = Some(TrackerPicker {
            results,
            selected: 0,
        });
    }

    fn set_tracker_error(&mut self, message: String) {
        self.set_tracker_message(message, true);
    }

    pub fn tracker_picker_previous(&mut self) {
        if let Some(picker) = self.tracker_picker.as_mut() {
            picker.selected = picker.selected.saturating_sub(1);
        }
    }

    pub fn tracker_picker_next(&mut self) {
        if let Some(picker) = self.tracker_picker.as_mut() {
            picker.selected = (picker.selected + 1).min(picker.results.len().saturating_sub(1));
        }
    }

    pub fn cancel_tracker_picker(&mut self) {
        self.tracker_picker = None;
    }

    pub fn confirm_tracker_picker(&mut self) {
        let Some(picker) = self.tracker_picker.take() else {
            return;
        };
        let Some(media) = picker.results.get(picker.selected).cloned() else {
            return;
        };
        let Some(key) = self.current_story_key() else {
            return;
        };
        let progress = self.current_read_chapter_count();
        self.config.tracking.insert(
            key,
            TrackerLink {
                service: media.service.clone(),
                media_id: media.id,
                title: media.title.clone(),
                total_chapters: media.chapters,
                progress,
            },
        );
        self.persist_settings();
        self.remote.status = Some(if self.config.anilist_token.is_some() {
            format!("Linked AniList {}", media.title)
        } else {
            format!(
                "Linked {} · set an AniList token to sync progress",
                media.title
            )
        });
        self.remote.error = false;
    }

    pub fn cancel_tracker_unlink(&mut self) {
        self.tracker_unlink_confirm = false;
        self.remote.status = None;
    }

    pub fn confirm_tracker_unlink(&mut self) {
        self.tracker_unlink_confirm = false;
        let Some(key) = self.current_story_key() else {
            return;
        };
        let removed = self.config.tracking.remove(&key);
        if let Some(link) = removed {
            self.persist_settings();
            self.set_tracker_message(
                format!("Unlinked {}", Self::tracker_service_label(&link.service)),
                false,
            );
        }
    }

    fn open_anilist_token(&mut self) {
        let current = self.config.anilist_token.clone().unwrap_or_default();
        self.open_text_input(TextAction::AniListToken, "AniList token", &current);
    }

    fn set_anilist_token(&mut self, value: &str) {
        let value = value.trim();
        self.config.anilist_token = (!value.is_empty()).then(|| value.to_owned());
        self.persist_settings();
        self.set_tracker_message(
            if self.config.anilist_token.is_some() {
                "Saved AniList token".to_owned()
            } else {
                "Cleared AniList token".to_owned()
            },
            false,
        );
    }

    fn open_mal_client_id(&mut self) {
        let current = self.config.mal_client_id.clone().unwrap_or_default();
        self.open_text_input(TextAction::MalClientId, "MyAnimeList client ID", &current);
    }

    fn set_mal_client_id(&mut self, value: &str) {
        let value = value.trim();
        self.config.mal_client_id = (!value.is_empty()).then(|| value.to_owned());
        self.persist_settings();
        self.set_tracker_message(
            if self.config.mal_client_id.is_some() {
                "Saved MyAnimeList client ID".to_owned()
            } else {
                "Cleared MyAnimeList client ID".to_owned()
            },
            false,
        );
    }

    fn open_mal_token(&mut self) {
        let current = self.config.mal_token.clone().unwrap_or_default();
        self.open_text_input(TextAction::MalToken, "MyAnimeList token", &current);
    }

    fn set_mal_token(&mut self, value: &str) {
        let value = value.trim();
        self.config.mal_token = (!value.is_empty()).then(|| value.to_owned());
        self.persist_settings();
        self.set_tracker_message(
            if self.config.mal_token.is_some() {
                "Saved MyAnimeList token".to_owned()
            } else {
                "Cleared MyAnimeList token".to_owned()
            },
            false,
        );
    }

    fn tracker_token_for(&self, service: &str) -> Option<String> {
        (if service == "mal" {
            self.config.mal_token.clone()
        } else {
            self.config.anilist_token.clone()
        })
        .filter(|token| !token.trim().is_empty())
    }

    fn sync_tracker_progress(&mut self) {
        let Some(key) = self.current_story_key() else {
            return;
        };
        let Some(link) = self.config.tracking.get(&key).cloned() else {
            return;
        };
        let progress = self.current_read_chapter_count();
        if progress <= link.progress {
            return;
        }
        let Some(token) = self.tracker_token_for(&link.service) else {
            return;
        };
        if let Some(link) = self.config.tracking.get_mut(&key) {
            link.progress = progress;
        }
        self.persist_settings();
        let total = link.total_chapters;
        if let Some(worker) = self.tracker_worker.as_ref() {
            worker.request(TrackerRequest::Update {
                service: link.service.clone(),
                token,
                media_id: link.media_id,
                progress,
                total,
            });
        } else {
            let _ = if link.service == "mal" {
                update_mal_progress(&token, link.media_id, progress, total)
            } else {
                update_anilist_progress(&token, link.media_id, progress, total)
            };
        }
    }

    pub fn poll_tracker_worker(&mut self) -> bool {
        let Some(event) = self
            .tracker_worker
            .as_ref()
            .and_then(TrackerWorker::try_result)
        else {
            return false;
        };
        match event {
            TrackerEvent::Search { generation, result } => {
                if generation != self.tracker_generation {
                    return false;
                }
                match result {
                    Ok(results) => self.show_tracker_results(results),
                    Err(error) => self.set_tracker_error(error),
                }
            }
            TrackerEvent::Update {
                result,
                media_id,
                progress,
            } => {
                if let Err(error) = result {
                    self.set_tracker_message(format!("AniList update failed: {error}"), true);
                    let _ = media_id;
                    let _ = progress;
                }
            }
        }
        true
    }

    pub fn open_tag_picker(&mut self) {
        let tags = self.current_story_tags();
        if tags.is_empty() {
            self.status = Some("This story has no tags.".to_owned());
            return;
        }
        self.tag_picker = Some(TagPicker { tags, selected: 0 });
    }

    pub fn tag_picker_previous(&mut self) {
        if let Some(picker) = self.tag_picker.as_mut() {
            picker.selected = picker.selected.saturating_sub(1);
        }
    }

    pub fn tag_picker_next(&mut self) {
        if let Some(picker) = self.tag_picker.as_mut() {
            if !picker.tags.is_empty() {
                picker.selected = (picker.selected + 1).min(picker.tags.len() - 1);
            }
        }
    }

    pub fn cancel_tag_picker(&mut self) {
        self.tag_picker = None;
    }

    pub fn confirm_tag_picker(&mut self) {
        let Some(picker) = self.tag_picker.take() else {
            return;
        };
        let Some(tag) = picker.tags.get(picker.selected).cloned() else {
            return;
        };
        if let Some(manga) = self.remote_story_manga().cloned() {
            let Some(source) = self
                .remote
                .sources
                .iter()
                .find(|source| source.id == manga.source_id)
                .cloned()
            else {
                self.status = Some("Source is not loaded; open it from Browse first.".to_owned());
                return;
            };
            self.record_source_opened_source(&source);
            self.tab = 3;
            self.remote.source = Some(source);
            self.remote.query = tag;
            self.remote.listing = RemoteListing::Search;
            self.remote.page = 1;
            self.remote.filters.clear();
            self.remote.default_filters.clear();
            self.remote.filters_loaded = false;
            self.load_remote_filters();
            self.load_remote_results(RemoteListing::Search);
            self.browse_view = BrowseView::RemoteResults;
            self.browse_selection = 0;
            self.browse_grid_offset = 0;
            self.select_browse_entry(0);
            return;
        }
        self.tab = 3;
        self.browse_local_query = tag.clone();
        self.browse_view = BrowseView::Local;
        self.browse_selection = 0;
        self.browse_grid_offset = 0;
        self.select_browse_entry(0);
        self.status = Some(format!("Filtering local series by {tag}"));
    }

    pub fn open_category_picker(&mut self) {
        if let Some(manga) = self.remote_story_manga().cloned() {
            let Some(favorite) = self.config.remote_library.iter().find(|favorite| {
                favorite.source_id == manga.source_id && favorite.url == manga.url
            }) else {
                self.status =
                    Some("Save this manga to the library to change categories.".to_owned());
                return;
            };
            let chosen = favorite.categories.iter().cloned().collect();
            self.category_picker = Some(CategoryPicker {
                selected: 0,
                chosen,
                local: false,
            });
            return;
        }
        let series = if self.tab == 0 && self.view == LibraryView::Chapters {
            self.selected_series().cloned()
        } else if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            self.selected_browse_local_series().cloned()
        } else {
            None
        };
        let Some(series) = series else {
            self.status = Some("Open a story to change its categories.".to_owned());
            return;
        };
        let chosen = series.categories.iter().cloned().collect();
        self.category_picker = Some(CategoryPicker {
            selected: 0,
            chosen,
            local: true,
        });
    }

    pub fn category_picker_previous(&mut self) {
        if let Some(picker) = self.category_picker.as_mut() {
            picker.selected = picker.selected.saturating_sub(1);
        }
    }

    pub fn category_picker_next(&mut self) {
        let count = self.library.categories.len();
        if let Some(picker) = self.category_picker.as_mut() {
            if count > 0 {
                picker.selected = (picker.selected + 1).min(count - 1);
            }
        }
    }

    pub fn category_picker_toggle(&mut self) {
        let Some(picker) = self.category_picker.as_mut() else {
            return;
        };
        let Some(category) = self.library.categories.get(picker.selected).cloned() else {
            return;
        };
        if !picker.chosen.remove(&category) {
            picker.chosen.insert(category);
        }
    }

    pub fn cancel_category_picker(&mut self) {
        self.category_picker = None;
    }

    pub fn confirm_category_picker(&mut self) {
        let Some(picker) = self.category_picker.take() else {
            return;
        };
        let mut categories: Vec<String> = picker.chosen.into_iter().collect();
        categories.sort();
        if picker.local {
            let series = if self.tab == 0 && self.view == LibraryView::Chapters {
                self.selected_series().cloned()
            } else if self.tab == 3 && self.browse_view == BrowseView::Chapters {
                self.selected_browse_local_series().cloned()
            } else {
                None
            };
            let Some(series) = series else {
                return;
            };
            if categories.is_empty() {
                categories.push("Default".to_owned());
            }
            if let Some(target) = self
                .library
                .series
                .iter_mut()
                .find(|candidate| candidate.path == series.path)
            {
                target.categories = categories;
            }
            self.save_library_categories();
            self.status = Some("Updated categories".to_owned());
            return;
        }
        let Some(manga) = self.remote_story_manga().cloned() else {
            return;
        };
        if let Some(favorite) = self
            .config
            .remote_library
            .iter_mut()
            .find(|favorite| favorite.source_id == manga.source_id && favorite.url == manga.url)
        {
            favorite.categories = categories;
        }
        self.persist_settings();
        self.merge_remote_library();
        self.status = Some("Updated categories".to_owned());
    }

    /// Jump to Migrate with this saved manga selected.
    pub fn migrate_this_story(&mut self) {
        let Some(manga) = self.remote_story_manga().cloned() else {
            self.status = Some("Migration is only available for source manga.".to_owned());
            return;
        };
        let Some(favorite) = self
            .config
            .remote_library
            .iter()
            .find(|favorite| favorite.source_id == manga.source_id && favorite.url == manga.url)
            .cloned()
        else {
            self.status = Some("Save this manga to the library before migrating.".to_owned());
            return;
        };
        self.begin_migration(favorite);
    }

    fn begin_migration(&mut self, favorite: RemoteFavorite) {
        self.tab = 3;
        self.browse_tab = BrowseTab::Migrate;
        self.browse_view = BrowseView::Migrate;
        self.migrate.screen = MigrateScreen::Favorites;
        self.migrate.source_id = Some(favorite.source_id.clone());
        self.migrate.favorites = vec![favorite];
        self.migrate.list_state = ListState::default();
        self.browse_selection = 0;
        self.select_browse_entry(0);
        self.status = Some("Press M to match this manga against enabled sources.".to_owned());
    }

    /// Open Migrate for the selected source result when it is already saved.
    pub fn migrate_selected_result(&mut self) {
        let manga = match self.browse_view {
            BrowseView::RemoteResults => self.visible_remote_result(self.browse_selection).cloned(),
            BrowseView::GlobalResults => self
                .remote
                .global_groups
                .get(self.remote.global_group)
                .and_then(|group| group.mangas.get(self.remote.global_item))
                .cloned(),
            _ => None,
        };
        let Some(manga) = manga else {
            return;
        };
        let Some(favorite) = self
            .config
            .remote_library
            .iter()
            .find(|favorite| favorite.source_id == manga.source_id && favorite.url == manga.url)
            .cloned()
        else {
            self.remote.status = Some("Save this manga before migrating.".to_owned());
            self.remote.error = false;
            return;
        };
        self.begin_migration(favorite);
        self.status = Some("Press M to migrate this manga to another source.".to_owned());
    }

    /// Open Migrate for the duplicate selected in the duplicate dialog.
    pub fn duplicate_migrate(&mut self) {
        let Some(dialog) = self.duplicate_dialog.take() else {
            return;
        };
        let Some(favorite) = dialog.duplicates.get(dialog.selected).cloned() else {
            return;
        };
        self.begin_migration(favorite);
        self.status = Some("Press M to migrate the existing entry to another source.".to_owned());
    }

    pub fn open_chapter_settings(&mut self) {
        self.chapter_settings_modal = Some(ChapterSettingsModal::default());
    }

    pub fn close_chapter_settings(&mut self) {
        self.chapter_settings_modal = None;
    }

    pub fn chapter_settings_previous(&mut self) {
        if let Some(modal) = self.chapter_settings_modal.as_mut() {
            modal.selected = modal.selected.saturating_sub(1);
            modal.list_state.select(Some(modal.selected));
        }
    }

    pub fn chapter_settings_next(&mut self) {
        let rows = self.chapter_setting_rows().len();
        if let Some(modal) = self.chapter_settings_modal.as_mut() {
            if rows > 0 {
                modal.selected = (modal.selected + 1).min(rows - 1);
                modal.list_state.select(Some(modal.selected));
            }
        }
    }

    pub fn chapter_settings_adjust(&mut self, delta: i32) {
        let Some(modal) = self.chapter_settings_modal.as_ref() else {
            return;
        };
        let rows = self.chapter_setting_rows();
        let Some(row) = rows.get(modal.selected).copied() else {
            return;
        };
        let scanlators = self.chapter_scanlators();
        match row {
            ChapterSettingRow::Downloaded => self.update_chapter_settings(|settings| {
                settings.downloaded = cycle_tri_state(settings.downloaded, delta);
            }),
            ChapterSettingRow::Unread => self.update_chapter_settings(|settings| {
                settings.unread = cycle_tri_state(settings.unread, delta);
            }),
            ChapterSettingRow::Bookmarked => self.update_chapter_settings(|settings| {
                settings.bookmarked = cycle_tri_state(settings.bookmarked, delta);
            }),
            ChapterSettingRow::Sort => self.update_chapter_settings(|settings| {
                settings.sort = if delta < 0 {
                    match settings.sort {
                        ChapterSort::Source => ChapterSort::Alphabet,
                        ChapterSort::Number => ChapterSort::Source,
                        ChapterSort::UploadDate => ChapterSort::Number,
                        ChapterSort::Alphabet => ChapterSort::UploadDate,
                    }
                } else {
                    settings.sort.next()
                };
            }),
            ChapterSettingRow::Direction => self.update_chapter_settings(|settings| {
                settings.descending = !settings.descending;
            }),
            ChapterSettingRow::Display => self.update_chapter_settings(|settings| {
                settings.display = if delta < 0 {
                    match settings.display {
                        ChapterDisplay::Title => ChapterDisplay::Number,
                        ChapterDisplay::Number => ChapterDisplay::Title,
                    }
                } else {
                    settings.display.next()
                };
            }),
            ChapterSettingRow::DownloadNewChapters => self.update_chapter_settings(|settings| {
                settings.download_new_chapters = !settings.download_new_chapters;
            }),
            ChapterSettingRow::Reset => {
                let defaults = self.config.chapter_defaults.clone();
                self.update_chapter_settings(|settings| *settings = defaults);
            }
            ChapterSettingRow::Scanlator(index) => {
                let Some(scanlator) = scanlators.get(index).cloned() else {
                    return;
                };
                self.update_chapter_settings(|settings| {
                    if !settings.excluded_scanlators.remove(&scanlator) {
                        settings.excluded_scanlators.insert(scanlator);
                    }
                });
            }
        }
        self.clamp_story_cursor();
    }

    fn clamp_story_cursor(&mut self) {
        let indices = self.story_chapter_indices();
        if indices.is_empty() {
            self.sync_story_list_state();
            return;
        }
        let cursor = self.story_chapter_cursor().unwrap_or(0);
        if !indices.contains(&cursor) {
            self.set_story_chapter_cursor(indices[0]);
        }
        self.sync_story_list_state();
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
                        self.clamp_story_cursor();
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
    /// Kitty keeps decoded images under a terminal memory quota and silently evicts old
    /// ones. Cover protocols only transmit once, so after a large webtoon the terminal may
    /// have dropped cover images while the app still thinks they are present. Rebuild the
    /// protocols so the next grid draw transmits them again.
    fn reset_cover_protocols(&mut self) {
        if self.image_picker.as_ref().map(Picker::protocol_type) == Some(ProtocolType::Kitty) {
            self.tile_covers.clear();
        }
    }

    pub fn close_reader(&mut self) {
        if self.reader.is_none() {
            return;
        }
        self.finish_reading_session();
        self.flush_progress();
        self.reader = None;
        self.reset_cover_protocols();
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
                    .min(self.history_count().saturating_sub(1));
                self.history_list_state.select(Some(self.history_selection));
            }
            ReaderOrigin::Updates => {
                self.tab = 1;
                self.updates_selection = self
                    .updates_selection
                    .min(self.updates.entries().len().saturating_sub(1));
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
            LibraryView::Chapters | LibraryView::RemoteDetails => {
                let position = self.story_chapter_position();
                self.select_story_position(position.saturating_sub(1));
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
            LibraryView::Chapters | LibraryView::RemoteDetails => {
                let position = self.story_chapter_position();
                let count = self.story_chapter_indices().len();
                if count > 0 {
                    self.select_story_position((position + 1).min(count - 1));
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
        } else if self.previous_page_key_advances() {
            self.change_page(false);
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
        } else if self.previous_page_key_advances() {
            self.change_page(true);
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
        self.reading_direction = match mode {
            ReadingMode::RightToLeft => ReadingDirection::RightToLeft,
            ReadingMode::Vertical => ReadingDirection::Vertical,
            _ => ReadingDirection::LeftToRight,
        };
    }

    /// Paged mode with up/down page turns instead of left/right.
    pub fn reader_vertical_paging(&self) -> bool {
        self.reader_mode == ReaderMode::Page && self.reading_direction == ReadingDirection::Vertical
    }

    /// In RTL page mode the left arrow advances and the right arrow goes back.
    pub fn previous_page_key_advances(&self) -> bool {
        self.reader_mode == ReaderMode::Page
            && self.reading_direction == ReadingDirection::RightToLeft
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
            let entries = self.history_entries();
            let (key, _) = entries.get(self.history_selection)?;
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
            BrowseView::Migrate => {
                self.migrate
                    .favorites
                    .get(self.browse_selection)
                    .map(|favorite| {
                        (
                            SelectionScope::Migrate,
                            format!("migrate:{}\n{}", favorite.source_id, favorite.url),
                        )
                    })
            }
            BrowseView::SourceFilters
            | BrowseView::SourcePrefs
            | BrowseView::Filters
            | BrowseView::ExtensionDetails => None,
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

    pub fn select_all_story_chapters(&mut self) {
        let Some((scope, keys)) = self.story_selection_keys() else {
            return;
        };
        if keys.is_empty() {
            self.multi_select = None;
            return;
        }
        let count = keys.len();
        self.multi_select = Some(MultiSelect {
            scope,
            keys: keys.into_iter().collect(),
        });
        self.status = Some(format!("Selected {count} chapters"));
    }

    pub fn invert_story_selection(&mut self) {
        let Some((scope, keys)) = self.story_selection_keys() else {
            return;
        };
        let all: HashSet<String> = keys.into_iter().collect();
        let existing: HashSet<String> = self
            .multi_select
            .as_ref()
            .filter(|selection| selection.scope == scope)
            .map(|selection| selection.keys.clone())
            .unwrap_or_default();
        let inverted: HashSet<String> = all.difference(&existing).cloned().collect();
        if inverted.is_empty() {
            self.multi_select = None;
            self.status = Some("Selection cleared".to_owned());
        } else {
            let count = inverted.len();
            self.multi_select = Some(MultiSelect {
                scope,
                keys: inverted,
            });
            self.status = Some(format!("Selected {count} chapters"));
        }
    }

    fn story_selection_keys(&self) -> Option<(SelectionScope, Vec<String>)> {
        let indices = self.story_chapter_indices();
        if self.tab == 0 && self.view == LibraryView::Chapters {
            let series = self.selected_series()?;
            let keys = indices
                .iter()
                .filter_map(|index| series.chapters.get(*index))
                .map(Self::local_chapter_selection_key)
                .collect();
            return Some((SelectionScope::LocalChapters, keys));
        }
        if self.tab == 3 && self.browse_view == BrowseView::Chapters {
            let series = self.selected_browse_local_series()?;
            let keys = indices
                .iter()
                .filter_map(|index| series.chapters.get(*index))
                .map(Self::local_chapter_selection_key)
                .collect();
            return Some((SelectionScope::LocalChapters, keys));
        }
        let manga = self.remote_story_manga()?;
        let chapters = self.story_remote_chapters()?;
        let keys = indices
            .iter()
            .filter_map(|index| chapters.get(*index))
            .map(|chapter| {
                Self::remote_chapter_selection_key(&manga.source_id, &manga.url, &chapter.url)
            })
            .collect();
        Some((SelectionScope::RemoteChapters, keys))
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

    /// Update rows whose series and chapter still exist in the library.
    pub fn update_rows(&self) -> Vec<&UpdateEntry> {
        self.updates
            .entries()
            .iter()
            .filter(|entry| {
                if let Some(remote) = &entry.remote {
                    return self.config.remote_library.iter().any(|favorite| {
                        favorite.source_id == remote.source_id && favorite.url == remote.manga_url
                    });
                }
                self.library.series.iter().any(|series| {
                    series.path.to_string_lossy() == entry.series_key
                        && series
                            .chapters
                            .iter()
                            .any(|chapter| chapter.path.to_string_lossy() == entry.chapter_key)
                })
            })
            .collect()
    }

    pub fn updates_count(&self) -> usize {
        self.update_rows().len()
    }

    pub fn updates_previous(&mut self) {
        self.updates_selection = self.updates_selection.saturating_sub(1);
    }

    pub fn updates_next(&mut self) {
        let last = self.updates_count().saturating_sub(1);
        self.updates_selection = (self.updates_selection + 1).min(last);
    }

    pub fn open_update_entry(&mut self) {
        let Some(entry) = self
            .update_rows()
            .get(self.updates_selection)
            .map(|entry| (*entry).clone())
        else {
            return;
        };
        if let Some(remote) = &entry.remote {
            self.open_remote_update(remote);
            return;
        }
        let Some(series) = self
            .library
            .series
            .iter()
            .find(|series| series.path.to_string_lossy() == entry.series_key)
        else {
            self.status = Some("That series is no longer in the library.".to_owned());
            return;
        };
        let Some(chapter) = series
            .chapters
            .iter()
            .find(|chapter| chapter.path.to_string_lossy() == entry.chapter_key)
        else {
            self.status = Some("That chapter is no longer in the library.".to_owned());
            return;
        };
        let pages = match chapter_pages(chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.status = Some(error.to_string());
                return;
            }
        };
        let start_page = self
            .progress
            .entry(&chapter.path)
            .map(|progress| progress.page)
            .unwrap_or(0);
        self.start_local_reader(
            series.title.clone(),
            chapter.title.clone(),
            chapter.path.clone(),
            pages,
            start_page,
            ReaderOrigin::Updates,
        );
    }

    fn open_remote_update(&mut self, remote: &RemoteUpdate) {
        if !self.is_favorite(&remote.source_id, &remote.manga_url) {
            self.status = Some("That manga is no longer saved.".to_owned());
            return;
        }
        let Some(story) = load_cached_story(&remote.source_id, &remote.manga_url) else {
            self.status = Some("The saved story cache is missing; refresh the library.".to_owned());
            return;
        };
        let Some(index) = story
            .chapters
            .iter()
            .position(|chapter| chapter.url == remote.chapter_url)
        else {
            self.status = Some("That chapter is no longer in the saved story.".to_owned());
            return;
        };
        let source_name = remote
            .source_name
            .clone()
            .or_else(|| self.remote_source_display_name(&remote.source_id));
        self.tab = 0;
        self.view = LibraryView::RemoteDetails;
        self.story_updated_at = Some(story.updated_at);
        self.library_remote.source_name = source_name;
        self.library_remote.manga = Some(story.manga);
        self.library_remote.chapters = story.chapters;
        self.library_remote.chapter_index = index;
        self.library_remote.list_state.select(Some(index));
        self.open_library_remote_chapter(index);
    }

    pub fn dismiss_update(&mut self) {
        let Some(entry) = self
            .update_rows()
            .get(self.updates_selection)
            .map(|entry| (*entry).clone())
        else {
            return;
        };
        if !self.updates.dismiss(&entry.series_key, &entry.chapter_key) {
            return;
        }
        let _ = self.updates.save();
        self.updates_selection = self
            .updates_selection
            .min(self.updates_count().saturating_sub(1));
        self.status = Some(format!("Dismissed {}", entry.chapter_title));
    }

    pub fn clear_updates(&mut self) {
        if self.updates.entries().is_empty() {
            return;
        }
        self.updates.clear();
        let _ = self.updates.save();
        self.updates_selection = 0;
        self.status = Some("Cleared all updates".to_owned());
    }

    pub fn history_entries(&self) -> Vec<(String, HistoryEntry)> {
        let entries = if self.history_query.trim().is_empty() {
            self.history.latest_per_series()
        } else {
            self.history.search_latest(&self.history_query)
        };
        entries
            .into_iter()
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect()
    }

    pub fn history_count(&self) -> usize {
        self.history_entries().len()
    }

    pub fn history_section_next(&mut self) {
        self.history_section = match self.history_section {
            HistorySection::History => HistorySection::Stats,
            HistorySection::Stats => HistorySection::History,
        };
        if self.history_section == HistorySection::Stats {
            self.history_search = false;
        }
    }

    pub fn history_section_previous(&mut self) {
        self.history_section_next();
    }

    pub fn history_search_start(&mut self) {
        if self.history_section == HistorySection::History {
            self.history_search = true;
        }
    }

    pub fn history_search_stop(&mut self, clear: bool) {
        self.history_search = false;
        if clear {
            self.history_query.clear();
        }
        self.history_selection = 0;
        self.history_scroll = 0;
        self.clamp_history_selection();
    }

    pub fn history_search_push(&mut self, character: char) {
        self.history_query.push(character);
        self.history_selection = 0;
        self.history_scroll = 0;
        self.clamp_history_selection();
    }

    pub fn history_search_pop(&mut self) {
        self.history_query.pop();
        self.history_selection = 0;
        self.history_scroll = 0;
        self.clamp_history_selection();
    }

    fn clamp_history_selection(&mut self) {
        let count = self.history_count();
        self.history_selection = self.history_selection.min(count.saturating_sub(1));
        self.history_list_state
            .select((count > 0).then_some(self.history_selection));
    }

    pub fn history_next(&mut self) {
        if self.history_section == HistorySection::Stats {
            self.history_stats_scroll = self.history_stats_scroll.saturating_add(1);
            return;
        }
        if self.history_section != HistorySection::History {
            return;
        }
        if let Some(last) = self.history_count().checked_sub(1) {
            self.history_selection = (self.history_selection + 1).min(last);
            self.history_list_state.select(Some(self.history_selection));
        }
    }

    pub fn history_previous(&mut self) {
        if self.history_section == HistorySection::Stats {
            self.history_stats_scroll = self.history_stats_scroll.saturating_sub(1);
            return;
        }
        if self.history_section != HistorySection::History {
            return;
        }
        self.history_selection = self.history_selection.saturating_sub(1);
        self.history_list_state.select(Some(self.history_selection));
    }

    pub fn resume_latest_history(&mut self) {
        if self.history_section != HistorySection::History {
            self.history_section = HistorySection::History;
        }
        self.history_query.clear();
        self.history_search = false;
        self.history_selection = 0;
        self.history_scroll = 0;
        self.open_history_entry();
    }

    pub fn favorite_selected_history(&mut self) {
        if self.history_section != HistorySection::History {
            return;
        }
        let Some((_, entry)) = self.history_entries().get(self.history_selection).cloned() else {
            return;
        };
        let Some(remote) = entry.remote else {
            self.status = Some("Local history is already in the library.".to_owned());
            return;
        };
        if self.is_favorite(&remote.manga.source_id, &remote.manga.url) {
            self.status = Some("Already saved to the library.".to_owned());
            return;
        }
        self.request_save_manga(remote.manga);
    }

    pub fn open_history_details(&mut self) {
        if self.history_section != HistorySection::History {
            return;
        }
        let Some((_, entry)) = self.history_entries().get(self.history_selection).cloned() else {
            return;
        };
        if let Some(local) = entry.local {
            if let Some(index) = self
                .library
                .series
                .iter()
                .position(|series| series.path == local.series_path)
            {
                self.tab = 0;
                self.selected = index;
                self.view = LibraryView::Chapters;
                self.chapter_index = 0;
                self.chapter_list_state.select(Some(0));
                self.status = None;
                return;
            }
            self.status = Some("Series is no longer in the library.".to_owned());
            return;
        }
        let Some(remote) = entry.remote else {
            return;
        };
        if self.is_favorite(&remote.manga.source_id, &remote.manga.url) {
            self.tab = 0;
            self.open_library_remote_series(
                RemoteSeries {
                    source_id: remote.manga.source_id,
                    url: remote.manga.url,
                    thumbnail_url: remote.manga.thumbnail_url,
                    memo: remote.manga.memo,
                },
                remote.manga.title,
            );
            return;
        }
        self.tab = 3;
        self.remote.source = self
            .remote
            .sources
            .iter()
            .find(|source| source.id == remote.manga.source_id)
            .cloned();
        self.remote.manga = Some(remote.manga);
        self.remote.remote_details_origin = BrowseView::Sources;
        self.browse_view = BrowseView::RemoteDetails;
        self.browse_selection = 0;
        self.load_remote_details();
        self.load_remote_chapters();
        self.select_browse_entry(0);
    }

    pub fn open_history_delete_dialog(&mut self) {
        if self.history_section != HistorySection::History {
            return;
        }
        let mut keys: Vec<String> = Vec::new();
        if let Some(selected) = self.selected_keys(SelectionScope::History) {
            keys = selected
                .iter()
                .filter_map(|key| key.strip_prefix("history:").map(str::to_owned))
                .collect();
        }
        if keys.is_empty() {
            if let Some((key, _)) = self.history_entries().get(self.history_selection) {
                keys.push(key.clone());
            }
        }
        if keys.is_empty() {
            return;
        }
        let series_keys: Vec<String> = keys
            .iter()
            .filter_map(|key| self.history.get(key).map(HistoryEntry::series_key))
            .collect();
        self.history_delete = Some(HistoryDeleteDialog {
            keys,
            series_keys,
            all_series: false,
        });
    }

    pub fn toggle_history_delete_all(&mut self) {
        if let Some(dialog) = self.history_delete.as_mut() {
            dialog.all_series = !dialog.all_series;
        }
    }

    pub fn cancel_history_delete(&mut self) {
        self.history_delete = None;
    }

    pub fn confirm_history_delete(&mut self) {
        let Some(dialog) = self.history_delete.take() else {
            return;
        };
        let mut removed = 0usize;
        if dialog.all_series {
            let mut series = dialog.series_keys;
            series.sort();
            series.dedup();
            for series_key in series {
                let keys: Vec<String> = self
                    .history
                    .recent()
                    .into_iter()
                    .filter(|(_, entry)| entry.series_key() == series_key)
                    .map(|(key, _)| key.clone())
                    .collect();
                removed += self.history.remove_series(&series_key);
                for key in keys {
                    self.progress.remove(Path::new(&key));
                }
                self.clear_local_series_progress(&series_key);
            }
        } else {
            for key in dialog.keys {
                if self.history.remove(&key) {
                    removed += 1;
                    self.progress.remove(Path::new(&key));
                }
            }
        }
        let _ = self.history.save();
        let _ = self.progress.save();
        self.reading_stats = self.history.reading_stats();
        self.history_dirty = false;
        self.multi_select = None;
        self.clamp_history_selection();
        self.status = Some(format!(
            "Removed {removed} history {} · chapters marked unread",
            if removed == 1 { "entry" } else { "entries" }
        ));
    }

    fn clear_local_series_progress(&mut self, series_key: &str) {
        if let Some(path) = series_key.strip_prefix("local:") {
            let Some(series) = self
                .library
                .series
                .iter()
                .find(|series| series.path.display().to_string() == path)
            else {
                return;
            };
            for chapter in &series.chapters {
                self.progress.remove(&chapter.path);
            }
        } else if let Some(remote) = series_key.strip_prefix("remote:") {
            let Some((source_id, manga_url)) = remote.split_once('\n') else {
                return;
            };
            self.progress
                .remove_prefix(&progress_prefix(source_id, manga_url));
        }
    }

    pub fn open_history_clear_confirm(&mut self) {
        if self.history_section == HistorySection::History {
            self.history_clear_confirm = true;
        }
    }

    pub fn cancel_history_clear(&mut self) {
        self.history_clear_confirm = false;
    }

    pub fn confirm_history_clear(&mut self) {
        self.history_clear_confirm = false;
        self.history.clear();
        self.progress.clear();
        let _ = self.history.save();
        let _ = self.progress.save();
        self.reading_stats = self.history.reading_stats();
        self.history_dirty = false;
        self.history_selection = 0;
        self.history_scroll = 0;
        self.multi_select = None;
        self.clamp_history_selection();
        self.status = Some("History cleared · chapters marked unread".to_owned());
    }

    pub fn open_history_entry(&mut self) {
        if self.history_section != HistorySection::History {
            return;
        }
        let Some((key, entry)) = self.history_entries().get(self.history_selection).cloned() else {
            return;
        };

        if let Some(local) = &entry.local {
            let series = self
                .library
                .series
                .iter()
                .find(|series| series.path == local.series_path)
                .cloned();
            let Some(series) = series else {
                self.status = Some("That series is no longer in the library.".to_owned());
                return;
            };
            let Some(index) = series
                .chapters
                .iter()
                .position(|chapter| chapter.path == local.chapter_path)
            else {
                self.status = Some("That chapter is no longer in the library.".to_owned());
                return;
            };
            let current = &series.chapters[index];
            let finished = self.chapter_finished(&current.path);
            let target = if finished {
                index
                    .checked_sub(1)
                    .and_then(|next| series.chapters.get(next))
            } else {
                Some(current)
            };
            let Some(chapter) = target else {
                self.status = Some("No next chapter.".to_owned());
                return;
            };
            let pages = match chapter_pages(chapter) {
                Ok(pages) => pages,
                Err(error) => {
                    self.status = Some(error.to_string());
                    return;
                }
            };
            let start_page = if finished {
                0
            } else {
                self.progress
                    .entry(&chapter.path)
                    .map(|entry| entry.page)
                    .unwrap_or(0)
            };
            self.start_local_reader(
                series.title.clone(),
                chapter.title.clone(),
                chapter.path.clone(),
                pages,
                start_page,
                ReaderOrigin::History,
            );
            return;
        }

        let Some(remote) = &entry.remote else {
            return;
        };
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        self.remote.source = self
            .remote
            .sources
            .iter()
            .find(|source| source.id == remote.manga.source_id)
            .cloned();
        self.remote.manga = Some(remote.manga.clone());
        self.load_remote_chapters();
        let Some(index) = self
            .remote
            .chapters
            .iter()
            .position(|chapter| chapter.url == remote.chapter.url)
        else {
            self.status = Some("That chapter is no longer available.".to_owned());
            return;
        };
        let key_for = |chapter: &RemoteChapter| {
            remote_progress_key(&remote.manga.source_id, &remote.manga.url, &chapter.url)
        };
        let current = &self.remote.chapters[index];
        let finished = self.chapter_finished(&key_for(current));
        let target = if finished {
            index
                .checked_sub(1)
                .and_then(|next| self.remote.chapters.get(next))
                .cloned()
        } else {
            Some(current.clone())
        };
        let Some(chapter) = target else {
            self.status = Some("No next chapter.".to_owned());
            return;
        };
        let pages = match self.remote_chapter_pages(&remote.manga, &chapter) {
            Ok(pages) => pages,
            Err(error) => {
                self.set_remote_error(error);
                return;
            }
        };
        let start_page = if finished {
            0
        } else {
            self.progress
                .entry(&key_for(&chapter))
                .map(|entry| entry.page)
                .unwrap_or(0)
        };
        self.start_local_reader(
            entry.series_title.clone(),
            chapter.name.clone(),
            key_for(&chapter),
            pages,
            start_page,
            ReaderOrigin::History,
        );
        let _ = key;
    }

    fn chapter_finished(&self, chapter: &std::path::Path) -> bool {
        self.progress
            .entry(chapter)
            .is_some_and(|entry| entry.pages > 0 && entry.page + 1 >= entry.pages)
    }

    fn start_local_reader(
        &mut self,
        series_title: String,
        chapter_title: String,
        chapter_path: PathBuf,
        pages: Vec<Page>,
        start_page: usize,
        origin: ReaderOrigin,
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
        self.reader_origin = origin;
        self.tab = match origin {
            ReaderOrigin::History => 2,
            ReaderOrigin::Updates => 1,
            _ => self.tab,
        };
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
    fn chapter_incognito(&self, chapter: &std::path::Path) -> bool {
        if self.config.incognito_extensions.is_empty() {
            return false;
        }
        let key = chapter.to_string_lossy();
        let Some(rest) = key.strip_prefix("remote://") else {
            return false;
        };
        let Some(source_id) = rest.split('/').next() else {
            return false;
        };
        self.remote.sources.iter().any(|source| {
            source.id == source_id
                && source.pkg.as_ref().is_some_and(|pkg| {
                    self.config
                        .incognito_extensions
                        .iter()
                        .any(|candidate| candidate == pkg)
                })
        })
    }

    fn record_progress(&mut self, chapter: &std::path::Path, page: usize, pages: usize) {
        if self.config.incognito || self.chapter_incognito(chapter) {
            return;
        }
        self.progress.update(chapter, page, pages);
        self.record_history(chapter, page, pages);
        self.sync_tracker_progress();
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
            ReaderOrigin::Library | ReaderOrigin::BrowseLocal | ReaderOrigin::Updates => {
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
                if let Some(mut existing) = self.history.get(&key).cloned() {
                    existing.page = page;
                    existing.pages = pages;
                    existing.last_read = 0;
                    existing
                } else if let Some(series) = self
                    .library
                    .series
                    .iter()
                    .find(|series| series.chapters.iter().any(|known| known.path == chapter))
                {
                    let Some(known) = series.chapters.iter().find(|known| known.path == chapter)
                    else {
                        return;
                    };
                    HistoryEntry::local(
                        series_title,
                        known.title.clone(),
                        series.path.clone(),
                        chapter.to_path_buf(),
                        series.cover.clone(),
                        page,
                        pages,
                    )
                } else if let Some(manga) = self.remote.manga.clone() {
                    let Some(known) = self.remote.chapters.iter().find(|known| {
                        remote_progress_key(&manga.source_id, &manga.url, &known.url) == chapter
                    }) else {
                        return;
                    };
                    let source = self.remote_source_name(&manga.source_id);
                    HistoryEntry::remote(
                        series_title,
                        known.name.clone(),
                        source,
                        manga,
                        known.clone(),
                        page,
                        pages,
                    )
                } else {
                    return;
                }
            }
        };

        if let Some(remote) = &entry.remote {
            let manga = remote.manga.clone();
            let chapter = remote.chapter.clone();
            self.queue_remote_state(&manga, &chapter, page, pages);
        }
        self.history.record(key, entry);
        self.history_dirty = true;
    }

    /// Whether the in-memory progress has been idle long enough to write.
    fn state_key(state: &RemoteReadState) -> String {
        format!(
            "{}\n{}\n{}",
            state.source_id, state.manga_url, state.chapter_url
        )
    }

    fn queue_remote_state(
        &mut self,
        manga: &RemoteManga,
        chapter: &RemoteChapter,
        page: usize,
        pages: usize,
    ) {
        let key = remote_progress_key(&manga.source_id, &manga.url, &chapter.url);
        let state = RemoteReadState {
            source_id: manga.source_id.clone(),
            manga_url: manga.url.clone(),
            chapter_url: chapter.url.clone(),
            page,
            pages,
            read: pages > 0 && page + 1 >= pages,
            bookmark: self.bookmarks.is_bookmarked(&key),
        };
        self.pending_states.insert(Self::state_key(&state), state);
        self.state_dirty = true;
        self.progress_changed_at = Some(Instant::now());
    }

    /// Merge host read-state into local stores. Local values win when both exist.
    pub fn load_remote_states(&mut self) {
        if self.host_states_loaded {
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let states = match client.read_states() {
            Ok(states) => states,
            Err(_) => {
                self.host_states_loaded = true;
                return;
            }
        };
        let mut applied = false;
        for state in states {
            let key = remote_progress_key(&state.source_id, &state.manga_url, &state.chapter_url);
            let local = self.progress.entry(&key);
            let local_bookmark = self.bookmarks.is_bookmarked(&key);
            if local.is_none() && !local_bookmark {
                applied = true;
                if state.read {
                    let pages = state.pages.max(1);
                    self.progress.update(&key, pages.saturating_sub(1), pages);
                } else if state.page > 0 {
                    let pages = state.pages.max(state.page + 2);
                    self.progress.update(&key, state.page, pages);
                }
                if state.bookmark {
                    self.bookmarks.set(&key, true);
                }
            } else {
                let pages = local.map(|entry| entry.pages).unwrap_or(0);
                let page = local.map(|entry| entry.page).unwrap_or(0);
                self.pending_states.insert(
                    Self::state_key(&state),
                    RemoteReadState {
                        source_id: state.source_id.clone(),
                        manga_url: state.manga_url.clone(),
                        chapter_url: state.chapter_url.clone(),
                        page,
                        pages,
                        read: pages > 0 && page + 1 >= pages,
                        bookmark: local_bookmark,
                    },
                );
                self.state_dirty = true;
            }
        }
        self.host_states_loaded = true;
        if applied {
            self.progress_dirty = true;
            self.progress_changed_at = Some(Instant::now());
            let _ = self.bookmarks.save();
        }
        if self.state_dirty {
            self.flush_remote_states();
        }
    }

    /// Push pending remote read-state to the host, including removals.
    pub fn flush_remote_states(&mut self) {
        if !self.state_dirty || self.pending_states.is_empty() {
            self.state_dirty = false;
            return;
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let mut chapters = Vec::new();
        let mut clear = Vec::new();
        for (key, state) in &self.pending_states {
            if state.page == 0 && state.pages == 0 && !state.read && !state.bookmark {
                clear.push(key.clone());
            } else {
                chapters.push(state.clone());
            }
        }
        if client.put_read_states(&chapters, &clear).is_err() {
            return;
        }
        self.pending_states.clear();
        self.state_dirty = false;
    }

    pub fn progress_save_due(&self) -> bool {
        (self.progress_dirty || self.history_dirty || self.state_dirty)
            && self
                .progress_changed_at
                .is_some_and(|changed| changed.elapsed() >= Duration::from_millis(500))
    }

    /// Write pending progress immediately. Safe to call when nothing is dirty.
    pub fn reading_session_due(&self) -> bool {
        self.reading_session.as_ref().is_some_and(|session| {
            let active = session
                .last_activity
                .elapsed()
                .min(READING_IDLE_CUTOFF)
                .as_secs();
            session.accumulated_secs.saturating_add(active) >= 60
        })
    }

    /// Record an input event while the reader is open. Gaps longer than the idle
    /// cutoff are discarded rather than counted as reading time.
    pub fn note_reading_activity(&mut self) {
        let Some(session) = self.reading_session.as_mut() else {
            return;
        };
        let now = Instant::now();
        let delta = now.saturating_duration_since(session.last_activity);
        if delta <= READING_IDLE_CUTOFF {
            session.accumulated_secs = session.accumulated_secs.saturating_add(delta.as_secs());
        }
        session.last_activity = now;
    }

    /// Start timing a chapter, flushing any previous session first.
    fn start_reading_session(&mut self) {
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        let chapter_path = reader.chapter_path.clone();
        let chapter_key = chapter_path.to_string_lossy().into_owned();
        if self
            .reading_session
            .as_ref()
            .is_some_and(|session| session.chapter_key == chapter_key)
        {
            return;
        }
        self.flush_reading_session();
        if self.config.incognito || self.chapter_incognito(&chapter_path) {
            self.reading_session = None;
            return;
        }

        self.reading_session = Some(ReadingSession {
            last_activity: Instant::now(),
            accumulated_secs: 0,
            chapter_key,
        });
    }

    pub fn flush_reading_session(&mut self) {
        self.note_reading_activity();
        let Some(session) = self.reading_session.as_mut() else {
            return;
        };
        let seconds = session.accumulated_secs;
        session.accumulated_secs = 0;
        if seconds == 0 {
            return;
        }
        self.history
            .add_duration(&session.chapter_key, seconds, &today_key());
        self.reading_stats = self.history.reading_stats();
        self.history_dirty = true;
        self.progress_changed_at = Some(Instant::now());
    }

    pub fn finish_reading_session(&mut self) {
        self.flush_reading_session();
        self.reading_session = None;
    }

    pub fn flush_progress(&mut self) {
        self.flush_remote_states();
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
            SettingId::ShowNsfw => {
                self.config.show_nsfw_extensions = !self.config.show_nsfw_extensions;
                self.persist_settings();
            }
            SettingId::RescanLibrary => self.refresh_async(),
            SettingId::AniListToken => self.open_anilist_token(),
            SettingId::MalClientId => self.open_mal_client_id(),
            SettingId::MalToken => self.open_mal_token(),
            SettingId::HostUrl => {
                let current = self.config.host_url.clone().unwrap_or_default();
                self.open_text_input(TextAction::SetHostUrl, "Extension host URL", &current);
            }
            SettingId::FlareSolverr => {
                let current = self.config.flaresolverr_url.clone().unwrap_or_default();
                self.open_text_input(TextAction::SetFlareSolverr, "FlareSolverr URL", &current);
            }
            SettingId::ImportBackup => {
                self.open_text_input(TextAction::ImportBackup, "Mihon backup path", "");
            }
            SettingId::ExportBackup => {
                self.open_text_input(TextAction::ExportBackup, "Export backup to", "");
            }
            SettingId::DownloadsPath => {
                let current = downloads_dir(&self.config)
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                self.open_text_input(TextAction::SetDownloadsPath, "Downloads folder", &current);
            }
            SettingId::ReadingMode
            | SettingId::ReaderFit
            | SettingId::LibrarySort
            | SettingId::DefaultCategory => self.adjust_setting(1),
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
        if self.more_section == MoreSection::Storage && self.storage_snapshot.is_none() {
            self.refresh_storage();
        }
        if self.more_section == MoreSection::Downloads && !self.downloads_loaded {
            self.refresh_downloads();
        }
    }

    pub fn category_manage_previous(&mut self) {
        self.category_manage_index = self.category_manage_index.saturating_sub(1);
    }

    pub fn category_manage_next(&mut self) {
        let count = self.library.categories.len();
        if count > 0 {
            self.category_manage_index =
                (self.category_manage_index + 1).min(count.saturating_sub(1));
        }
    }

    pub fn move_manage_category(&mut self, delta: i32) {
        let index = self.category_manage_index;
        let Some(target) = index
            .checked_add_signed(delta as isize)
            .filter(|target| *target < self.library.categories.len())
        else {
            return;
        };
        let Some(name) = self.library.categories.get(index).cloned() else {
            return;
        };
        if name == "Default" {
            self.status = Some("Default stays first.".to_owned());
            return;
        }
        if target == 0
            && self
                .library
                .categories
                .first()
                .is_some_and(|first| first == "Default")
        {
            self.status = Some("Default stays first.".to_owned());
            return;
        }
        self.library.categories.swap(index, target);
        self.category_manage_index = target;
        self.save_library_categories();
        self.status = Some(format!("Moved category {name}"));
    }

    fn selected_manage_category(&self) -> Option<String> {
        self.library
            .categories
            .get(self.category_manage_index)
            .cloned()
    }

    pub fn open_add_category(&mut self) {
        if self.tab == 4 && self.more_section == MoreSection::Categories {
            self.open_text_input(TextAction::AddCategory, "New category", "");
        }
    }

    pub fn open_rename_category(&mut self) {
        if self.tab != 4 || self.more_section != MoreSection::Categories {
            return;
        }
        let Some(name) = self.selected_manage_category() else {
            return;
        };
        if name == "Default" {
            self.status = Some("Default cannot be renamed.".to_owned());
            return;
        }
        self.open_text_input(TextAction::RenameCategory, "Rename category", &name);
    }

    pub fn delete_manage_category(&mut self) {
        if self.tab != 4 || self.more_section != MoreSection::Categories {
            return;
        }
        let Some(name) = self.selected_manage_category() else {
            return;
        };
        if name == "Default" {
            self.status = Some("Default cannot be removed.".to_owned());
            return;
        }
        self.library.categories.retain(|category| category != &name);
        for series in &mut self.library.series {
            series.categories.retain(|category| category != &name);
        }
        self.category_manage_index = self
            .category_manage_index
            .min(self.library.categories.len().saturating_sub(1));
        self.save_library_categories();
        self.status = Some(format!("Removed category {name}"));
    }

    fn add_category(&mut self, value: &str) {
        let name = value.trim();
        if name.is_empty() {
            return;
        }
        if name == "Default" || self.library.categories.iter().any(|known| known == name) {
            self.status = Some(format!("Category {name} already exists"));
            return;
        }
        self.library.categories.push(name.to_owned());
        self.save_library_categories();
        self.status = Some(format!("Added category {name}"));
    }

    fn rename_category(&mut self, value: &str) {
        let name = value.trim();
        let Some(old) = self.selected_manage_category() else {
            return;
        };
        if name.is_empty() || old == "Default" || name == old {
            return;
        }
        if name == "Default" || self.library.categories.iter().any(|known| known == name) {
            self.status = Some(format!("Category {name} already exists"));
            return;
        }
        if let Some(category) = self
            .library
            .categories
            .iter_mut()
            .find(|category| **category == old)
        {
            *category = name.to_owned();
        }
        for series in &mut self.library.series {
            for category in &mut series.categories {
                if category == &old {
                    *category = name.to_owned();
                }
            }
        }
        self.save_library_categories();
        self.status = Some(format!("Renamed {old} to {name}"));
    }

    fn save_library_categories(&mut self) {
        let entries: Vec<tsuiyomi_core::CategoryEntry> = self
            .library
            .categories
            .iter()
            .filter(|category| category.as_str() != "Default")
            .map(|category| tsuiyomi_core::CategoryEntry {
                name: category.clone(),
                series: self
                    .library
                    .series
                    .iter()
                    .filter(|series| series.categories.contains(category))
                    .map(|series| series.title.clone())
                    .collect(),
            })
            .collect();
        match tsuiyomi_core::save_category_entries(&self.library.root, &entries) {
            Ok(()) => {}
            Err(error) => self.status = Some(format!("Could not save categories: {error}")),
        }
    }

    /// Remove cached remote covers. Local custom covers live beside their series and
    /// are not touched. Covers are regenerated on the next draw.
    pub fn clear_cover_cache(&mut self) {
        let Some(dir) = tsuiyomi_core::covers_dir() else {
            self.status = Some("Cannot resolve the cover cache directory.".to_owned());
            return;
        };
        let mut removed = 0usize;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let result = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                if result.is_ok() {
                    removed += 1;
                }
            }
        }
        self.tile_covers.clear();
        self.failed_covers.clear();
        if self.storage_snapshot.is_some() {
            self.refresh_storage();
        }
        self.status = Some(format!("Removed {removed} cached covers"));
    }

    pub fn open_clear_downloads_confirm(&mut self) {
        if !self.storage_clear_downloads_confirm {
            self.storage_clear_downloads_confirm = true;
            self.status = Some("Press y/Enter to delete all downloaded chapters.".to_owned());
        }
    }

    pub fn cancel_clear_downloads(&mut self) {
        self.storage_clear_downloads_confirm = false;
    }

    pub fn confirm_clear_downloads(&mut self) {
        self.storage_clear_downloads_confirm = false;
        let Some(root) = downloads_dir(&self.config) else {
            self.status = Some("Cannot resolve the downloads folder.".to_owned());
            return;
        };
        if !root.exists() {
            self.status = Some("Downloads folder is already empty.".to_owned());
            return;
        }
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {
                let _ = std::fs::create_dir_all(&root);
                if self.storage_snapshot.is_some() {
                    self.refresh_storage();
                }
                self.status = Some("Deleted all downloaded chapters".to_owned());
                self.remote.error = false;
            }
            Err(error) => {
                self.status = Some(format!("Could not clear downloads: {error}"));
                self.remote.error = true;
            }
        }
    }

    pub fn refresh_downloads(&mut self) {
        self.downloaded_series = scan_downloaded_series(&self.config);
        self.downloads_loaded = true;
        self.downloads_selection = self
            .downloads_selection
            .min(self.downloaded_series.len().saturating_sub(1));
        self.status = Some(format!(
            "{} downloaded series",
            self.downloaded_series.len()
        ));
    }

    pub fn downloads_previous(&mut self) {
        self.downloads_selection = self.downloads_selection.saturating_sub(1);
    }

    pub fn downloads_next(&mut self) {
        if !self.downloaded_series.is_empty() {
            self.downloads_selection =
                (self.downloads_selection + 1).min(self.downloaded_series.len() - 1);
        }
    }

    pub fn open_downloads_delete_confirm(&mut self) {
        if self.downloaded_series.is_empty() {
            self.status = Some("No downloaded series to delete.".to_owned());
            return;
        }
        self.downloads_delete_confirm = true;
        let name = self
            .downloaded_series
            .get(self.downloads_selection)
            .map(|series| format!("{} · {}", series.source, series.series))
            .unwrap_or_default();
        self.status = Some(format!("Press y/Enter to delete downloads for {name}"));
    }

    pub fn cancel_downloads_delete(&mut self) {
        self.downloads_delete_confirm = false;
    }

    pub fn confirm_downloads_delete(&mut self) {
        self.downloads_delete_confirm = false;
        let Some(series) = self
            .downloaded_series
            .get(self.downloads_selection)
            .cloned()
        else {
            return;
        };
        match std::fs::remove_dir_all(&series.path) {
            Ok(()) => {
                self.refresh_downloads();
                if self.storage_snapshot.is_some() {
                    self.refresh_storage();
                }
                self.status = Some(format!(
                    "Deleted downloads for {} · {}",
                    series.source, series.series
                ));
                self.remote.error = false;
            }
            Err(error) => {
                self.status = Some(format!("Could not delete downloads: {error}"));
                self.remote.error = true;
            }
        }
    }

    /// Recompute on-disk storage usage for the Data and storage screen.
    pub fn refresh_storage(&mut self) {
        let library_bytes = directory_size(&self.library.root);
        let downloads_bytes = downloads_dir(&self.config)
            .map(|path| directory_size(&path))
            .unwrap_or(0);
        let host_bytes = host_data_dir()
            .map(|path| directory_size(&path))
            .unwrap_or(0);
        self.storage_snapshot = Some(StorageSnapshot {
            library_bytes,
            downloads_bytes,
            host_bytes,
            series: self.library.series.len(),
            extensions: self
                .remote
                .extensions
                .iter()
                .filter(|extension| extension.installed)
                .count(),
        });
        self.status = Some("Storage usage refreshed".to_owned());
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
            SettingId::ReaderFit => {
                let fits = [
                    tsuiyomi_core::ReaderFit::Fit,
                    tsuiyomi_core::ReaderFit::Crop,
                    tsuiyomi_core::ReaderFit::Scale,
                ];
                let index = fits
                    .iter()
                    .position(|fit| *fit == self.config.reader_fit)
                    .unwrap_or(0);
                let next = (index as i32 + delta).rem_euclid(fits.len() as i32) as usize;
                self.config.reader_fit = fits[next];
                if self.reader.is_some() && self.reader_mode == ReaderMode::Page {
                    self.load_page();
                }
            }
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
            SettingId::UpdateInterval => {
                let value = (self.config.update_interval_hours as i32 + delta).clamp(0, 168);
                self.config.update_interval_hours = value as u64;
                self.auto_update_checked_at = None;
            }
            SettingId::LibrarySort => {
                let sorts = [
                    LibrarySort::Natural,
                    LibrarySort::TitleAsc,
                    LibrarySort::TitleDesc,
                    LibrarySort::ChapterCountDesc,
                    LibrarySort::LatestChapterDesc,
                    LibrarySort::UnreadDesc,
                    LibrarySort::LastReadDesc,
                    LibrarySort::DateAddedDesc,
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
            SettingId::ShowNsfw => {
                self.config.show_nsfw_extensions = !self.config.show_nsfw_extensions;
            }
            SettingId::LibraryPath
            | SettingId::AniListToken
            | SettingId::MalClientId
            | SettingId::MalToken
            | SettingId::RescanLibrary
            | SettingId::HostUrl
            | SettingId::FlareSolverr
            | SettingId::DownloadsPath
            | SettingId::ImportBackup
            | SettingId::ExportBackup => return,
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

    pub fn pan_page(&mut self, dx: f32, dy: f32) {
        if self.reader.is_none() || self.reader_mode != ReaderMode::Page || self.zoom <= 100 {
            return;
        }
        self.page_pan.0 = (self.page_pan.0 + dx).clamp(-1.0, 1.0);
        self.page_pan.1 = (self.page_pan.1 + dy).clamp(-1.0, 1.0);
        self.load_page();
    }

    fn set_zoom(&mut self, zoom: u16) {
        if self.zoom == zoom {
            return;
        }
        self.zoom = zoom;
        self.page_pan = (0.0, 0.0);
        if self.reader.is_some() {
            match self.reader_mode {
                ReaderMode::Page => self.load_page(),
                ReaderMode::Scroll => self.build_scroll(terminal_width()),
            }
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
            LibraryView::Chapters | LibraryView::RemoteDetails
                if index < self.story_chapter_indices().len() =>
            {
                self.select_story_position(index);
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
        self.page_pan = (0.0, 0.0);
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
        self.step_reader_chapter(-1);
    }

    pub fn previous_chapter(&mut self) {
        self.step_reader_chapter(1);
    }

    /// Move through the filtered chapter list in reader order (newest first).
    fn step_reader_chapter(&mut self, delta: isize) {
        if self.reader.is_none()
            || matches!(
                self.reader_origin,
                ReaderOrigin::History | ReaderOrigin::Updates
            )
        {
            return;
        }
        let origin = self.reader_origin;
        let indices = self.story_chapter_indices();
        let current = match origin {
            ReaderOrigin::Remote => self.remote.chapter_index,
            ReaderOrigin::LibraryRemote => self.library_remote.chapter_index,
            ReaderOrigin::BrowseLocal => self.browse_local_chapter,
            ReaderOrigin::Library => self.chapter_index,
            ReaderOrigin::History | ReaderOrigin::Updates => return,
        };
        let Some(position) = indices.iter().position(|index| *index == current) else {
            return;
        };
        let Some(target) = position
            .checked_add_signed(delta)
            .and_then(|next| indices.get(next).copied())
        else {
            return;
        };
        match origin {
            ReaderOrigin::Remote => {
                self.remote.chapter_index = target;
                self.open_remote_chapter(target);
            }
            ReaderOrigin::LibraryRemote => {
                self.library_remote.chapter_index = target;
                self.open_library_remote_chapter(target);
            }
            ReaderOrigin::BrowseLocal => {
                self.browse_local_chapter = target;
                self.open_browse_local_chapter(target);
            }
            ReaderOrigin::Library => {
                self.chapter_index = target;
                self.open_chapter();
            }
            ReaderOrigin::History | ReaderOrigin::Updates => {}
        }
    }

    fn load_page(&mut self) {
        self.start_reading_session();
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
        let image = zoomed_page_image(image, self.zoom, self.page_pan);

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
        if index == 2 && self.tab == 2 {
            self.resume_latest_history();
            return;
        }
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
            self.story_updated_at = None;
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
            self.story_updated_at = Some(story.updated_at);
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
        self.fetch_library_remote_series(remote, title, false);
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
            .insert(key.clone(), (manga.clone(), chapters.clone()));
        let source_name = self
            .remote
            .sources
            .iter()
            .find(|source| source.id == manga.source_id)
            .map(RemoteSource::display_name);
        let mut story = CachedStory::new(manga, chapters);
        story.source_name = source_name;
        let _ = save_cached_story(&story);
        let current_key = self
            .remote
            .manga
            .as_ref()
            .map(|manga| remote_story_key(&manga.source_id, &manga.url))
            .or_else(|| {
                self.library_remote
                    .manga
                    .as_ref()
                    .map(|manga| remote_story_key(&manga.source_id, &manga.url))
            });
        if current_key.as_deref() == Some(key.as_str()) {
            self.story_updated_at = Some(story.updated_at);
        }
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
        self.library_remote.downloaded = self
            .library_remote
            .manga
            .as_ref()
            .map(|manga| self.mark_downloaded(manga, &self.library_remote.chapters))
            .unwrap_or_default();
        self.view = LibraryView::RemoteDetails;
        self.clamp_story_cursor();
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
        self.fetch_library_remote_series(remote, manga.title.clone(), true);
    }

    fn fetch_library_remote_series(
        &mut self,
        remote: RemoteSeries,
        title: String,
        auto_download: bool,
    ) {
        if self.host_client.is_none() && !self.ensure_host() {
            return;
        }
        if self.remote.sources.is_empty() {
            self.load_remote_sources();
        }
        let Some(client) = self.host_client.clone() else {
            return;
        };
        let old_urls: HashSet<String> =
            if auto_download
                && self.library_remote.manga.as_ref().is_some_and(|manga| {
                    manga.source_id == remote.source_id && manga.url == remote.url
                })
            {
                self.library_remote
                    .chapters
                    .iter()
                    .map(|chapter| chapter.url.clone())
                    .collect()
            } else {
                HashSet::new()
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
        let manga_for_download = manga.clone();
        let chapters_for_download = chapters.clone();
        self.show_library_remote_story(manga, chapters, source);
        self.auto_download_new_chapters(&manga_for_download, &old_urls, &chapters_for_download);
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
            let categories = if favorite.categories.is_empty() {
                vec![category.clone()]
            } else {
                favorite.categories.clone()
            };
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
                categories,
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
        self.record_series_added();
    }

    fn record_series_added(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let mut changed = false;
        for series in &self.library.series {
            let key = series_identity(series);
            if let std::collections::btree_map::Entry::Vacant(slot) =
                self.config.series_added_at.entry(key)
            {
                slot.insert(now);
                changed = true;
            }
        }
        if changed {
            self.persist_settings();
        }
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
        let context = ScanContext {
            previous_series: self.selected_series().map(|series| series.path.clone()),
            previous_view: self.view,
            previous_chapter: self.chapter_index,
            previous_category: self.category_name_or_default(),
        };
        match scan_library(&self.library.root) {
            Ok(library) => self.apply_scanned_library(library, &context),
            Err(error) => self.status = Some(error.to_string()),
        }
    }

    /// Rescan in the background when a worker is available.
    pub fn refresh_async(&mut self) {
        if self.library.root.as_os_str().is_empty() {
            self.status = Some("No library configured. Set TSUIYOMI_LIBRARY.".to_owned());
            return;
        }
        let context = ScanContext {
            previous_series: self.selected_series().map(|series| series.path.clone()),
            previous_view: self.view,
            previous_chapter: self.chapter_index,
            previous_category: self.category_name_or_default(),
        };
        if let Some(worker) = self.scan_worker.as_ref() {
            self.scan_generation = self.scan_generation.wrapping_add(1);
            self.scan_context = Some(context);
            self.status = Some("Scanning the library…".to_owned());
            worker.request(ScanRequest {
                generation: self.scan_generation,
                root: self.library.root.clone(),
            });
            return;
        }
        self.refresh();
    }

    /// Apply a completed background scan. Returns whether the UI changed.
    pub fn poll_scan_worker(&mut self) -> bool {
        let Some(event) = self.scan_worker.as_ref().and_then(ScanWorker::try_result) else {
            return false;
        };
        if event.generation != self.scan_generation {
            return false;
        }
        let Some(context) = self.scan_context.take() else {
            return false;
        };
        match event.result {
            Ok(library) => self.apply_scanned_library(library, &context),
            Err(error) => self.status = Some(error),
        }
        true
    }

    fn apply_scanned_library(&mut self, library: Library, context: &ScanContext) {
        let _ = self.updates.observe_now(&library);
        self.library = library;
        self.tile_covers.clear();
        self.apply_hidden_series();

        self.category_index =
            self.library
                .categories
                .iter()
                .position(|name| name == &context.previous_category)
                .or_else(|| {
                    self.config.default_category.as_deref().and_then(|name| {
                        self.library.categories.iter().position(|item| item == name)
                    })
                })
                .unwrap_or(0);

        let restored = context.previous_series.as_ref().and_then(|path| {
            self.visible_indices()
                .into_iter()
                .position(|index| self.library.series[index].path == *path)
        });
        if let Some(position) = restored {
            self.selected = position;
            self.view = context.previous_view;
            self.chapter_index = context
                .previous_chapter
                .min(self.chapter_count().saturating_sub(1));
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
        let browse_series_exists = self
            .browse_local_series
            .as_deref()
            .is_none_or(|path| self.library.series.iter().any(|series| series.path == path));
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

fn scan_downloaded_series(config: &Config) -> Vec<DownloadedSeries> {
    let Some(root) = downloads_dir(config) else {
        return Vec::new();
    };
    let mut series = Vec::new();
    let Ok(sources) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    for source in sources.filter_map(Result::ok) {
        if !source.path().is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(source.path()) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if !entry.path().is_dir() {
                continue;
            }
            let chapters = std::fs::read_dir(entry.path())
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .filter(|chapter| chapter.path().is_dir())
                        .count()
                })
                .unwrap_or(0);
            if chapters == 0 {
                continue;
            }
            series.push(DownloadedSeries {
                source: source.file_name().to_string_lossy().into_owned(),
                series: entry.file_name().to_string_lossy().into_owned(),
                chapters,
                bytes: directory_size(&entry.path()),
                path: entry.path(),
            });
        }
    }
    series.sort_by(|left, right| {
        left.source
            .to_lowercase()
            .cmp(&right.source.to_lowercase())
            .then_with(|| left.series.to_lowercase().cmp(&right.series.to_lowercase()))
    });
    series
}

fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

/// Crop or pad a page for the current page-mode zoom.
///
/// Zoom-in crops the center; zoom-out pads the image so fitting it into the viewport
/// makes the page smaller. This re-encodes the page when zoom changes.
fn zoomed_page_image(
    image: image::DynamicImage,
    zoom: u16,
    pan: (f32, f32),
) -> image::DynamicImage {
    let scale = f32::from(zoom) / 100.0;
    if (scale - 1.0).abs() < f32::EPSILON {
        return image;
    }
    let (width, height) = (image.width(), image.height());
    if scale > 1.0 {
        let crop_width = ((width as f32) / scale).round().max(1.0) as u32;
        let crop_height = ((height as f32) / scale).round().max(1.0) as u32;
        let max_x = width.saturating_sub(crop_width) as f32;
        let max_y = height.saturating_sub(crop_height) as f32;
        let x = (max_x * (pan.0.clamp(-1.0, 1.0) + 1.0) / 2.0).round() as u32;
        let y = (max_y * (pan.1.clamp(-1.0, 1.0) + 1.0) / 2.0).round() as u32;
        image.crop_imm(x, y, crop_width, crop_height)
    } else {
        let canvas_width = ((width as f32) / scale).round().max(1.0) as u32;
        let canvas_height = ((height as f32) / scale).round().max(1.0) as u32;
        let mut canvas =
            image::RgbaImage::from_pixel(canvas_width, canvas_height, image::Rgba([0, 0, 0, 255]));
        let rgba = image.to_rgba8();
        let x = (canvas_width.saturating_sub(rgba.width())) / 2;
        let y = (canvas_height.saturating_sub(rgba.height())) / 2;
        image::imageops::overlay(&mut canvas, &rgba, i64::from(x), i64::from(y));
        image::DynamicImage::ImageRgba8(canvas)
    }
}

fn current_unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_seconds(value: u64) -> u64 {
    if value > 100_000_000_000 {
        value / 1000
    } else {
        value
    }
}

fn open_external_target(target: &str) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(target)
        .spawn()
        .map(|_| ())
}

fn write_osc52_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;

    let encoded = base64_encode(text.as_bytes());
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
    stdout.flush()
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(TABLE[((value >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((value >> 12) & 0x3f) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((value >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(value & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn global_group_cmp(
    left: &GlobalGroup,
    right: &GlobalGroup,
    pinned: &[String],
) -> std::cmp::Ordering {
    let rank = |group: &GlobalGroup| {
        if group.error.is_some() || group.mangas.is_empty() {
            2
        } else if pinned.iter().any(|key| pin_key_matches(key, &group.source)) {
            0
        } else {
            1
        }
    };
    rank(left)
        .cmp(&rank(right))
        .then_with(|| natural_cmp(&left.source.display_name(), &right.source.display_name()))
}

fn remote_story_key(source_id: &str, url: &str) -> String {
    format!("{source_id}\n{url}")
}

fn remote_series_identity(source_id: &str, url: &str) -> String {
    format!("remote:{source_id}\n{url}")
}

fn series_identity(series: &Series) -> String {
    match &series.remote {
        Some(remote) => remote_series_identity(&remote.source_id, &remote.url),
        None => format!("local:{}", series.path.display()),
    }
}

fn filesystem_added_at(series: &Series) -> Option<u64> {
    if series.remote.is_some() {
        return None;
    }
    let metadata = std::fs::metadata(&series.path).ok()?;
    let time = metadata.created().or_else(|_| metadata.modified()).ok()?;
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn join_source_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_owned();
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn migration_has_updates(matched: &MigrationMatch) -> bool {
    latest_chapter_number(&matched.target_chapters) > latest_chapter_number(&matched.old_chapters)
}

fn backup_added_at(manga: &BackupManga) -> Option<u64> {
    if manga.date_added > 0 {
        Some(manga.date_added as u64)
    } else {
        manga
            .favorite_modified_at
            .filter(|value| *value > 0)
            .map(|value| value as u64 * 1000)
    }
}

fn local_chapter_metas<'a>(
    series: &'a Series,
    progress: &ProgressStore,
    bookmarks: &BookmarkStore,
) -> Vec<ChapterMeta<'a>> {
    series
        .chapters
        .iter()
        .map(|chapter| {
            let read = read_state(
                progress
                    .entry(&chapter.path)
                    .map(|entry| (entry.page, entry.pages)),
            );
            let upload_date = chapter.date.map(|date| {
                i64::from(date.year) * 10_000 + i64::from(date.month) * 100 + i64::from(date.day)
            });
            ChapterMeta {
                title: &chapter.title,
                number: chapter.number,
                upload_date,
                scanlator: chapter.scanlator.as_deref(),
                downloaded: true,
                bookmarked: bookmarks.is_bookmarked(&chapter.path),
                read,
            }
        })
        .collect()
}

fn remote_chapter_metas<'a>(
    source_id: &str,
    manga_url: &str,
    chapters: &'a [RemoteChapter],
    downloaded: &[bool],
    progress: &ProgressStore,
    bookmarks: &BookmarkStore,
) -> Vec<ChapterMeta<'a>> {
    chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            let key = remote_progress_key(source_id, manga_url, &chapter.url);
            let read = read_state(progress.entry(&key).map(|entry| (entry.page, entry.pages)));
            ChapterMeta {
                title: &chapter.name,
                number: (chapter.chapter_number >= 0.0).then_some(chapter.chapter_number),
                upload_date: (chapter.upload_date > 0).then_some(chapter.upload_date),
                scanlator: chapter.scanlator.as_deref(),
                downloaded: downloaded.get(index).copied().unwrap_or(false),
                bookmarked: bookmarks.is_bookmarked(&key),
                read,
            }
        })
        .collect()
}

fn cycle_tri_state(state: TriState, delta: i32) -> TriState {
    if delta < 0 {
        match state {
            TriState::Ignore => TriState::Exclude,
            TriState::Include => TriState::Ignore,
            TriState::Exclude => TriState::Include,
        }
    } else {
        state.cycle()
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
    fn reader_fit_setting_cycles() {
        let mut app = AppState {
            more_section: MoreSection::Settings,
            settings_page: SettingsPage::Reader,
            setting_index: 1,
            ..AppState::default()
        };
        app.adjust_setting(1);
        assert_eq!(app.config.reader_fit, tsuiyomi_core::ReaderFit::Crop);
        app.adjust_setting(1);
        assert_eq!(app.config.reader_fit, tsuiyomi_core::ReaderFit::Scale);
        app.adjust_setting(1);
        assert_eq!(app.config.reader_fit, tsuiyomi_core::ReaderFit::Fit);
    }

    #[test]
    fn right_to_left_mode_reverses_page_keys() {
        let mut app = AppState::default();
        app.set_reading_mode(ReadingMode::RightToLeft);
        assert_eq!(app.reader_mode, ReaderMode::Page);
        assert_eq!(app.reading_direction, ReadingDirection::RightToLeft);
        assert!(app.previous_page_key_advances());

        app.set_reading_mode(ReadingMode::LeftToRight);
        assert_eq!(app.reading_direction, ReadingDirection::LeftToRight);
        assert!(!app.previous_page_key_advances());
    }

    #[test]
    fn paged_vertical_mode_turns_pages_vertically() {
        let mut app = AppState::default();
        app.set_reading_mode(ReadingMode::Vertical);
        assert_eq!(app.reader_mode, ReaderMode::Page);
        assert_eq!(app.reading_direction, ReadingDirection::Vertical);
        assert!(app.reader_vertical_paging());
        assert!(!app.previous_page_key_advances());

        app.set_reading_mode(ReadingMode::Webtoon);
        assert!(!app.reader_vertical_paging());
    }

    #[test]
    fn local_story_categories_can_be_edited() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::new(library(), None);
        app.library.root = root.path().to_path_buf();
        app.select_index(1);
        app.open();
        app.open_category_picker();
        assert!(app
            .category_picker
            .as_ref()
            .is_some_and(|picker| picker.local));
        app.category_picker_toggle();
        app.confirm_category_picker();
        let series = app
            .library
            .series
            .iter()
            .find(|series| series.title == "Berserk")
            .expect("series");
        assert_eq!(series.categories, ["Reading"]);
        assert!(root.path().join("categories.json").exists());
    }

    #[test]
    fn category_management_renames_and_removes_categories() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::new(library(), None);
        app.library.root = root.path().to_path_buf();
        app.tab = 4;
        app.more_section = MoreSection::Categories;
        app.more_focus = MoreFocus::Content;
        app.category_manage_index = 1;

        app.rename_category("Favorites");
        assert!(app.library.categories.contains(&"Favorites".to_owned()));
        assert!(!app.library.categories.contains(&"Reading".to_owned()));
        assert!(app
            .library
            .series
            .iter()
            .any(|series| series.categories.contains(&"Favorites".to_owned())));
        assert!(root.path().join("categories.json").exists());

        app.delete_manage_category();
        assert!(!app.library.categories.contains(&"Favorites".to_owned()));
        assert!(app
            .library
            .series
            .iter()
            .all(|series| !series.categories.contains(&"Favorites".to_owned())));

        app.add_category("Plan to Read");
        assert!(app.library.categories.contains(&"Plan to Read".to_owned()));

        app.add_category("Later");
        app.category_manage_index = 2;
        let before = app.library.categories.clone();
        app.move_manage_category(-1);
        assert_eq!(app.library.categories[1], before[2]);
        assert_eq!(app.category_manage_index, 1);
        app.move_manage_category(-1);
        assert_eq!(app.library.categories[0], "Default");
        assert_eq!(app.category_manage_index, 1);
    }

    #[test]
    fn applying_a_scan_preserves_the_selected_series() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        let context = ScanContext {
            previous_series: Some("Berserk".into()),
            previous_view: LibraryView::Series,
            previous_chapter: 0,
            previous_category: "Default".to_owned(),
        };
        app.apply_scanned_library(library(), &context);
        assert_eq!(
            app.selected_series().map(|series| series.title.as_str()),
            Some("Berserk")
        );
    }

    fn downloads_app(root: &std::path::Path) -> AppState {
        let mut app = AppState::new(library(), None);
        app.config.downloads_path = Some(root.to_path_buf());
        app
    }

    #[test]
    fn imports_a_series_folder_into_the_library() {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let source = temp.path().join("outside").join("Imported Series");
        std::fs::create_dir_all(source.join("Chapter 1")).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(source.join("Chapter 1").join("001.jpg"), b"page")
            .unwrap_or_else(|error| panic!("{error}"));
        let library_root = temp.path().join("local");
        std::fs::create_dir_all(&library_root).unwrap_or_else(|error| panic!("{error}"));

        let mut app = AppState::new(library(), None);
        app.library.root = library_root.clone();
        app.tab = 0;
        app.view = LibraryView::Series;

        app.import_series_folder(&source.to_string_lossy());

        assert!(library_root
            .join("Imported Series")
            .join("Chapter 1")
            .join("001.jpg")
            .is_file());
        assert!(app
            .library
            .series
            .iter()
            .any(|series| series.title == "Imported Series"));
    }

    #[test]
    fn updates_track_new_chapters_and_dismiss() {
        let mut app = AppState::new(library(), None);
        app.updates
            .observe(&app.library, 100)
            .unwrap_or_else(|error| panic!("{error}"));

        app.library.series[0].chapters.push(chapter("Chapter 3"));
        app.observe_updates();

        assert_eq!(app.updates_count(), 1);
        assert_eq!(app.updates_selection, 0);
        assert_eq!(app.update_rows()[0].chapter_title, "Chapter 3");

        app.dismiss_update();
        assert_eq!(app.updates_count(), 0);
        assert_eq!(app.updates_selection, 0);
    }

    #[test]
    fn saved_source_updates_require_the_favorite() {
        let mut app = remote_app();
        let chapter = RemoteChapter {
            url: "/chapter/3".to_owned(),
            name: "Chapter 3".to_owned(),
            chapter_number: 3.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        };
        app.updates.record_remote(
            "1",
            Some("Alpha (EN)"),
            "/manga/test",
            "Test Manga",
            &chapter,
            100,
        );
        assert_eq!(app.updates_count(), 0);

        app.config.remote_library.push(RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/test".to_owned(),
            title: "Test Manga".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: Vec::new(),
        });
        assert_eq!(app.updates_count(), 1);
        let row = app.update_rows()[0];
        assert_eq!(row.chapter_title, "Chapter 3");
        assert_eq!(
            row.remote
                .as_ref()
                .map(|remote| remote.chapter_url.as_str()),
            Some("/chapter/3")
        );
    }

    #[test]
    fn updates_clear_without_reseeding() {
        let mut app = AppState::new(library(), None);
        app.updates
            .observe(&app.library, 100)
            .unwrap_or_else(|error| panic!("{error}"));
        app.library.series[0].chapters.push(chapter("Chapter 3"));
        app.observe_updates();
        assert_eq!(app.updates_count(), 1);

        app.clear_updates();
        assert_eq!(app.updates_count(), 0);

        // Clearing the feed must not make every existing chapter new again.
        app.observe_updates();
        assert_eq!(app.updates_count(), 0);
    }

    #[test]
    fn exports_metadata_for_the_open_local_series() {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = temp.path().join("Series");
        std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));

        let mut app = AppState::new(library(), None);
        app.tab = 0;
        app.view = LibraryView::Chapters;
        app.selected = 0;
        app.library.series[0].path = path.clone();
        app.library.series[0].title = "Exported Series".to_owned();
        app.library.series[0].author = Some("Author".to_owned());
        app.library.series[0].tags = vec!["Action".to_owned()];

        app.export_story_metadata();
        let file = path.join("ComicInfo.xml");
        let xml = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{error}"));
        assert!(xml.contains("<Title>Exported Series</Title>"));
        assert!(xml.contains("<Writer>Author</Writer>"));
        assert!(xml.contains("<Genre>Action</Genre>"));

        // A second export must keep an existing, possibly hand-edited file.
        std::fs::write(&file, "manual").unwrap_or_else(|error| panic!("{error}"));
        app.export_story_metadata();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{error}")),
            "manual"
        );
    }

    #[test]
    fn story_tags_follow_the_open_local_series() {
        let mut app = AppState::new(library(), None);
        app.tab = 0;
        app.view = LibraryView::Chapters;
        app.selected = 0;
        assert!(app.current_story_tags().is_empty());

        app.library.series[0].tags = vec!["Comedy".to_owned(), "Slice of life".to_owned()];
        assert_eq!(
            app.current_story_tags(),
            vec!["Comedy".to_owned(), "Slice of life".to_owned()]
        );
    }

    #[test]
    fn downloads_manager_scans_series_with_chapters() {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let root = temp.path();
        let chapters = root.join("Asura Scans").join("Berserk");
        std::fs::create_dir_all(chapters.join("Chapter 1"))
            .unwrap_or_else(|error| panic!("{error}"));
        std::fs::create_dir_all(chapters.join("Chapter 2"))
            .unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(chapters.join("Chapter 1").join("001.jpg"), b"page")
            .unwrap_or_else(|error| panic!("{error}"));
        // A series directory without chapter directories is not downloaded content.
        std::fs::create_dir_all(root.join("Asura Scans").join("Empty"))
            .unwrap_or_else(|error| panic!("{error}"));

        let mut app = downloads_app(root);
        app.refresh_downloads();

        assert!(app.downloads_loaded);
        assert_eq!(app.downloaded_series.len(), 1);
        let series = &app.downloaded_series[0];
        assert_eq!(series.source, "Asura Scans");
        assert_eq!(series.series, "Berserk");
        assert_eq!(series.chapters, 2);
        assert!(series.bytes > 0);
    }

    #[test]
    fn downloads_manager_deletes_selected_series_only() {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let root = temp.path();
        let target = root.join("Source").join("Berserk");
        let keep = root.join("Source").join("Vagabond");
        for series in [&target, &keep] {
            let chapter = series.join("Chapter 1");
            std::fs::create_dir_all(&chapter).unwrap_or_else(|error| panic!("{error}"));
            std::fs::write(chapter.join("001.jpg"), b"page")
                .unwrap_or_else(|error| panic!("{error}"));
        }

        let mut app = downloads_app(root);
        app.refresh_downloads();
        assert_eq!(app.downloaded_series.len(), 2);

        // Selection is alphabetical: Berserk is first.
        assert_eq!(app.downloads_selection, 0);
        app.open_downloads_delete_confirm();
        assert!(app.downloads_delete_confirm);
        app.confirm_downloads_delete();

        assert!(!app.downloads_delete_confirm);
        assert!(!target.exists());
        assert!(keep.exists());
        assert_eq!(app.downloaded_series.len(), 1);
        assert_eq!(app.downloaded_series[0].series, "Vagabond");
    }

    #[test]
    fn downloads_manager_selection_stays_in_bounds() {
        let mut app = AppState::default();
        app.downloads_previous();
        assert_eq!(app.downloads_selection, 0);
        app.downloads_next();
        assert_eq!(app.downloads_selection, 0);
        app.open_downloads_delete_confirm();
        assert!(!app.downloads_delete_confirm);
    }

    #[test]
    fn library_sorts_by_last_read_and_date_added() {
        let mut app = AppState::new(library(), None);

        let mut older = HistoryEntry::local(
            "Azumanga".to_owned(),
            "Chapter 1".to_owned(),
            "Azumanga".into(),
            "Azumanga/Chapter 1".into(),
            None,
            0,
            1,
        );
        older.last_read = 200;
        app.history.record("Azumanga/Chapter 1".to_owned(), older);
        let mut newer = HistoryEntry::local(
            "Berserk".to_owned(),
            "Chapter 1".to_owned(),
            "Berserk".into(),
            "Berserk/Chapter 1".into(),
            None,
            0,
            1,
        );
        newer.last_read = 300;
        app.history.record("Berserk/Chapter 1".to_owned(), newer);

        app.config.library_sort = LibrarySort::LastReadDesc;
        assert_eq!(
            app.visible_indices()
                .first()
                .and_then(|index| app.library.series.get(*index))
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );

        app.config
            .series_added_at
            .insert("local:Azumanga".to_owned(), 2_000);
        app.config
            .series_added_at
            .insert("local:Berserk".to_owned(), 1_000);
        app.config.library_sort = LibrarySort::DateAddedDesc;
        assert_eq!(
            app.visible_indices()
                .first()
                .and_then(|index| app.library.series.get(*index))
                .map(|series| series.title.as_str()),
            Some("Azumanga")
        );
    }

    #[test]
    fn library_sort_uses_chapter_and_unread_counts() {
        let mut app = AppState::new(library(), None);
        app.config.library_sort = LibrarySort::ChapterCountDesc;
        assert_eq!(
            app.visible_indices()
                .first()
                .and_then(|index| app.library.series.get(*index))
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );

        app.progress.update(Path::new("Chapter 1"), 0, 1);
        app.config.library_sort = LibrarySort::UnreadDesc;
        assert_eq!(
            app.visible_indices()
                .first()
                .and_then(|index| app.library.series.get(*index))
                .map(|series| series.title.as_str()),
            Some("Berserk")
        );
    }

    #[test]
    fn library_search_filters_titles() {
        let mut app = AppState::new(library(), None);
        app.library_search_start();
        for character in "ber".chars() {
            app.library_search_push(character);
        }
        assert_eq!(app.visible_count(), 1);
        assert_eq!(
            app.selected_series().map(|series| series.title.as_str()),
            Some("Berserk")
        );
        app.library_search_pop();
        assert_eq!(app.visible_count(), 1);
        app.library_search_stop(true);
        assert!(app.library_query.is_empty());
        assert!(!app.library_search);
        assert_eq!(app.visible_count(), 2);
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
    fn story_filters_sort_and_selection_follow_visible_chapters() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.open();
        app.progress.update(Path::new("Chapter 1"), 0, 1);

        let key = "local:Berserk".to_owned();
        app.config.chapter_settings.insert(
            key.clone(),
            ChapterSettings {
                unread: TriState::Include,
                sort: ChapterSort::Number,
                descending: true,
                ..ChapterSettings::default()
            },
        );
        let indices = app.story_chapter_indices();
        assert_eq!(indices, vec![1]);
        assert_eq!(app.story_missing_chapters(), 0);

        app.chapter_index = 0;
        app.select_story_position(0);
        assert_eq!(app.chapter_index, 1);

        app.select_all_story_chapters();
        assert_eq!(
            app.multi_select.as_ref().map(|state| state.keys.len()),
            Some(1)
        );
        app.invert_story_selection();
        assert!(app.multi_select.is_none());

        app.invert_story_selection();
        assert_eq!(
            app.multi_select.as_ref().map(|state| state.keys.len()),
            Some(1)
        );

        app.config.chapter_settings.remove(&key);
    }

    #[test]
    fn story_mark_read_updates_progress_and_read_state() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.open();

        app.mark_selected_read(true);
        let entry = app
            .progress
            .entry(Path::new("Chapter 1"))
            .expect("read progress");
        assert_eq!(entry.page + 1, entry.pages);
        assert_eq!(app.story_chapter_read_state(0), ReadState::Read);

        app.mark_selected_read(false);
        assert!(app.progress.entry(Path::new("Chapter 1")).is_none());
        assert_eq!(app.story_chapter_read_state(0), ReadState::Unread);

        app.mark_previous_read();
        assert!(app.progress.entry(Path::new("Chapter 2")).is_some());
    }

    #[test]
    fn continue_reading_prefers_in_progress_then_unread() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.open();
        app.progress.update(Path::new("Chapter 1"), 0, 10);
        app.progress.update(Path::new("Chapter 2"), 0, 10);

        app.continue_reading();
        assert_eq!(app.chapter_index, 0);

        app.progress.remove(Path::new("Chapter 1"));
        app.progress.remove(Path::new("Chapter 2"));
        app.chapter_index = 1;
        app.continue_reading();
        assert_eq!(app.chapter_index, 0);
    }

    #[test]
    fn page_zoom_crops_and_pads_the_source_image() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            200,
            image::Rgb([10, 20, 30]),
        ));
        let zoomed_in = zoomed_page_image(image.clone(), 200, (0.0, 0.0));
        assert_eq!((zoomed_in.width(), zoomed_in.height()), (50, 100));
        let panned = zoomed_page_image(image.clone(), 200, (1.0, -1.0));
        assert_eq!((panned.width(), panned.height()), (50, 100));
        let zoomed_out = zoomed_page_image(image, 50, (0.0, 0.0));
        assert_eq!((zoomed_out.width(), zoomed_out.height()), (200, 400));
    }

    #[test]
    fn tracker_action_offers_to_unlink_a_linked_story() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.view = LibraryView::Chapters;
        app.config.tracking.insert(
            "local:Berserk".to_owned(),
            TrackerLink {
                service: "anilist".to_owned(),
                media_id: 7,
                title: "Berserk".to_owned(),
                total_chapters: Some(380),
                progress: 1,
            },
        );
        app.open_tracker();
        assert!(app.tracker_unlink_confirm);
        app.cancel_tracker_unlink();
        assert!(!app.tracker_unlink_confirm);
    }

    #[test]
    fn page_zoom_pan_selects_page_edges() {
        let mut source = image::RgbaImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                let color = if x < 5 {
                    image::Rgba([255, 0, 0, 255])
                } else {
                    image::Rgba([0, 0, 255, 255])
                };
                source.put_pixel(x, y, color);
            }
        }
        let image = image::DynamicImage::ImageRgba8(source);
        let left = zoomed_page_image(image.clone(), 200, (-1.0, 0.0)).to_rgba8();
        let right = zoomed_page_image(image, 200, (1.0, 0.0)).to_rgba8();
        assert_eq!(left.get_pixel(0, 0), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(right.get_pixel(0, 0), &image::Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn continue_from_the_library_grid_enters_the_chapter_list() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.continue_selected_series();
        assert_eq!(app.view, LibraryView::Chapters);
        assert_eq!(app.chapter_index, 0);
    }

    #[test]
    fn story_notes_and_categories_persist_in_config() {
        let mut app = AppState::new(library(), None);
        app.select_index(1);
        app.open();
        app.save_story_notes("reread the eclipse arc");
        assert_eq!(
            app.config
                .story_notes
                .get("local:Berserk")
                .map(String::as_str),
            Some("reread the eclipse arc")
        );
        app.save_story_notes("  ");
        assert!(!app.config.story_notes.contains_key("local:Berserk"));

        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::RemoteDetails;
        app.remote.manga = Some(RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "ongoing".to_owned(),
            thumbnail_url: None,
            memo: None,
        });
        app.config.remote_library = vec![RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: Vec::new(),
        }];
        app.open_category_picker();
        app.category_picker_next();
        app.category_picker_toggle();
        app.confirm_category_picker();
        assert!(app.config.remote_library[0]
            .categories
            .contains(&"Reading".to_owned()));
    }

    #[test]
    fn download_menu_lists_mihon_batch_actions() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::RemoteDetails;
        app.remote.manga = Some(RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "ongoing".to_owned(),
            thumbnail_url: None,
            memo: None,
        });
        app.remote.chapters = vec![RemoteChapter {
            url: "/manga/alpha/1".to_owned(),
            name: "Chapter 1".to_owned(),
            chapter_number: 1.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        }];
        app.open_download_menu();
        let rows = app.download_menu_rows();
        assert!(rows
            .iter()
            .any(|(_, choice)| *choice == DownloadMenuChoice::Next(25)));
        assert!(rows
            .iter()
            .any(|(_, choice)| *choice == DownloadMenuChoice::Unread));
        assert!(
            rows.iter()
                .any(|(_, choice)| *choice == DownloadMenuChoice::Cancel)
                == app.download_queue.running()
        );
    }

    #[test]
    fn osc52_base64_matches_standard_encoding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"manga/1"), "bWFuZ2EvMQ==");
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
    fn saving_a_new_manga_opens_the_category_choice() {
        let mut app = remote_app();
        let manga = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/new".to_owned(),
            title: "New Manga".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };

        app.request_save_manga(manga);

        let dialog = app.save_categories.as_ref().expect("save categories");
        assert!(dialog.chosen.contains("Default"));
        assert_eq!(dialog.manga.title, "New Manga");
        app.save_categories_next();
        app.save_categories_toggle();
        assert!(app
            .save_categories
            .as_ref()
            .is_some_and(|dialog| dialog.chosen.contains("Reading")));
        app.cancel_save_categories();
        assert!(app.save_categories.is_none());
    }

    #[test]
    fn saving_a_duplicate_opens_the_duplicate_dialog() {
        let mut app = remote_app();
        app.remote.favorites = vec![RemoteFavorite {
            source_id: "2".to_owned(),
            url: "/manga/solo".to_owned(),
            title: "Solo Leveling!".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: vec!["Reading".to_owned()],
        }];
        let manga = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/other".to_owned(),
            title: "Solo  Leveling".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };

        app.request_save_manga(manga);

        let dialog = app.duplicate_dialog.as_ref().expect("duplicate dialog");
        assert_eq!(dialog.duplicates.len(), 1);
        assert_eq!(dialog.duplicates[0].title, "Solo Leveling!");
    }

    #[test]
    fn saved_remote_categories_feed_the_library_grid() {
        let mut app = remote_app();
        app.remote.favorites_loaded = false;
        app.config.remote_library = vec![RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: vec!["Reading".to_owned()],
        }];
        app.library.categories.push("Reading".to_owned());

        app.merge_remote_library();

        let series = app
            .library
            .series
            .iter()
            .find(|series| series.title == "Alpha")
            .expect("saved series");
        assert_eq!(series.categories, ["Reading"]);
    }

    #[test]
    fn removing_a_saved_result_requires_confirmation() {
        let mut app = remote_app();
        let manga = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        app.remote.favorites = vec![RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: Vec::new(),
        }];

        app.request_save_manga(manga);

        assert!(app.duplicate_dialog.is_none());
        assert_eq!(
            app.remove_favorite
                .as_ref()
                .map(|manga| manga.title.as_str()),
            Some("Alpha")
        );
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
            categories: Vec::new(),
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
                id: String::new(),
                base_url: String::new(),
            }],
            repo_url: "https://example.org/index.min.json".to_owned(),
            available_version: Some("1.1".to_owned()),
            obsolete: false,
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
        let (mut app, root) = local_reader_app();

        assert_eq!(app.history_count(), 1);
        let (title, entry) = {
            let recent = app.history.recent();
            let (_, entry) = recent[0];
            (entry.series_title.clone(), entry.clone())
        };
        assert_eq!(title, "History Series");
        assert!(entry.local.is_some());

        app.back();
        app.progress.update(root.path(), 0, 2);
        app.tab = 2;
        app.clamp_history_selection();
        app.open_history_entry();

        assert!(app.reader.is_some());
        assert_eq!(app.reader_origin, ReaderOrigin::History);
        assert_eq!(app.tab, 2);
    }

    #[test]
    fn history_uses_latest_chapter_and_title_search() {
        let mut app = AppState::new(library(), None);
        let make = |title: &str, chapter: &str, time: u64| HistoryEntry {
            series_title: title.to_owned(),
            chapter_title: chapter.to_owned(),
            source: "Local".to_owned(),
            cover: None,
            local: Some(tsuiyomi_core::LocalHistory {
                series_path: PathBuf::from(format!("/library/{title}")),
                chapter_path: PathBuf::from(format!("/library/{title}/{chapter}")),
            }),
            remote: None,
            page: 0,
            pages: 10,
            last_read: time,
            read_duration_secs: 0,
            daily_duration_secs: std::collections::BTreeMap::new(),
        };
        app.history.record(
            "/library/Alpha/Chapter 1".to_owned(),
            make("Alpha", "Chapter 1", 10),
        );
        app.history.record(
            "/library/Alpha/Chapter 2".to_owned(),
            make("Alpha", "Chapter 2", 20),
        );
        app.history.record(
            "/library/Beta/Chapter 1".to_owned(),
            make("Beta", "Chapter 1", 15),
        );

        assert_eq!(app.history_count(), 2);
        assert_eq!(app.history_entries()[0].1.chapter_title, "Chapter 2");

        app.history_query = "beta".to_owned();
        assert_eq!(app.history_count(), 1);
        assert_eq!(app.history_entries()[0].1.series_title, "Beta");
    }

    #[test]
    fn history_delete_clears_chapter_progress() {
        let (mut app, root) = local_reader_app();
        app.back();
        app.tab = 2;
        app.clamp_history_selection();
        assert!(app.progress.entry(root.path()).is_some());

        app.open_history_delete_dialog();
        app.confirm_history_delete();

        assert_eq!(app.history_count(), 0);
        assert!(app.progress.entry(root.path()).is_none());
    }

    #[test]
    fn deleting_history_removes_its_stats() {
        let (mut app, root) = local_reader_app();
        let chapter = root.path().to_path_buf();
        let key = chapter.to_string_lossy().into_owned();
        app.history
            .add_duration(&key, 45, &tsuiyomi_core::today_key());
        app.reading_stats = app.history.reading_stats();
        assert!(app.reading_stats.total_seconds >= 45);
        assert!(!app.reading_stats.top_series(1).is_empty());

        app.back();
        app.tab = 2;
        app.open_history_delete_dialog();
        app.confirm_history_delete();

        assert_eq!(app.history_count(), 0);
        assert_eq!(app.reading_stats.total_seconds, 0);
        assert!(app.reading_stats.top_series(1).is_empty());
        assert_eq!(app.reading_stats.range_seconds(30), 0);
    }

    #[test]
    fn history_clear_all_clears_progress() {
        let (mut app, root) = local_reader_app();
        app.back();
        app.tab = 2;
        app.open_history_clear_confirm();
        app.confirm_history_clear();

        assert_eq!(app.history_count(), 0);
        assert!(app.progress.entry(root.path()).is_none());
    }

    #[test]
    fn finished_history_chapter_opens_the_next_one() {
        let root = tempfile::tempdir().unwrap();
        for (name, color) in [("Chapter 1", [10, 20, 30]), ("Chapter 2", [40, 50, 60])] {
            let dir = root.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            image::RgbImage::from_pixel(10, 20, image::Rgb(color))
                .save(dir.join("001.png"))
                .unwrap();
        }
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
            chapters: vec![
                Chapter {
                    path: root.path().join("Chapter 2"),
                    title: "Chapter 2".to_owned(),
                    number: Some(2.0),
                    kind: ChapterKind::Directory,
                    scanlator: None,
                    date: None,
                },
                Chapter {
                    path: root.path().join("Chapter 1"),
                    title: "Chapter 1".to_owned(),
                    number: Some(1.0),
                    kind: ChapterKind::Directory,
                    scanlator: None,
                    date: None,
                },
            ],
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
        let chapter_one = root.path().join("Chapter 1");
        app.progress.update(&chapter_one, 0, 1);
        app.history.record(
            chapter_one.to_string_lossy().into_owned(),
            HistoryEntry::local(
                "History Series".to_owned(),
                "Chapter 1".to_owned(),
                root.path().to_path_buf(),
                chapter_one,
                None,
                0,
                1,
            ),
        );
        app.history_selection = 0;

        app.open_history_entry();

        assert_eq!(
            app.reader
                .as_ref()
                .map(|reader| reader.chapter_title.as_str()),
            Some("Chapter 2")
        );
    }

    #[test]
    fn cancelling_marks_the_active_download() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut app = AppState::default();
        app.download_queue.active =
            Some(("chapter".to_owned(), "label".to_owned(), cancel.clone()));
        app.cancel_downloads();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(app.download_queue.cancelling);
    }

    #[test]
    fn backup_export_includes_local_series_and_state() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::new(library(), None);
        app.progress.update(Path::new("Chapter 1"), 0, 1);
        app.bookmarks.set(Path::new("Chapter 2"), true);
        let path = root.path().join("backup.tachibk");

        app.export_backup(&path.display().to_string());

        let backup = parse_mihon_backup(&std::fs::read(&path).unwrap()).unwrap();
        assert!(backup.mangas.iter().any(|manga| {
            manga.source_id == 0
                && manga.title == "Berserk"
                && manga.favorite
                && manga.chapters.iter().any(|chapter| chapter.read)
                && manga.chapters.iter().any(|chapter| chapter.bookmark)
        }));
        assert!(backup.sources.iter().any(|source| source.source_id == 0));
    }

    #[test]
    fn storage_cleanup_clears_downloads() {
        let root = tempfile::tempdir().unwrap();
        let downloads = root.path().join("downloads");
        std::fs::create_dir_all(downloads.join("Source/Series/Chapter 1")).unwrap();
        std::fs::write(downloads.join("Source/Series/Chapter 1/001.jpg"), b"x").unwrap();
        let mut app = AppState::default();
        app.config.downloads_path = Some(downloads.clone());

        app.confirm_clear_downloads();

        assert!(downloads.is_dir());
        assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    }

    #[test]
    fn directory_size_walks_nested_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a"), b"12345").unwrap();
        std::fs::create_dir_all(root.path().join("nested")).unwrap();
        std::fs::write(root.path().join("nested/b"), b"123").unwrap();
        assert_eq!(directory_size(root.path()), 8);
    }

    #[test]
    fn backup_import_reports_missing_and_empty_files() {
        let mut app = AppState::default();
        app.import_backup("/definitely/missing/backup.tachibk");
        assert!(app.status.as_deref().unwrap_or("").contains("Cannot read"));

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("empty.tachibk");
        std::fs::write(&path, []).unwrap();
        app.import_backup(&path.display().to_string());
        assert_eq!(app.status.as_deref(), Some("Backup contains no manga."));
    }

    #[test]
    fn downloads_folder_setting_persists_and_clears() {
        let root = tempfile::tempdir().unwrap();
        let mut app = AppState::default();
        app.set_downloads_path(&root.path().join("downloads").display().to_string());
        assert!(app.config.downloads_path.is_some());
        assert!(root.path().join("downloads").is_dir());
        app.set_downloads_path("");
        assert!(app.config.downloads_path.is_none());
    }

    #[test]
    fn flaresolverr_requires_an_http_url() {
        let mut app = AppState::default();
        app.set_flaresolverr_url("127.0.0.1:8191");
        assert!(app.config.flaresolverr_url.is_none());
        app.set_flaresolverr_url("http://127.0.0.1:8191");
        assert_eq!(
            app.config.flaresolverr_url.as_deref(),
            Some("http://127.0.0.1:8191")
        );
        app.set_flaresolverr_url("");
        assert!(app.config.flaresolverr_url.is_none());
    }

    #[test]
    fn reading_time_discards_idle_gaps() {
        let mut app = AppState {
            reading_session: Some(ReadingSession {
                last_activity: Instant::now()
                    .checked_sub(Duration::from_secs(300))
                    .unwrap_or_else(Instant::now),
                accumulated_secs: 7,
                chapter_key: "chapter".to_owned(),
            }),
            ..AppState::default()
        };
        app.note_reading_activity();
        assert_eq!(
            app.reading_session
                .as_ref()
                .map(|session| session.accumulated_secs),
            Some(7)
        );
    }

    #[test]
    fn story_tag_picker_filters_the_local_source() {
        let mut app = AppState::new(library(), None);
        app.library.series[1].tags = vec!["Action".to_owned(), "Dark Fantasy".to_owned()];
        app.select_index(1);
        app.open();

        app.open_tag_picker();
        assert_eq!(
            app.tag_picker.as_ref().map(|picker| picker.tags.len()),
            Some(2)
        );
        app.tag_picker_next();
        app.confirm_tag_picker();

        assert_eq!(app.tab, 3);
        assert_eq!(app.browse_view, BrowseView::Local);
        assert_eq!(app.browse_local_query, "Dark Fantasy");
    }

    #[test]
    fn duplicate_dialog_can_start_migration() {
        let mut app = remote_app();
        let favorite = RemoteFavorite {
            source_id: "2".to_owned(),
            url: "/manga/solo".to_owned(),
            title: "Solo Leveling".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: vec!["Reading".to_owned()],
        };
        app.duplicate_dialog = Some(DuplicateDialog {
            manga: RemoteManga {
                source_id: "1".to_owned(),
                url: "/manga/other".to_owned(),
                title: "Solo Leveling".to_owned(),
                author: None,
                artist: None,
                description: None,
                genre: Vec::new(),
                status: "unknown".to_owned(),
                thumbnail_url: None,
                memo: None,
            },
            duplicates: vec![favorite.clone()],
            selected: 0,
        });

        app.duplicate_migrate();

        assert_eq!(app.tab, 3);
        assert_eq!(app.browse_view, BrowseView::Migrate);
        assert_eq!(app.migrate.source_id.as_deref(), Some("2"));
        assert_eq!(app.migrate.favorites.len(), 1);
        assert_eq!(app.migrate.favorites[0].url, favorite.url);
    }

    #[test]
    fn history_stats_scroll_uses_arrow_keys() {
        let mut app = AppState {
            history_section: HistorySection::Stats,
            ..AppState::default()
        };
        app.history_next();
        app.history_next();
        assert_eq!(app.history_stats_scroll, 2);
        app.history_previous();
        assert_eq!(app.history_stats_scroll, 1);
        app.history_previous();
        app.history_previous();
        assert_eq!(app.history_stats_scroll, 0);
    }

    #[test]
    fn history_entries_can_be_deleted() {
        let (mut app, _root) = local_reader_app();
        assert_eq!(app.history_count(), 1);

        app.tab = 2;
        app.history_selection = 0;
        app.open_history_delete_dialog();
        app.confirm_history_delete();

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
            categories: Vec::new(),
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
                categories: Vec::new(),
            },
            RemoteFavorite {
                source_id: "2".to_owned(),
                url: "/manga/b".to_owned(),
                title: "Beta Remote".to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
                categories: Vec::new(),
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
    fn kitty_reader_close_forces_cover_retransmission() {
        let mut app = AppState::default();
        app.set_image_picker(Picker::halfblocks());
        let path = PathBuf::from("cover.jpg");
        app.insert_tile_cover(
            path.clone(),
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                10,
                20,
                image::Rgb([1, 2, 3]),
            )),
        );
        assert!(app.tile_covers.contains_key(&path));

        if let Some(picker) = app.image_picker.as_mut() {
            picker.set_protocol_type(ProtocolType::Kitty);
        }
        app.reset_cover_protocols();
        assert!(!app.tile_covers.contains_key(&path));
    }

    #[test]
    fn extension_incognito_suppresses_remote_progress_only() {
        let mut app = remote_app();
        app.config.incognito_extensions = vec!["pkg.a".to_owned()];
        let remote = std::path::PathBuf::from("remote://1/hash/hash");
        app.record_progress(&remote, 1, 5);
        assert!(app.progress.entry(&remote).is_none());

        let local = std::path::PathBuf::from("/tmp/library/Chapter 1");
        app.record_progress(&local, 1, 5);
        assert!(app.progress.entry(&local).is_some());
    }

    #[test]
    fn migration_target_order_follows_configuration() {
        let mut app = remote_app();
        app.config.migration_sources =
            vec!["pkg.b|Beta|en".to_owned(), "pkg.a|Alpha|en".to_owned()];
        let targets = app.migration_targets();
        let names: Vec<&str> = targets.iter().map(|source| source.name.as_str()).collect();
        assert_eq!(names, ["Beta", "Alpha"]);

        app.tab = 3;
        app.browse_view = BrowseView::Migrate;
        app.open_migration_sources();
        app.migration_sources_move(1);
        app.apply_migration_sources();
        assert_eq!(
            app.config.migration_sources,
            ["pkg.a|Alpha|en", "pkg.b|Beta|en"]
        );
    }

    #[test]
    fn migration_target_sources_can_be_restricted() {
        let mut app = remote_app();
        assert_eq!(app.migration_targets().len(), 3);
        app.config.migration_sources = vec!["pkg.a|Alpha|en".to_owned()];
        let targets = app.migration_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "Alpha");
    }

    #[test]
    fn manual_migration_search_prefills_the_old_title() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::Migrate;
        app.migrate.screen = MigrateScreen::Matches;
        app.migrate.matches = vec![MigrationMatch {
            old: RemoteFavorite {
                source_id: "1".to_owned(),
                url: "/manga/old".to_owned(),
                title: "Old Title".to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
                categories: Vec::new(),
            },
            old_chapters: Vec::new(),
            target: None,
            target_source: None,
            target_chapters: Vec::new(),
            score: 0.0,
            error: Some("No match found".to_owned()),
            skipped: false,
        }];
        app.browse_selection = 0;

        app.migrate_open();

        let input = app.text_input.as_ref().expect("search input");
        assert_eq!(input.action, TextAction::MigrationSearch);
        assert_eq!(input.value, "Old Title");
    }

    #[test]
    fn local_backup_entries_restore_state() {
        let mut app = AppState::new(library(), None);
        let manga = BackupManga {
            source_id: 0,
            url: "Berserk".to_owned(),
            title: "Berserk".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            thumbnail_url: None,
            chapters: vec![
                BackupChapter {
                    url: "Chapter 1".to_owned(),
                    name: "Chapter 1".to_owned(),
                    scanlator: None,
                    read: true,
                    bookmark: true,
                    last_page_read: 4,
                    date_upload: 0,
                    chapter_number: 1.0,
                },
                BackupChapter {
                    url: "Chapter 2".to_owned(),
                    name: "Chapter 2".to_owned(),
                    scanlator: None,
                    read: false,
                    bookmark: false,
                    last_page_read: 2,
                    date_upload: 0,
                    chapter_number: 2.0,
                },
            ],
            category_ids: vec![7],
            favorite: true,
            notes: "Remember this".to_owned(),
            history: vec![BackupHistory {
                url: "Chapter 1".to_owned(),
                last_read: 1_700_000_000_000,
                read_duration: 90,
            }],
            date_added: 1_600_000_000_000,
            favorite_modified_at: None,
        };
        let categories = std::collections::HashMap::from([(7, "Reading".to_owned())]);

        let result = app
            .import_local_backup_manga(&manga, &categories)
            .expect("local series matches");

        assert_eq!(result, (2, 1, 1, true));
        let first = Path::new("Chapter 1");
        let second = Path::new("Chapter 2");
        assert_eq!(
            app.progress
                .entry(first)
                .map(|entry| (entry.page, entry.pages)),
            Some((4, 5))
        );
        assert_eq!(
            app.progress
                .entry(second)
                .map(|entry| (entry.page, entry.pages)),
            Some((2, 4))
        );
        assert!(app.bookmarks.is_bookmarked(first));
        let history = app.history.get("Chapter 1").expect("history restored");
        assert_eq!(history.last_read, 1_700_000_000);
        assert_eq!(history.read_duration_secs, 90);
        assert_eq!(app.library.series[1].categories, ["Reading"]);
        assert!(app.library.categories.iter().any(|name| name == "Reading"));
        assert_eq!(
            app.config.series_added_at.get("local:Berserk"),
            Some(&1_600_000_000_000)
        );
        assert_eq!(
            app.config
                .story_notes
                .get("local:Berserk")
                .map(String::as_str),
            Some("Remember this")
        );
    }

    #[test]
    fn local_backup_entries_without_a_series_are_skipped() {
        let mut app = AppState::new(library(), None);
        let manga = BackupManga {
            source_id: 0,
            url: "Missing".to_owned(),
            title: "Missing".to_owned(),
            chapters: Vec::new(),
            ..BackupManga::default()
        };
        assert!(app
            .import_local_backup_manga(&manga, &std::collections::HashMap::new())
            .is_none());
    }

    #[test]
    fn extension_base_urls_resolve_relative_story_links() {
        let mut app = remote_app();
        app.remote.extensions = vec![ExtensionInfo {
            name: "Alpha".to_owned(),
            pkg: "pkg.a".to_owned(),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: "alpha.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: "Alpha".to_owned(),
                lang: "en".to_owned(),
                id: "1".to_owned(),
                base_url: "https://example.org".to_owned(),
            }],
            repo_url: "https://example.org/index.min.json".to_owned(),
            available_version: None,
            obsolete: false,
        }];

        assert_eq!(app.source_base_url("1"), Some("https://example.org"));
        assert_eq!(
            join_source_url("https://example.org/", "/manga/test"),
            "https://example.org/manga/test"
        );
        assert_eq!(
            join_source_url("https://example.org", "https://cdn.example/manga"),
            "https://cdn.example/manga"
        );
    }

    #[test]
    fn migrate_apply_reports_when_nothing_is_matched() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::Migrate;
        app.migrate.screen = MigrateScreen::Matches;
        app.migrate.matches.clear();

        app.migrate_apply(true);

        assert!(!app.migrate.applying);
        assert_eq!(
            app.remote.status.as_deref(),
            Some("No matched manga to migrate.")
        );
    }

    fn migration_match_fixture(title: &str, matched: bool, chapters: usize) -> MigrationMatch {
        MigrationMatch {
            old: RemoteFavorite {
                source_id: "1".to_owned(),
                url: format!("/manga/{title}"),
                title: title.to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
                categories: Vec::new(),
            },
            old_chapters: Vec::new(),
            target: matched.then(|| RemoteManga {
                source_id: "2".to_owned(),
                url: format!("/target/{title}"),
                title: title.to_owned(),
                author: None,
                artist: None,
                description: None,
                genre: Vec::new(),
                status: "unknown".to_owned(),
                thumbnail_url: None,
                memo: None,
            }),
            target_source: matched.then(|| RemoteSource {
                id: "2".to_owned(),
                name: "Target".to_owned(),
                lang: "en".to_owned(),
                pkg: None,
                supports_latest: true,
            }),
            target_chapters: (0..chapters)
                .map(|index| RemoteChapter {
                    url: format!("/target/{title}/{index}"),
                    name: format!("Chapter {}", index + 1),
                    chapter_number: index as f32 + 1.0,
                    scanlator: None,
                    upload_date: 0,
                    memo: None,
                })
                .collect(),
            score: if matched { 1.0 } else { 0.0 },
            error: (!matched).then(|| "No match found".to_owned()),
            skipped: false,
        }
    }

    #[test]
    fn migration_match_options_filter_and_prioritize() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::Migrate;
        app.migrate.screen = MigrateScreen::Matches;
        app.migrate.matches = vec![
            migration_match_fixture("Alpha", true, 2),
            migration_match_fixture("Beta", false, 0),
            migration_match_fixture("Gamma", true, 5),
        ];

        app.migrate.hide_unmatched = true;
        assert_eq!(app.migration_visible_indices(), vec![0, 2]);

        app.migrate.hide_unmatched = false;
        app.migrate.prioritize_chapters = true;
        assert_eq!(app.migration_visible_indices(), vec![2, 0, 1]);

        app.migrate.hide_unmatched = true;
        assert_eq!(app.migration_visible_indices(), vec![2, 0]);
        app.browse_selection = 1;
        assert_eq!(app.selected_migration_index(), Some(0));
        app.migrate_toggle_skip();
        assert!(app.migrate.matches[0].skipped);

        app.migrate.hide_unmatched = false;
        app.migrate.prioritize_chapters = false;
        app.migrate.matches[0].old_chapters = vec![RemoteChapter {
            url: "/old/1".to_owned(),
            name: "Chapter 1".to_owned(),
            chapter_number: 1.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        }];
        app.migrate.matches[2].old_chapters = vec![RemoteChapter {
            url: "/old/2".to_owned(),
            name: "Chapter 10".to_owned(),
            chapter_number: 10.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        }];
        app.migrate.hide_without_updates = true;
        assert_eq!(app.migration_visible_indices(), vec![0]);
    }

    #[test]
    fn backing_out_of_migration_cancels_matching() {
        let mut app = remote_app();
        app.migrate.screen = MigrateScreen::Favorites;
        app.migrate.matching = true;
        app.migrate_back();
        assert!(!app.migrate.matching);
        assert_eq!(app.migrate.screen, MigrateScreen::Sources);
    }

    #[test]
    fn migration_sources_and_favorites_follow_saved_library() {
        let mut app = remote_app();
        let favorite = |source: &str, title: &str, categories: Vec<&str>| RemoteFavorite {
            source_id: source.to_owned(),
            url: format!("/manga/{title}"),
            title: title.to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: categories.into_iter().map(str::to_owned).collect(),
        };
        app.config.remote_library = vec![
            favorite("1", "Alpha", vec!["Reading"]),
            favorite("1", "Beta", vec![]),
            favorite("2", "Gamma", vec![]),
        ];

        let sources = app.migrate_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].1, 2);
        assert_eq!(sources[1].1, 1);

        app.browse_selection = 0;
        app.migrate_open();
        assert_eq!(app.migrate.screen, MigrateScreen::Favorites);
        assert_eq!(app.migrate.favorites.len(), 2);
        assert_eq!(app.migrate.favorites[0].title, "Alpha");
    }

    #[test]
    fn migration_copies_and_moves_reading_state() {
        let mut app = remote_app();
        let old_manga = RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/old".to_owned(),
            title: "Old Title".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        let target_manga = RemoteManga {
            source_id: "2".to_owned(),
            url: "/manga/new".to_owned(),
            title: "New Title".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        };
        let chapter = |url: &str, number: f32| RemoteChapter {
            url: url.to_owned(),
            name: format!("Chapter {number}"),
            chapter_number: number,
            scanlator: None,
            upload_date: 0,
            memo: None,
        };
        let old_chapter = chapter("/old/1", 1.0);
        let target_chapter = chapter("/new/1", 1.0);
        let old_key = remote_progress_key("1", "/manga/old", "/old/1");
        let new_key = remote_progress_key("2", "/manga/new", "/new/1");
        app.history.record(
            old_key.to_string_lossy().into_owned(),
            HistoryEntry::remote(
                "Old Title".to_owned(),
                "Chapter 1".to_owned(),
                "Source One".to_owned(),
                old_manga.clone(),
                old_chapter.clone(),
                3,
                10,
            ),
        );
        app.progress.update(&old_key, 3, 10);
        app.bookmarks.set(&old_key, true);
        let matched = MigrationMatch {
            old: RemoteFavorite {
                source_id: "1".to_owned(),
                url: "/manga/old".to_owned(),
                title: "Old Title".to_owned(),
                thumbnail_url: None,
                last_read_url: None,
                last_read_name: None,
                categories: vec!["Reading".to_owned()],
            },
            old_chapters: vec![old_chapter],
            target: Some(target_manga.clone()),
            target_source: Some(app.remote.sources[1].clone()),
            target_chapters: vec![target_chapter],
            score: 1.0,
            error: None,
            skipped: false,
        };

        app.copy_migration_reading_state(&matched, true);

        let new_entry = app
            .history
            .get(&new_key.to_string_lossy())
            .expect("new history");
        assert_eq!(new_entry.page, 3);
        assert_eq!(new_entry.series_title, "New Title");
        assert!(app.history.get(&old_key.to_string_lossy()).is_none());
        assert_eq!(
            app.progress.entry(&new_key).map(|entry| entry.page),
            Some(3)
        );
        assert!(app.progress.entry(&old_key).is_none());
        assert!(app.bookmarks.is_bookmarked(&new_key));
        assert!(!app.bookmarks.is_bookmarked(&old_key));
    }

    #[test]
    fn extension_rows_group_updates_installed_and_available() {
        let mut app = remote_app();
        let extension =
            |name: &str, pkg: &str, installed: bool, available: Option<&str>| ExtensionInfo {
                name: name.to_owned(),
                pkg: pkg.to_owned(),
                lang: "en".to_owned(),
                version: "1.0".to_owned(),
                apk: format!("{pkg}.apk"),
                installed,
                nsfw: false,
                source_count: 1,
                sources: Vec::new(),
                repo_url: "repo".to_owned(),
                available_version: available.map(str::to_owned),
                obsolete: false,
            };
        app.remote.extensions = vec![
            extension("Zeta", "pkg.zeta", false, Some("1.0")),
            extension("Alpha", "pkg.alpha", true, Some("2.0")),
            extension("Beta", "pkg.beta", true, Some("1.0")),
        ];

        let rows = app.extension_rows();
        let labels: Vec<String> = rows
            .iter()
            .map(|row| match row {
                ExtensionRow::Header(label) => format!("[{label}]"),
                ExtensionRow::NotLoaded(index) => app.remote.not_loaded[*index].name.clone(),
                ExtensionRow::Extension(index) => app.remote.extensions[*index].name.clone(),
            })
            .collect();
        assert_eq!(
            labels,
            ["[Updates]", "Alpha", "[Installed]", "Beta", "[EN]", "Zeta"]
        );
        assert_eq!(app.filtered_extension_count(), 3);
    }

    #[test]
    fn not_loaded_extensions_are_grouped_and_removable() {
        let mut app = remote_app();
        app.host_autostart = false;
        app.tab = 3;
        app.browse_view = BrowseView::Extensions;
        app.remote.extensions = vec![ExtensionInfo {
            name: "Comick".to_owned(),
            pkg: "en.comick".to_owned(),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: "comick.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: "Comick".to_owned(),
                lang: "en".to_owned(),
                id: String::new(),
                base_url: String::new(),
            }],
            repo_url: "https://repo/index.min.json".to_owned(),
            available_version: Some("1.0".to_owned()),
            obsolete: false,
        }];
        app.remote.not_loaded = vec![NotLoadedExtension {
            pkg: "en.bad".to_owned(),
            name: "Bad Extension".to_owned(),
            file: "bad.apk".to_owned(),
            reason: "failed".to_owned(),
            message: "manifest missing".to_owned(),
            has_update: false,
        }];

        let labels: Vec<String> = app
            .extension_rows()
            .into_iter()
            .map(|row| match row {
                ExtensionRow::Header(label) => format!("[{label}]"),
                ExtensionRow::NotLoaded(index) => app.remote.not_loaded[index].name.clone(),
                ExtensionRow::Extension(index) => app.remote.extensions[index].name.clone(),
            })
            .collect();
        assert_eq!(
            labels,
            ["[Not loaded]", "Bad Extension", "[Installed]", "Comick"]
        );
        assert_eq!(app.filtered_extension_count(), 2);

        app.browse_selection = 0;
        assert_eq!(
            app.selected_not_loaded()
                .map(|extension| extension.name.as_str()),
            Some("Bad Extension")
        );
        app.install_selected_extension();
        assert!(app
            .remote
            .status
            .as_deref()
            .unwrap_or("")
            .contains("manifest missing"));

        app.uninstall_selected_extension();
        assert_eq!(
            app.uninstall_confirm
                .as_ref()
                .and_then(|items| items.first())
                .map(|(pkg, _)| pkg.as_str()),
            Some("not-loaded:bad.apk")
        );
    }

    #[test]
    fn extension_job_labels_report_progress() {
        let state = ExtensionJobState {
            job_id: "1".to_owned(),
            pkg: "en.comick".to_owned(),
            name: "Comick".to_owned(),
            verb: "Installed".to_owned(),
            apk: "comick.apk".to_owned(),
            state: "downloading".to_owned(),
            progress: 0.42,
            error: None,
        };
        assert_eq!(state.label(), "downloading 42%");
    }

    #[test]
    fn extension_details_toggle_sources_and_actions() {
        let mut app = remote_app();
        let extension = ExtensionInfo {
            name: "MangaDex".to_owned(),
            pkg: "pkg.mangadex".to_owned(),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: "x.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 2,
            sources: Vec::new(),
            repo_url: "repo".to_owned(),
            available_version: None,
            obsolete: false,
        };
        let sources = app.remote.sources[..2].to_vec();
        app.extension_details = Some(ExtensionDetailsState {
            extension,
            sources,
            list_state: ListState::default(),
            fingerprint: None,
        });
        app.browse_view = BrowseView::ExtensionDetails;

        app.browse_selection = 0;
        app.activate_extension_detail_selection();
        assert!(!app.source_enabled(&app.remote.sources[0].clone()));

        app.browse_selection = 2;
        app.activate_extension_detail_selection();
        assert!(app.source_enabled(&app.remote.sources[0].clone()));

        app.browse_selection = 3;
        app.activate_extension_detail_selection();
        assert!(!app.source_enabled(&app.remote.sources[0].clone()));
        assert!(!app.source_enabled(&app.remote.sources[1].clone()));

        app.browse_selection = 6;
        app.activate_extension_detail_selection();
        assert!(app.uninstall_confirm.is_some());
    }

    #[test]
    fn extension_lists_respect_language_and_nsfw_filters() {
        let mut app = remote_app();
        app.config.enabled_languages = Some(vec!["en".to_owned()]);
        let extension = |name: &str, lang: &str, nsfw: bool| ExtensionInfo {
            name: name.to_owned(),
            pkg: format!("pkg.{name}"),
            lang: lang.to_owned(),
            version: "1.0".to_owned(),
            apk: format!("{name}.apk"),
            installed: false,
            nsfw,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: name.to_owned(),
                lang: lang.to_owned(),
                id: String::new(),
                base_url: String::new(),
            }],
            repo_url: "https://repo/index.min.json".to_owned(),
            available_version: Some("1.0".to_owned()),
            obsolete: false,
        };
        app.remote.extensions = vec![
            extension("English", "en", false),
            extension("Japanese", "ja", false),
            extension("Adult", "en", true),
        ];

        assert_eq!(app.filtered_extension_count(), 2);
        app.config.show_nsfw_extensions = false;
        assert_eq!(app.filtered_extension_count(), 1);
        assert_eq!(
            app.extension_rows().into_iter().find_map(|row| match row {
                ExtensionRow::Extension(index) => Some(app.remote.extensions[index].name.as_str()),
                _ => None,
            }),
            Some("English")
        );
    }

    #[test]
    fn extension_search_accepts_comma_separated_terms() {
        let mut app = remote_app();
        let extension = |name: &str| ExtensionInfo {
            name: name.to_owned(),
            pkg: format!("pkg.{name}"),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: format!("{name}.apk"),
            installed: false,
            nsfw: false,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: name.to_owned(),
                lang: "en".to_owned(),
                id: String::new(),
                base_url: String::new(),
            }],
            repo_url: "https://repo/index.min.json".to_owned(),
            available_version: Some("1.0".to_owned()),
            obsolete: false,
        };
        app.remote.extensions = vec![extension("Alpha Beta"), extension("Alpha Gamma")];

        app.extension_query = "alpha, beta".to_owned();
        assert_eq!(app.filtered_extension_count(), 1);
        app.extension_query = "alpha, missing".to_owned();
        assert_eq!(app.filtered_extension_count(), 0);

        app.remote.extensions[0].sources[0].id = "source-42".to_owned();
        app.remote.extensions[0].sources[0].base_url = "https://alpha.example".to_owned();
        app.extension_query = "source-42".to_owned();
        assert_eq!(app.filtered_extension_count(), 1);
        app.extension_query = "alpha.example".to_owned();
        assert_eq!(app.filtered_extension_count(), 1);
    }

    #[test]
    fn source_base_urls_come_from_extension_metadata() {
        let mut app = remote_app();
        let mut extension = ExtensionInfo {
            name: "Alpha".to_owned(),
            pkg: "pkg.alpha".to_owned(),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: "alpha.apk".to_owned(),
            installed: true,
            nsfw: false,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: "Alpha".to_owned(),
                lang: "en".to_owned(),
                id: "1".to_owned(),
                base_url: "https://alpha.example".to_owned(),
            }],
            repo_url: "https://repo/index.min.json".to_owned(),
            available_version: Some("1.0".to_owned()),
            obsolete: false,
        };
        extension.sources[0].id = "1".to_owned();
        app.remote.extensions = vec![extension.clone()];
        app.extension_details = Some(ExtensionDetailsState {
            extension,
            sources: vec![RemoteSource {
                id: "1".to_owned(),
                name: "Alpha".to_owned(),
                lang: "en".to_owned(),
                pkg: Some("pkg.alpha".to_owned()),
                supports_latest: true,
            }],
            list_state: ListState::default(),
            fingerprint: None,
        });
        app.browse_view = BrowseView::ExtensionDetails;
        app.browse_selection = 0;

        assert_eq!(
            app.selected_source_base_url().as_deref(),
            Some("https://alpha.example")
        );
    }

    #[test]
    fn browse_back_clears_result_queries_before_leaving_the_source() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::RemoteResults;
        app.remote.query = "berserk".to_owned();
        app.remote.listing = RemoteListing::Search;

        app.browse_parent();

        assert_eq!(app.browse_view, BrowseView::RemoteResults);
        assert!(app.remote.query.is_empty());
        assert_eq!(app.remote.listing, RemoteListing::Popular);

        app.browse_parent();
        assert_eq!(app.browse_view, BrowseView::Sources);
    }

    #[test]
    fn saved_source_results_can_start_migration() {
        let mut app = remote_app();
        app.tab = 3;
        app.browse_view = BrowseView::RemoteResults;
        app.remote.results = vec![RemoteManga {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: "unknown".to_owned(),
            thumbnail_url: None,
            memo: None,
        }];
        app.config.remote_library = vec![RemoteFavorite {
            source_id: "1".to_owned(),
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: Vec::new(),
        }];
        app.migrate_selected_result();
        assert_eq!(app.browse_view, BrowseView::Migrate);
        assert_eq!(app.migrate.favorites.len(), 1);
    }

    #[test]
    fn global_groups_sort_pinned_first_and_empty_last() {
        let mut app = remote_app();
        app.config.pinned_sources = vec!["pkg.b|Beta|en".to_owned()];
        let group = |index: usize, mangas: usize| GlobalGroup {
            source: app.remote.sources[index].clone(),
            mangas: (0..mangas)
                .map(|position| RemoteManga {
                    source_id: app.remote.sources[index].id.clone(),
                    url: format!("/manga/{position}"),
                    title: format!("Manga {position}"),
                    author: None,
                    artist: None,
                    description: None,
                    genre: Vec::new(),
                    status: "unknown".to_owned(),
                    thumbnail_url: None,
                    memo: None,
                })
                .collect(),
            error: None,
            loading: false,
        };
        app.remote.global_groups = vec![group(0, 0), group(1, 1), group(2, 1)];
        app.sort_global_groups();
        let names: Vec<&str> = app
            .remote
            .global_groups
            .iter()
            .map(|group| group.source.name.as_str())
            .collect();
        assert_eq!(names, ["Beta", "Gamma", "Alpha"]);
    }

    #[test]
    fn global_search_group_and_item_actions_navigate() {
        let mut app = remote_app();
        app.remote.global_query = "berserk".to_owned();
        app.remote.global_groups = vec![GlobalGroup {
            source: app.remote.sources[0].clone(),
            mangas: vec![RemoteManga {
                source_id: "1".to_owned(),
                url: "/manga/berserk".to_owned(),
                title: "Berserk".to_owned(),
                author: None,
                artist: None,
                description: None,
                genre: Vec::new(),
                status: "ongoing".to_owned(),
                thumbnail_url: None,
                memo: None,
            }],
            error: None,
            loading: false,
        }];
        app.remote.global_group = 0;
        app.remote.global_item = 0;

        app.browse_view = BrowseView::GlobalResults;
        app.global_open();
        assert_eq!(app.browse_view, BrowseView::RemoteDetails);
        assert_eq!(
            app.remote.manga.as_ref().map(|manga| manga.title.as_str()),
            Some("Berserk")
        );

        app.browse_view = BrowseView::GlobalResults;
        app.global_open_group();
        assert_eq!(app.browse_view, BrowseView::RemoteResults);
        assert_eq!(app.remote.query, "berserk");
        assert_eq!(app.remote.listing, RemoteListing::Search);
        assert_eq!(
            app.remote.source.as_ref().map(|source| source.id.as_str()),
            Some("1")
        );
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
            obsolete: false,
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
                        id: String::new(),
                        base_url: String::new(),
                    },
                    ExtensionSource {
                        name: "MangaDex".to_owned(),
                        lang: "ja".to_owned(),
                        id: String::new(),
                        base_url: String::new(),
                    },
                ],
                repo_url: "repo".to_owned(),
                available_version: None,
                obsolete: false,
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
                obsolete: false,
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
    fn manual_library_update_reports_an_empty_saved_library() {
        let mut app = remote_app();
        app.config.remote_library.clear();
        app.update_library_now();
        assert_eq!(
            app.status.as_deref(),
            Some("No saved source manga to update.")
        );
    }

    #[test]
    fn store_names_fall_back_to_the_url_host() {
        let mut app = remote_app();
        let store = "https://github.com/keiyoushi/extensions/index.min.json".to_owned();
        assert_eq!(app.store_label(&store), "github.com");

        app.config
            .store_names
            .insert(store.clone(), "Keiyoushi".to_owned());
        assert_eq!(app.store_label(&store), "Keiyoushi");

        app.config
            .store_names
            .insert(store.clone(), "  ".to_owned());
        assert_eq!(app.store_label(&store), "github.com");
    }

    #[test]
    fn extension_search_matches_source_ids_and_base_urls() {
        let mut app = remote_app();
        let extension = |name: &str, pkg: &str, sources: Vec<ExtensionSource>| ExtensionInfo {
            name: name.to_owned(),
            pkg: pkg.to_owned(),
            lang: "en".to_owned(),
            version: "1.0".to_owned(),
            apk: format!("{pkg}.apk"),
            installed: false,
            nsfw: false,
            source_count: sources.len(),
            sources,
            repo_url: "repo".to_owned(),
            available_version: None,
            obsolete: false,
        };
        app.remote.extensions = vec![
            extension(
                "Repo Extension",
                "pkg.repo",
                vec![ExtensionSource {
                    name: "Example".to_owned(),
                    lang: "en".to_owned(),
                    id: "123".to_owned(),
                    base_url: "https://source.example".to_owned(),
                }],
            ),
            extension("Other", "pkg.other", Vec::new()),
        ];
        app.browse_view = BrowseView::Extensions;
        app.browse_tab = BrowseTab::Extensions;

        app.extension_search_start();
        for character in "source.example".chars() {
            app.extension_search_push(character);
        }
        assert_eq!(app.filtered_extension_count(), 1);
        assert_eq!(
            app.selected_extension()
                .map(|extension| extension.name.as_str()),
            Some("Repo Extension")
        );

        app.extension_search_stop(true);
        app.extension_search_start();
        for character in "123".chars() {
            app.extension_search_push(character);
        }
        assert_eq!(app.filtered_extension_count(), 1);
        assert_eq!(
            app.selected_extension()
                .map(|extension| extension.name.as_str()),
            Some("Repo Extension")
        );
    }

    #[test]
    fn global_search_only_results_skips_empty_and_error_groups() {
        let mut app = remote_app();
        app.config.global_search_only_results = true;
        app.remote.global_generation = 3;
        app.remote.global_total = 2;
        app.remote.global_loading = true;

        assert!(app.apply_global_search_event(GlobalSearchEvent::Group {
            generation: 3,
            source: app.remote.sources[0].clone(),
            mangas: Vec::new(),
        }));
        assert!(app.remote.global_groups.is_empty());

        assert!(app.apply_global_search_event(GlobalSearchEvent::Error {
            generation: 3,
            source: app.remote.sources[1].clone(),
            message: "timeout".to_owned(),
        }));
        assert!(app.remote.global_groups.is_empty());
        assert_eq!(app.remote.global_failures.len(), 1);

        app.toggle_global_search_only_results();
        assert!(!app.config.global_search_only_results);
        assert_eq!(
            app.remote.status.as_deref(),
            Some("Showing empty and failed sources")
        );
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
                error: None,
                loading: false,
            },
            GlobalGroup {
                source: app.remote.sources[1].clone(),
                mangas: vec![manga("Gamma")],
                error: None,
                loading: false,
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

        app.setting_index = 4;
        app.adjust_setting(1);
        assert_eq!(app.config.update_interval_hours, 13);
        app.adjust_setting(-20);
        assert_eq!(app.config.update_interval_hours, 0);
        app.adjust_setting(1);
        assert_eq!(app.config.update_interval_hours, 1);
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
