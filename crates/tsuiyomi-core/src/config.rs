use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::{chapter_view::ChapterSettings, host::RemoteFavorite};

/// Reader/viewer modes, mirroring Mihon's `ReadingMode` enum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingMode {
    Default,
    LeftToRight,
    RightToLeft,
    Vertical,
    #[default]
    Webtoon,
    ContinuousVertical,
}

/// Ordering of the library list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    /// Scanner order: natural, case-insensitive folder-name order.
    #[default]
    Natural,
    TitleAsc,
    TitleDesc,
    /// Most chapters first.
    ChapterCountDesc,
    /// Series whose newest chapter is newest first.
    LatestChapterDesc,
    /// Most unread chapters first.
    UnreadDesc,
    /// Most recently read first.
    LastReadDesc,
    /// Most recently added first.
    DateAddedDesc,
    /// Stable pseudo-random order, reseeded when the sort or random action is selected.
    Random,
}

/// How a page image fills the reader viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderFit {
    /// Fit the whole image, preserving aspect ratio.
    #[default]
    Fit,
    /// Fill the viewport, cropping overflow.
    Crop,
    /// Stretch to the viewport.
    Scale,
}

/// Layout for remote source results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceDisplayMode {
    #[default]
    Comfortable,
    Compact,
    List,
}

/// Sources included in Browse's global search.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalSearchScope {
    #[default]
    Pinned,
    All,
}

/// Ordering of the Browse Local source grid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSourceOrder {
    #[default]
    TitleAsc,
    TitleDesc,
}

/// Built-in color themes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    CatppuccinLatte,
    Nord,
    SolarizedDark,
    SolarizedLight,
    GruvboxDark,
    GruvboxLight,
    MaterialDark,
    Dracula,
    OneDark,
    TokyoNight,
    #[serde(other)]
    Unknown,
}

/// OAuth credentials for one tracker service.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackerAuth {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Unix seconds. `None` means the tracker did not provide an expiry.
    #[serde(default)]
    pub expires_at: Option<u64>,
}

/// Library grid layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryDisplayMode {
    /// Cover grid with title and chapter count.
    #[default]
    Comfortable,
    /// Cover grid with title only.
    Compact,
    /// One row per series.
    List,
}

/// Library-wide filters. Each flag is an include-only constraint.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryFilters {
    #[serde(default)]
    pub unread: bool,
    #[serde(default)]
    pub in_progress: bool,
    #[serde(default)]
    pub bookmarked: bool,
    #[serde(default)]
    pub tracked: bool,
    #[serde(default)]
    pub missing_chapters: bool,
    #[serde(default)]
    pub downloaded: bool,
}

/// A story's link to an external tracker.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TrackerLink {
    /// Tracker service identifier, currently `anilist`.
    pub service: String,
    pub media_id: u64,
    pub title: String,
    /// Total chapters reported by the tracker, when known.
    #[serde(default)]
    pub total_chapters: Option<u32>,
    /// Last progress value sent to the tracker.
    #[serde(default)]
    pub progress: u32,
    /// Portable tracker status (`reading`, `completed`, `on_hold`, `dropped`, `planning`).
    #[serde(default)]
    pub status: Option<String>,
    /// 0–10 score, matching Mihon's UI.
    #[serde(default)]
    pub score: Option<f32>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub start_date: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub finish_date: Option<String>,
    /// AniList private-entry flag.
    #[serde(default)]
    pub private: Option<bool>,
}

/// Raw Mihon per-series flags preserved across backup round-trips.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupSeriesExtra {
    /// Mihon viewer flags; the low three bits are the reading mode.
    #[serde(default)]
    pub viewer_flags: Option<i64>,
    /// Mihon chapter flags (tri-state filters, sorting, direction).
    #[serde(default)]
    pub chapter_flags: i64,
    /// Mihon update strategy enum.
    #[serde(default)]
    pub update_strategy: i64,
    /// Scanlators excluded from the chapter list.
    #[serde(default)]
    pub excluded_scanlators: Vec<String>,
    /// Milliseconds since the epoch of the next scheduled update.
    #[serde(default)]
    pub next_update: i64,
}

/// Application config, stored as TOML under the XDG config directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root of the library, containing one directory per series.
    pub library_path: Option<PathBuf>,
    /// Root of downloaded chapters. Defaults to the sibling `downloads/` directory
    /// when the library lives in a Mihon-style `local/` folder.
    pub downloads_path: Option<PathBuf>,
    /// Number of following chapters to queue while reading. Zero disables read-ahead.
    pub download_ahead: usize,
    /// Delete a downloaded chapter after its last page is read.
    pub delete_after_read: bool,
    /// Keep downloaded chapters that are bookmarked when delete-after-read is on.
    pub keep_bookmarked_downloads: bool,
    /// Save downloaded chapters as CBZ archives instead of folders.
    pub save_chapters_as_cbz: bool,
    /// Maximum chapters downloaded at the same time (1–5).
    pub download_concurrency: usize,
    /// Categories whose downloaded chapters are kept when delete-after-read runs.
    pub download_excluded_categories: BTreeSet<String>,
    /// Download newly discovered chapters for every saved series.
    pub auto_download_new_chapters: bool,
    /// Categories the global new-chapter download policy applies to. Empty means all.
    pub auto_download_categories: BTreeSet<String>,
    /// Library update restrictions: skip entries with no unread chapters.
    pub smart_update_unread_only: bool,
    /// Skip saved entries that have never been read.
    pub smart_update_skip_unstarted: bool,
    /// Skip saved entries whose status is completed.
    pub smart_update_skip_completed: bool,
    /// Skip entries whose newest chapter is newer than this many days. Zero disables it.
    pub smart_update_release_days: u64,
    /// Categories the automatic library update applies to. Empty means all.
    pub update_categories: BTreeSet<String>,
    /// Show the new-chapter count on the Updates tab.
    pub updates_tab_badge: bool,
    /// Send desktop notifications for newly discovered chapters.
    pub notify_updates: bool,
    /// Send desktop notifications when downloads fail or finish.
    pub notify_downloads: bool,
    /// Send desktop notifications when a tracker update fails.
    pub notify_tracker_failures: bool,
    /// External extension-host base URL, e.g. `http://127.0.0.1:4567`. When unset,
    /// tsuiyomi starts the bundled host.
    pub host_url: Option<String>,
    /// Explicit `miwayomi.jar` path for development. `None` uses the bundled runtime.
    pub host_jar: Option<PathBuf>,
    /// Optional FlareSolverr endpoint passed to the bundled host.
    pub flaresolverr_url: Option<String>,
    /// Default reader mode.
    pub reading_mode: ReadingMode,
    /// UI color theme.
    pub theme: Theme,
    /// Accent override: a palette name (`blue`) or hex (`#aabbcc`). `None` uses the
    /// theme's default accent.
    pub accent: Option<String>,
    /// Start with the title and commands boxes hidden.
    pub fullscreen: bool,
    /// Minimum width of a cover tile in terminal columns, 8–28.
    pub thumbnail_width: u16,
    /// Library list ordering.
    pub library_sort: LibrarySort,
    /// Active library-wide filters.
    pub library_filters: LibraryFilters,
    /// Library grid layout.
    pub library_display_mode: LibraryDisplayMode,
    /// Hide missing-chapter counts in the story header and details.
    pub hide_missing_chapters: bool,
    /// Mark chapters with the same number from other scanlators as read.
    pub mark_duplicate_chapters_read: bool,
    /// Hours between automatic saved-source chapter checks. Zero disables them.
    pub update_interval_hours: u64,
    /// Category selected on startup. `None` keeps the scanner's first category.
    pub default_category: Option<String>,
    /// Do not write reading progress while enabled.
    pub incognito: bool,
    /// Sources pinned to the top of Browse, keyed by `pkg|name|lang`.
    pub pinned_sources: Vec<String>,
    /// Most recently opened source: `local` or a `pkg|name|lang` key.
    pub last_used_source: Option<String>,
    /// Languages shown in Browse and available extensions. `None` means all.
    pub enabled_languages: Option<Vec<String>>,
    /// Sources hidden from Browse, keyed by `pkg|name|lang`.
    pub disabled_sources: Vec<String>,
    /// Extensions whose reading progress is not recorded.
    pub incognito_extensions: Vec<String>,
    /// Ordering of the Local source grid.
    pub local_source_order: LocalSourceOrder,
    /// Sources included in global search.
    pub global_search_scope: GlobalSearchScope,
    /// Hide sources with no results or errors in global search.
    pub global_search_only_results: bool,
    /// Layout for remote source results.
    pub source_display_mode: SourceDisplayMode,
    /// Hide manga already saved in the library from source results.
    pub browse_hide_saved: bool,
    /// Show extensions whose source marks them NSFW.
    pub show_nsfw_extensions: bool,
    /// Sources used for migration matching, keyed by `pkg|name|lang`. Empty
    /// means every enabled source.
    pub migration_sources: Vec<String>,
    /// Migration transfers chapter progress/bookmarks/history.
    pub migration_transfer_chapters: bool,
    /// Migration transfers categories.
    pub migration_transfer_categories: bool,
    /// Migration transfers story notes.
    pub migration_transfer_notes: bool,
    /// Migration transfers the custom cover.
    pub migration_transfer_cover: bool,
    /// Replace migrations delete old downloads instead of moving them.
    pub migration_remove_downloads: bool,
    /// Display names for extension repository URLs; missing entries use the URL host.
    pub store_names: BTreeMap<String, String>,
    /// Legacy AniList token, kept as a fallback and migrated into `tracker_auth`.
    pub anilist_token: Option<String>,
    /// AniList OAuth application client ID.
    pub anilist_client_id: Option<String>,
    /// AniList OAuth application client secret.
    pub anilist_client_secret: Option<String>,
    /// MyAnimeList API client ID, used for search and OAuth.
    pub mal_client_id: Option<String>,
    /// MyAnimeList OAuth application client secret.
    pub mal_client_secret: Option<String>,
    /// Legacy MyAnimeList token, kept as a fallback and migrated into `tracker_auth`.
    pub mal_token: Option<String>,
    /// OAuth bundles keyed by service (`anilist`, `mal`).
    pub tracker_auth: BTreeMap<String, TrackerAuth>,
    /// Mihon flags retained for lossless backup export, keyed by story key.
    pub backup_series_extra: BTreeMap<String, BackupSeriesExtra>,
    /// Linked tracker entries, keyed by the history series key.
    pub tracking: BTreeMap<String, TrackerLink>,
    /// Trusted extension file fingerprints, keyed by package name.
    pub trusted_extensions: BTreeMap<String, String>,
    /// Saved remote manga, mirrored from the host so the library renders offline.
    pub remote_library: Vec<RemoteFavorite>,
    /// When a series first appeared in the library, keyed by the history series key
    /// (`local:<path>` or `remote:<source>\n<url>`), in milliseconds since the epoch.
    pub series_added_at: BTreeMap<String, u64>,
    /// Local series removed from the library but kept on disk, relative to the
    /// library root. Mihon only drops its database entry; files stay.
    pub hidden_series: Vec<PathBuf>,
    /// Chapter filters, sorting, and display used by new stories. Per-series
    /// overrides live in `chapter_settings`.
    pub chapter_defaults: ChapterSettings,
    /// Per-series chapter settings, keyed by the history series key
    /// (`local:<path>` or `remote:<source>\n<url>`).
    pub chapter_settings: BTreeMap<String, ChapterSettings>,
    /// Free-form per-series notes, using the same keys as `chapter_settings`.
    /// Mihon keeps notes in its manga table; local JSON is the TUI equivalent.
    pub story_notes: BTreeMap<String, String>,
    /// Scroll-mode zoom, 50–300%.
    pub reader_zoom: u16,
    /// Skip already-read chapters when moving between chapters.
    pub reader_skip_read: bool,
    /// Skip chapters with the same number as the current one.
    pub reader_skip_duplicates: bool,
    /// Advance to the next chapter at the last page and back at the first.
    pub reader_chapter_transitions: bool,
    /// Trim uniform page borders before display.
    pub reader_crop_borders: bool,
    /// Render pages in grayscale.
    pub reader_grayscale: bool,
    /// Page-mode image fit.
    pub reader_fit: ReaderFit,
    /// Pages prepared before the viewport.
    pub scroll_prefetch_behind: usize,
    /// Pages prepared after the viewport.
    pub scroll_prefetch_ahead: usize,
    /// Extra cached pages around the viewport.
    pub scroll_cache_margin: usize,
    /// Unknown keys from existing user configs, preserved on save.
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: default_library_path(),
            downloads_path: None,
            download_ahead: 0,
            delete_after_read: false,
            keep_bookmarked_downloads: true,
            save_chapters_as_cbz: false,
            download_concurrency: 1,
            download_excluded_categories: BTreeSet::new(),
            auto_download_new_chapters: false,
            auto_download_categories: BTreeSet::new(),
            smart_update_unread_only: false,
            smart_update_skip_unstarted: false,
            smart_update_skip_completed: false,
            smart_update_release_days: 0,
            update_categories: BTreeSet::new(),
            updates_tab_badge: true,
            notify_updates: true,
            notify_downloads: true,
            notify_tracker_failures: true,
            host_url: None,
            host_jar: None,
            flaresolverr_url: None,
            reading_mode: ReadingMode::default(),
            theme: Theme::default(),
            accent: None,
            fullscreen: false,
            thumbnail_width: 14,
            library_sort: LibrarySort::default(),
            library_filters: LibraryFilters::default(),
            library_display_mode: LibraryDisplayMode::default(),
            hide_missing_chapters: false,
            mark_duplicate_chapters_read: true,
            update_interval_hours: 12,
            default_category: None,
            incognito: false,
            pinned_sources: Vec::new(),
            last_used_source: None,
            enabled_languages: None,
            disabled_sources: Vec::new(),
            incognito_extensions: Vec::new(),
            local_source_order: LocalSourceOrder::default(),
            global_search_scope: GlobalSearchScope::default(),
            global_search_only_results: false,
            source_display_mode: SourceDisplayMode::default(),
            browse_hide_saved: false,
            show_nsfw_extensions: true,
            migration_sources: Vec::new(),
            migration_transfer_chapters: true,
            migration_transfer_categories: true,
            migration_transfer_notes: true,
            migration_transfer_cover: true,
            migration_remove_downloads: false,
            store_names: BTreeMap::new(),
            anilist_token: None,
            anilist_client_id: None,
            anilist_client_secret: None,
            mal_client_id: None,
            mal_client_secret: None,
            mal_token: None,
            tracker_auth: BTreeMap::new(),
            backup_series_extra: BTreeMap::new(),
            tracking: BTreeMap::new(),
            trusted_extensions: BTreeMap::new(),
            remote_library: Vec::new(),
            series_added_at: BTreeMap::new(),
            hidden_series: Vec::new(),
            chapter_defaults: ChapterSettings::default(),
            chapter_settings: BTreeMap::new(),
            story_notes: BTreeMap::new(),
            reader_zoom: 100,
            reader_skip_read: false,
            reader_skip_duplicates: true,
            reader_chapter_transitions: true,
            reader_crop_borders: false,
            reader_grayscale: false,
            reader_fit: ReaderFit::Fit,
            scroll_prefetch_behind: 2,
            scroll_prefetch_ahead: 2,
            scroll_cache_margin: 4,
            extra: BTreeMap::new(),
        }
    }
}

/// `$XDG_DATA_HOME/tsuiyomi/local`, i.e. `~/.local/share/tsuiyomi/local`.
fn default_library_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("local"))
}

/// Load the config. `TSUIYOMI_LIBRARY` overrides the stored path.
pub fn load_config() -> Config {
    let mut config: Config = confy::load("tsuiyomi", "config").unwrap_or_else(|error| {
        eprintln!("failed to load config: {error}; using defaults");
        Config::default()
    });

    if let Some(path) = env::var_os("TSUIYOMI_LIBRARY") {
        config.library_path = Some(PathBuf::from(path));
    }
    if let Ok(url) = env::var("TSUIYOMI_HOST_URL") {
        let url = url.trim();
        config.host_url = (!url.is_empty()).then(|| url.to_owned());
    }

    config
}

/// Persist the config, including unknown keys preserved from the existing file.
///
/// Environment overrides are session-local. Never write `TSUIYOMI_LIBRARY` or
/// `TSUIYOMI_HOST_URL` into the file: a test or temporary shell override would
/// otherwise replace the user's real setting.
pub fn save_config(config: &Config) {
    let mut persisted = config.clone();
    let env_library = env::var_os("TSUIYOMI_LIBRARY").is_some();
    let env_host = env::var_os("TSUIYOMI_HOST_URL").is_some();
    if env_library || env_host {
        let stored = confy::load::<Config>("tsuiyomi", "config").ok();
        if env_library {
            persisted.library_path = stored
                .as_ref()
                .and_then(|stored| stored.library_path.clone())
                .or_else(default_library_path);
        }
        if env_host {
            persisted.host_url = stored.as_ref().and_then(|stored| stored.host_url.clone());
        }
    }
    if let Err(error) = confy::store("tsuiyomi", "config", &persisted) {
        eprintln!("failed to save config: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_and_preserves_unknown_config_keys() {
        let config: Config = toml::from_str(
            r#"
library_path = "/tmp/manga"
host_url = "http://127.0.0.1:4567"
theme = "default"

[player]
external_mpv = ["mpv"]
"#,
        )
        .unwrap();

        assert_eq!(config.library_path, Some(PathBuf::from("/tmp/manga")));
        assert_eq!(config.host_url.as_deref(), Some("http://127.0.0.1:4567"));
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
        // A legacy `theme` value is now a known key; unknown names fall back.
        assert_eq!(config.theme, Theme::Unknown);
        assert_eq!(config.reader_zoom, 100);
        assert!(config.extra.contains_key("player"));

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("[player]"));
    }

    #[test]
    fn browse_source_preferences_roundtrip() {
        let config: Config = toml::from_str(
            r#"
last_used_source = "pkg.a|Alpha|en"
enabled_languages = ["en", "ja"]
disabled_sources = ["pkg.b|Beta|en"]
incognito_extensions = ["pkg.private"]
local_source_order = "title_desc"
global_search_scope = "all"
global_search_only_results = true
source_display_mode = "list"
browse_hide_saved = true
"#,
        )
        .unwrap();
        assert_eq!(config.last_used_source.as_deref(), Some("pkg.a|Alpha|en"));
        assert_eq!(
            config.enabled_languages.as_deref(),
            Some(["en".to_owned(), "ja".to_owned()].as_slice())
        );
        assert_eq!(config.disabled_sources, ["pkg.b|Beta|en"]);
        assert_eq!(config.incognito_extensions, ["pkg.private"]);
        assert_eq!(config.local_source_order, LocalSourceOrder::TitleDesc);
        assert_eq!(config.global_search_scope, GlobalSearchScope::All);
        assert!(config.global_search_only_results);
        assert_eq!(config.source_display_mode, SourceDisplayMode::List);
        assert!(config.browse_hide_saved);

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("last_used_source = \"pkg.a|Alpha|en\""));
        assert!(serialized.contains("enabled_languages"));
        assert!(serialized.contains("disabled_sources"));
        assert!(serialized.contains("incognito_extensions"));
        assert!(serialized.contains("local_source_order = \"title_desc\""));
        assert!(serialized.contains("global_search_scope = \"all\""));
        assert!(serialized.contains("global_search_only_results = true"));
        assert!(serialized.contains("source_display_mode = \"list\""));
        assert!(serialized.contains("browse_hide_saved = true"));
    }

    #[test]
    fn chapter_settings_roundtrip_per_series_and_defaults() {
        use crate::chapter_view::{ChapterDisplay, ChapterSort, TriState};

        let config: Config = toml::from_str(
            r#"
[chapter_defaults]
unread = "include"
sort = "number"
descending = true
display = "number"

[chapter_settings."local:/tmp/manga/Alpha"]
bookmarked = "include"
sort = "alphabet"
excluded_scanlators = ["Team A"]
"#,
        )
        .unwrap();
        assert_eq!(config.chapter_defaults.unread, TriState::Include);
        assert_eq!(config.chapter_defaults.sort, ChapterSort::Number);
        assert!(config.chapter_defaults.descending);
        assert_eq!(config.chapter_defaults.display, ChapterDisplay::Number);
        let series = config
            .chapter_settings
            .get("local:/tmp/manga/Alpha")
            .expect("series settings");
        assert_eq!(series.bookmarked, TriState::Include);
        assert_eq!(series.sort, ChapterSort::Alphabet);
        assert!(series.excluded_scanlators.contains("Team A"));

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("[chapter_defaults]"));
        assert!(serialized.contains("[chapter_settings."));
    }

    #[test]
    fn missing_keys_use_defaults() {
        let config: Config = toml::from_str("theme = \"default\"").unwrap();
        assert_eq!(config.library_path, Config::default().library_path);
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
        assert_eq!(config.reader_zoom, 100);
        assert_eq!(config.scroll_prefetch_behind, 2);
        assert_eq!(config.scroll_prefetch_ahead, 2);
        assert_eq!(config.scroll_cache_margin, 4);
    }

    #[test]
    fn theme_and_accent_roundtrip() {
        let config: Config = toml::from_str(
            r##"
theme = "tokyo_night"
accent = "#ffcc00"
fullscreen = true
thumbnail_width = 18
library_sort = "title_asc"
default_category = "Reading"
incognito = true
"##,
        )
        .unwrap();
        assert_eq!(config.theme, Theme::TokyoNight);
        assert_eq!(config.accent.as_deref(), Some("#ffcc00"));
        assert!(config.fullscreen);
        assert_eq!(config.thumbnail_width, 18);
        assert_eq!(config.library_sort, LibrarySort::TitleAsc);
        assert_eq!(config.default_category.as_deref(), Some("Reading"));
        assert!(config.incognito);

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("theme = \"tokyo_night\""));
        assert!(serialized.contains("accent = \"#ffcc00\""));
        assert!(serialized.contains("fullscreen = true"));
        assert!(serialized.contains("thumbnail_width = 18"));
        assert!(serialized.contains("library_sort = \"title_asc\""));
        assert!(serialized.contains("default_category = \"Reading\""));
        assert!(serialized.contains("incognito = true"));
    }

    #[test]
    fn reading_mode_matches_mihon_names() {
        let config: Config = toml::from_str("reading_mode = \"left_to_right\"").unwrap();
        assert_eq!(config.reading_mode, ReadingMode::LeftToRight);

        let config: Config = toml::from_str("reading_mode = \"webtoon\"").unwrap();
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
    }
}
