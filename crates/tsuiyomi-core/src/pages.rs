use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use zip::ZipArchive;

use crate::{
    chapter::natural_cmp,
    error::CoreError,
    library::{Chapter, ChapterKind},
};

/// One readable page of a chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Display name: file name for folders, archive entry path for archives.
    pub name: String,
    pub source: PageSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageSource {
    File(PathBuf),
    Archive { path: PathBuf, name: String },
}

/// List the pages of a chapter in natural reading order.
pub fn chapter_pages(chapter: &Chapter) -> Result<Vec<Page>, CoreError> {
    match chapter.kind {
        ChapterKind::Directory => pages_in_directory(&chapter.path),
        ChapterKind::Zip => pages_in_zip(&chapter.path),
        ChapterKind::Rar | ChapterKind::Epub => Err(CoreError::UnsupportedArchive {
            path: chapter.path.clone(),
        }),
    }
}

/// Read the bytes for a page.
pub fn read_page(page: &Page) -> Result<Vec<u8>, CoreError> {
    match &page.source {
        PageSource::File(path) => fs::read(path).map_err(|source| CoreError::ReadFile {
            path: path.clone(),
            source,
        }),
        PageSource::Archive { path, name } => read_zip_entry(path, name),
    }
}

fn pages_in_directory(path: &Path) -> Result<Vec<Page>, CoreError> {
    let entries = fs::read_dir(path).map_err(|source| CoreError::ReadDir {
        path: path.to_path_buf(),
        source,
    })?;

    let mut pages = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let entry_path = entry.path();
        if entry_path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if !entry_path.is_file() || !is_image(&entry_path) {
            continue;
        }
        pages.push(Page {
            name: entry_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            source: PageSource::File(entry_path),
        });
    }

    pages.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    Ok(pages)
}

fn pages_in_zip(path: &Path) -> Result<Vec<Page>, CoreError> {
    let file = fs::File::open(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| CoreError::Archive {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| CoreError::Archive {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| CoreError::Archive {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?
            .into_owned();
        if name.starts_with('.') || !is_image_name(&name) {
            continue;
        }
        pages.push(Page {
            name: name.clone(),
            source: PageSource::Archive {
                path: path.to_path_buf(),
                name,
            },
        });
    }

    pages.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    Ok(pages)
}

fn read_zip_entry(path: &Path, name: &str) -> Result<Vec<u8>, CoreError> {
    let file = fs::File::open(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|error| CoreError::Archive {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut entry = archive.by_name(name).map_err(|_| CoreError::MissingPage {
        path: path.to_path_buf(),
        name: name.to_owned(),
    })?;

    let mut data = Vec::new();
    entry
        .read_to_end(&mut data)
        .map_err(|source| CoreError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(data)
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_image_extension)
}

fn is_image_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_image_extension)
}

fn is_image_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "bmp"
    )
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::*;

    fn chapter(path: PathBuf, kind: ChapterKind) -> Chapter {
        Chapter {
            path,
            title: "Chapter 1".to_owned(),
            number: Some(1.0),
            kind,
        }
    }

    #[test]
    fn lists_directory_pages_in_natural_order() {
        let root = tempdir().unwrap();
        for name in ["10.jpg", "2.png", "001.JPG", "notes.txt"] {
            fs::write(root.path().join(name), b"x").unwrap();
        }
        fs::create_dir(root.path().join("nested")).unwrap();

        let pages =
            chapter_pages(&chapter(root.path().to_path_buf(), ChapterKind::Directory)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(names, ["001.JPG", "2.png", "10.jpg"]);
    }

    #[test]
    fn lists_zip_pages_ignoring_directories_and_non_images() {
        let root = tempdir().unwrap();
        let path = root.path().join("Chapter 1.cbz");
        let file = fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        writer.start_file("002.jpg", options).unwrap();
        writer.write_all(b"page two").unwrap();
        writer.start_file("nested/001.png", options).unwrap();
        writer.write_all(b"page one").unwrap();
        writer.start_file("notes.txt", options).unwrap();
        writer.write_all(b"ignore me").unwrap();
        writer.add_directory("folder/", options).unwrap();
        writer.finish().unwrap();

        let pages = chapter_pages(&chapter(path.clone(), ChapterKind::Zip)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(names, ["002.jpg", "nested/001.png"]);
        assert_eq!(read_page(&pages[1]).unwrap(), b"page one");
    }

    #[test]
    fn unsupported_archives_return_an_error() {
        let root = tempdir().unwrap();
        let result = chapter_pages(&chapter(root.path().join("x.epub"), ChapterKind::Epub));
        assert!(matches!(result, Err(CoreError::UnsupportedArchive { .. })));
    }
}
