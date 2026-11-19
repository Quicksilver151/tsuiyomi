//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, backups, or the extension
//! host lives here. The TUI (and any future frontend) should only consume the plain data
//! types in this crate.

mod archive;
pub mod bookmarks;
pub mod chapter;
pub mod comic_info;
pub mod config;
pub mod downloads;
mod epub;
pub mod error;
pub mod host;
pub mod library;
pub mod pages;
pub mod progress;
pub mod runtime;
pub mod story_cache;

pub use bookmarks::BookmarkStore;
pub use chapter::parse_chapter_number;
pub use comic_info::{parse_comic_info, ChapterDate, ComicInfo};
pub use config::{load_config, save_config, Config, LibrarySort, ReadingMode, Theme};
pub use downloads::{
    chapter_dir, chapter_dir_name, download_chapter, downloads_dir, is_downloaded, series_dir_name,
    source_dir_name,
};
pub use error::CoreError;
pub use host::{
    fetch_bytes, progress_key, ExtensionInfo, ExtensionSource, HostClient, InstallOutcome,
    RemoteChapter, RemoteFavorite, RemoteManga, RemoteMangaPage, RemoteSource,
};
pub use library::{
    ensure_series_cover, read_cover, scan_library, Chapter, ChapterKind, Library, RemoteSeries,
    ScanError, Series,
};
pub use pages::{chapter_pages, page_dimensions, read_page, Page, PageSource};
pub use progress::{ProgressEntry, ProgressStore};
pub use runtime::{
    ensure_runtime, host_data_dir, legacy_remote_cover_path, remote_cover_path, HostProcess,
    HostRuntime, HOST_VERSION,
};
pub use story_cache::{load_cached_story, save_cached_story, story_cache_path, CachedStory};
