//! Chapter downloads using Mihon's on-disk layout.
//!
//! `<downloads>/<source>/<series>/<chapter>/<page>.<ext>`, where the source
//! directory is the source's display name (for example `Asura Scans (EN)`) and the
//! series and chapter directories are sanitized the same way as Mihon's
//! `DownloadProvider` (`DiskUtil.buildValidFilename`).
//!
//! Pages are numbered from 1 with at least three digits. The file extension comes
//! from the image bytes, not the URL, because CDN URLs often lie about the format.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::Config,
    error::CoreError,
    pages::{Page, PageSource},
};

/// Mihon's `DiskUtil.MAX_FILE_NAME_BYTES`.
const MAX_NAME_BYTES: usize = 240;
/// Reserve space for the hash suffix and `.cbz`, like Mihon's chapter directory names.
const MAX_CHAPTER_NAME_BYTES: usize = MAX_NAME_BYTES - 11;

/// Root of the download tree.
///
/// A Mihon root keeps `local/` and `downloads/` side by side, so a library that lives
/// in a `local` folder gets its sibling `downloads` folder. Explicit
/// `downloads_path`/`TSUIYOMI_DOWNLOADS` wins; otherwise the app data directory.
pub fn downloads_dir(config: &Config) -> Option<PathBuf> {
    // Read the environment here instead of copying it into Config: configs are
    // persisted from memory, and a session-local override must not end up on disk.
    if let Some(path) = std::env::var_os("TSUIYOMI_DOWNLOADS") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    if let Some(path) = &config.downloads_path {
        return Some(path.clone());
    }
    if let Some(library) = &config.library_path {
        if library.file_name().and_then(|name| name.to_str()) == Some("local") {
            if let Some(parent) = library.parent() {
                return Some(parent.join("downloads"));
            }
        }
    }
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("downloads"))
}

/// Sanitize one path component like Mihon's `DiskUtil.buildValidFilename`.
pub fn sanitize_component(value: &str) -> String {
    let name = value.trim_matches(|character| character == '.' || character == ' ');
    if name.is_empty() {
        return "(invalid)".to_owned();
    }

    let mut sanitized = String::with_capacity(name.len());
    for character in name.chars() {
        match character {
            '\u{00}'..='\u{1f}'
            | '"'
            | '*'
            | '/'
            | ':'
            | '<'
            | '>'
            | '?'
            | '\\'
            | '|'
            | '\u{7f}' => sanitized.push('_'),
            _ => sanitized.push(character),
        }
    }
    truncate_utf8(&sanitized, MAX_NAME_BYTES)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

pub fn source_dir_name(source_name: &str) -> String {
    sanitize_component(source_name)
}

pub fn series_dir_name(title: &str) -> String {
    sanitize_component(title)
}

/// Directory name for one chapter.
///
/// Mihon prefixes the scanlator when there is one, and trims the result to leave room
/// for the optional URL hash suffix it appends when `enableChapterNameHash` is on.
pub fn chapter_dir_name(chapter_name: &str, scanlator: Option<&str>) -> String {
    let name = chapter_name.trim();
    let name = if name.is_empty() { "Chapter" } else { name };
    let base = match scanlator.map(str::trim).filter(|value| !value.is_empty()) {
        Some(scanlator) => format!("{scanlator}_{name}"),
        None => name.to_owned(),
    };
    let sanitized = sanitize_component(&base);
    truncate_utf8(&sanitized, MAX_CHAPTER_NAME_BYTES)
}

/// Full directory for a chapter, without downloading anything.
pub fn chapter_dir(
    root: &Path,
    source_name: &str,
    series_title: &str,
    chapter_name: &str,
    scanlator: Option<&str>,
) -> PathBuf {
    root.join(source_dir_name(source_name))
        .join(series_dir_name(series_title))
        .join(chapter_dir_name(chapter_name, scanlator))
}

/// Move every downloaded chapter of one series to another source/series.
///
/// Migrate replace uses this so pages follow the manga to its new source. The old
/// series directory is removed when it becomes empty. Existing chapter directories at
/// the destination win, matching Mihon's replace behavior. Returns whether any
/// downloaded content was found for the old series.
pub fn move_series_downloads(
    root: &Path,
    old_source_name: &str,
    old_series_title: &str,
    new_source_name: &str,
    new_series_title: &str,
) -> Result<bool, CoreError> {
    let old = root
        .join(source_dir_name(old_source_name))
        .join(series_dir_name(old_series_title));
    let new = root
        .join(source_dir_name(new_source_name))
        .join(series_dir_name(new_series_title));
    if old == new || !old.is_dir() {
        return Ok(false);
    }

    if !new.exists() {
        if let Some(parent) = new.parent() {
            fs::create_dir_all(parent).map_err(|source| CoreError::WriteFile {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::rename(&old, &new).map_err(|source| CoreError::WriteFile {
            path: new.clone(),
            source,
        })?;
    } else {
        let entries = fs::read_dir(&old).map_err(|source| CoreError::ReadDir {
            path: old.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| CoreError::ReadDir {
                path: old.clone(),
                source,
            })?;
            let target = new.join(entry.file_name());
            if target.exists() {
                continue;
            }
            fs::rename(entry.path(), &target).map_err(|source| CoreError::WriteFile {
                path: target,
                source,
            })?;
        }
        // Only removes the directory when every chapter moved.
        let _ = fs::remove_dir(&old);
    }

    // Drop the old source directory once its last series moved.
    if let Some(source_dir) = old.parent() {
        if fs::read_dir(source_dir).is_ok_and(|mut entries| entries.next().is_none()) {
            let _ = fs::remove_dir(source_dir);
        }
    }
    Ok(true)
}

/// Delete every downloaded chapter for one series. Returns whether the directory existed.
pub fn remove_series_downloads(
    root: &Path,
    source_name: &str,
    series_title: &str,
) -> Result<bool, CoreError> {
    let source_dir = root.join(source_dir_name(source_name));
    let series_dir = source_dir.join(series_dir_name(series_title));
    if !series_dir.is_dir() {
        return Ok(false);
    }
    fs::remove_dir_all(&series_dir).map_err(|source| CoreError::WriteFile {
        path: series_dir,
        source,
    })?;
    if fs::read_dir(&source_dir).is_ok_and(|mut entries| entries.next().is_none()) {
        let _ = fs::remove_dir(&source_dir);
    }
    Ok(true)
}

/// Whether a chapter already has a complete download directory.
pub fn is_downloaded(
    root: &Path,
    source_name: &str,
    series_title: &str,
    chapter_name: &str,
    scanlator: Option<&str>,
) -> bool {
    let dir = chapter_dir(root, source_name, series_title, chapter_name, scanlator);
    dir.is_dir()
        && fs::read_dir(&dir).is_ok_and(|mut entries| {
            entries.any(|entry| {
                entry.is_ok_and(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            })
        })
}

/// Download every remote page of a chapter.
///
/// Pages are written to a temporary sibling directory and renamed into place only
/// after all of them succeed, so a partial download never looks complete.
pub fn download_chapter(
    root: &Path,
    source_name: &str,
    series_title: &str,
    chapter_name: &str,
    scanlator: Option<&str>,
    pages: &[Page],
) -> Result<PathBuf, CoreError> {
    download_chapter_with_cancel(
        root,
        source_name,
        series_title,
        chapter_name,
        scanlator,
        pages,
        || false,
    )
}

/// Download a chapter, checking `cancelled` between page fetches. A cancelled
/// download removes its temporary directory and never replaces an existing chapter.
pub fn download_chapter_with_cancel(
    root: &Path,
    source_name: &str,
    series_title: &str,
    chapter_name: &str,
    scanlator: Option<&str>,
    pages: &[Page],
    cancelled: impl Fn() -> bool,
) -> Result<PathBuf, CoreError> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let target = chapter_dir(root, source_name, series_title, chapter_name, scanlator);
    let temporary = target.with_extension("tmp");
    if temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    fs::create_dir_all(&temporary).map_err(|source| CoreError::WriteFile {
        path: temporary.clone(),
        source,
    })?;

    let digits = pages.len().max(1).to_string().len().max(3);
    for (index, page) in pages.iter().enumerate() {
        if cancelled() {
            let _ = fs::remove_dir_all(&temporary);
            return Err(CoreError::Cancelled);
        }
        let PageSource::Remote { url } = &page.source else {
            continue;
        };
        let bytes = crate::host::fetch_bytes(url)?;
        if cancelled() {
            let _ = fs::remove_dir_all(&temporary);
            return Err(CoreError::Cancelled);
        }
        let extension = image_extension(&bytes);
        let name = format!("{:0width$}.{extension}", index + 1, width = digits);
        fs::write(temporary.join(name), bytes).map_err(|source| CoreError::WriteFile {
            path: temporary.clone(),
            source,
        })?;
    }

    if target.exists() {
        fs::remove_dir_all(&target).map_err(|source| CoreError::WriteFile {
            path: target.clone(),
            source,
        })?;
    }
    fs::rename(&temporary, &target).map_err(|source| CoreError::WriteFile {
        path: target.clone(),
        source,
    })?;
    Ok(target)
}

fn image_extension(bytes: &[u8]) -> &'static str {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::Jpeg) => "jpg",
        Ok(image::ImageFormat::WebP) => "webp",
        Ok(image::ImageFormat::Gif) => "gif",
        Ok(image::ImageFormat::Avif) => "avif",
        Ok(image::ImageFormat::Bmp) => "bmp",
        _ => "jpg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_like_mihon() {
        assert_eq!(sanitize_component(" Asura Scans (EN) "), "Asura Scans (EN)");
        assert_eq!(sanitize_component("a/b:c?d"), "a_b_c_d");
        assert_eq!(sanitize_component("..."), "(invalid)");
        assert_eq!(sanitize_component(".hidden"), "hidden");
    }

    #[test]
    fn cancelled_downloads_do_not_create_directories() {
        let root = tempfile::tempdir().unwrap();
        let error = download_chapter_with_cancel(
            root.path(),
            "Source",
            "Series",
            "Chapter 1",
            None,
            &[],
            || true,
        )
        .unwrap_err();
        assert!(matches!(error, CoreError::Cancelled));
        assert!(!chapter_dir(root.path(), "Source", "Series", "Chapter 1", None).exists());
    }

    #[test]
    fn removes_a_series_download_tree() {
        let root = tempfile::tempdir().unwrap();
        let series = root.path().join("Source").join("Series");
        std::fs::create_dir_all(series.join("Chapter 1")).unwrap();
        std::fs::write(series.join("Chapter 1").join("001.jpg"), b"page").unwrap();

        assert!(remove_series_downloads(root.path(), "Source", "Series").unwrap());
        assert!(!series.exists());
        assert!(!root.path().join("Source").exists());
        assert!(!remove_series_downloads(root.path(), "Source", "Series").unwrap());
    }

    #[test]
    fn moves_downloads_between_sources() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("Old Source").join("Series");
        std::fs::create_dir_all(old.join("Chapter 1")).unwrap();
        std::fs::write(old.join("Chapter 1").join("001.jpg"), b"page").unwrap();

        assert!(
            move_series_downloads(root.path(), "Old Source", "Series", "New Source", "Series")
                .unwrap()
        );
        let new = root.path().join("New Source").join("Series");
        assert!(new.join("Chapter 1").join("001.jpg").is_file());
        assert!(!root.path().join("Old Source").exists());
    }

    #[test]
    fn moving_downloads_keeps_existing_destination_chapters() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("Old Source").join("Series");
        let new = root.path().join("New Source").join("Series");
        for (path, page) in [(&old, "Chapter 1"), (&new, "Chapter 2")] {
            std::fs::create_dir_all(path.join(page)).unwrap();
            std::fs::write(path.join(page).join("001.jpg"), b"page").unwrap();
        }

        assert!(
            move_series_downloads(root.path(), "Old Source", "Series", "New Source", "Series")
                .unwrap()
        );
        assert!(new.join("Chapter 1").join("001.jpg").is_file());
        assert!(new.join("Chapter 2").join("001.jpg").is_file());
        assert!(!old.exists());
    }

    #[test]
    fn moving_missing_downloads_is_a_no_op() {
        let root = tempfile::tempdir().unwrap();
        assert!(!move_series_downloads(root.path(), "Missing", "Series", "New", "Series").unwrap());
        assert!(!root.path().join("New").exists());
    }

    #[test]
    fn builds_the_mihon_directory_layout() {
        let root = Path::new("/downloads");
        let dir = chapter_dir(
            root,
            "Asura Scans (EN)",
            "Overgeared",
            "Chapter 348 - The Enemy of My Enemy",
            Some("Asura"),
        );

        assert_eq!(
            dir,
            root.join("Asura Scans (EN)")
                .join("Overgeared")
                .join("Asura_Chapter 348 - The Enemy of My Enemy")
        );
    }

    #[test]
    fn chapter_names_have_a_fallback_and_are_bounded() {
        assert_eq!(chapter_dir_name("", None), "Chapter");

        let long = "x".repeat(400);
        let name = chapter_dir_name(&long, None);
        assert!(name.len() <= MAX_CHAPTER_NAME_BYTES);
    }

    #[test]
    fn page_extensions_come_from_the_bytes() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        let jpeg = [0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0];
        assert_eq!(image_extension(&png), "png");
        assert_eq!(image_extension(&jpeg), "jpg");
    }
}
