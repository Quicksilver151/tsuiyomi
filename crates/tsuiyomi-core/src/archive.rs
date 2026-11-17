//! Shared readers for the archive formats Mihon's Local source accepts.

use std::{fs, io::Read, path::Path};

use sevenz_rust2::{ArchiveReader, Password};
use tar::Archive as TarArchive;
use unrar::Archive as RarArchive;
use zip::ZipArchive;

use crate::{error::CoreError, library::ChapterKind};

/// List regular file entries in an archive.
pub(crate) fn list_entries(path: &Path, kind: ChapterKind) -> Result<Vec<String>, CoreError> {
    match kind {
        ChapterKind::Zip | ChapterKind::Epub => list_zip(path),
        ChapterKind::Rar => list_rar(path),
        ChapterKind::SevenZip => list_seven_zip(path),
        ChapterKind::Tar => list_tar(path),
        ChapterKind::Directory => Ok(Vec::new()),
    }
}

/// Read one archive entry by name.
pub(crate) fn read_entry(path: &Path, kind: ChapterKind, name: &str) -> Result<Vec<u8>, CoreError> {
    read_entry_optional(path, kind, name)?.ok_or_else(|| CoreError::MissingPage {
        path: path.to_path_buf(),
        name: name.to_owned(),
    })
}

/// Read one archive entry, returning `None` if it does not exist.
pub(crate) fn read_entry_optional(
    path: &Path,
    kind: ChapterKind,
    name: &str,
) -> Result<Option<Vec<u8>>, CoreError> {
    match kind {
        ChapterKind::Zip | ChapterKind::Epub => read_zip_entry(path, name),
        ChapterKind::Rar => read_rar_entry(path, name),
        ChapterKind::SevenZip => read_seven_zip_entry(path, name),
        ChapterKind::Tar => read_tar_entry(path, name),
        ChapterKind::Directory => Ok(None),
    }
}

/// Find a root-level entry by case-insensitive name.
pub(crate) fn find_root_entry(
    path: &Path,
    kind: ChapterKind,
    name: &str,
) -> Result<Option<String>, CoreError> {
    Ok(list_entries(path, kind)?
        .into_iter()
        .find(|entry| !entry.contains(['/', '\\']) && entry.eq_ignore_ascii_case(name)))
}

fn open_zip(path: &Path) -> Result<ZipArchive<fs::File>, CoreError> {
    let file = fs::File::open(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    ZipArchive::new(file).map_err(|error| archive_error(path, error))
}

fn list_zip(path: &Path) -> Result<Vec<String>, CoreError> {
    let mut archive = open_zip(path)?;
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| archive_error(path, error))?;
        if entry.is_dir() {
            continue;
        }
        names.push(
            entry
                .name()
                .map_err(|error| archive_error(path, error))?
                .into_owned(),
        );
    }
    Ok(names)
}

fn read_zip_entry(path: &Path, name: &str) -> Result<Option<Vec<u8>>, CoreError> {
    let mut archive = open_zip(path)?;
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(archive_error(path, error)),
    };
    let mut data = Vec::new();
    entry
        .read_to_end(&mut data)
        .map_err(|source| CoreError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(Some(data))
}

fn list_rar(path: &Path) -> Result<Vec<String>, CoreError> {
    let archive = RarArchive::new(path)
        .open_for_listing()
        .map_err(|error| archive_error(path, error))?;
    let mut names = Vec::new();
    for entry in archive {
        let entry = entry.map_err(|error| archive_error(path, error))?;
        if entry.is_file() {
            names.push(normalize_name(&entry.filename.to_string_lossy()));
        }
    }
    Ok(names)
}

fn read_rar_entry(path: &Path, name: &str) -> Result<Option<Vec<u8>>, CoreError> {
    let mut archive = RarArchive::new(path)
        .open_for_processing()
        .map_err(|error| archive_error(path, error))?;
    while let Some(header) = archive
        .read_header()
        .map_err(|error| archive_error(path, error))?
    {
        let entry_name = normalize_name(&header.entry().filename.to_string_lossy());
        if header.entry().is_file() && entry_name == normalize_name(name) {
            let (data, _rest) = header.read().map_err(|error| archive_error(path, error))?;
            return Ok(Some(data));
        }
        archive = header.skip().map_err(|error| archive_error(path, error))?;
    }
    Ok(None)
}

fn open_seven_zip(path: &Path) -> Result<ArchiveReader<fs::File>, CoreError> {
    let file = fs::File::open(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    ArchiveReader::new(file, Password::empty()).map_err(|error| archive_error(path, error))
}

fn list_seven_zip(path: &Path) -> Result<Vec<String>, CoreError> {
    let reader = open_seven_zip(path)?;
    Ok(reader
        .archive()
        .files
        .iter()
        .filter(|entry| !entry.is_directory())
        .map(|entry| entry.name().to_owned())
        .collect())
}

fn read_seven_zip_entry(path: &Path, name: &str) -> Result<Option<Vec<u8>>, CoreError> {
    let mut reader = open_seven_zip(path)?;
    match reader.read_file(name) {
        Ok(data) => Ok(Some(data)),
        Err(sevenz_rust2::Error::FileNotFound) => Ok(None),
        Err(error) => Err(archive_error(path, error)),
    }
}

fn list_tar(path: &Path) -> Result<Vec<String>, CoreError> {
    let file = fs::File::open(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = TarArchive::new(file);
    let mut names = Vec::new();
    for entry in archive
        .entries()
        .map_err(|error| archive_error(path, error))?
    {
        let entry = entry.map_err(|error| archive_error(path, error))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let entry_path = entry.path().map_err(|error| archive_error(path, error))?;
        names.push(normalize_name(&entry_path.to_string_lossy()));
    }
    Ok(names)
}

fn read_tar_entry(path: &Path, name: &str) -> Result<Option<Vec<u8>>, CoreError> {
    let file = fs::File::open(path).map_err(|source| CoreError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = TarArchive::new(file);
    for entry in archive
        .entries()
        .map_err(|error| archive_error(path, error))?
    {
        let mut entry = entry.map_err(|error| archive_error(path, error))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let entry_path = entry.path().map_err(|error| archive_error(path, error))?;
        if normalize_name(&entry_path.to_string_lossy()) == normalize_name(name) {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|source| CoreError::ReadFile {
                    path: path.to_path_buf(),
                    source,
                })?;
            return Ok(Some(data));
        }
    }
    Ok(None)
}

fn normalize_name(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches("./").to_owned()
}

fn archive_error(path: &Path, error: impl std::fmt::Display) -> CoreError {
    CoreError::Archive {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
