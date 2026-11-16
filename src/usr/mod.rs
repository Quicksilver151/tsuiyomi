use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub library_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: env::var_os("TSUIYOMI_LIBRARY").map(PathBuf::from),
        }
    }
}

pub fn load_config() -> Config {
    confy::load("tsuiyomi", None).unwrap_or_else(|error| {
        eprintln!("failed to load config: {error}; using defaults");
        Config::default()
    })
}
