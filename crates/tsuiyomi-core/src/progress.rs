use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProgressEntry {
    /// Zero-based page index.
    pub page: usize,
    pub pages: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressStore {
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(default)]
    entries: BTreeMap<String, ProgressEntry>,
}

impl ProgressStore {
    /// Load the store from `$XDG_DATA_HOME/tsuiyomi/progress.json`.
    pub fn load_default() -> Self {
        let Some(path) = default_progress_path() else {
            return Self::default();
        };
        Self::load(&path).unwrap_or_else(|_| Self {
            path: Some(path),
            entries: BTreeMap::new(),
        })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let entries = match fs::read(path) {
            Ok(bytes) => {
                let store: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                store.entries
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error),
        };

        Ok(Self {
            path: Some(path.to_path_buf()),
            entries,
        })
    }

    pub fn entry(&self, chapter: &Path) -> Option<ProgressEntry> {
        self.entries.get(&key(chapter)).copied()
    }

    pub fn record(&mut self, chapter: &Path, page: usize, pages: usize) -> io::Result<()> {
        self.entries
            .insert(key(chapter), ProgressEntry { page, pages });
        self.save()
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

fn default_progress_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("progress.json"))
}

fn key(chapter: &Path) -> String {
    chapter.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn records_and_reloads_progress() {
        let root = tempdir().unwrap();
        let path = root.path().join("progress.json");
        let chapter = root.path().join("Series/Chapter 1");

        let mut store = ProgressStore::load(&path).unwrap();
        store.record(&chapter, 4, 20).unwrap();

        let reloaded = ProgressStore::load(&path).unwrap();
        let entry = reloaded.entry(&chapter).unwrap();
        assert_eq!(entry.page, 4);
        assert_eq!(entry.pages, 20);
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let root = tempdir().unwrap();
        let store = ProgressStore::load(&root.path().join("missing.json")).unwrap();
        assert!(store.entry(Path::new("/nope")).is_none());
    }

    #[test]
    fn corrupt_file_returns_an_error() {
        let root = tempdir().unwrap();
        let path = root.path().join("progress.json");
        fs::write(&path, b"{not json").unwrap();

        assert!(ProgressStore::load(&path).is_err());
    }
}
