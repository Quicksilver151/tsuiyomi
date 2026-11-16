use std::{
    cmp::Ordering,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    chapter::{natural_cmp, parse_chapter_number},
    error::CoreError,
};

/// A scanned Mihon `local/` directory.
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub root: PathBuf,
    pub series: Vec<Series>,
    /// Series that could not be read. A broken folder never aborts the scan.
    pub errors: Vec<ScanError>,
}

#[derive(Debug, Clone)]
pub struct Series {
    pub path: PathBuf,
    /// `details.json` title, falling back to the directory name.
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: String,
    pub tags: Vec<String>,
    /// Newest first, matching Mihon.
    pub chapters: Vec<Chapter>,
    /// Non-fatal metadata problems, e.g. malformed `details.json`.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub path: PathBuf,
    pub title: String,
    pub number: Option<f32>,
    pub kind: ChapterKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterKind {
    Directory,
    Zip,
    Rar,
    Epub,
}

#[derive(Debug, Clone)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Default, Deserialize)]
struct DetailsJson {
    title: Option<String>,
    author: Option<String>,
    artist: Option<String>,
    description: Option<String>,
    genre: Option<Vec<String>>,
}

/// Scan every series directory under `root`.
///
/// Only the root directory itself is fatal. Per-series problems are collected in
/// [`Library::errors`] so one broken folder cannot hide the rest of the library.
pub fn scan_library(root: &Path) -> Result<Library, CoreError> {
    let entries = fs::read_dir(root).map_err(|source| CoreError::ReadDir {
        path: root.to_path_buf(),
        source,
    })?;

    let mut library = Library {
        root: root.to_path_buf(),
        ..Library::default()
    };
    let mut series_paths = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) => {
                library.errors.push(ScanError {
                    path: root.to_path_buf(),
                    message: source.to_string(),
                });
                continue;
            }
        };
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => series_paths.push(path),
            Ok(_) => {}
            Err(source) => library.errors.push(ScanError {
                path,
                message: source.to_string(),
            }),
        }
    }

    series_paths.sort_by(|left, right| natural_cmp(&file_name(left), &file_name(right)));

    for path in series_paths {
        match scan_series(&path) {
            Ok(series) => library.series.push(series),
            Err(error) => library.errors.push(ScanError {
                path,
                message: error.to_string(),
            }),
        }
    }

    Ok(library)
}

fn scan_series(path: &Path) -> Result<Series, CoreError> {
    let mut warnings = Vec::new();
    let details = read_details(path, &mut warnings);
    let fallback_title = file_name(path);
    let description = details
        .as_ref()
        .and_then(|details| details.description.clone())
        .or_else(|| read_optional(path.join(".desc")))
        .unwrap_or_default();

    Ok(Series {
        path: path.to_path_buf(),
        title: details
            .as_ref()
            .and_then(|details| details.title.clone())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(fallback_title),
        author: details.as_ref().and_then(|details| details.author.clone()),
        artist: details.as_ref().and_then(|details| details.artist.clone()),
        tags: details
            .as_ref()
            .and_then(|details| details.genre.clone())
            .unwrap_or_default(),
        description,
        chapters: scan_chapters(path)?,
        warnings,
    })
}

fn read_details(path: &Path, warnings: &mut Vec<String>) -> Option<DetailsJson> {
    let text = read_optional(path.join("details.json"))?;
    match serde_json::from_str(&text) {
        Ok(details) => Some(details),
        Err(error) => {
            warnings.push(format!("details.json: {error}"));
            None
        }
    }
}

fn scan_chapters(series_path: &Path) -> Result<Vec<Chapter>, CoreError> {
    let entries = fs::read_dir(series_path).map_err(|source| CoreError::ReadDir {
        path: series_path.to_path_buf(),
        source,
    })?;

    let mut chapters = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            let title = file_name(&path);
            chapters.push(Chapter {
                number: parse_chapter_number(&title),
                path,
                title,
                kind: ChapterKind::Directory,
            });
        } else if let Some(kind) = archive_kind(&path) {
            let title = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            chapters.push(Chapter {
                number: parse_chapter_number(&title),
                path,
                title,
                kind,
            });
        }
    }

    chapters.sort_by(|left, right| {
        chapter_order(left, right).then_with(|| natural_cmp(&right.title, &left.title))
    });
    Ok(chapters)
}

fn chapter_order(left: &Chapter, right: &Chapter) -> Ordering {
    match (left.number, right.number) {
        (Some(left), Some(right)) => right.partial_cmp(&left).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn archive_kind(path: &Path) -> Option<ChapterKind> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "zip" | "cbz" => Some(ChapterKind::Zip),
        "rar" | "cbr" => Some(ChapterKind::Rar),
        "epub" => Some(ChapterKind::Epub),
        _ => None,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

fn read_optional(path: PathBuf) -> Option<String> {
    fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn scans_series_and_chapters() {
        let root = tempdir().unwrap();
        touch(&root.path().join("Series A/Chapter 1/001.jpg"));
        touch(&root.path().join("Series A/Ch. 2.cbz"));
        touch(&root.path().join("Series A/cover.jpg"));
        touch(&root.path().join("Series B/Chapter 1/001.jpg"));
        touch(&root.path().join(".hidden/Chapter 1/001.jpg"));

        let library = scan_library(root.path()).unwrap();

        assert!(library.errors.is_empty());
        assert_eq!(library.series.len(), 2);
        assert_eq!(library.series[0].title, "Series A");
        assert_eq!(library.series[0].chapters.len(), 2);
        assert_eq!(library.series[0].chapters[0].title, "Ch. 2");
        assert_eq!(library.series[0].chapters[0].number, Some(2.0));
        assert_eq!(library.series[0].chapters[1].title, "Chapter 1");
    }

    #[test]
    fn reads_details_json_and_desc() {
        let root = tempdir().unwrap();
        let series = root.path().join("Folder Name");
        fs::create_dir_all(&series).unwrap();
        fs::write(
            series.join("details.json"),
            r#"{"title":"Real Title","author":"Writer","genre":["action"]}"#,
        )
        .unwrap();
        fs::write(series.join(".desc"), "A description").unwrap();

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.title, "Real Title");
        assert_eq!(series.author.as_deref(), Some("Writer"));
        assert_eq!(series.tags, ["action"]);
        assert_eq!(series.description, "A description");
    }

    #[test]
    fn malformed_details_does_not_hide_series() {
        let root = tempdir().unwrap();
        let series = root.path().join("Series");
        fs::create_dir_all(&series).unwrap();
        fs::write(series.join("details.json"), "{not json").unwrap();

        let library = scan_library(root.path()).unwrap();

        assert_eq!(library.series.len(), 1);
        assert_eq!(library.series[0].title, "Series");
        assert_eq!(library.series[0].warnings.len(), 1);
    }
}
