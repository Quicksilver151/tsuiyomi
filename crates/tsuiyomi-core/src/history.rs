//! Reading history.
//!
//! History is stored one record per chapter, but the UI shows the most recently
//! read chapter per series, matching Mihon's `historyView` query. The journal is
//! append-only so an unbounded history does not require rewriting the whole file
//! on every progress update.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::host::{RemoteChapter, RemoteManga};

const COMPACT_BYTES: u64 = 16 * 1024 * 1024;

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
    /// Time spent reading this chapter, accumulated across sessions.
    #[serde(default)]
    pub read_duration_secs: u64,
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
            read_duration_secs: 0,
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
            read_duration_secs: 0,
        }
    }

    /// Stable identity for the manga/series that owns this chapter.
    pub fn series_key(&self) -> String {
        if let Some(local) = &self.local {
            format!("local:{}", local.series_path.display())
        } else if let Some(remote) = &self.remote {
            format!("remote:{}\n{}", remote.manga.source_id, remote.manga.url)
        } else {
            format!("title:{}", self.series_title)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HistoryRecord {
    Entry { entry: Box<HistoryEntry> },
    Delete { key: String },
    Clear,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryStore {
    path: Option<PathBuf>,
    entries: BTreeMap<String, HistoryEntry>,
    pending: Vec<HistoryRecord>,
}

impl HistoryStore {
    pub fn load_default() -> Self {
        let Some(path) = default_history_path() else {
            return Self::default();
        };
        if path.exists() {
            return Self::load(&path).unwrap_or_else(|_| Self {
                path: Some(path),
                entries: BTreeMap::new(),
                pending: Vec::new(),
            });
        }

        let legacy = default_legacy_history_path();
        if let Some(legacy) = legacy.filter(|legacy| legacy.exists()) {
            if let Ok(entries) = load_legacy_entries(&legacy) {
                let store = Self {
                    path: Some(path),
                    entries,
                    pending: Vec::new(),
                };
                let _ = store.write_snapshot();
                return store;
            }
        }

        Self {
            path: Some(path),
            entries: BTreeMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let entries = match fs::read(path) {
            Ok(bytes) => parse_records(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            path: Some(path.to_path_buf()),
            entries,
            pending: Vec::new(),
        })
    }

    pub fn get(&self, key: &str) -> Option<&HistoryEntry> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every chapter record, newest first. Prefer [`Self::latest_per_series`] for the UI.
    pub fn recent(&self) -> Vec<(&String, &HistoryEntry)> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.last_read));
        entries
    }

    /// One record per series: its most recently read chapter.
    pub fn latest_per_series(&self) -> Vec<(&String, &HistoryEntry)> {
        self.latest_per_series_matching(None)
    }

    pub fn search_latest(&self, query: &str) -> Vec<(&String, &HistoryEntry)> {
        self.latest_per_series_matching(Some(query))
    }

    fn latest_per_series_matching(&self, query: Option<&str>) -> Vec<(&String, &HistoryEntry)> {
        let query = query
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(str::to_lowercase);
        let mut latest: BTreeMap<String, (&String, &HistoryEntry)> = BTreeMap::new();
        for (key, entry) in &self.entries {
            if query
                .as_ref()
                .is_some_and(|query| !entry.series_title.to_lowercase().contains(query))
            {
                continue;
            }
            let series = entry.series_key();
            match latest.get(&series) {
                Some((_, known)) if known.last_read >= entry.last_read => {}
                _ => {
                    latest.insert(series, (key, entry));
                }
            }
        }
        let mut entries: Vec<_> = latest.into_values().collect();
        entries.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.last_read));
        entries
    }

    pub fn record(&mut self, key: String, mut entry: HistoryEntry) {
        if entry.last_read == 0 {
            entry.last_read = unix_seconds();
        }
        if let Some(previous) = self.entries.get(&key) {
            if entry.read_duration_secs == 0 {
                entry.read_duration_secs = previous.read_duration_secs;
            }
        }
        self.entries.insert(key, entry.clone());
        self.pending.push(HistoryRecord::Entry {
            entry: Box::new(entry),
        });
    }

    /// Accumulate reading time without changing the last-read timestamp.
    pub fn add_duration(&mut self, key: &str, seconds: u64) {
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        entry.read_duration_secs = entry.read_duration_secs.saturating_add(seconds);
        let entry = entry.clone();
        self.pending.push(HistoryRecord::Entry {
            entry: Box::new(entry),
        });
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let removed = self.entries.remove(key).is_some();
        if removed {
            self.pending.push(HistoryRecord::Delete {
                key: key.to_owned(),
            });
        }
        removed
    }

    pub fn remove_series(&mut self, series_key: &str) -> usize {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.series_key() == series_key)
            .map(|(key, _)| key.clone())
            .collect();
        for key in &keys {
            self.entries.remove(key);
            self.pending
                .push(HistoryRecord::Delete { key: key.clone() });
        }
        keys.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.pending.push(HistoryRecord::Clear);
    }

    pub fn save(&mut self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if self.pending.is_empty() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        for record in self.pending.drain(..) {
            serde_json::to_writer(&mut file, &record).map_err(io::Error::other)?;
            file.write_all(b"\n")?;
        }
        file.flush()?;
        if fs::metadata(path).is_ok_and(|metadata| metadata.len() > COMPACT_BYTES) {
            self.compact()?;
        }
        Ok(())
    }

    fn compact(&mut self) -> io::Result<()> {
        self.pending.clear();
        self.write_snapshot()
    }

    fn write_snapshot(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("jsonl.tmp");
        {
            let mut file = fs::File::create(&temporary)?;
            for entry in self.entries.values() {
                let record = HistoryRecord::Entry {
                    entry: Box::new(entry.clone()),
                };
                serde_json::to_writer(&mut file, &record).map_err(io::Error::other)?;
                file.write_all(b"\n")?;
            }
            file.flush()?;
        }
        fs::rename(temporary, path)
    }
}

fn parse_records(bytes: &[u8]) -> BTreeMap<String, HistoryEntry> {
    let mut entries: BTreeMap<String, HistoryEntry> = BTreeMap::new();
    let lines: Vec<&[u8]> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    for (index, line) in lines.iter().enumerate() {
        match serde_json::from_slice::<HistoryRecord>(line) {
            Ok(HistoryRecord::Entry { entry }) => {
                entries.insert(history_key(&entry), *entry);
            }
            Ok(HistoryRecord::Delete { key }) => {
                entries.remove(&key);
            }
            Ok(HistoryRecord::Clear) => entries.clear(),
            Err(_) if index + 1 == lines.len() => {
                // A crash can leave the final append partial.
            }
            Err(_) => {}
        }
    }
    entries
}

fn history_key(entry: &HistoryEntry) -> String {
    if let Some(local) = &entry.local {
        local.chapter_path.to_string_lossy().into_owned()
    } else if let Some(remote) = &entry.remote {
        crate::host::progress_key(
            &remote.manga.source_id,
            &remote.manga.url,
            &remote.chapter.url,
        )
        .to_string_lossy()
        .into_owned()
    } else {
        format!("{}:{}", entry.series_title, entry.chapter_title)
    }
}

fn load_legacy_entries(path: &Path) -> io::Result<BTreeMap<String, HistoryEntry>> {
    #[derive(Deserialize)]
    struct Legacy {
        #[serde(default)]
        entries: BTreeMap<String, HistoryEntry>,
    }
    let bytes = fs::read(path)?;
    let legacy: Legacy = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    Ok(legacy.entries)
}

fn default_history_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("history.jsonl"))
}

fn default_legacy_history_path() -> Option<PathBuf> {
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

    fn entry(title: &str, chapter: &str, time: u64) -> HistoryEntry {
        HistoryEntry {
            series_title: title.to_owned(),
            chapter_title: chapter.to_owned(),
            source: "Local".to_owned(),
            cover: None,
            local: Some(LocalHistory {
                series_path: PathBuf::from(format!("/library/{title}")),
                chapter_path: PathBuf::from(format!("/library/{title}/{chapter}")),
            }),
            remote: None,
            page: 0,
            pages: 10,
            last_read: time,
            read_duration_secs: 0,
        }
    }

    #[test]
    fn records_newest_first_and_removes() {
        let root = tempdir().unwrap();
        let path = root.path().join("history.jsonl");
        let mut store = HistoryStore::load(&path).unwrap();
        let a = entry("A", "Chapter 1", 1);
        let b = entry("B", "Chapter 2", 2);
        let a_key = history_key(&a);
        let b_key = history_key(&b);
        store.record(a_key.clone(), a);
        store.record(b_key, b);
        store.save().unwrap();

        let reloaded = HistoryStore::load(&path).unwrap();
        let titles: Vec<&str> = reloaded
            .recent()
            .into_iter()
            .map(|(_, entry)| entry.series_title.as_str())
            .collect();
        assert_eq!(titles, ["B", "A"]);

        let mut reloaded = reloaded;
        assert!(reloaded.remove(&a_key));
        assert!(reloaded.get(&a_key).is_none());
    }

    #[test]
    fn latest_per_series_hides_older_chapters_and_delete_reveals_them() {
        let mut store = HistoryStore::default();
        let a1 = entry("A", "Chapter 1", 10);
        let a2 = entry("A", "Chapter 2", 20);
        let b1 = entry("B", "Chapter 1", 15);
        let a2_key = history_key(&a2);
        store.record(history_key(&a1), a1);
        store.record(a2_key.clone(), a2);
        store.record(history_key(&b1), b1);

        let latest = store.latest_per_series();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].1.chapter_title, "Chapter 2");
        assert_eq!(latest[1].1.series_title, "B");

        assert!(store.remove(&a2_key));
        let latest = store.latest_per_series();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].1.series_title, "B");
        assert!(latest
            .iter()
            .any(|(_, entry)| entry.series_title == "A" && entry.chapter_title == "Chapter 1"));
    }

    #[test]
    fn history_is_unbounded() {
        let mut store = HistoryStore::default();
        for index in 0..600 {
            store.record(
                format!("/chapter-{index}"),
                entry("Series", &format!("Chapter {index}"), index as u64),
            );
        }
        assert_eq!(store.len(), 600);
        assert_eq!(store.latest_per_series().len(), 1);
    }

    #[test]
    fn journals_append_tombstones_and_clear_records() {
        let root = tempdir().unwrap();
        let path = root.path().join("history.jsonl");
        let mut store = HistoryStore::load(&path).unwrap();
        let a = entry("A", "Chapter 1", 1);
        let b = entry("B", "Chapter 1", 2);
        let a_key = history_key(&a);
        let b_key = history_key(&b);
        store.record(a_key.clone(), a);
        store.record(b_key.clone(), b);
        store.save().unwrap();
        store.remove(&a_key);
        store.save().unwrap();

        let reloaded = HistoryStore::load(&path).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert!(reloaded.get(&b_key).is_some());

        let mut reloaded = reloaded;
        reloaded.clear();
        reloaded.save().unwrap();
        assert!(HistoryStore::load(&path).unwrap().is_empty());
    }

    #[test]
    fn preserves_duration_and_adds_time() {
        let mut store = HistoryStore::default();
        let mut first = entry("A", "Chapter 1", 1);
        first.read_duration_secs = 20;
        let key = history_key(&first);
        store.record(key.clone(), first);
        store.add_duration(&key, 15);
        assert_eq!(store.get(&key).unwrap().read_duration_secs, 35);

        let mut refresh = entry("A", "Chapter 1", 2);
        refresh.read_duration_secs = 0;
        store.record(key.clone(), refresh);
        assert_eq!(store.get(&key).unwrap().read_duration_secs, 35);
    }

    #[test]
    fn searches_latest_titles_only() {
        let mut store = HistoryStore::default();
        let one_piece = entry("One Piece", "Chapter 1", 1);
        let berserk = entry("Berserk", "Chapter 1", 2);
        store.record(history_key(&one_piece), one_piece);
        store.record(history_key(&berserk), berserk);

        let results = store.search_latest("piece");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.series_title, "One Piece");
        assert!(store.search_latest("chapter").is_empty());
    }

    #[test]
    fn migrates_legacy_json() {
        let root = tempdir().unwrap();
        let legacy = root.path().join("history.json");
        let journal = root.path().join("history.jsonl");
        let mut entries = BTreeMap::new();
        entries.insert("/a".to_owned(), entry("A", "Chapter 1", 1));
        fs::write(
            &legacy,
            serde_json::to_vec(&serde_json::json!({ "entries": entries })).unwrap(),
        )
        .unwrap();

        let imported = load_legacy_entries(&legacy).unwrap();
        let store = HistoryStore {
            path: Some(journal.clone()),
            entries: imported,
            pending: Vec::new(),
        };
        store.write_snapshot().unwrap();

        assert_eq!(HistoryStore::load(&journal).unwrap().len(), 1);
    }
}
