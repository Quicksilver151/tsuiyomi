//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, backups, or the extension
//! host lives here. The TUI (and any future frontend) should only consume the plain data
//! types in this crate.

mod archive;
pub mod backup;
pub mod bookmarks;
pub mod browse;
pub mod chapter;
pub mod chapter_view;
pub mod comic_info;
pub mod config;
pub mod downloads;
mod epub;
pub mod error;
pub mod extensions;
pub mod filters;
pub mod history;
pub mod host;
pub mod import;
pub mod library;
pub mod migration;
pub mod oauth;
pub mod pages;
pub mod progress;
pub mod reading_stats;
pub mod runtime;
pub mod source_prefs;
pub mod story_cache;
pub mod tracking;
pub mod updates;

pub use backup::{
    encode_mihon_backup, parse_mihon_backup, write_mihon_backup, BackupCategory, BackupChapter,
    BackupHistory, BackupManga, BackupSource, BackupTracking, MihonBackup,
};
pub use bookmarks::BookmarkStore;
pub use browse::normalized_title;
pub use chapter::parse_chapter_number;
pub use chapter_view::{
    missing_chapter_count, read_state, visible_chapters, ChapterDisplay, ChapterMeta,
    ChapterSettings, ChapterSort, ReadState, TriState,
};
pub use comic_info::{parse_comic_info, write_comic_info, ChapterDate, ComicInfo};
pub use config::{
    load_config, save_config, BackupSeriesExtra, CategoryDisplay, Config, GlobalSearchScope,
    LibraryDisplayMode, LibraryFilters, LibrarySort, LocalSourceOrder, ReaderFit, ReadingMode,
    SourceDisplayMode, Theme, TrackerAuth, TrackerLink,
};
pub use downloads::{
    chapter_archive_path, chapter_dir, chapter_dir_name, download_chapter,
    download_chapter_with_cancel, downloads_dir, existing_chapter_path, is_downloaded,
    move_series_downloads, remove_chapter_path, remove_series_downloads, series_dir_name,
    source_dir_name, ChapterDownload,
};
pub use error::CoreError;
pub use extensions::merge_extensions;
pub use filters::SourceFilter;
pub use history::{HistoryEntry, HistoryStore, LocalHistory, RemoteHistory};
pub use host::{
    fetch_bytes, progress_key, progress_prefix, set_default_user_agent, ExtensionInfo,
    ExtensionSource, HostClient, InstallJob, InstallOutcome, NotLoadedExtension, RemoteChapter,
    RemoteFavorite, RemoteManga, RemoteMangaPage, RemoteReadState, RemoteSource,
};
pub use import::import_series;
pub use library::{
    ensure_series_cover, load_category_entries, read_cover, save_category_entries, scan_library,
    CategoryEntry, Chapter, ChapterKind, Library, RemoteSeries, ScanError, Series,
};
pub use migration::{
    best_match, deep_queries, latest_chapter_number, title_similarity, MigrationMatch,
    MATCH_THRESHOLD,
};
pub use oauth::{login as tracker_login, refresh as tracker_refresh, TrackerService};
pub use pages::{chapter_pages, page_dimensions, read_page, Page, PageSource};
pub use progress::{ProgressEntry, ProgressStore};
pub use reading_stats::{date_key_for_unix, today_key, ReadingStats, SeriesReadingTime};
pub use runtime::{
    covers_dir, ensure_runtime, extension_fingerprint, fingerprint_bytes, host_data_dir,
    legacy_remote_cover_path, remote_cover_path, HostProcess, HostRuntime, HOST_VERSION,
};
pub use source_prefs::{
    preference_body, SourcePreference, SourcePreferenceKind, SourcePreferenceValue,
    SourcePreferences,
};
pub use story_cache::{load_cached_story, save_cached_story, story_cache_path, CachedStory};
pub use tracking::{
    search_anilist, search_mal, update_anilist_entry, update_anilist_progress, update_mal_entry,
    update_mal_progress, TrackerEntry, TrackerMedia, TrackerStatus,
};
pub use updates::{RemoteUpdate, UpdateEntry, UpdateStore};
