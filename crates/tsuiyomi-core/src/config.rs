use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Application config, stored as TOML under the XDG config directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root of the library, containing one directory per series.
    pub library_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: default_library_path(),
        }
    }
}

/// `$XDG_DATA_HOME/tsuiyomi/local`, i.e. `~/.local/share/tsuiyomi/local`.
fn default_library_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("local"))
}

/// Load the config. `TSUIYOMI_LIBRARY` overrides the stored path.
pub fn load_config() -> Config {
    let mut config: Config = confy::load("tsuiyomi", None).unwrap_or_else(|error| {
        eprintln!("failed to load config: {error}; using defaults");
        Config::default()
    });

    if let Some(path) = env::var_os("TSUIYOMI_LIBRARY") {
        config.library_path = Some(PathBuf::from(path));
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_unknown_config_keys() {
        let config: Config =
            serde_json::from_str(r#"{"theme":"default","library_path":"/tmp/manga"}"#).unwrap();
        assert_eq!(config.library_path, Some(PathBuf::from("/tmp/manga")));
    }

    #[test]
    fn missing_keys_use_defaults() {
        let config: Config = serde_json::from_str(r#"{"theme":"default"}"#).unwrap();
        assert_eq!(config.library_path, Config::default().library_path);
    }
}
