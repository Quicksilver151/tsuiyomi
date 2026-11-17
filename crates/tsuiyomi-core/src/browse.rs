//! Directory listing for the TUI's local import browser.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{chapter::natural_cmp, error::CoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseKind {
    Parent,
    Directory,
    Archive,
}

impl BrowseKind {
    fn rank(self) -> u8 {
        match self {
            Self::Parent => 0,
            Self::Directory => 1,
            Self::Archive => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: BrowseKind,
    pub size: Option<u64>,
}

/// Start browsing in `$HOME`, falling back to the working directory and then `/`.
pub fn browse_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// List the parent, child directories, and supported archives in a directory.
pub fn list_directory(directory: &Path) -> Result<Vec<BrowseEntry>, CoreError> {
    let read_dir = fs::read_dir(directory).map_err(|source| CoreError::ReadDir {
        path: directory.to_path_buf(),
        source,
    })?;

    let mut entries = Vec::new();
    if let Some(parent) = directory.parent() {
        entries.push(BrowseEntry {
            path: parent.to_path_buf(),
            name: "..".to_owned(),
            kind: BrowseKind::Parent,
            size: None,
        });
    }
    for entry in read_dir.filter_map(Result::ok) {
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_dir() {
            entries.push(BrowseEntry {
                path,
                name,
                kind: BrowseKind::Directory,
                size: None,
            });
        } else if file_type.is_file() && is_supported_archive(&path) {
            let size = entry.metadata().ok().map(|metadata| metadata.len());
            entries.push(BrowseEntry {
                path,
                name,
                kind: BrowseKind::Archive,
                size,
            });
        }
    }

    entries.sort_by(|left, right| {
        left.kind
            .rank()
            .cmp(&right.kind.rank())
            .then_with(|| natural_cmp(&left.name, &right.name))
    });
    Ok(entries)
}

pub(crate) fn is_supported_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "zip" | "cbz" | "rar" | "cbr" | "7z" | "cb7" | "tar" | "cbt" | "epub"
            )
        })
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn lists_parent_directories_and_archives_in_order() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("Beta")).unwrap();
        fs::create_dir(root.path().join("alpha")).unwrap();
        fs::create_dir(root.path().join(".hidden")).unwrap();
        fs::write(root.path().join("Chapter 1.cbz"), b"archive").unwrap();
        fs::write(root.path().join("notes.txt"), b"notes").unwrap();

        let entries = list_directory(root.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();

        assert_eq!(names, ["..", "alpha", "Beta", "Chapter 1.cbz"]);
        assert_eq!(entries[0].kind, BrowseKind::Parent);
        assert_eq!(entries[3].kind, BrowseKind::Archive);
        assert_eq!(entries[3].size, Some(7));
    }
}
