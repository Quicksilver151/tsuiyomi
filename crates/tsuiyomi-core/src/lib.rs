//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, or backups lives here. The
//! TUI (and any future frontend) should only consume the plain data types in this crate.

pub mod chapter;
pub mod config;
pub mod error;
pub mod library;
pub mod pages;
pub mod progress;

pub use chapter::parse_chapter_number;
pub use config::{load_config, save_config, Config, ReadingMode, Theme};
pub use error::CoreError;
pub use library::{scan_library, Chapter, ChapterKind, Library, ScanError, Series};
pub use pages::{chapter_pages, read_page, Page, PageSource};
pub use progress::{ProgressEntry, ProgressStore};
