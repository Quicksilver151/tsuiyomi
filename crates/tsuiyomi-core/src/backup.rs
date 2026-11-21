//! Read-only Mihon backup parsing.
//!
//! Mihon writes `.tachibk` / `.proto.gz` files: optional gzip (magic `1f 8b`)
//! followed by a protobuf `Backup` message. Field 1 is a repeated `BackupManga`;
//! categories are field 2 and sources field 101. The parser is intentionally
//! tolerant: unknown fields are skipped and malformed nested entries are ignored.

use std::io::Read;

use flate2::read::GzDecoder;

use crate::error::CoreError;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MihonBackup {
    pub mangas: Vec<BackupManga>,
    pub categories: Vec<BackupCategory>,
    pub sources: Vec<BackupSource>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackupManga {
    pub source_id: i64,
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genre: Vec<String>,
    pub thumbnail_url: Option<String>,
    pub chapters: Vec<BackupChapter>,
    pub category_ids: Vec<i64>,
    pub favorite: bool,
    pub notes: String,
    pub history: Vec<BackupHistory>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackupChapter {
    pub url: String,
    pub name: String,
    pub scanlator: Option<String>,
    pub read: bool,
    pub bookmark: bool,
    pub last_page_read: i64,
    pub date_upload: i64,
    pub chapter_number: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BackupHistory {
    pub url: String,
    pub last_read: i64,
    pub read_duration: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupCategory {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupSource {
    pub name: String,
    pub source_id: i64,
}

/// Parse a Mihon backup from bytes, transparently decompressing gzip.
pub fn parse_mihon_backup(bytes: &[u8]) -> Result<MihonBackup, CoreError> {
    let data = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes);
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|error| CoreError::Backup {
                message: format!("gzip: {error}"),
            })?;
        output
    } else {
        bytes.to_vec()
    };
    let mut backup = MihonBackup::default();
    for (field, wire, value) in fields(&data)? {
        if wire != 2 {
            continue;
        }
        match field {
            1 => {
                if let Some(manga) = parse_manga(value) {
                    backup.mangas.push(manga);
                }
            }
            2 => {
                if let Some(category) = parse_category(value) {
                    backup.categories.push(category);
                }
            }
            101 => {
                if let Some(source) = parse_source(value) {
                    backup.sources.push(source);
                }
            }
            _ => {}
        }
    }
    Ok(backup)
}

fn parse_manga(data: &[u8]) -> Option<BackupManga> {
    let mut manga = BackupManga {
        favorite: true,
        ..BackupManga::default()
    };
    for (field, wire, value) in fields(data).ok()? {
        match (field, wire) {
            (1, 0) => manga.source_id = varint(value) as i64,
            (2, 2) => manga.url = text(value),
            (3, 2) => manga.title = text(value),
            (4, 2) => manga.artist = nonempty(text(value)),
            (5, 2) => manga.author = nonempty(text(value)),
            (6, 2) => manga.description = nonempty(text(value)),
            (7, 2) => manga.genre.push(text(value)),
            (9, 2) => manga.thumbnail_url = nonempty(text(value)),
            (16, 2) => {
                if let Some(chapter) = parse_chapter(value) {
                    manga.chapters.push(chapter);
                }
            }
            (17, 0) => manga.category_ids.push(varint(value) as i64),
            (100, 0) => manga.favorite = varint(value) != 0,
            (104, 2) => {
                if let Some(history) = parse_history(value) {
                    manga.history.push(history);
                }
            }
            (110, 2) => manga.notes = text(value),
            _ => {}
        }
    }
    Some(manga)
}

fn parse_chapter(data: &[u8]) -> Option<BackupChapter> {
    let mut chapter = BackupChapter::default();
    for (field, wire, value) in fields(data).ok()? {
        match (field, wire) {
            (1, 2) => chapter.url = text(value),
            (2, 2) => chapter.name = text(value),
            (3, 2) => chapter.scanlator = nonempty(text(value)),
            (4, 0) => chapter.read = varint(value) != 0,
            (5, 0) => chapter.bookmark = varint(value) != 0,
            (6, 0) => chapter.last_page_read = varint(value) as i64,
            (8, 0) => chapter.date_upload = varint(value) as i64,
            (9, 5) if value.len() >= 4 => {
                chapter.chapter_number =
                    f32::from_le_bytes([value[0], value[1], value[2], value[3]]);
            }
            _ => {}
        }
    }
    Some(chapter)
}

fn parse_history(data: &[u8]) -> Option<BackupHistory> {
    let mut history = BackupHistory::default();
    for (field, wire, value) in fields(data).ok()? {
        match (field, wire) {
            (1, 2) => history.url = text(value),
            (2, 0) => history.last_read = varint(value) as i64,
            (3, 0) => history.read_duration = varint(value) as i64,
            _ => {}
        }
    }
    Some(history)
}

fn parse_category(data: &[u8]) -> Option<BackupCategory> {
    let mut category = BackupCategory::default();
    for (field, wire, value) in fields(data).ok()? {
        match (field, wire) {
            (1, 2) => category.name = text(value),
            (3, 0) => category.id = varint(value) as i64,
            _ => {}
        }
    }
    Some(category)
}

fn parse_source(data: &[u8]) -> Option<BackupSource> {
    let mut source = BackupSource::default();
    for (field, wire, value) in fields(data).ok()? {
        match (field, wire) {
            (1, 2) => source.name = text(value),
            (2, 0) => source.source_id = varint(value) as i64,
            _ => {}
        }
    }
    Some(source)
}

fn text(value: &[u8]) -> String {
    String::from_utf8_lossy(value).into_owned()
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn varint(value: &[u8]) -> u64 {
    let mut result = 0u64;
    for (shift, byte) in value.iter().take(10).enumerate() {
        result |= u64::from(*byte & 0x7f) << (shift * 7);
        if byte & 0x80 == 0 {
            break;
        }
    }
    result
}

type FieldList<'a> = Vec<(u32, u8, &'a [u8])>;

/// Return `(field number, wire type, value bytes)` for each protobuf field.
/// Varints are returned as their original encoded bytes.
fn fields<'a>(mut data: &'a [u8]) -> Result<FieldList<'a>, CoreError> {
    let mut fields = Vec::new();
    while !data.is_empty() {
        let (key, rest) = read_varint(data)?;
        data = rest;
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u8;
        match wire {
            0 => {
                let start = data;
                let (_, rest) = read_varint(data)?;
                let consumed = start.len().saturating_sub(rest.len());
                fields.push((field, wire, &start[..consumed]));
                data = rest;
            }
            1 => {
                if data.len() < 8 {
                    return Err(invalid("truncated fixed64"));
                }
                fields.push((field, wire, &data[..8]));
                data = &data[8..];
            }
            2 => {
                let (length, rest) = read_varint(data)?;
                data = rest;
                let length = usize::try_from(length).map_err(|_| invalid("length overflow"))?;
                if data.len() < length {
                    return Err(invalid("truncated length-delimited field"));
                }
                fields.push((field, wire, &data[..length]));
                data = &data[length..];
            }
            5 => {
                if data.len() < 4 {
                    return Err(invalid("truncated fixed32"));
                }
                fields.push((field, wire, &data[..4]));
                data = &data[4..];
            }
            _ => return Err(invalid("unsupported wire type")),
        }
    }
    Ok(fields)
}

fn read_varint(mut data: &[u8]) -> Result<(u64, &[u8]), CoreError> {
    let mut result = 0u64;
    for shift in 0..10 {
        let Some((byte, rest)) = data.split_first() else {
            return Err(invalid("truncated varint"));
        };
        result |= u64::from(byte & 0x7f) << (shift * 7);
        data = rest;
        if byte & 0x80 == 0 {
            return Ok((result, data));
        }
    }
    Err(invalid("varint too long"))
}

fn invalid(message: &str) -> CoreError {
    CoreError::Backup {
        message: message.to_owned(),
    }
}

/// Encode a backup in Mihon's protobuf format and gzip it.
pub fn encode_mihon_backup(backup: &MihonBackup) -> Vec<u8> {
    let mut data = Vec::new();
    for manga in &backup.mangas {
        let mut message = Vec::new();
        put_varint(&mut message, 1, manga.source_id.max(0) as u64);
        put_bytes(&mut message, 2, manga.url.as_bytes());
        put_bytes(&mut message, 3, manga.title.as_bytes());
        if let Some(artist) = &manga.artist {
            put_bytes(&mut message, 4, artist.as_bytes());
        }
        if let Some(author) = &manga.author {
            put_bytes(&mut message, 5, author.as_bytes());
        }
        if let Some(description) = &manga.description {
            put_bytes(&mut message, 6, description.as_bytes());
        }
        for genre in &manga.genre {
            put_bytes(&mut message, 7, genre.as_bytes());
        }
        if let Some(thumbnail) = &manga.thumbnail_url {
            put_bytes(&mut message, 9, thumbnail.as_bytes());
        }
        for chapter in &manga.chapters {
            let mut chapter_message = Vec::new();
            put_bytes(&mut chapter_message, 1, chapter.url.as_bytes());
            put_bytes(&mut chapter_message, 2, chapter.name.as_bytes());
            if let Some(scanlator) = &chapter.scanlator {
                put_bytes(&mut chapter_message, 3, scanlator.as_bytes());
            }
            if chapter.read {
                put_varint(&mut chapter_message, 4, 1);
            }
            if chapter.bookmark {
                put_varint(&mut chapter_message, 5, 1);
            }
            if chapter.last_page_read > 0 {
                put_varint(&mut chapter_message, 6, chapter.last_page_read as u64);
            }
            if chapter.date_upload != 0 {
                put_varint(&mut chapter_message, 8, chapter.date_upload.max(0) as u64);
            }
            if chapter.chapter_number != 0.0 {
                put_fixed32(&mut chapter_message, 9, chapter.chapter_number);
            }
            put_bytes(&mut message, 16, &chapter_message);
        }
        for category in &manga.category_ids {
            put_varint(&mut message, 17, (*category).max(0) as u64);
        }
        for history in &manga.history {
            let mut history_message = Vec::new();
            put_bytes(&mut history_message, 1, history.url.as_bytes());
            put_varint(&mut history_message, 2, history.last_read.max(0) as u64);
            if history.read_duration > 0 {
                put_varint(&mut history_message, 3, history.read_duration as u64);
            }
            put_bytes(&mut message, 104, &history_message);
        }
        if manga.favorite {
            put_varint(&mut message, 100, 1);
        }
        if !manga.notes.is_empty() {
            put_bytes(&mut message, 110, manga.notes.as_bytes());
        }
        put_bytes(&mut data, 1, &message);
    }
    for category in &backup.categories {
        let mut message = Vec::new();
        put_bytes(&mut message, 1, category.name.as_bytes());
        put_varint(&mut message, 3, category.id.max(0) as u64);
        put_bytes(&mut data, 2, &message);
    }
    for source in &backup.sources {
        let mut message = Vec::new();
        put_bytes(&mut message, 1, source.name.as_bytes());
        put_varint(&mut message, 2, source.source_id.max(0) as u64);
        put_bytes(&mut data, 101, &message);
    }
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &data).expect("gzip write to memory");
    encoder.finish().expect("gzip finish")
}

/// Write a gzipped Mihon backup to `path`.
pub fn write_mihon_backup(path: &std::path::Path, backup: &MihonBackup) -> std::io::Result<()> {
    std::fs::write(path, encode_mihon_backup(backup))
}

fn put_key(out: &mut Vec<u8>, field: u32, wire: u8) {
    let mut key = (u64::from(field) << 3) | u64::from(wire);
    loop {
        let mut byte = (key & 0x7f) as u8;
        key >>= 7;
        if key != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if key == 0 {
            break;
        }
    }
}

fn put_varint(out: &mut Vec<u8>, field: u32, value: u64) {
    put_key(out, field, 0);
    let mut value = value;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn put_bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    put_key(out, field, 2);
    let mut length = value.len() as u64;
    loop {
        let mut byte = (length & 0x7f) as u8;
        length >>= 7;
        if length != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if length == 0 {
            break;
        }
    }
    out.extend_from_slice(value);
}

fn put_fixed32(out: &mut Vec<u8>, field: u32, value: f32) {
    put_key(out, field, 5);
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint_field(field: u32, value: u64) -> Vec<u8> {
        let mut out = encode_varint(u64::from(field) << 3);
        out.extend(encode_varint(value));
        out
    }

    fn bytes_field(field: u32, value: &[u8]) -> Vec<u8> {
        let mut out = encode_varint((u64::from(field) << 3) | 2);
        out.extend(encode_varint(value.len() as u64));
        out.extend_from_slice(value);
        out
    }

    fn fixed32_field(field: u32, value: f32) -> Vec<u8> {
        let mut out = encode_varint((u64::from(field) << 3) | 5);
        out.extend_from_slice(&value.to_le_bytes());
        out
    }

    fn encode_varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                break;
            }
        }
        out
    }

    fn chapter() -> Vec<u8> {
        let mut out = bytes_field(1, b"/chapter/1");
        out.extend(bytes_field(2, b"Chapter 1"));
        out.extend(varint_field(4, 1));
        out.extend(varint_field(5, 1));
        out.extend(varint_field(6, 9));
        out.extend(fixed32_field(9, 1.0));
        out
    }

    #[test]
    fn parses_backup_manga_categories_sources_and_chapters() {
        let mut manga = varint_field(1, 42);
        manga.extend(bytes_field(2, b"/manga/alpha"));
        manga.extend(bytes_field(3, b"Alpha"));
        manga.extend(bytes_field(7, b"Action"));
        manga.extend(bytes_field(16, &chapter()));
        manga.extend(varint_field(17, 7));
        manga.extend(varint_field(100, 1));
        let category = {
            let mut out = bytes_field(1, b"Reading");
            out.extend(varint_field(3, 7));
            out
        };
        let source = {
            let mut out = bytes_field(1, b"Asura Scans");
            out.extend(varint_field(2, 42));
            out
        };
        let mut backup = bytes_field(1, &manga);
        backup.extend(bytes_field(2, &category));
        backup.extend(bytes_field(101, &source));

        let parsed = parse_mihon_backup(&backup).unwrap();

        assert_eq!(parsed.mangas.len(), 1);
        assert_eq!(parsed.mangas[0].title, "Alpha");
        assert_eq!(parsed.mangas[0].source_id, 42);
        assert_eq!(parsed.mangas[0].chapters[0].name, "Chapter 1");
        assert!(parsed.mangas[0].chapters[0].read);
        assert_eq!(parsed.categories[0].name, "Reading");
        assert_eq!(parsed.sources[0].name, "Asura Scans");
    }

    #[test]
    fn encodes_a_backup_that_parses_back() {
        let backup = MihonBackup {
            mangas: vec![BackupManga {
                source_id: 42,
                url: "/manga/alpha".to_owned(),
                title: "Alpha".to_owned(),
                favorite: true,
                notes: "note".to_owned(),
                chapters: vec![BackupChapter {
                    url: "/chapter/1".to_owned(),
                    name: "Chapter 1".to_owned(),
                    read: true,
                    bookmark: true,
                    last_page_read: 9,
                    chapter_number: 1.0,
                    ..BackupChapter::default()
                }],
                category_ids: vec![7],
                history: vec![BackupHistory {
                    url: "/chapter/1".to_owned(),
                    last_read: 1_700_000_000_000,
                    read_duration: 60,
                }],
                ..BackupManga::default()
            }],
            categories: vec![BackupCategory {
                id: 7,
                name: "Reading".to_owned(),
            }],
            sources: vec![BackupSource {
                name: "Source".to_owned(),
                source_id: 42,
            }],
        };
        let parsed = parse_mihon_backup(&encode_mihon_backup(&backup)).unwrap();
        assert_eq!(parsed, backup);
    }

    #[test]
    fn decompresses_gzip_backups() {
        use std::io::Write;
        let mut data = varint_field(1, 42);
        data.extend(bytes_field(2, b"/manga/alpha"));
        data.extend(bytes_field(3, b"Alpha"));
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&bytes_field(1, &data)).unwrap();
        let gz = encoder.finish().unwrap();
        let parsed = parse_mihon_backup(&gz).unwrap();
        assert_eq!(parsed.mangas[0].title, "Alpha");
    }

    #[test]
    fn tolerates_unknown_fields_and_rejects_truncated_files() {
        let mut data = varint_field(999, 1);
        data.extend(bytes_field(1, b"not a message"));
        assert!(parse_mihon_backup(&data).is_ok());
        assert!(parse_mihon_backup(&[0x0a, 0x05, 0x01]).is_err());
    }
}
