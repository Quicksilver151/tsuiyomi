//! Copy a local folder or archive into the Mihon-compatible library layout.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{browse::is_supported_archive, error::CoreError};

/// A series copied into the library by [`import_series`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSeries {
    pub path: PathBuf,
    pub title: String,
}

/// Import a local directory or archive as a series.
///
/// Directories are copied recursively. A single archive becomes a one-chapter series
/// directory named after the archive stem. The copy is staged in a hidden directory and
/// renamed into place so a failed import never leaves a half-visible series.
pub fn import_series(source: &Path, library_root: &Path) -> Result<ImportedSeries, CoreError> {
    let source = fs::canonicalize(source).map_err(|error| import_error(source, error))?;
    if library_root.as_os_str().is_empty() {
        return Err(CoreError::Import {
            path: source,
            message: "library path is not configured".to_owned(),
        });
    }
    fs::create_dir_all(library_root).map_err(|error| import_error(library_root, error))?;
    let library_root =
        fs::canonicalize(library_root).map_err(|error| import_error(library_root, error))?;
    if source.starts_with(&library_root) {
        return Err(CoreError::Import {
            path: source,
            message: "source is already inside the library".to_owned(),
        });
    }
    if library_root.starts_with(&source) {
        return Err(CoreError::Import {
            path: source,
            message: "the library is inside the source folder".to_owned(),
        });
    }

    let metadata = fs::metadata(&source).map_err(|error| import_error(&source, error))?;
    let (title, directory) = if metadata.is_dir() {
        if !has_chapters(&source) {
            return Err(CoreError::Import {
                path: source,
                message: "no chapter directories or supported archives found".to_owned(),
            });
        }
        (file_name(&source), true)
    } else if metadata.is_file() && is_supported_archive(&source) {
        (file_stem(&source), false)
    } else {
        return Err(CoreError::Import {
            path: source,
            message: "not a directory or supported archive".to_owned(),
        });
    };

    let destination = unique_destination(&library_root, &title);
    let temporary = unique_temporary(&library_root, &title);
    let result = if directory {
        copy_tree(&source, &temporary)
    } else {
        fs::create_dir(&temporary).and_then(|()| {
            let name = source
                .file_name()
                .map(|name| name.to_os_string())
                .unwrap_or_default();
            fs::copy(&source, temporary.join(name)).map(|_| ())
        })
    };
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&temporary);
        return Err(import_error(&source, error));
    }
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(import_error(&destination, error));
    }

    Ok(ImportedSeries {
        title: file_name(&destination),
        path: destination,
    })
}

fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn has_chapters(directory: &Path) -> bool {
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        if is_hidden(&path) {
            return false;
        }
        entry
            .file_type()
            .map(|kind| kind.is_dir() || (kind.is_file() && is_supported_archive(&path)))
            .unwrap_or(false)
    })
}

fn unique_destination(library_root: &Path, title: &str) -> PathBuf {
    let title = if title.is_empty() { "Imported" } else { title };
    let mut candidate = library_root.join(title);
    let mut number = 2;
    while candidate.exists() {
        candidate = library_root.join(format!("{title} ({number})"));
        number += 1;
    }
    candidate
}

fn unique_temporary(library_root: &Path, title: &str) -> PathBuf {
    let title = title.replace(['/', '\\'], "_");
    let mut candidate =
        library_root.join(format!(".tsuiyomi-import-{}-{title}", std::process::id()));
    let mut number = 2;
    while candidate.exists() {
        candidate = library_root.join(format!(
            ".tsuiyomi-import-{}-{title}-{number}",
            std::process::id()
        ));
        number += 1;
    }
    candidate
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn import_error(path: &Path, error: impl std::fmt::Display) -> CoreError {
    CoreError::Import {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn imports_a_chapter_directory() {
        let root = tempdir().unwrap();
        let source = root.path().join("source/My Series");
        fs::create_dir_all(source.join("Chapter 1")).unwrap();
        fs::write(source.join("Chapter 1/001.jpg"), b"page").unwrap();
        fs::write(source.join(".desc"), b"description").unwrap();
        let library = root.path().join("library");

        let imported = import_series(&source, &library).unwrap();

        assert_eq!(imported.title, "My Series");
        assert_eq!(imported.path, library.join("My Series"));
        assert_eq!(
            fs::read(imported.path.join("Chapter 1/001.jpg")).unwrap(),
            b"page"
        );
        assert_eq!(
            fs::read(imported.path.join(".desc")).unwrap(),
            b"description"
        );
    }

    #[test]
    fn imports_an_archive_as_a_one_chapter_series() {
        let root = tempdir().unwrap();
        let source = root.path().join("source/Chapter 1.cbz");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"archive").unwrap();
        let library = root.path().join("library");

        let imported = import_series(&source, &library).unwrap();

        assert_eq!(imported.title, "Chapter 1");
        assert_eq!(imported.path, library.join("Chapter 1"));
        assert_eq!(
            fs::read(imported.path.join("Chapter 1.cbz")).unwrap(),
            b"archive"
        );
    }

    #[test]
    fn uses_a_unique_destination_name() {
        let root = tempdir().unwrap();
        let source = root.path().join("source/Series");
        fs::create_dir_all(source.join("Chapter 1")).unwrap();
        fs::write(source.join("Chapter 1/001.jpg"), b"page").unwrap();
        let library = root.path().join("library");
        fs::create_dir_all(library.join("Series")).unwrap();
        fs::write(library.join("Series/keep.txt"), b"keep").unwrap();

        let imported = import_series(&source, &library).unwrap();

        assert_eq!(imported.path, library.join("Series (2)"));
        assert_eq!(fs::read(library.join("Series/keep.txt")).unwrap(), b"keep");
    }

    #[test]
    fn rejects_sources_inside_the_library() {
        let root = tempdir().unwrap();
        let library = root.path().join("library");
        let source = library.join("Series");
        fs::create_dir_all(source.join("Chapter 1")).unwrap();
        fs::write(source.join("Chapter 1/001.jpg"), b"page").unwrap();

        let error = import_series(&source, &library).unwrap_err();

        assert!(error.to_string().contains("already inside the library"));
    }

    #[test]
    fn rejects_a_library_inside_the_source() {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("Chapter 1")).unwrap();
        fs::write(source.join("Chapter 1/001.jpg"), b"page").unwrap();
        let library = source.join("library");

        let error = import_series(&source, &library).unwrap_err();

        assert!(error.to_string().contains("library is inside the source"));
    }

    #[test]
    fn rejects_empty_directories_and_unsupported_files() {
        let root = tempdir().unwrap();
        let library = root.path().join("library");
        let empty = root.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(import_series(&empty, &library).is_err());

        let file = root.path().join("notes.txt");
        fs::write(&file, b"notes").unwrap();
        assert!(import_series(&file, &library).is_err());
    }
}
