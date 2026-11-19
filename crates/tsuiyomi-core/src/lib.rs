//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, backups, or the extension
//! host lives here. The TUI (and any future frontend) should only consume the plain data
//! types in this crate.

mod archive;
pub mod chapter;
pub mod comic_info;
pub mod config;
mod epub;
pub mod error;
pub mod host;
pub mod library;
pub mod pages;
pub mod progress;
pub mod runtime;

pub use chapter::parse_chapter_number;
pub use comic_info::{parse_comic_info, ChapterDate, ComicInfo};
pub use config::{load_config, save_config, Config, LibrarySort, ReadingMode, Theme};
pub use error::CoreError;
pub use host::{
    fetch_bytes, progress_key, ExtensionInfo, ExtensionSource, HostClient, InstallOutcome,
    RemoteChapter, RemoteFavorite, RemoteManga, RemoteMangaPage, RemoteSource,
};
pub use library::{
    read_cover, scan_library, Chapter, ChapterKind, Library, RemoteSeries, ScanError, Series,
};
pub use pages::{chapter_pages, page_dimensions, read_page, Page, PageSource};
pub use progress::{ProgressEntry, ProgressStore};
pub use runtime::{
    ensure_runtime, host_data_dir, remote_cover_path, HostProcess, HostRuntime, HOST_VERSION,
};
