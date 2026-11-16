use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Application config, stored as TOML under the XDG config directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Root of the Mihon `local/` library.
    pub library_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: env::var_os("TSUIYOMI_LIBRARY").map(PathBuf::from),
        }
    }
}

/// Load the config, falling back to defaults (and creating the file) when missing.
pub fn load_config() -> Config {
    confy::load("tsuiyomi", None).unwrap_or_else(|error| {
        eprintln!("failed to load config: {error}; using defaults");
        Config::default()
    })
}
