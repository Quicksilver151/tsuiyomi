//! Parser for the ComicInfo.xml metadata format.
//!
//! Mihon reads the same fields for series and chapter metadata. The schema is
//! community-maintained; unknown elements are ignored so newer files still load.
//! See `tachiyomi/core/metadata/comicinfo/ComicInfo.kt` and the local source's
//! `copyFromComicInfo` / `setChapterDetailsFromComicInfoFile`.

use std::{fs, path::Path};

use quick_xml::{events::Event, Reader};

use crate::error::CoreError;

/// A chapter date from ComicInfo's `Year`/`Month`/`Day` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl ChapterDate {
    fn from_fields(year: Option<&str>, month: Option<&str>, day: Option<&str>) -> Option<Self> {
        let year = year?.trim().parse::<i32>().ok()?;
        if year <= 0 {
            return None;
        }
        let month = month
            .and_then(|value| value.trim().parse::<u8>().ok())
            .map(|value| value.clamp(1, 12))
            .unwrap_or(1);
        let day = day
            .and_then(|value| value.trim().parse::<u8>().ok())
            .map(|value| value.clamp(1, 31))
            .unwrap_or(1);
        Some(Self { year, month, day })
    }
}

/// Subset of ComicInfo.xml that Mihon maps into its series and chapter models.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComicInfo {
    pub title: Option<String>,
    pub series: Option<String>,
    pub number: Option<String>,
    pub summary: Option<String>,
    pub writer: Option<String>,
    pub penciller: Option<String>,
    pub inker: Option<String>,
    pub colorist: Option<String>,
    pub letterer: Option<String>,
    pub cover_artist: Option<String>,
    pub translator: Option<String>,
    pub genre: Option<String>,
    pub tags: Option<String>,
    pub categories: Option<String>,
    pub publishing_status: Option<String>,
    pub year: Option<String>,
    pub month: Option<String>,
    pub day: Option<String>,
}

impl ComicInfo {
    /// Series tags: `Genre`, `Tags`, and Mihon's `Categories` extension, deduplicated.
    pub fn series_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        for value in [&self.genre, &self.tags, &self.categories]
            .into_iter()
            .flatten()
        {
            for tag in split_list(value) {
                if !tags.iter().any(|existing| existing == &tag) {
                    tags.push(tag);
                }
            }
        }
        tags
    }

    /// Series artist credit, matching Mihon's combined penciller/inker/etc. field.
    pub fn series_artist(&self) -> Option<String> {
        let mut artists = Vec::new();
        for value in [
            &self.penciller,
            &self.inker,
            &self.colorist,
            &self.letterer,
            &self.cover_artist,
        ]
        .into_iter()
        .flatten()
        {
            for artist in split_list(value) {
                if !artists.iter().any(|existing| existing == &artist) {
                    artists.push(artist);
                }
            }
        }
        (!artists.is_empty()).then(|| artists.join(", "))
    }

    pub fn chapter_number(&self) -> Option<f32> {
        self.number
            .as_deref()
            .and_then(|value| value.trim().parse::<f32>().ok())
    }

    pub fn chapter_date(&self) -> Option<ChapterDate> {
        ChapterDate::from_fields(
            self.year.as_deref(),
            self.month.as_deref(),
            self.day.as_deref(),
        )
    }
}

impl ComicInfo {
    /// Serialize the fields tsuiyomi reads back into a Mihon-readable `ComicInfo.xml`.
    ///
    /// Empty fields are omitted instead of written as empty elements. Unknown
    /// extensions are intentionally not preserved: callers should avoid overwriting
    /// an existing file.
    pub fn to_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo>\n");
        for (element, value) in [
            ("Title", &self.title),
            ("Series", &self.series),
            ("Number", &self.number),
            ("Summary", &self.summary),
            ("Writer", &self.writer),
            ("Penciller", &self.penciller),
            ("Inker", &self.inker),
            ("Colorist", &self.colorist),
            ("Letterer", &self.letterer),
            ("CoverArtist", &self.cover_artist),
            ("Translator", &self.translator),
            ("Genre", &self.genre),
            ("Tags", &self.tags),
            ("Categories", &self.categories),
            ("PublishingStatusTachiyomi", &self.publishing_status),
            ("Year", &self.year),
            ("Month", &self.month),
            ("Day", &self.day),
        ] {
            if let Some(value) = value {
                if !value.trim().is_empty() {
                    xml.push_str(&format!("  <{element}>{}</{element}>\n", escape_xml(value)));
                }
            }
        }
        xml.push_str("</ComicInfo>\n");
        xml
    }
}

/// Write `ComicInfo.xml` atomically next to a series directory.
pub fn write_comic_info(path: &Path, info: &ComicInfo) -> Result<(), CoreError> {
    let temporary = path.with_extension("xml.tmp");
    fs::write(&temporary, info.to_xml()).map_err(|source| CoreError::WriteFile {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, path).map_err(|source| {
        let _ = fs::remove_file(&temporary);
        CoreError::WriteFile {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Parse a ComicInfo.xml document.
///
/// Unknown elements are ignored. A malformed document returns an error string suitable
/// for a non-fatal metadata warning.
pub fn parse_comic_info(xml: &str) -> Result<ComicInfo, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = true;

    let mut info = ComicInfo::default();
    let mut current: Option<String> = None;
    let mut text = String::new();
    let mut depth = 0usize;

    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match event {
            Event::Start(element) => {
                depth += 1;
                current = Some(local_name(element.name().as_ref()));
                text.clear();
            }
            Event::Empty(_) => {
                current = None;
                text.clear();
            }
            Event::Text(value) => {
                if current.is_some() {
                    let decoded = value.decode().map_err(|error| error.to_string())?;
                    text.push_str(&decoded);
                }
            }
            Event::CData(value) => {
                if current.is_some() {
                    let decoded = value.decode().map_err(|error| error.to_string())?;
                    text.push_str(&decoded);
                }
            }
            Event::GeneralRef(value) => {
                if current.is_some() {
                    if let Ok(Some(character)) = value.resolve_char_ref() {
                        text.push(character);
                    } else {
                        let name = value.decode().map_err(|error| error.to_string())?;
                        let resolved = match name.as_ref() {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            _ => "",
                        };
                        text.push_str(resolved);
                    }
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if let Some(name) = current.take() {
                    set_field(&mut info, &name, text.trim());
                }
                text.clear();
            }
            Event::Eof => {
                if depth != 0 {
                    return Err("unclosed XML element".to_owned());
                }
                break;
            }
            _ => {}
        }
    }

    Ok(info)
}

fn local_name(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

fn set_field(info: &mut ComicInfo, name: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let value = value.to_owned();
    match name {
        "title" => info.title = Some(value),
        "series" => info.series = Some(value),
        "number" => info.number = Some(value),
        "summary" => info.summary = Some(value),
        "writer" => info.writer = Some(value),
        "penciller" => info.penciller = Some(value),
        "inker" => info.inker = Some(value),
        "colorist" => info.colorist = Some(value),
        "letterer" => info.letterer = Some(value),
        "coverartist" => info.cover_artist = Some(value),
        "translator" => info.translator = Some(value),
        "genre" => info.genre = Some(value),
        "tags" => info.tags = Some(value),
        "categories" => info.categories = Some(value),
        "publishingstatustachiyomi" => info.publishing_status = Some(value),
        "year" => info.year = Some(value),
        "month" => info.month = Some(value),
        "day" => info.day = Some(value),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mihon_series_and_chapter_fields() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ComicInfo xmlns:xsd="http://www.w3.org/2001/XMLSchema"
           xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Title>Chapter 12</Title>
  <Series>Real &amp; Title</Series>
  <Number>12</Number>
  <Summary>Some description</Summary>
  <Writer>Author Name</Writer>
  <Penciller>Artist One</Penciller>
  <Inker>Artist Two</Inker>
  <Genre>Action, Fantasy</Genre>
  <Tags>Magic</Tags>
  <Categories>Reading</Categories>
  <PublishingStatusTachiyomi xmlns:ty="http://www.w3.org/2001/XMLSchema">Ongoing</PublishingStatusTachiyomi>
  <Translator>Scan Group</Translator>
  <Year>2024</Year>
  <Month>3</Month>
  <Day>9</Day>
</ComicInfo>"#;

        let info = parse_comic_info(xml).unwrap();

        assert_eq!(info.title.as_deref(), Some("Chapter 12"));
        assert_eq!(info.series.as_deref(), Some("Real & Title"));
        assert_eq!(info.chapter_number(), Some(12.0));
        assert_eq!(info.summary.as_deref(), Some("Some description"));
        assert_eq!(info.writer.as_deref(), Some("Author Name"));
        assert_eq!(
            info.series_artist().as_deref(),
            Some("Artist One, Artist Two")
        );
        assert_eq!(
            info.series_tags(),
            ["Action", "Fantasy", "Magic", "Reading"]
        );
        assert_eq!(info.publishing_status.as_deref(), Some("Ongoing"));
        assert_eq!(
            info.chapter_date(),
            Some(ChapterDate {
                year: 2024,
                month: 3,
                day: 9
            })
        );
    }

    #[test]
    fn ignores_unknown_elements_and_empty_values() {
        let info = parse_comic_info(
            "<ComicInfo><Title></Title><Unknown>value</Unknown><Series>Title</Series></ComicInfo>",
        )
        .unwrap();
        assert_eq!(info.title, None);
        assert_eq!(info.series.as_deref(), Some("Title"));
    }

    #[test]
    fn malformed_xml_is_an_error() {
        assert!(parse_comic_info("<ComicInfo><Title>broken").is_err());
    }

    #[test]
    fn partial_dates_default_month_and_day() {
        let info = parse_comic_info("<ComicInfo><Year>2023</Year></ComicInfo>").unwrap();
        assert_eq!(
            info.chapter_date(),
            Some(ChapterDate {
                year: 2023,
                month: 1,
                day: 1
            })
        );
    }
}

#[cfg(test)]
mod serializer_tests {
    use super::*;

    #[test]
    fn serializes_and_parses_back() {
        let info = ComicInfo {
            title: Some("Berserk".to_owned()),
            series: Some("Berserk".to_owned()),
            summary: Some("A swordsman & a dream".to_owned()),
            writer: Some("Kentaro Miura".to_owned()),
            penciller: Some("Kentaro Miura".to_owned()),
            genre: Some("Action, Drama".to_owned()),
            publishing_status: Some("Ongoing".to_owned()),
            ..ComicInfo::default()
        };
        let parsed = parse_comic_info(&info.to_xml()).unwrap();
        assert_eq!(parsed, info);
        assert!(info.to_xml().contains("A swordsman &amp; a dream"));
    }

    #[test]
    fn writes_a_comic_info_file() {
        let root = tempfile::tempdir().unwrap();
        let info = ComicInfo {
            title: Some("Series".to_owned()),
            ..ComicInfo::default()
        };
        let path = root.path().join("ComicInfo.xml");
        write_comic_info(&path, &info).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            parse_comic_info(&text).unwrap().title.as_deref(),
            Some("Series")
        );
        assert!(!path.with_extension("xml.tmp").exists());
    }
}
