//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, or backups lives here. The
//! TUI (and any future frontend) should only consume the plain data types in this crate.

mod archive;
pub mod browse;
pub mod chapter;
pub mod comic_info;
pub mod config;
mod epub;
pub mod error;
pub mod import;
pub mod library;
pub mod pages;
pub mod progress;

pub use browse::{browse_home, list_directory, BrowseEntry, BrowseKind};
pub use chapter::parse_chapter_number;
pub use comic_info::{parse_comic_info, ChapterDate, ComicInfo};
pub use config::{load_config, save_config, Config, LibrarySort, ReadingMode, Theme};
pub use error::CoreError;
pub use import::{import_series, ImportedSeries};
pub use library::{read_cover, scan_library, Chapter, ChapterKind, Library, ScanError, Series};
pub use pages::{chapter_pages, read_page, Page, PageSource};
pub use progress::{ProgressEntry, ProgressStore};
