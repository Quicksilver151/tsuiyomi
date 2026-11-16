use std::{
    cmp::Ordering,
    fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    chapter::{natural_cmp, parse_chapter_number},
    error::CoreError,
};

/// A scanned library root.
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub root: PathBuf,
    pub series: Vec<Series>,
    /// Category names, `Default` first when it exists.
    pub categories: Vec<String>,
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
    /// Category names this series belongs to.
    pub categories: Vec<String>,
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

#[derive(Debug, Default, Deserialize)]
struct CategoriesFile {
    #[serde(default)]
    categories: Vec<CategoryEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct CategoryEntry {
    name: String,
    #[serde(default)]
    series: Vec<String>,
}

/// Assign categories from `categories.json` in the library root.
///
/// The file looks like:
///
/// ```json
/// {
///   "categories": [
///     { "name": "Reading", "series": ["One Piece", "Frieren"] }
///   ]
/// }
/// ```
///
/// Series not listed anywhere land in the system `Default` category, like Mihon.
fn apply_categories(library: &mut Library) {
    let mut categories = Vec::new();

    if let Ok(text) = fs::read_to_string(library.root.join("categories.json")) {
        if let Ok(file) = serde_json::from_str::<CategoriesFile>(&text) {
            for entry in file.categories {
                let name = entry.name.trim();
                if name.is_empty() || categories.iter().any(|existing| existing == name) {
                    continue;
                }
                let name = name.to_owned();
                for title in entry.series {
                    if let Some(series) = library
                        .series
                        .iter_mut()
                        .find(|series| series.title == title)
                    {
                        if !series.categories.contains(&name) {
                            series.categories.push(name.clone());
                        }
                    }
                }
                categories.push(name);
            }
        }
    }

    let uncategorized = library
        .series
        .iter()
        .any(|series| series.categories.is_empty());
    if categories.is_empty() || uncategorized {
        for series in &mut library.series {
            if series.categories.is_empty() {
                series.categories.push("Default".to_owned());
            }
        }
        if !categories.iter().any(|name| name == "Default") {
            categories.insert(0, "Default".to_owned());
        }
    }

    if categories.is_empty() {
        categories.push("Default".to_owned());
    }

    library.categories = categories;
}

/// Scan every series directory under `root`.
///
/// A missing root produces an empty library instead of an error. Per-series problems are
/// collected in [`Library::errors`] so one broken folder cannot hide the rest.
pub fn scan_library(root: &Path) -> Result<Library, CoreError> {
    let mut library = Library {
        root: root.to_path_buf(),
        ..Library::default()
    };

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            apply_categories(&mut library);
            return Ok(library);
        }
        Err(source) => {
            return Err(CoreError::ReadDir {
                path: root.to_path_buf(),
                source,
            });
        }
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
        if is_hidden(&path) || is_reserved_root_dir(&path) {
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

    apply_categories(&mut library);

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
        categories: Vec::new(),
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

/// App-data folders that live next to the library but are not series.
fn is_reserved_root_dir(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        matches!(
            name.to_string_lossy().to_ascii_lowercase().as_str(),
            "downloads" | "extensions"
        )
    })
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

    #[test]
    fn assigns_categories_and_default() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("Reading Series")).unwrap();
        fs::create_dir_all(root.path().join("Loose Series")).unwrap();
        fs::write(
            root.path().join("categories.json"),
            r#"{"categories":[{"name":"Reading","series":["Reading Series"]}]}"#,
        )
        .unwrap();

        let library = scan_library(root.path()).unwrap();

        assert_eq!(library.categories, ["Default", "Reading"]);
        let reading = library
            .series
            .iter()
            .find(|series| series.title == "Reading Series")
            .unwrap();
        let loose = library
            .series
            .iter()
            .find(|series| series.title == "Loose Series")
            .unwrap();
        assert_eq!(reading.categories, ["Reading"]);
        assert_eq!(loose.categories, ["Default"]);
    }

    #[test]
    fn missing_root_is_an_empty_library() {
        let root = tempdir().unwrap();
        let library = scan_library(&root.path().join("missing")).unwrap();

        assert!(library.series.is_empty());
        assert!(library.errors.is_empty());
        assert_eq!(library.categories, ["Default"]);
    }

    #[test]
    fn ignores_downloads_and_extensions_folders() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("downloads")).unwrap();
        fs::create_dir_all(root.path().join("extensions")).unwrap();
        fs::create_dir_all(root.path().join("Real Series")).unwrap();

        let library = scan_library(root.path()).unwrap();

        assert_eq!(library.series.len(), 1);
        assert_eq!(library.series[0].title, "Real Series");
    }
}
