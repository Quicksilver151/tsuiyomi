//! UI-agnostic backend for tsuiyomi.
//!
//! Everything that touches the filesystem, Mihon metadata, or backups lives here. The
//! TUI (and any future frontend) should only consume the plain data types in this crate.

pub mod chapter;
pub mod config;
pub mod error;
pub mod library;

pub use chapter::parse_chapter_number;
pub use config::{load_config, Config};
pub use error::CoreError;
pub use library::{scan_library, Chapter, ChapterKind, Library, ScanError, Series};
