use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    archive,
    chapter::natural_cmp,
    epub,
    error::CoreError,
    library::{Chapter, ChapterKind},
};

/// One readable page of a chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Display name: file name for folders, archive entry path for archives.
    pub name: String,
    pub source: PageSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageSource {
    File(PathBuf),
    Archive {
        path: PathBuf,
        name: String,
        kind: ChapterKind,
    },
    Remote {
        url: String,
    },
}

/// List the pages of a chapter in natural reading order.
pub fn chapter_pages(chapter: &Chapter) -> Result<Vec<Page>, CoreError> {
    match chapter.kind {
        ChapterKind::Directory => pages_in_directory(&chapter.path),
        ChapterKind::Epub => pages_in_epub(&chapter.path),
        ChapterKind::Zip | ChapterKind::Rar | ChapterKind::SevenZip | ChapterKind::Tar => {
            pages_in_archive(&chapter.path, chapter.kind)
        }
    }
}

/// Read the bytes for a page.
pub fn read_page(page: &Page) -> Result<Vec<u8>, CoreError> {
    match &page.source {
        PageSource::File(path) => fs::read(path).map_err(|source| CoreError::ReadFile {
            path: path.clone(),
            source,
        }),
        PageSource::Archive { path, name, kind } => archive::read_entry(path, *kind, name),
        PageSource::Remote { url } => {
            let response = ureq::get(url).call().map_err(|error| CoreError::Network {
                url: url.clone(),
                message: error.to_string(),
            })?;
            let mut data = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut data)
                .map_err(|error| CoreError::Network {
                    url: url.clone(),
                    message: error.to_string(),
                })?;
            Ok(data)
        }
    }
}

fn pages_in_directory(path: &Path) -> Result<Vec<Page>, CoreError> {
    let entries = fs::read_dir(path).map_err(|source| CoreError::ReadDir {
        path: path.to_path_buf(),
        source,
    })?;

    let mut pages = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let entry_path = entry.path();
        if entry_path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if !entry_path.is_file() || !is_image(&entry_path) {
            continue;
        }
        pages.push(Page {
            name: entry_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            source: PageSource::File(entry_path),
        });
    }

    pages.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    Ok(pages)
}

fn pages_in_archive(path: &Path, kind: ChapterKind) -> Result<Vec<Page>, CoreError> {
    let mut names: Vec<String> = archive::list_entries(path, kind)?
        .into_iter()
        .filter(|name| !name.starts_with('.') && is_image_name(name))
        .collect();
    names.sort_by(|left, right| natural_cmp(left, right));
    Ok(names
        .into_iter()
        .map(|name| Page {
            source: PageSource::Archive {
                path: path.to_path_buf(),
                name: name.clone(),
                kind,
            },
            name,
        })
        .collect())
}

fn pages_in_epub(path: &Path) -> Result<Vec<Page>, CoreError> {
    Ok(epub::image_pages(path)?
        .into_iter()
        .map(|name| Page {
            source: PageSource::Archive {
                path: path.to_path_buf(),
                name: name.clone(),
                kind: ChapterKind::Epub,
            },
            name,
        })
        .collect())
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_image_extension)
}

fn is_image_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_image_extension)
}

fn is_image_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "bmp"
    )
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::*;

    fn chapter(path: PathBuf, kind: ChapterKind) -> Chapter {
        Chapter {
            path,
            title: "Chapter 1".to_owned(),
            number: Some(1.0),
            kind,
            scanlator: None,
            date: None,
        }
    }

    #[test]
    fn lists_directory_pages_in_natural_order() {
        let root = tempdir().unwrap();
        for name in ["10.jpg", "2.png", "001.JPG", "notes.txt"] {
            fs::write(root.path().join(name), b"x").unwrap();
        }
        fs::create_dir(root.path().join("nested")).unwrap();

        let pages =
            chapter_pages(&chapter(root.path().to_path_buf(), ChapterKind::Directory)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(names, ["001.JPG", "2.png", "10.jpg"]);
    }

    #[test]
    fn lists_zip_pages_ignoring_directories_and_non_images() {
        let root = tempdir().unwrap();
        let path = root.path().join("Chapter 1.cbz");
        let file = fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        writer.start_file("002.jpg", options).unwrap();
        writer.write_all(b"page two").unwrap();
        writer.start_file("nested/001.png", options).unwrap();
        writer.write_all(b"page one").unwrap();
        writer.start_file("notes.txt", options).unwrap();
        writer.write_all(b"ignore me").unwrap();
        writer.add_directory("folder/", options).unwrap();
        writer.finish().unwrap();

        let pages = chapter_pages(&chapter(path.clone(), ChapterKind::Zip)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(names, ["002.jpg", "nested/001.png"]);
        assert_eq!(read_page(&pages[1]).unwrap(), b"page one");
    }

    #[test]
    fn lists_tar_pages() {
        let root = tempdir().unwrap();
        let path = root.path().join("Chapter 1.cbt");
        let file = fs::File::create(&path).unwrap();
        let mut builder = tar::Builder::new(file);
        for (name, data) in [
            ("002.jpg", b"page two".as_slice()),
            ("001.jpg", b"page one"),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        }
        builder.finish().unwrap();

        let pages = chapter_pages(&chapter(path.clone(), ChapterKind::Tar)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(names, ["001.jpg", "002.jpg"]);
        assert_eq!(read_page(&pages[1]).unwrap(), b"page two");
    }

    #[test]
    fn lists_seven_zip_pages() {
        let root = tempdir().unwrap();
        let path = root.path().join("Chapter 1.cb7");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        for (name, data) in [
            ("002.jpg", b"page two".as_slice()),
            ("001.jpg", b"page one"),
        ] {
            writer
                .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(data))
                .unwrap();
        }
        writer.finish().unwrap();

        let pages = chapter_pages(&chapter(path.clone(), ChapterKind::SevenZip)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(names, ["001.jpg", "002.jpg"]);
        assert_eq!(read_page(&pages[1]).unwrap(), b"page two");
    }

    fn write_epub(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        let entries = [
            (
                "META-INF/container.xml",
                r#"<?xml version="1.0"?>
<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
            ),
            (
                "OEBPS/content.opf",
                r#"<?xml version="1.0"?>
<package>
  <metadata>
    <dc:title>Epub Chapter</dc:title>
    <dc:creator>Creator Name</dc:creator>
    <dc:publisher>Publisher Name</dc:publisher>
    <dc:date>2024-01-02T03:04:05Z</dc:date>
  </metadata>
  <manifest>
    <item id="p1" href="Text/page1.xhtml" media-type="application/xhtml+xml"/>
    <item id="p2" href="Text/page2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="p1"/><itemref idref="p2"/></spine>
</package>"#,
            ),
            (
                "OEBPS/Text/page1.xhtml",
                r#"<html xmlns:xlink="http://www.w3.org/1999/xlink"><body>
                    <img src="../Images/Page%201.jpg"/>
                    <image xlink:href="../Images/two.png"/>
                </body></html>"#,
            ),
            (
                "OEBPS/Text/page2.xhtml",
                r#"<html><body><img src="../Images/three.jpg"/></body></html>"#,
            ),
        ];
        for (name, data) in entries {
            writer.start_file(name, options).unwrap();
            writer.write_all(data.as_bytes()).unwrap();
        }
        for (name, data) in [
            ("OEBPS/Images/Page 1.jpg", b"page one".as_slice()),
            ("OEBPS/Images/two.png", b"page two"),
            ("OEBPS/Images/three.jpg", b"page three"),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn reads_epub_spine_images_in_order() {
        let root = tempdir().unwrap();
        let path = root.path().join("Chapter 1.epub");
        write_epub(&path);

        let pages = chapter_pages(&chapter(path.clone(), ChapterKind::Epub)).unwrap();
        let names: Vec<&str> = pages.iter().map(|page| page.name.as_str()).collect();

        assert_eq!(
            names,
            [
                "OEBPS/Images/Page 1.jpg",
                "OEBPS/Images/two.png",
                "OEBPS/Images/three.jpg"
            ]
        );
        assert_eq!(read_page(&pages[0]).unwrap(), b"page one");
    }
}
