//! New local chapters detected between scans, used by the Updates tab.
//!
//! The first observation only seeds the chapter snapshot. Later observations add one
//! entry per newly seen chapter, so a fresh install does not show the whole library as
//! updated. Entries are kept for 30 days and dropped when their chapter disappears.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    host::{RemoteChapter, RemoteFavorite},
    library::Library,
};

/// How long a detected chapter stays in the Updates list.
const RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteUpdate {
    pub source_id: String,
    pub manga_url: String,
    pub chapter_url: String,
    #[serde(default)]
    pub source_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateEntry {
    /// Series directory path, used as the stable identity.
    pub series_key: String,
    pub series_title: String,
    /// Chapter directory or archive path, used as the stable identity.
    pub chapter_key: String,
    pub chapter_title: String,
    pub chapter_number: Option<f32>,
    pub found_at: u64,
    /// Set for a chapter discovered in a saved source manga.
    #[serde(default)]
    pub remote: Option<RemoteUpdate>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateStore {
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(default)]
    seeded: bool,
    #[serde(default)]
    seen: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    entries: Vec<UpdateEntry>,
}

impl UpdateStore {
    /// Load the store from `$XDG_DATA_HOME/tsuiyomi/updates.json`.
    pub fn load_default() -> Self {
        let Some(path) = default_updates_path() else {
            return Self::default();
        };
        Self::load(&path).unwrap_or_else(|_| Self {
            path: Some(path),
            ..Self::default()
        })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    path: Some(path.to_path_buf()),
                    ..Self::default()
                });
            }
            Err(error) => return Err(error),
        };
        let mut store: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        store.path = Some(path.to_path_buf());
        Ok(store)
    }

    /// Compare the library against the previous snapshot and record new chapters.
    ///
    /// Returns how many entries were added. The first call seeds the snapshot without
    /// adding entries. Saves the result so a crash after a scan does not replay updates.
    pub fn observe(&mut self, library: &Library, now: u64) -> io::Result<usize> {
        let mut seen = BTreeMap::new();
        let mut current = BTreeSet::new();
        let mut found = Vec::new();

        for series in &library.series {
            let series_key = series.path.to_string_lossy().into_owned();
            let previous = self.seen.get(&series_key);
            let mut chapters = BTreeSet::new();
            for chapter in &series.chapters {
                let chapter_key = chapter.path.to_string_lossy().into_owned();
                if self.seeded && !previous.is_some_and(|known| known.contains(&chapter_key)) {
                    found.push(UpdateEntry {
                        series_key: series_key.clone(),
                        series_title: series.title.clone(),
                        chapter_key: chapter_key.clone(),
                        chapter_title: chapter.title.clone(),
                        chapter_number: chapter.number,
                        found_at: now,
                        remote: None,
                    });
                }
                chapters.insert(chapter_key.clone());
                current.insert(chapter_key);
            }
            seen.insert(series_key, chapters);
        }

        let mut added = 0usize;
        for entry in found {
            if self.entries.iter().any(|known| {
                known.series_key == entry.series_key && known.chapter_key == entry.chapter_key
            }) {
                continue;
            }
            self.entries.push(entry);
            added += 1;
        }

        self.seen = seen;
        self.entries.retain(|entry| {
            let fresh = now.saturating_sub(entry.found_at) <= RETENTION_SECS;
            if entry.remote.is_some() {
                fresh
            } else {
                fresh && current.contains(&entry.chapter_key)
            }
        });
        self.entries.sort_by(|left, right| {
            right
                .found_at
                .cmp(&left.found_at)
                .then_with(|| left.series_title.cmp(&right.series_title))
                .then_with(|| left.chapter_title.cmp(&right.chapter_title))
        });
        self.seeded = true;
        self.save()?;
        Ok(added)
    }

    /// Observe with the current wall clock.
    pub fn observe_now(&mut self, library: &Library) -> io::Result<usize> {
        self.observe(library, unix_seconds())
    }

    /// Record a chapter discovered for a saved source manga.
    ///
    /// Returns whether a new entry was added. Duplicate series/chapter pairs are ignored.
    pub fn record_remote(
        &mut self,
        source_id: &str,
        source_name: Option<&str>,
        manga_url: &str,
        manga_title: &str,
        chapter: &RemoteChapter,
        now: u64,
    ) -> bool {
        let series_key = format!("remote:{source_id}\n{manga_url}");
        let chapter_key = format!("{series_key}\n{}", chapter.url);
        if self
            .entries
            .iter()
            .any(|entry| entry.series_key == series_key && entry.chapter_key == chapter_key)
        {
            return false;
        }
        self.entries.push(UpdateEntry {
            series_key,
            series_title: manga_title.to_owned(),
            chapter_key,
            chapter_title: chapter.name.clone(),
            chapter_number: (chapter.chapter_number >= 0.0).then_some(chapter.chapter_number),
            found_at: now,
            remote: Some(RemoteUpdate {
                source_id: source_id.to_owned(),
                manga_url: manga_url.to_owned(),
                chapter_url: chapter.url.clone(),
                source_name: source_name.map(str::to_owned),
            }),
        });
        self.entries.sort_by(|left, right| {
            right
                .found_at
                .cmp(&left.found_at)
                .then_with(|| left.series_title.cmp(&right.series_title))
                .then_with(|| left.chapter_title.cmp(&right.chapter_title))
        });
        true
    }

    /// Drop remote entries whose saved favorite no longer exists. Local entries are kept.
    pub fn retain_favorites(&mut self, favorites: &[RemoteFavorite]) -> usize {
        let before = self.entries.len();
        self.entries.retain(|entry| {
            let Some(remote) = &entry.remote else {
                return true;
            };
            favorites.iter().any(|favorite| {
                favorite.source_id == remote.source_id && favorite.url == remote.manga_url
            })
        });
        before.saturating_sub(self.entries.len())
    }

    /// Newest first.
    pub fn entries(&self) -> &[UpdateEntry] {
        &self.entries
    }

    pub fn dismiss(&mut self, series_key: &str, chapter_key: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.series_key != series_key || entry.chapter_key != chapter_key);
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
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
        crate::platform::replace_file(&temporary, path)
    }
}

fn default_updates_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("updates.json"))
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
    use crate::{Chapter, ChapterKind, Series};

    fn series(path: &str, chapters: &[&str]) -> Series {
        Series {
            path: path.into(),
            remote: None,
            title: path.to_owned(),
            author: None,
            artist: None,
            description: String::new(),
            tags: Vec::new(),
            status: None,
            cover: None,
            categories: vec!["Default".to_owned()],
            chapters: chapters
                .iter()
                .map(|name| Chapter {
                    path: format!("{path}/{name}").into(),
                    title: (*name).to_owned(),
                    number: Some(1.0),
                    kind: ChapterKind::Directory,
                    scanlator: None,
                    date: None,
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    fn library(series: Vec<Series>) -> Library {
        Library {
            root: "/library".into(),
            series,
            categories: vec!["Default".to_owned()],
            errors: Vec::new(),
        }
    }

    #[test]
    fn first_observation_seeds_without_entries() {
        let root = tempdir().unwrap();
        let mut store = UpdateStore::load(&root.path().join("updates.json")).unwrap();
        let added = store
            .observe(&library(vec![series("Berserk", &["1", "2"])]), 100)
            .unwrap();
        assert_eq!(added, 0);
        assert!(store.entries().is_empty());
    }

    #[test]
    fn detects_only_new_chapters_and_survives_reload() {
        let root = tempdir().unwrap();
        let path = root.path().join("updates.json");
        let mut store = UpdateStore::load(&path).unwrap();
        store
            .observe(&library(vec![series("Berserk", &["1"])]), 100)
            .unwrap();

        let added = store
            .observe(&library(vec![series("Berserk", &["1", "2"])]), 200)
            .unwrap();
        assert_eq!(added, 1);
        assert_eq!(store.entries().len(), 1);
        assert_eq!(store.entries()[0].chapter_title, "2");

        let added = store
            .observe(&library(vec![series("Berserk", &["1", "2"])]), 300)
            .unwrap();
        assert_eq!(added, 0);

        let mut reloaded = UpdateStore::load(&path).unwrap();
        assert_eq!(reloaded.entries(), store.entries());
        assert_eq!(
            reloaded
                .observe(&library(vec![series("Berserk", &["1", "2", "3"])]), 400)
                .unwrap(),
            1
        );
    }

    #[test]
    fn remote_entries_survive_local_scans_and_are_deduplicated() {
        let mut store = UpdateStore::default();
        store
            .observe(&library(vec![series("Berserk", &["1"])]), 100)
            .unwrap();
        let chapter = RemoteChapter {
            url: "/chapter/2".to_owned(),
            name: "Chapter 2".to_owned(),
            chapter_number: 2.0,
            scanlator: None,
            upload_date: 0,
            memo: None,
        };
        assert!(store.record_remote(
            "7",
            Some("Source (EN)"),
            "/manga/berserk",
            "Berserk",
            &chapter,
            200,
        ));
        assert!(!store.record_remote(
            "7",
            Some("Source (EN)"),
            "/manga/berserk",
            "Berserk",
            &chapter,
            300,
        ));
        store
            .observe(&library(vec![series("Berserk", &["1"])]), 400)
            .unwrap();
        assert_eq!(store.entries().len(), 1);
        assert!(store.entries()[0].remote.is_some());
        let favorite = RemoteFavorite {
            source_id: "7".to_owned(),
            url: "/manga/berserk".to_owned(),
            title: "Berserk".to_owned(),
            thumbnail_url: None,
            last_read_url: None,
            last_read_name: None,
            categories: Vec::new(),
        };
        assert_eq!(store.retain_favorites(&[]), 1);
        assert!(store.entries().is_empty());
        store.record_remote(
            "7",
            Some("Source (EN)"),
            "/manga/berserk",
            "Berserk",
            &chapter,
            500,
        );
        assert_eq!(store.retain_favorites(&[favorite]), 0);
        assert_eq!(store.entries().len(), 1);
    }

    #[test]
    fn drops_entries_for_removed_chapters() {
        let mut store = UpdateStore::default();
        store
            .observe(&library(vec![series("Berserk", &["1"])]), 100)
            .unwrap();
        store
            .observe(&library(vec![series("Berserk", &["1", "2"])]), 200)
            .unwrap();
        assert_eq!(store.entries().len(), 1);
        store
            .observe(&library(vec![series("Berserk", &["1"])]), 300)
            .unwrap();
        assert!(store.entries().is_empty());
    }
}
