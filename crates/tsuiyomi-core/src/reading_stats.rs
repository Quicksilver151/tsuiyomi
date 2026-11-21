//! Reading-time aggregation derived from live history.
//!
//! Stats are intentionally not a separate ledger: removing history must remove
//! its contribution from the Stats view. Per-chapter daily buckets live in each
//! history entry, so aggregate totals, charts, and top-series lists can be rebuilt
//! after a deletion.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration as ChronoDuration, Local};

#[derive(Debug, Clone, Default)]
pub struct SeriesReadingTime {
    pub title: String,
    pub seconds: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ReadingStats {
    pub total_seconds: u64,
    /// Local date (`YYYY-MM-DD`) to seconds.
    pub daily_seconds: BTreeMap<String, u64>,
    /// Stable series key to accumulated duration.
    pub series_seconds: BTreeMap<String, SeriesReadingTime>,
}

impl ReadingStats {
    /// Add one chapter's duration and its daily buckets. `fallback_date` is used
    /// for entries created before per-day buckets existed.
    pub fn record_entry(
        &mut self,
        series_key: &str,
        series_title: &str,
        seconds: u64,
        daily: &BTreeMap<String, u64>,
        fallback_date: &str,
    ) {
        if seconds == 0 {
            return;
        }
        self.total_seconds = self.total_seconds.saturating_add(seconds);
        let series = self
            .series_seconds
            .entry(series_key.to_owned())
            .or_insert_with(|| SeriesReadingTime {
                title: series_title.to_owned(),
                seconds: 0,
            });
        series.title = series_title.to_owned();
        series.seconds = series.seconds.saturating_add(seconds);

        if daily.is_empty() {
            let day = self
                .daily_seconds
                .entry(fallback_date.to_owned())
                .or_default();
            *day = day.saturating_add(seconds);
        } else {
            for (date, value) in daily {
                let day = self.daily_seconds.entry(date.clone()).or_default();
                *day = day.saturating_add(*value);
            }
        }
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
}

pub fn today_key() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn date_key_for_unix(seconds: u64) -> String {
    DateTime::from_timestamp(seconds as i64, 0)
        .map(|utc| utc.with_timezone(&Local).date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_entries_and_uses_fallback_days() {
        let mut stats = ReadingStats::default();
        let mut daily = BTreeMap::new();
        daily.insert("2026-01-02".to_owned(), 90);
        stats.record_entry("a", "A", 120, &daily, "2026-01-01");
        stats.record_entry("b", "B", 60, &BTreeMap::new(), "2026-01-03");

        assert_eq!(stats.total_seconds, 180);
        assert_eq!(stats.day_seconds("2026-01-02"), 90);
        assert_eq!(stats.day_seconds("2026-01-03"), 60);
        assert_eq!(stats.active_days(), 2);
        assert_eq!(stats.top_series(1)[0].1.title, "A");
    }

    #[test]
    fn top_series_sort_by_duration() {
        let mut stats = ReadingStats::default();
        stats.record_entry("a", "A", 10, &BTreeMap::new(), "2026-01-01");
        stats.record_entry("b", "B", 30, &BTreeMap::new(), "2026-01-01");
        stats.record_entry("c", "C", 20, &BTreeMap::new(), "2026-01-01");
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
