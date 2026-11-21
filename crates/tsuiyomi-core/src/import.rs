//! Import a series folder into the Mihon Local source layout.
//!
//! The terminal equivalent of Mihon's local-source import picker: the user enters a
//! source directory, and tsuiyomi copies it to one direct subdirectory of the library
//! root. Existing series are never overwritten; a numeric suffix keeps both copies.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::CoreError;

/// Copy `source` into `library_root` as one series directory.
///
/// Returns the destination path. The source must be a directory. Symbolic links are
/// skipped so a link cycle cannot make the copy unbounded.
pub fn import_series(source: &Path, library_root: &Path) -> Result<PathBuf, CoreError> {
    if !source.is_dir() {
        return Err(CoreError::Import {
            path: source.to_path_buf(),
            message: "source is not a directory".to_owned(),
        });
    }
    if library_root.as_os_str().is_empty() {
        return Err(CoreError::Import {
            path: library_root.to_path_buf(),
            message: "no library root is configured".to_owned(),
        });
    }
    if source == library_root {
        return Err(CoreError::Import {
            path: source.to_path_buf(),
            message: "source is the library root".to_owned(),
        });
    }
    let Some(name) = source.file_name() else {
        return Err(CoreError::Import {
            path: source.to_path_buf(),
            message: "source folder has no name".to_owned(),
        });
    };
    let name = name.to_string_lossy();
    if name.trim().is_empty() {
        return Err(CoreError::Import {
            path: source.to_path_buf(),
            message: "source folder name is empty".to_owned(),
        });
    }

    fs::create_dir_all(library_root).map_err(|error| CoreError::WriteFile {
        path: library_root.to_path_buf(),
        source: error,
    })?;
    let destination = unique_destination(library_root, &name);
    if let Err(error) = copy_directory(source, &destination) {
        // Never leave a half-imported series in the library.
        let _ = fs::remove_dir_all(&destination);
        return Err(error);
    }
    Ok(destination)
}

fn unique_destination(root: &Path, name: &str) -> PathBuf {
    let first = root.join(name);
    if !first.exists() {
        return first;
    }
    for suffix in 2u32.. {
        let candidate = root.join(format!("{name} ({suffix})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 suffixes cannot all exist")
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(destination).map_err(|error| CoreError::WriteFile {
        path: destination.to_path_buf(),
        source: error,
    })?;
    let entries = fs::read_dir(source).map_err(|error| CoreError::ReadDir {
        path: source.to_path_buf(),
        source: error,
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| CoreError::ReadDir {
            path: source.to_path_buf(),
            source: error,
        })?;
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&path, &target)?;
        } else if file_type.is_file() {
            fs::copy(&path, &target).map_err(|error| CoreError::WriteFile {
                path: target,
                source: error,
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_series(path: &Path) {
        let chapter = path.join("Chapter 1");
        fs::create_dir_all(&chapter).unwrap();
        fs::write(chapter.join("001.jpg"), b"page").unwrap();
        fs::write(
            path.join("ComicInfo.xml"),
            "<ComicInfo><Title>Imported</Title></ComicInfo>",
        )
        .unwrap();
    }

    #[test]
    fn copies_a_series_folder_and_keeps_its_name() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("local");
        let source = root.path().join("Imported Series");
        write_series(&source);

        let destination = import_series(&source, &library).unwrap();

        assert_eq!(destination, library.join("Imported Series"));
        assert_eq!(
            fs::read(destination.join("Chapter 1").join("001.jpg")).unwrap(),
            b"page"
        );
        assert!(destination.join("ComicInfo.xml").is_file());
    }

    #[test]
    fn does_not_overwrite_an_existing_series() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("local");
        let source = root.path().join("Imported Series");
        write_series(&source);
        fs::create_dir_all(library.join("Imported Series")).unwrap();
        fs::write(library.join("Imported Series").join("keep.txt"), b"keep").unwrap();

        let destination = import_series(&source, &library).unwrap();

        assert_eq!(destination, library.join("Imported Series (2)"));
        assert!(library.join("Imported Series").join("keep.txt").is_file());
        assert!(destination.join("Chapter 1").join("001.jpg").is_file());
    }

    #[test]
    fn rejects_a_missing_source() {
        let root = tempfile::tempdir().unwrap();
        let error = import_series(&root.path().join("missing"), root.path()).unwrap_err();
        assert!(matches!(error, CoreError::Import { .. }));
    }
}
