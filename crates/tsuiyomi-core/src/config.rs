use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Reader/viewer modes, mirroring Mihon's `ReadingMode` enum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingMode {
    Default,
    LeftToRight,
    RightToLeft,
    Vertical,
    #[default]
    Webtoon,
    ContinuousVertical,
}

/// Application config, stored as TOML under the XDG config directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root of the library, containing one directory per series.
    pub library_path: Option<PathBuf>,
    /// Default reader mode. Long-strip scroll (`webtoon`) unless configured otherwise.
    pub reading_mode: ReadingMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: default_library_path(),
            reading_mode: ReadingMode::default(),
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
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
    }

    #[test]
    fn missing_keys_use_defaults() {
        let config: Config = serde_json::from_str(r#"{"theme":"default"}"#).unwrap();
        assert_eq!(config.library_path, Config::default().library_path);
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
    }

    #[test]
    fn reading_mode_matches_mihon_names() {
        let config: Config = serde_json::from_str(r#"{"reading_mode":"left_to_right"}"#).unwrap();
        assert_eq!(config.reading_mode, ReadingMode::LeftToRight);

        let config: Config = serde_json::from_str(r#"{"reading_mode":"webtoon"}"#).unwrap();
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
    }
}
