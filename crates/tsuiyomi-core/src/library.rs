use std::{
    cmp::Ordering,
    fs, io,
    io::Cursor,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    archive,
    chapter::{natural_cmp, parse_chapter_number},
    comic_info::{parse_comic_info, ChapterDate, ComicInfo},
    epub,
    error::CoreError,
    pages::{chapter_pages, read_page},
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
    /// `ComicInfo.xml` or `details.json` title, falling back to the directory name.
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: String,
    pub tags: Vec<String>,
    /// Publishing status, e.g. `Ongoing` from ComicInfo.xml.
    pub status: Option<String>,
    /// A custom `cover.*` image in the series directory.
    pub cover: Option<PathBuf>,
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
    pub scanlator: Option<String>,
    pub date: Option<ChapterDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterKind {
    Directory,
    Zip,
    Rar,
    SevenZip,
    Tar,
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
    status: Option<String>,
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

/// Read a series cover image. Kept in core so frontends never touch the filesystem.
pub fn read_cover(path: &Path) -> Result<Vec<u8>, CoreError> {
    fs::read(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
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
    // Mihon prefers ComicInfo.xml; details.json is the legacy fallback.
    let comic = read_comic_info_file(&path.join("ComicInfo.xml"), &mut warnings);
    let details = if comic.is_none() {
        read_details(path, &mut warnings)
    } else {
        None
    };

    let fallback_title = file_name(path);
    let title = comic
        .as_ref()
        .and_then(|info| info.series.clone())
        .filter(|title| !title.trim().is_empty())
        .or_else(|| {
            details
                .as_ref()
                .and_then(|details| details.title.clone())
                .filter(|title| !title.trim().is_empty())
        })
        .unwrap_or(fallback_title);
    let description = comic
        .as_ref()
        .and_then(|info| info.summary.clone())
        .or_else(|| {
            details
                .as_ref()
                .and_then(|details| details.description.clone())
        })
        .or_else(|| read_optional(path.join(".desc")))
        .unwrap_or_default();
    let author = comic
        .as_ref()
        .and_then(|info| info.writer.clone())
        .or_else(|| details.as_ref().and_then(|details| details.author.clone()));
    let artist = comic
        .as_ref()
        .and_then(ComicInfo::series_artist)
        .or_else(|| details.as_ref().and_then(|details| details.artist.clone()));
    let tags = comic
        .as_ref()
        .map(ComicInfo::series_tags)
        .filter(|tags| !tags.is_empty())
        .or_else(|| details.as_ref().and_then(|details| details.genre.clone()))
        .unwrap_or_default();
    let status = comic
        .as_ref()
        .and_then(|info| info.publishing_status.clone())
        .or_else(|| details.as_ref().and_then(|details| details.status.clone()));
    let chapters = scan_chapters(path, &mut warnings)?;
    let cover = find_cover(path).or_else(|| generate_cover(path, &chapters));

    Ok(Series {
        path: path.to_path_buf(),
        title,
        author,
        artist,
        tags,
        status,
        cover,
        description,
        categories: Vec::new(),
        chapters,
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

fn scan_chapters(
    series_path: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<Chapter>, CoreError> {
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

        let (kind, title) = if file_type.is_dir() {
            (ChapterKind::Directory, file_name(&path))
        } else if let Some(kind) = archive_kind(&path) {
            let title = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            (kind, title)
        } else {
            continue;
        };

        let mut chapter = Chapter {
            path: path.clone(),
            title,
            number: None,
            kind,
            scanlator: None,
            date: None,
        };
        chapter.number = parse_chapter_number(&chapter.title);
        if let Some(info) = read_chapter_comic_info(&path, kind, warnings) {
            apply_chapter_info(&info, &mut chapter);
        }
        chapters.push(chapter);
    }

    chapters.sort_by(|left, right| {
        chapter_order(left, right).then_with(|| natural_cmp(&right.title, &left.title))
    });
    Ok(chapters)
}

fn apply_chapter_info(info: &ComicInfo, chapter: &mut Chapter) {
    if let Some(title) = info
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    {
        chapter.title = title.to_owned();
    }
    if let Some(number) = info.chapter_number() {
        chapter.number = Some(number);
    }
    chapter.scanlator = info
        .translator
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    chapter.date = info.chapter_date();
}

fn read_chapter_comic_info(
    path: &Path,
    kind: ChapterKind,
    warnings: &mut Vec<String>,
) -> Option<ComicInfo> {
    match kind {
        ChapterKind::Directory => read_comic_info_file(&path.join("ComicInfo.xml"), warnings),
        ChapterKind::Epub => epub::chapter_metadata(path, warnings),
        ChapterKind::Zip | ChapterKind::Rar | ChapterKind::SevenZip | ChapterKind::Tar => {
            read_archived_comic_info(path, kind, warnings)
        }
    }
}

fn read_comic_info_file(path: &Path, warnings: &mut Vec<String>) -> Option<ComicInfo> {
    let text = read_optional(path.to_path_buf())?;
    match parse_comic_info(&text) {
        Ok(info) => Some(info),
        Err(error) => {
            warnings.push(format!("{}: {error}", file_name(path)));
            None
        }
    }
}

fn read_archived_comic_info(
    path: &Path,
    kind: ChapterKind,
    warnings: &mut Vec<String>,
) -> Option<ComicInfo> {
    let name = match archive::find_root_entry(path, kind, "ComicInfo.xml") {
        Ok(Some(name)) => name,
        Ok(None) => return None,
        Err(error) => {
            warnings.push(format!("{}: {error}", file_name(path)));
            return None;
        }
    };
    let data = match archive::read_entry_optional(path, kind, &name) {
        Ok(Some(data)) => data,
        Ok(None) => return None,
        Err(error) => {
            warnings.push(format!("{}: {error}", file_name(path)));
            return None;
        }
    };
    let text = match String::from_utf8(data) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("{}: {error}", file_name(path)));
            return None;
        }
    };
    match parse_comic_info(&text) {
        Ok(info) => Some(info),
        Err(error) => {
            warnings.push(format!("{}: {error}", file_name(path)));
            None
        }
    }
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
        "7z" | "cb7" => Some(ChapterKind::SevenZip),
        "tar" | "cbt" => Some(ChapterKind::Tar),
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

/// Find a Mihon custom cover: any image file whose stem is `cover`, case-insensitive.
fn find_cover(dir: &Path) -> Option<PathBuf> {
    let mut covers: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image(path))
        .filter(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.eq_ignore_ascii_case("cover"))
        })
        // Mihon skips non-images with a cover name.
        .filter(|path| image::open(path).is_ok())
        .collect();
    covers.sort_by(|left, right| natural_cmp(&file_name(left), &file_name(right)));
    covers.into_iter().next()
}

/// Mihon creates `cover.jpg` from the first image of the first chapter when the series
/// has no custom cover. Mirror that so the same library layout works in both apps.
fn generate_cover(series_path: &Path, chapters: &[Chapter]) -> Option<PathBuf> {
    let chapter = chapters.last()?;
    for page in chapter_pages(chapter).ok()? {
        let Ok(bytes) = read_page(&page) else {
            continue;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            continue;
        };
        let thumbnail = if image.width() > 512 || image.height() > 512 {
            image.thumbnail(512, 512)
        } else {
            image
        }
        .to_rgb8();

        let mut encoded = Cursor::new(Vec::new());
        if thumbnail
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .is_err()
        {
            continue;
        }

        let cover_path = series_path.join("cover.jpg");
        let temporary = series_path.join(".cover.jpg.tmp");
        if fs::write(&temporary, encoded.into_inner()).is_err() {
            continue;
        }
        let _ = fs::remove_file(&cover_path);
        if fs::rename(&temporary, &cover_path).is_err() {
            let _ = fs::remove_file(&temporary);
            return None;
        }
        return Some(cover_path);
    }
    None
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "bmp"
            )
        })
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

    fn write_image(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        image::RgbImage::from_pixel(30, 45, image::Rgb([20, 40, 60]))
            .save(path)
            .unwrap();
    }

    fn jpeg_bytes() -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(30, 45, image::Rgb([80, 40, 20]))
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .unwrap();
        encoded.into_inner()
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
            r#"{"title":"Real Title","author":"Writer","genre":["action"],"status":"Completed"}"#,
        )
        .unwrap();
        fs::write(series.join(".desc"), "A description").unwrap();

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.title, "Real Title");
        assert_eq!(series.author.as_deref(), Some("Writer"));
        assert_eq!(series.tags, ["action"]);
        assert_eq!(series.status.as_deref(), Some("Completed"));
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
    fn reads_series_comic_info_and_cover() {
        let root = tempdir().unwrap();
        let series = root.path().join("Folder Name");
        fs::create_dir_all(&series).unwrap();
        fs::write(
            series.join("ComicInfo.xml"),
            r#"<ComicInfo>
  <Title>Ignored Series Title</Title>
  <Series>Real &amp; Title</Series>
  <Summary>From ComicInfo</Summary>
  <Writer>Writer Name</Writer>
  <Penciller>Penciller</Penciller>
  <Inker>Inker</Inker>
  <Genre>Action, Fantasy</Genre>
  <Tags>Magic</Tags>
  <Categories>Reading</Categories>
  <PublishingStatusTachiyomi>Ongoing</PublishingStatusTachiyomi>
</ComicInfo>"#,
        )
        .unwrap();
        write_image(&series.join("cover.png"));

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.title, "Real & Title");
        assert_eq!(series.author.as_deref(), Some("Writer Name"));
        assert_eq!(series.artist.as_deref(), Some("Penciller, Inker"));
        assert_eq!(series.description, "From ComicInfo");
        assert_eq!(series.tags, ["Action", "Fantasy", "Magic", "Reading"]);
        assert_eq!(series.status.as_deref(), Some("Ongoing"));
        assert_eq!(series.cover, Some(series.path.join("cover.png")));
    }

    #[test]
    fn comic_info_wins_over_details_json() {
        let root = tempdir().unwrap();
        let series = root.path().join("Series");
        fs::create_dir_all(&series).unwrap();
        fs::write(
            series.join("ComicInfo.xml"),
            "<ComicInfo><Series>XML Title</Series></ComicInfo>",
        )
        .unwrap();
        fs::write(
            series.join("details.json"),
            r#"{"title":"JSON Title","author":"JSON Author"}"#,
        )
        .unwrap();

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.title, "XML Title");
        assert_eq!(series.author, None);
    }

    #[test]
    fn malformed_comic_info_falls_back_to_details_json() {
        let root = tempdir().unwrap();
        let series = root.path().join("Series");
        fs::create_dir_all(&series).unwrap();
        fs::write(series.join("ComicInfo.xml"), "<ComicInfo><Series>broken").unwrap();
        fs::write(series.join("details.json"), r#"{"title":"JSON Title"}"#).unwrap();

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.title, "JSON Title");
        assert_eq!(series.warnings.len(), 1);
    }

    #[test]
    fn reads_chapter_comic_info_from_directories() {
        let root = tempdir().unwrap();
        let chapter = root.path().join("Series/Chapter 1");
        fs::create_dir_all(&chapter).unwrap();
        fs::write(
            chapter.join("ComicInfo.xml"),
            "<ComicInfo><Title>Prologue</Title><Number>0.5</Number>\
             <Translator>Scan Group</Translator><Year>2024</Year><Month>2</Month><Day>7</Day>\
             </ComicInfo>",
        )
        .unwrap();

        let library = scan_library(root.path()).unwrap();
        let chapter = &library.series[0].chapters[0];

        assert_eq!(chapter.title, "Prologue");
        assert_eq!(chapter.number, Some(0.5));
        assert_eq!(chapter.scanlator.as_deref(), Some("Scan Group"));
        assert_eq!(
            chapter.date,
            Some(ChapterDate {
                year: 2024,
                month: 2,
                day: 7
            })
        );
    }

    #[test]
    fn reads_chapter_comic_info_from_zip() {
        use std::io::Write;

        use zip::{write::SimpleFileOptions, ZipWriter};

        let root = tempdir().unwrap();
        let series = root.path().join("Series");
        fs::create_dir_all(&series).unwrap();
        let chapter_path = series.join("Chapter 3.cbz");
        let file = fs::File::create(&chapter_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer.start_file("ComicInfo.xml", options).unwrap();
        writer
            .write_all(
                b"<ComicInfo><Title>Archive Title</Title><Number>3</Number>\
                  <Translator>Archive Group</Translator></ComicInfo>",
            )
            .unwrap();
        writer.start_file("001.jpg", options).unwrap();
        writer.write_all(b"page").unwrap();
        writer.finish().unwrap();

        let library = scan_library(root.path()).unwrap();
        let chapter = &library.series[0].chapters[0];

        assert_eq!(chapter.title, "Archive Title");
        assert_eq!(chapter.number, Some(3.0));
        assert_eq!(chapter.scanlator.as_deref(), Some("Archive Group"));
    }

    #[test]
    fn reads_chapter_metadata_from_seven_zip() {
        let root = tempdir().unwrap();
        let series = root.path().join("Series");
        fs::create_dir_all(&series).unwrap();
        let chapter_path = series.join("Chapter 4.cb7");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&chapter_path).unwrap();
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("ComicInfo.xml"),
                Some(
                    b"<ComicInfo><Title>7z Title</Title><Number>4</Number></ComicInfo>".as_slice(),
                ),
            )
            .unwrap();
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file("001.jpg"),
                Some(b"page".as_slice()),
            )
            .unwrap();
        writer.finish().unwrap();

        let library = scan_library(root.path()).unwrap();
        let chapter = &library.series[0].chapters[0];

        assert_eq!(chapter.title, "7z Title");
        assert_eq!(chapter.number, Some(4.0));
    }

    #[test]
    fn reads_chapter_metadata_from_epub() {
        use std::io::Write;

        use zip::{write::SimpleFileOptions, ZipWriter};

        let root = tempdir().unwrap();
        let series = root.path().join("Series");
        fs::create_dir_all(&series).unwrap();
        let chapter_path = series.join("Chapter 1.epub");
        let file = fs::File::create(&chapter_path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("META-INF/container.xml", options)
            .unwrap();
        writer
            .write_all(
                b"<container><rootfiles><rootfile full-path=\"content.opf\"/></rootfiles></container>",
            )
            .unwrap();
        writer.start_file("content.opf", options).unwrap();
        writer
            .write_all(
                b"<package><metadata><dc:title>EPUB Chapter</dc:title>\
                  <dc:creator>Writer</dc:creator><dc:publisher>Publisher</dc:publisher>\
                  <dc:date>2025-06-07T00:00:00Z</dc:date></metadata>\
                  <manifest/><spine/></package>",
            )
            .unwrap();
        writer.finish().unwrap();

        let library = scan_library(root.path()).unwrap();
        let chapter = &library.series[0].chapters[0];

        assert_eq!(chapter.title, "EPUB Chapter");
        assert_eq!(chapter.scanlator.as_deref(), Some("Publisher"));
        assert_eq!(
            chapter.date,
            Some(ChapterDate {
                year: 2025,
                month: 6,
                day: 7
            })
        );
    }

    #[test]
    fn generates_cover_from_first_chapter_page() {
        let root = tempdir().unwrap();
        let series_path = root.path().join("Series");
        write_image(&series_path.join("Chapter 1/001.jpg"));
        write_image(&series_path.join("Chapter 1/002.jpg"));

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];
        let cover = series.cover.as_ref().expect("generated cover");

        assert_eq!(cover.file_name().unwrap(), "cover.jpg");
        assert!(cover.exists());
        let image = image::open(cover).unwrap();
        assert_eq!((image.width(), image.height()), (30, 45));
    }

    #[test]
    fn does_not_replace_an_existing_cover() {
        let root = tempdir().unwrap();
        let series_path = root.path().join("Series");
        write_image(&series_path.join("cover.png"));
        write_image(&series_path.join("Chapter 1/001.jpg"));

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.cover, Some(series_path.join("cover.png")));
        assert!(!series_path.join("cover.jpg").exists());
    }

    #[test]
    fn generates_cover_from_archive_page() {
        use std::io::Write;

        use zip::{write::SimpleFileOptions, ZipWriter};

        let root = tempdir().unwrap();
        let series_path = root.path().join("Series");
        fs::create_dir_all(&series_path).unwrap();
        let file = fs::File::create(series_path.join("Chapter 1.cbz")).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer.start_file("001.jpg", options).unwrap();
        writer.write_all(&jpeg_bytes()).unwrap();
        writer.finish().unwrap();

        let library = scan_library(root.path()).unwrap();
        let series = &library.series[0];

        assert_eq!(series.cover, Some(series_path.join("cover.jpg")));
    }

    #[test]
    fn generated_cover_skips_corrupt_first_page() {
        let root = tempdir().unwrap();
        let series_path = root.path().join("Series");
        touch(&series_path.join("Chapter 1/001.jpg"));
        write_image(&series_path.join("Chapter 1/002.jpg"));

        let library = scan_library(root.path()).unwrap();
        let cover = library.series[0].cover.as_ref().expect("generated cover");

        assert!(image::open(cover).is_ok());
    }

    #[test]
    fn replaces_an_invalid_cover_with_a_generated_one() {
        let root = tempdir().unwrap();
        let series_path = root.path().join("Series");
        fs::create_dir_all(&series_path).unwrap();
        fs::write(series_path.join("cover.jpg"), b"not an image").unwrap();
        write_image(&series_path.join("Chapter 1/001.jpg"));

        let library = scan_library(root.path()).unwrap();
        let cover = library.series[0].cover.as_ref().expect("generated cover");

        assert_eq!(cover, &series_path.join("cover.jpg"));
        assert!(image::open(cover).is_ok());
    }

    #[test]
    fn empty_chapter_does_not_generate_a_cover() {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("Series/Chapter 1")).unwrap();

        let library = scan_library(root.path()).unwrap();

        assert!(library.series[0].cover.is_none());
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
