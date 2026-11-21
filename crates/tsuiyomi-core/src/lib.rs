//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, backups, or the extension
//! host lives here. The TUI (and any future frontend) should only consume the plain data
//! types in this crate.

mod archive;
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
pub mod library;
pub mod migration;
pub mod pages;
pub mod progress;
pub mod reading_stats;
pub mod runtime;
pub mod source_prefs;
pub mod story_cache;

pub use bookmarks::BookmarkStore;
pub use browse::normalized_title;
pub use chapter::parse_chapter_number;
pub use chapter_view::{
    missing_chapter_count, read_state, visible_chapters, ChapterDisplay, ChapterMeta,
    ChapterSettings, ChapterSort, ReadState, TriState,
};
pub use comic_info::{parse_comic_info, ChapterDate, ComicInfo};
pub use config::{
    load_config, save_config, Config, GlobalSearchScope, LibrarySort, LocalSourceOrder, ReaderFit,
    ReadingMode, SourceDisplayMode, Theme,
};
pub use downloads::{
    chapter_dir, chapter_dir_name, download_chapter, download_chapter_with_cancel, downloads_dir,
    is_downloaded, series_dir_name, source_dir_name,
};
pub use error::CoreError;
pub use extensions::merge_extensions;
pub use filters::SourceFilter;
pub use history::{HistoryEntry, HistoryStore, LocalHistory, RemoteHistory};
pub use host::{
    fetch_bytes, progress_key, progress_prefix, ExtensionInfo, ExtensionSource, HostClient,
    InstallJob, InstallOutcome, NotLoadedExtension, RemoteChapter, RemoteFavorite, RemoteManga,
    RemoteMangaPage, RemoteSource,
};
pub use library::{
    ensure_series_cover, read_cover, scan_library, Chapter, ChapterKind, Library, RemoteSeries,
    ScanError, Series,
};
pub use migration::{best_match, deep_queries, title_similarity, MigrationMatch, MATCH_THRESHOLD};
pub use pages::{chapter_pages, page_dimensions, read_page, Page, PageSource};
pub use progress::{ProgressEntry, ProgressStore};
pub use reading_stats::{date_key_for_unix, today_key, ReadingStats, SeriesReadingTime};
pub use runtime::{
    ensure_runtime, host_data_dir, legacy_remote_cover_path, remote_cover_path, HostProcess,
    HostRuntime, HOST_VERSION,
};
pub use source_prefs::{
    preference_body, SourcePreference, SourcePreferenceKind, SourcePreferenceValue,
    SourcePreferences,
};
pub use story_cache::{load_cached_story, save_cached_story, story_cache_path, CachedStory};
