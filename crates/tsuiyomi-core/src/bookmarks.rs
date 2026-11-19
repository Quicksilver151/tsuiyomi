//! Per-chapter bookmarks.
//!
//! Keys use the same identifiers as reading progress: a filesystem path for local
//! chapters and the synthetic `remote://source/manga/chapter` key for remote ones.
//! Stored in `$XDG_DATA_HOME/tsuiyomi/bookmarks.json`.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookmarkStore {
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(default)]
    keys: BTreeSet<String>,
}

impl BookmarkStore {
    pub fn load_default() -> Self {
        let Some(path) = default_bookmark_path() else {
            return Self::default();
        };
        Self::load(&path).unwrap_or_else(|_| Self {
            path: Some(path),
            keys: BTreeSet::new(),
        })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let keys = match fs::read(path) {
            Ok(bytes) => {
                let store: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                store.keys
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeSet::new(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            path: Some(path.to_path_buf()),
            keys,
        })
    }

    pub fn is_bookmarked(&self, chapter: &Path) -> bool {
        self.keys.contains(&key(chapter))
    }

    /// Toggle a chapter and return its new bookmark state.
    pub fn toggle(&mut self, chapter: &Path) -> io::Result<bool> {
        let key = key(chapter);
        let bookmarked = if self.keys.remove(&key) {
            false
        } else {
            self.keys.insert(key);
            true
        };
        self.save()?;
        Ok(bookmarked)
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, path)
    }
}

fn default_bookmark_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("bookmarks.json"))
}

fn key(chapter: &Path) -> String {
    chapter.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn toggles_and_reloads_bookmarks() {
        let root = tempdir().unwrap();
        let path = root.path().join("bookmarks.json");
        let chapter = root.path().join("Series/Chapter 1");

        let mut store = BookmarkStore::load(&path).unwrap();
        assert!(store.toggle(&chapter).unwrap());
        assert!(BookmarkStore::load(&path).unwrap().is_bookmarked(&chapter));

        assert!(!store.toggle(&chapter).unwrap());
        assert!(!BookmarkStore::load(&path).unwrap().is_bookmarked(&chapter));
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let root = tempdir().unwrap();
        let store = BookmarkStore::load(&root.path().join("missing.json")).unwrap();
        assert!(!store.is_bookmarked(Path::new("/nope")));
    }
}
