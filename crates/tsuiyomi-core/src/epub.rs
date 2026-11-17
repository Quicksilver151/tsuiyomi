//! Minimal EPUB reader for image-based comics.
//!
//! Mihon's `EpubReader` walks the package spine and extracts only `<img>`/`<image>`
//! targets from each XHTML page. This module follows the same approach; prose is not
//! rendered.

use std::collections::HashMap;
use std::path::Path;

use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};

use crate::{
    archive,
    comic_info::{ChapterDate, ComicInfo},
    error::CoreError,
    library::ChapterKind,
};

const CONTAINER: &str = "META-INF/container.xml";
const DEFAULT_PACKAGE: &str = "OEBPS/content.opf";

#[derive(Debug, Default)]
struct Package {
    manifest: HashMap<String, ManifestItem>,
    spine: Vec<String>,
    metadata: EpubMetadata,
}

#[derive(Debug, Clone)]
struct ManifestItem {
    href: String,
    media_type: String,
}

#[derive(Debug, Default)]
struct EpubMetadata {
    title: Option<String>,
    creator: Option<String>,
    publisher: Option<String>,
    description: Option<String>,
    date: Option<ChapterDate>,
}

/// Read EPUB metadata into the chapter fields Mihon maps.
pub(crate) fn chapter_metadata(path: &Path, warnings: &mut Vec<String>) -> Option<ComicInfo> {
    let package = match read_package(path) {
        Ok(package) => package,
        Err(error) => {
            warnings.push(format!("{}: {error}", file_name(path)));
            return None;
        }
    };
    let metadata = package.metadata;
    let mut info = ComicInfo {
        title: metadata.title,
        translator: metadata.publisher.or(metadata.creator),
        ..ComicInfo::default()
    };
    if let Some(date) = metadata.date {
        info.year = Some(date.year.to_string());
        info.month = Some(date.month.to_string());
        info.day = Some(date.day.to_string());
    }
    Some(info)
}

/// List image entries in EPUB spine order.
pub(crate) fn image_pages(path: &Path) -> Result<Vec<String>, CoreError> {
    let package = read_package(path)?;
    let package_dir = parent_dir(&package_path(path)?);

    let mut images = Vec::new();
    for page_href in &package.spine {
        let Some(item) = package.manifest.get(page_href) else {
            continue;
        };
        if !item
            .media_type
            .eq_ignore_ascii_case("application/xhtml+xml")
        {
            continue;
        }
        let page_path = resolve_path(&package_dir, &item.href);
        let Some(bytes) = archive::read_entry_optional(path, ChapterKind::Epub, &page_path)? else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let page_dir = parent_dir(&page_path);
        images.extend(images_in_xhtml(&text, &page_dir));
    }
    Ok(images)
}

fn read_package(path: &Path) -> Result<Package, CoreError> {
    let package_path = package_path(path)?;
    let bytes =
        archive::read_entry_optional(path, ChapterKind::Epub, &package_path)?.ok_or_else(|| {
            CoreError::MissingPage {
                path: path.to_path_buf(),
                name: package_path.clone(),
            }
        })?;
    let text = String::from_utf8_lossy(&bytes);
    parse_package(&text)
}

fn package_path(path: &Path) -> Result<String, CoreError> {
    if let Some(bytes) = archive::read_entry_optional(path, ChapterKind::Epub, CONTAINER)? {
        let text = String::from_utf8_lossy(&bytes);
        if let Some(path) = parse_container(&text) {
            return Ok(path);
        }
    }
    Ok(DEFAULT_PACKAGE.to_owned())
}

fn parse_container(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().ok()? {
            Event::Start(element) | Event::Empty(element) => {
                if local_name(element.name().as_ref()) == "rootfile" {
                    if let Some(path) = attribute(&reader, &element, "full-path") {
                        return Some(path.trim_start_matches('/').to_owned());
                    }
                }
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

fn parse_package(xml: &str) -> Result<Package, CoreError> {
    let mut reader = Reader::from_str(xml);
    let mut package = Package::default();
    let mut current: Option<String> = None;
    let mut text = String::new();

    loop {
        let event = reader.read_event().map_err(|error| CoreError::Archive {
            path: Path::new("<epub>").to_path_buf(),
            message: error.to_string(),
        })?;
        match event {
            Event::Start(element) => {
                let name = local_name(element.name().as_ref());
                match name.as_str() {
                    "item" => {
                        let id = attribute(&reader, &element, "id");
                        let href = attribute(&reader, &element, "href");
                        let media_type = attribute(&reader, &element, "media-type");
                        if let (Some(id), Some(href), Some(media_type)) = (id, href, media_type) {
                            package
                                .manifest
                                .insert(id, ManifestItem { href, media_type });
                        }
                    }
                    "itemref" => {
                        if let Some(idref) = attribute(&reader, &element, "idref") {
                            package.spine.push(idref);
                        }
                    }
                    "meta"
                        if package.metadata.date.is_none()
                            && attribute(&reader, &element, "property")
                                .is_some_and(|value| value == "dcterms:modified") =>
                    {
                        package.metadata.date = attribute(&reader, &element, "content")
                            .and_then(|value| parse_date(&value));
                    }
                    _ => {
                        current = Some(name);
                        text.clear();
                    }
                }
            }
            Event::Empty(element) => {
                let name = local_name(element.name().as_ref());
                match name.as_str() {
                    "item" => {
                        let id = attribute(&reader, &element, "id");
                        let href = attribute(&reader, &element, "href");
                        let media_type = attribute(&reader, &element, "media-type");
                        if let (Some(id), Some(href), Some(media_type)) = (id, href, media_type) {
                            package
                                .manifest
                                .insert(id, ManifestItem { href, media_type });
                        }
                    }
                    "itemref" => {
                        if let Some(idref) = attribute(&reader, &element, "idref") {
                            package.spine.push(idref);
                        }
                    }
                    "meta"
                        if package.metadata.date.is_none()
                            && attribute(&reader, &element, "property")
                                .is_some_and(|value| value == "dcterms:modified") =>
                    {
                        package.metadata.date = attribute(&reader, &element, "content")
                            .and_then(|value| parse_date(&value));
                    }
                    _ => {}
                }
            }
            Event::Text(value) => {
                if current.is_some() {
                    let decoded = value.decode().map_err(|error| CoreError::Archive {
                        path: Path::new("<epub>").to_path_buf(),
                        message: error.to_string(),
                    })?;
                    text.push_str(&decoded);
                }
            }
            Event::End(_) => {
                if let Some(name) = current.take() {
                    let value = text.trim();
                    if !value.is_empty() {
                        match name.as_str() {
                            "title" => package.metadata.title = Some(value.to_owned()),
                            "creator" => package.metadata.creator = Some(value.to_owned()),
                            "publisher" => package.metadata.publisher = Some(value.to_owned()),
                            "description" => {
                                package.metadata.description = Some(value.to_owned());
                            }
                            "date" => package.metadata.date = parse_date(value),
                            _ => {}
                        }
                    }
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(package)
}

fn images_in_xhtml(xml: &str, base_dir: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut images = Vec::new();
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let name = local_name(element.name().as_ref());
                let href = match name.as_str() {
                    "img" => attribute(&reader, &element, "src"),
                    "image" => attribute(&reader, &element, "href"),
                    _ => None,
                };
                if let Some(href) = href {
                    let href = href.split(['#', '?']).next().unwrap_or(&href);
                    if !href.is_empty() {
                        images.push(resolve_path(base_dir, href));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    images
}

fn attribute(reader: &Reader<&[u8]>, element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .filter_map(Result::ok)
        .find(|attribute| local_name(attribute.key.as_ref()) == name)
        .and_then(|attribute| {
            attribute
                .decode_and_unescape_value(reader.decoder())
                .ok()
                .map(|value| value.into_owned())
        })
}

fn local_name(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_owned())
        .unwrap_or_default()
}

fn resolve_path(base_dir: &str, href: &str) -> String {
    let href = percent_decode(href);
    if href.starts_with('/') {
        return href.trim_start_matches('/').to_owned();
    }

    let mut parts: Vec<String> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').map(str::to_owned).collect()
    };
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part.to_owned()),
        }
    }
    parts.join("/")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn parse_date(value: &str) -> Option<ChapterDate> {
    let date = value.trim().get(..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u8>().ok()?;
    let day = parts.next()?.parse::<u8>().ok()?;
    if year <= 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(ChapterDate { year, month, day })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_and_encoded_paths() {
        assert_eq!(
            resolve_path("OEBPS/Text", "../Images/Page%201.jpg"),
            "OEBPS/Images/Page 1.jpg"
        );
        assert_eq!(
            resolve_path("OEBPS", "Images/Page 1.jpg"),
            "OEBPS/Images/Page 1.jpg"
        );
        assert_eq!(resolve_path("", "/root.jpg"), "root.jpg");
    }

    #[test]
    fn parses_partial_dates() {
        assert_eq!(
            parse_date("2024-03-09T12:00:00Z"),
            Some(ChapterDate {
                year: 2024,
                month: 3,
                day: 9
            })
        );
        assert_eq!(parse_date("not a date"), None);
    }

    #[test]
    fn reads_epub_metadata() {
        use std::io::Write;

        use tempfile::tempdir;
        use zip::{write::SimpleFileOptions, ZipWriter};

        let root = tempdir().unwrap();
        let path = root.path().join("Chapter.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("META-INF/container.xml", options)
            .unwrap();
        writer
            .write_all(
                b"<container><rootfiles><rootfile full-path=\"OEBPS/content.opf\"/></rootfiles></container>",
            )
            .unwrap();
        writer.start_file("OEBPS/content.opf", options).unwrap();
        writer
            .write_all(
                b"<package><metadata><dc:title>EPUB Title</dc:title>\
                  <dc:publisher>Publisher</dc:publisher>\
                  <dc:date>2023-05-06T00:00:00Z</dc:date></metadata>\
                  <manifest/><spine/></package>",
            )
            .unwrap();
        writer.finish().unwrap();

        let mut warnings = Vec::new();
        let info = chapter_metadata(&path, &mut warnings).unwrap();

        assert!(warnings.is_empty());
        assert_eq!(info.title.as_deref(), Some("EPUB Title"));
        assert_eq!(info.translator.as_deref(), Some("Publisher"));
        assert_eq!(
            info.chapter_date(),
            Some(ChapterDate {
                year: 2023,
                month: 5,
                day: 6
            })
        );
    }
}
