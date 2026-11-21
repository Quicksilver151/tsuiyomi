//! Chapter-list filtering, sorting, display, and read state.
//!
//! These rules mirror Mihon's chapter settings (`ChapterSettingsDialog` and
//! `MangaViewModel`): tri-state downloaded/unread/bookmarked filters, an excluded
//! scanlator set, four sort modes with a direction, and a title/number display mode.
//! Kept in core so both the TUI and future frontends share one implementation.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Filter state for a boolean chapter property.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriState {
    /// Do not filter on this property.
    #[default]
    Ignore,
    /// Show only chapters where the property is set.
    Include,
    /// Hide chapters where the property is set.
    Exclude,
}

impl TriState {
    pub fn cycle(self) -> Self {
        match self {
            Self::Ignore => Self::Include,
            Self::Include => Self::Exclude,
            Self::Exclude => Self::Ignore,
        }
    }

    pub fn label(self, property: &str) -> String {
        match self {
            Self::Ignore => format!("Any {property}"),
            Self::Include => format!("Only {property}"),
            Self::Exclude => format!("Not {property}"),
        }
    }

    fn accepts(self, value: bool) -> bool {
        match self {
            Self::Ignore => true,
            Self::Include => value,
            Self::Exclude => !value,
        }
    }
}

/// Chapter ordering, mirroring Mihon's `SetSorting` values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChapterSort {
    /// Keep the order supplied by the source or scanner (newest first).
    #[default]
    Source,
    /// Numeric chapter number.
    Number,
    /// Source upload date.
    UploadDate,
    /// Title, case-insensitively.
    Alphabet,
}

impl ChapterSort {
    pub fn next(self) -> Self {
        match self {
            Self::Source => Self::Number,
            Self::Number => Self::UploadDate,
            Self::UploadDate => Self::Alphabet,
            Self::Alphabet => Self::Source,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Number => "number",
            Self::UploadDate => "date",
            Self::Alphabet => "title",
        }
    }
}

/// Which chapter label to show, mirroring Mihon's `SetDisplayMode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChapterDisplay {
    /// Chapter title, with the parsed number as a secondary label.
    #[default]
    Title,
    /// `Chapter N` when a number exists, falling back to the title.
    Number,
}

impl ChapterDisplay {
    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::Number,
            Self::Number => Self::Title,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Number => "number",
        }
    }
}

/// Per-story chapter filters, sorting, and display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChapterSettings {
    pub downloaded: TriState,
    pub unread: TriState,
    pub bookmarked: TriState,
    pub excluded_scanlators: BTreeSet<String>,
    pub sort: ChapterSort,
    pub descending: bool,
    pub display: ChapterDisplay,
}

impl Default for ChapterSettings {
    fn default() -> Self {
        Self {
            downloaded: TriState::Ignore,
            unread: TriState::Ignore,
            bookmarked: TriState::Ignore,
            excluded_scanlators: BTreeSet::new(),
            sort: ChapterSort::Source,
            descending: false,
            display: ChapterDisplay::Title,
        }
    }
}

impl ChapterSettings {
    /// Whether any filter is narrowing the chapter list.
    pub fn filtering(&self) -> bool {
        self.downloaded != TriState::Ignore
            || self.unread != TriState::Ignore
            || self.bookmarked != TriState::Ignore
            || !self.excluded_scanlators.is_empty()
    }

    /// Compact state used in the chapter header, e.g.
    /// `unread · number ↓ · title`.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.unread != TriState::Ignore {
            parts.push(match self.unread {
                TriState::Include => "unread",
                TriState::Exclude => "read",
                TriState::Ignore => unreachable!(),
            });
        }
        if self.downloaded != TriState::Ignore {
            parts.push(match self.downloaded {
                TriState::Include => "downloaded",
                TriState::Exclude => "not downloaded",
                TriState::Ignore => unreachable!(),
            });
        }
        if self.bookmarked != TriState::Ignore {
            parts.push(match self.bookmarked {
                TriState::Include => "bookmarked",
                TriState::Exclude => "not bookmarked",
                TriState::Ignore => unreachable!(),
            });
        }
        if !self.excluded_scanlators.is_empty() {
            parts.push("scanlator filter");
        }
        parts.push(match self.sort {
            ChapterSort::Source => "source",
            ChapterSort::Number => "number",
            ChapterSort::UploadDate => "date",
            ChapterSort::Alphabet => "title",
        });
        parts.push(if self.descending { "↓" } else { "↑" });
        parts.push(self.display.label());
        parts.join(" · ")
    }
}

/// Derived read state for one chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadState {
    Unread,
    InProgress,
    Read,
}

impl ReadState {
    pub fn is_read(self) -> bool {
        matches!(self, Self::Read)
    }
}

/// Derive Mihon's read state from the stored page progress.
pub fn read_state(progress: Option<(usize, usize)>) -> ReadState {
    match progress {
        None => ReadState::Unread,
        Some((page, pages)) if pages == 0 || page + 1 >= pages => ReadState::Read,
        Some(_) => ReadState::InProgress,
    }
}

/// One chapter's filterable properties. `index` is the chapter's position in the
/// source list; returned visible indices refer back to it.
#[derive(Debug, Clone)]
pub struct ChapterMeta<'a> {
    pub title: &'a str,
    pub number: Option<f32>,
    pub upload_date: Option<i64>,
    pub scanlator: Option<&'a str>,
    pub downloaded: bool,
    pub bookmarked: bool,
    pub read: ReadState,
}

/// Filter and sort chapters, returning positions into `chapters`.
pub fn visible_chapters(chapters: &[ChapterMeta<'_>], settings: &ChapterSettings) -> Vec<usize> {
    let mut visible: Vec<usize> = chapters
        .iter()
        .enumerate()
        .filter(|(_, chapter)| {
            settings.downloaded.accepts(chapter.downloaded)
                && settings
                    .unread
                    .accepts(!matches!(chapter.read, ReadState::Read))
                && settings.bookmarked.accepts(chapter.bookmarked)
                && chapter
                    .scanlator
                    .is_none_or(|scanlator| !settings.excluded_scanlators.contains(scanlator))
        })
        .map(|(index, _)| index)
        .collect();

    match settings.sort {
        ChapterSort::Source => {}
        ChapterSort::Number => visible.sort_by(|left, right| {
            let left = chapters[*left].number;
            let right = chapters[*right].number;
            match (left, right) {
                (Some(left), Some(right)) => left.total_cmp(&right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        }),
        ChapterSort::UploadDate => visible.sort_by_key(|index| chapters[*index].upload_date),
        ChapterSort::Alphabet => visible.sort_by(|left, right| {
            chapters[*left]
                .title
                .to_lowercase()
                .cmp(&chapters[*right].title.to_lowercase())
        }),
    }
    if settings.descending {
        visible.reverse();
    }
    visible
}

/// Count numeric chapters missing between the lowest and highest integer number.
///
/// Fractional chapters (`12.5`) and unknown numbers are ignored, matching Mihon's
/// missing-chapter calculation.
pub fn missing_chapter_count(numbers: impl IntoIterator<Item = Option<f32>>) -> usize {
    let mut integers: Vec<i32> = numbers
        .into_iter()
        .flatten()
        .filter(|number| number.is_finite() && number.fract() == 0.0 && *number >= 0.0)
        .map(|number| number.round() as i32)
        .collect();
    integers.sort_unstable();
    integers.dedup();
    integers
        .windows(2)
        .map(|pair| (pair[1] - pair[0] - 1).max(0) as usize)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter<'a>(
        title: &'a str,
        number: Option<f32>,
        read: ReadState,
        downloaded: bool,
        bookmarked: bool,
    ) -> ChapterMeta<'a> {
        ChapterMeta {
            title,
            number,
            upload_date: number.map(|number| number as i64),
            scanlator: None,
            downloaded,
            bookmarked,
            read,
        }
    }

    #[test]
    fn tri_state_filters_combine() {
        let chapters = [
            chapter("One", Some(1.0), ReadState::Unread, false, true),
            chapter("Two", Some(2.0), ReadState::Read, true, false),
            chapter("Three", Some(3.0), ReadState::InProgress, false, false),
        ];
        let settings = ChapterSettings {
            unread: TriState::Include,
            downloaded: TriState::Exclude,
            ..ChapterSettings::default()
        };
        assert_eq!(visible_chapters(&chapters, &settings), vec![0, 2]);

        let settings = ChapterSettings {
            unread: TriState::Exclude,
            bookmarked: TriState::Include,
            ..ChapterSettings::default()
        };
        assert!(visible_chapters(&chapters, &settings).is_empty());
    }

    #[test]
    fn sorts_by_number_date_and_title() {
        let chapters = [
            chapter("Zeta", Some(10.5), ReadState::Unread, false, false),
            chapter("Alpha", Some(2.0), ReadState::Unread, false, false),
            chapter("Beta", None, ReadState::Unread, false, false),
        ];
        let by_number = ChapterSettings {
            sort: ChapterSort::Number,
            ..ChapterSettings::default()
        };
        assert_eq!(visible_chapters(&chapters, &by_number), vec![1, 0, 2]);

        let by_title = ChapterSettings {
            sort: ChapterSort::Alphabet,
            ..ChapterSettings::default()
        };
        assert_eq!(visible_chapters(&chapters, &by_title), vec![1, 2, 0]);

        let descending = ChapterSettings {
            sort: ChapterSort::Number,
            descending: true,
            ..ChapterSettings::default()
        };
        assert_eq!(visible_chapters(&chapters, &descending), vec![2, 0, 1]);
    }

    #[test]
    fn excluded_scanlators_are_hidden() {
        let mut chapter = chapter("One", Some(1.0), ReadState::Unread, false, false);
        chapter.scanlator = Some("Team A");
        let settings = ChapterSettings {
            excluded_scanlators: ["Team A".to_owned()].into_iter().collect(),
            ..ChapterSettings::default()
        };
        assert!(visible_chapters(&[chapter], &settings).is_empty());
    }

    #[test]
    fn read_state_uses_last_page() {
        assert_eq!(read_state(None), ReadState::Unread);
        assert_eq!(read_state(Some((0, 10))), ReadState::InProgress);
        assert_eq!(read_state(Some((9, 10))), ReadState::Read);
        assert_eq!(read_state(Some((0, 1))), ReadState::Read);
        assert_eq!(read_state(Some((0, 0))), ReadState::Read);
    }

    #[test]
    fn missing_numbers_ignore_fractions_and_duplicates() {
        assert_eq!(
            missing_chapter_count([Some(1.0), Some(2.0), Some(5.0), Some(5.5), None]),
            2
        );
        assert_eq!(missing_chapter_count([Some(1.0), Some(1.0)]), 0);
    }

    #[test]
    fn summary_lists_active_filters_and_sort() {
        let settings = ChapterSettings {
            unread: TriState::Include,
            sort: ChapterSort::Number,
            descending: true,
            ..ChapterSettings::default()
        };
        assert_eq!(settings.summary(), "unread · number · ↓ · title");
    }
}
