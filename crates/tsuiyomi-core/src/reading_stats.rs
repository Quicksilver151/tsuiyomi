//! Reading-time ledger.
//!
//! Kept separately from the history list so permanent history deletion does not
//! erase how much time was actually spent reading. Daily buckets power the Stats
//! mini-tab and remain small because there is at most one bucket per day.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use chrono::{Duration as ChronoDuration, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeriesReadingTime {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadingStats {
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(default)]
    pub total_seconds: u64,
    /// Local date (`YYYY-MM-DD`) to seconds.
    #[serde(default)]
    pub daily_seconds: BTreeMap<String, u64>,
    /// Stable series key to accumulated duration.
    #[serde(default)]
    pub series_seconds: BTreeMap<String, SeriesReadingTime>,
}

impl ReadingStats {
    pub fn load_default() -> Self {
        let Some(path) = default_stats_path() else {
            return Self::default();
        };
        Self::load(&path).unwrap_or_else(|_| Self {
            path: Some(path),
            ..Self::default()
        })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read(path) {
            Ok(bytes) => {
                let mut stats: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                stats.path = Some(path.to_path_buf());
                Ok(stats)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self {
                path: Some(path.to_path_buf()),
                ..Self::default()
            }),
            Err(error) => Err(error),
        }
    }

    pub fn record_session(
        &mut self,
        series_key: &str,
        series_title: &str,
        seconds: u64,
        date_key: &str,
    ) {
        if seconds == 0 {
            return;
        }
        self.total_seconds = self.total_seconds.saturating_add(seconds);
        let day = self.daily_seconds.entry(date_key.to_owned()).or_default();
        *day = day.saturating_add(seconds);
        let series = self
            .series_seconds
            .entry(series_key.to_owned())
            .or_insert_with(|| SeriesReadingTime {
                title: series_title.to_owned(),
                seconds: 0,
            });
        series.title = series_title.to_owned();
        series.seconds = series.seconds.saturating_add(seconds);
    }

    pub fn day_seconds(&self, date_key: &str) -> u64 {
        self.daily_seconds.get(date_key).copied().unwrap_or(0)
    }

    pub fn range_seconds(&self, days: u32) -> u64 {
        let today = Local::now().date_naive();
        (0..days)
            .map(|offset| {
                let date = today - ChronoDuration::days(i64::from(offset));
                self.day_seconds(&date.format("%Y-%m-%d").to_string())
            })
            .sum()
    }

    /// Streak ending today, or yesterday when today has not been read yet.
    pub fn reading_streak(&self) -> u64 {
        let mut day = Local::now().date_naive();
        if self.day_seconds(&day.format("%Y-%m-%d").to_string()) == 0 {
            day -= ChronoDuration::days(1);
        }
        let mut streak = 0u64;
        loop {
            let key = day.format("%Y-%m-%d").to_string();
            if self.day_seconds(&key) == 0 {
                break;
            }
            streak += 1;
            day -= ChronoDuration::days(1);
        }
        streak
    }

    /// Top series by total time, descending.
    pub fn top_series(&self, limit: usize) -> Vec<(&String, &SeriesReadingTime)> {
        let mut series: Vec<_> = self.series_seconds.iter().collect();
        series.sort_by_key(|(_, reading)| std::cmp::Reverse(reading.seconds));
        series.truncate(limit);
        series
    }

    pub fn active_days(&self) -> usize {
        self.daily_seconds
            .values()
            .filter(|seconds| **seconds > 0)
            .count()
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

pub fn today_key() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn date_key_for_unix(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .map(|utc| utc.with_timezone(&Local).date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string()
}

fn default_stats_path() -> Option<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().join("tsuiyomi").join("reading_stats.json"))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn records_sessions_and_roundtrips() {
        let root = tempdir().unwrap();
        let path = root.path().join("reading_stats.json");
        let mut stats = ReadingStats::load(&path).unwrap();
        stats.record_session("local:/A", "A", 60, "2026-01-01");
        stats.record_session("local:/A", "A", 30, "2026-01-02");
        stats.record_session("remote:1\n/m", "M", 120, "2026-01-02");
        stats.save().unwrap();

        let reloaded = ReadingStats::load(&path).unwrap();
        assert_eq!(reloaded.total_seconds, 210);
        assert_eq!(reloaded.day_seconds("2026-01-02"), 150);
        assert_eq!(reloaded.top_series(1)[0].1.title, "M");
        assert_eq!(reloaded.active_days(), 2);
    }

    #[test]
    fn top_series_sort_by_duration() {
        let mut stats = ReadingStats::default();
        stats.record_session("a", "A", 10, "2026-01-01");
        stats.record_session("b", "B", 30, "2026-01-01");
        stats.record_session("c", "C", 20, "2026-01-01");
        let names: Vec<&str> = stats
            .top_series(2)
            .into_iter()
            .map(|(_, series)| series.title.as_str())
            .collect();
        assert_eq!(names, ["B", "C"]);
    }

    #[test]
    fn date_keys_are_local_dates() {
        assert_eq!(today_key().len(), 10);
        assert_eq!(date_key_for_unix(0).len(), 10);
    }
}
