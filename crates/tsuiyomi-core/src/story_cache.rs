//! On-disk cache for remote story details and chapter lists.
//!
//! Mihon keeps this in its database and refreshes it in the background. tsuiyomi
//! uses one JSON file per source+URL so opening a saved story after a restart does
//! not wait for the extension host and the network.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::host::{RemoteChapter, RemoteManga};

/// Details and chapter list for one remote story.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedStory {
    pub manga: RemoteManga,
    pub chapters: Vec<RemoteChapter>,
    /// Source display name, so cached details still show it while the host is stopped.
    #[serde(default)]
    pub source_name: Option<String>,
    /// Unix seconds when the story was fetched.
    pub updated_at: u64,
}

impl CachedStory {
    pub fn new(manga: RemoteManga, chapters: Vec<RemoteChapter>) -> Self {
        Self {
            manga,
            chapters,
            source_name: None,
            updated_at: unix_seconds(),
        }
    }

    /// Seconds since the story was fetched.
    pub fn age_seconds(&self) -> u64 {
        unix_seconds().saturating_sub(self.updated_at)
    }
}

/// `$XDG_DATA_HOME/tsuiyomi/stories/<hash>.json` for one story.
pub fn story_cache_path(source_id: &str, manga_url: &str) -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| {
        dirs.data_dir()
            .join("tsuiyomi")
            .join("stories")
            .join(format!("{}.json", stable_hash(source_id, manga_url)))
    })
}

/// Load a cached story, treating a missing or corrupt file as a cache miss.
pub fn load_cached_story(source_id: &str, manga_url: &str) -> Option<CachedStory> {
    let path = story_cache_path(source_id, manga_url)?;
    load_story(&path).ok()
}

/// Persist a story after a successful fetch.
pub fn save_cached_story(story: &CachedStory) -> io::Result<()> {
    let path = story_cache_path(&story.manga.source_id, &story.manga.url)
        .ok_or_else(|| io::Error::other("cannot resolve the story cache directory"))?;
    save_story(&path, story)
}

pub fn load_story(path: &Path) -> io::Result<CachedStory> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub fn save_story(path: &Path, story: &CachedStory) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(story).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

fn stable_hash(source_id: &str, manga_url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source_id.bytes().chain(*b"\n").chain(manga_url.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
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

    fn sample() -> CachedStory {
        CachedStory::new(
            RemoteManga {
                source_id: "7".to_owned(),
                url: "/manga/1".to_owned(),
                title: "Berserk".to_owned(),
                author: Some("Miura".to_owned()),
                artist: None,
                description: Some("Dark fantasy".to_owned()),
                genre: vec!["Action".to_owned()],
                status: "ongoing".to_owned(),
                thumbnail_url: None,
                memo: Some("slug".to_owned()),
            },
            vec![RemoteChapter {
                url: "/chapter/1".to_owned(),
                name: "Chapter 1".to_owned(),
                chapter_number: 1.0,
                scanlator: Some("Team".to_owned()),
                upload_date: 123,
                memo: None,
            }],
        )
    }

    #[test]
    fn round_trips_a_cached_story() {
        let root = tempdir().unwrap();
        let path = root.path().join("story.json");
        let story = sample();

        save_story(&path, &story).unwrap();
        let loaded = load_story(&path).unwrap();

        assert_eq!(loaded.manga.title, "Berserk");
        assert_eq!(loaded.manga.description.as_deref(), Some("Dark fantasy"));
        assert_eq!(loaded.chapters.len(), 1);
        assert_eq!(loaded.chapters[0].scanlator.as_deref(), Some("Team"));
    }

    #[test]
    fn cache_keys_are_stable_and_distinct() {
        let first = story_cache_path("7", "/manga/1").unwrap();
        let again = story_cache_path("7", "/manga/1").unwrap();
        let other = story_cache_path("7", "/manga/2").unwrap();

        assert_eq!(first, again);
        assert_ne!(first, other);
        assert!(first.to_string_lossy().contains("tsuiyomi/stories"));
    }
}
