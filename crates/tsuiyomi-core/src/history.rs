//! Reading history.
//!
//! One entry per chapter, newest first in the UI. Local chapters are keyed by
//! filesystem path; remote chapters use the same synthetic progress key as reading
//! progress. Entries keep enough metadata to reopen the chapter and show the source,
//! independent of whether it is still in the library.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::host::{RemoteChapter, RemoteManga};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalHistory {
    pub series_path: PathBuf,
    pub chapter_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHistory {
    pub manga: RemoteManga,
    pub chapter: RemoteChapter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub series_title: String,
    pub chapter_title: String,
    /// Display name: `Local` or the source's display name.
    pub source: String,
    /// Local series cover, when known.
    #[serde(default)]
    pub cover: Option<PathBuf>,
    #[serde(default)]
    pub local: Option<LocalHistory>,
    #[serde(default)]
    pub remote: Option<RemoteHistory>,
    pub page: usize,
    pub pages: usize,
    /// Unix seconds of the most recent read.
    pub last_read: u64,
}

impl HistoryEntry {
    pub fn local(
        series_title: String,
        chapter_title: String,
        series_path: PathBuf,
        chapter_path: PathBuf,
        cover: Option<PathBuf>,
        page: usize,
        pages: usize,
    ) -> Self {
        Self {
            series_title,
            chapter_title,
            source: "Local".to_owned(),
            cover,
            local: Some(LocalHistory {
                series_path,
                chapter_path,
            }),
            remote: None,
            page,
            pages,
            last_read: unix_seconds(),
        }
    }

    pub fn remote(
        series_title: String,
        chapter_title: String,
        source: String,
        manga: RemoteManga,
        chapter: RemoteChapter,
        page: usize,
        pages: usize,
    ) -> Self {
        Self {
            series_title,
            chapter_title,
            source,
            cover: None,
            local: None,
            remote: Some(RemoteHistory { manga, chapter }),
            page,
            pages,
            last_read: unix_seconds(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryStore {
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(default)]
    entries: BTreeMap<String, HistoryEntry>,
}

impl HistoryStore {
    pub fn load_default() -> Self {
        let Some(path) = default_history_path() else {
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

    pub fn get(&self, key: &str) -> Option<&HistoryEntry> {
        self.entries.get(key)
    }

    /// Entries newest first.
    pub fn recent(&self) -> Vec<(&String, &HistoryEntry)> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.last_read));
        entries
    }

    pub fn record(&mut self, key: String, mut entry: HistoryEntry) {
        if entry.last_read == 0 {
            entry.last_read = unix_seconds();
        }
        self.entries.insert(key, entry);
        if self.entries.len() > 500 {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_read)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, path)
    }
}

fn default_history_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("history.json"))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn records_newest_first_and_removes() {
        let root = tempdir().unwrap();
        let path = root.path().join("history.json");
        let mut store = HistoryStore::load(&path).unwrap();
        let entry = |title: &str, time: u64| HistoryEntry {
            series_title: title.to_owned(),
            chapter_title: "Chapter 1".to_owned(),
            source: "Local".to_owned(),
            cover: None,
            local: None,
            remote: None,
            page: 0,
            pages: 10,
            last_read: time,
        };

        store.record("a".to_owned(), entry("A", 1));
        store.record("b".to_owned(), entry("B", 2));
        store.save().unwrap();

        let reloaded = HistoryStore::load(&path).unwrap();
        let titles: Vec<&str> = reloaded
            .recent()
            .into_iter()
            .map(|(_, entry)| entry.series_title.as_str())
            .collect();
        assert_eq!(titles, ["B", "A"]);

        let mut reloaded = reloaded;
        assert!(reloaded.remove("a"));
        assert!(reloaded.get("a").is_none());
    }
}
