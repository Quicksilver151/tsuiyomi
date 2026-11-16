use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Anime {
    pub title: String,
    pub description: String,
}

pub fn get_anime_folder_contents(root: &Path) -> io::Result<Vec<Anime>> {
    let mut folders: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    folders.sort();

    let mut anime_list = Vec::with_capacity(folders.len());
    for folder in folders {
        let title = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let description = fs::read_to_string(folder.join(".desc"))
            .unwrap_or_else(|_| "No description found.".to_owned());
        anime_list.push(Anime { title, description });
    }

    Ok(anime_list)
}
