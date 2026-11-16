use std::{collections::BTreeMap, env, path::PathBuf};

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

/// Built-in color themes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    CatppuccinLatte,
    Nord,
    SolarizedDark,
    SolarizedLight,
    GruvboxDark,
    GruvboxLight,
    MaterialDark,
    Dracula,
    OneDark,
    TokyoNight,
    #[serde(other)]
    Unknown,
}

/// Application config, stored as TOML under the XDG config directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root of the library, containing one directory per series.
    pub library_path: Option<PathBuf>,
    /// Default reader mode.
    pub reading_mode: ReadingMode,
    /// UI color theme.
    pub theme: Theme,
    /// Accent override: a palette name (`blue`) or hex (`#aabbcc`). `None` uses the
    /// theme's default accent.
    pub accent: Option<String>,
    /// Start with the title and commands boxes hidden.
    pub fullscreen: bool,
    /// Scroll-mode zoom, 50–300%.
    pub reader_zoom: u16,
    /// Pages prepared before the viewport.
    pub scroll_prefetch_behind: usize,
    /// Pages prepared after the viewport.
    pub scroll_prefetch_ahead: usize,
    /// Extra cached pages around the viewport.
    pub scroll_cache_margin: usize,
    /// Unknown keys from existing user configs, preserved on save.
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_path: default_library_path(),
            reading_mode: ReadingMode::default(),
            theme: Theme::default(),
            accent: None,
            fullscreen: false,
            reader_zoom: 100,
            scroll_prefetch_behind: 2,
            scroll_prefetch_ahead: 2,
            scroll_cache_margin: 4,
            extra: BTreeMap::new(),
        }
    }
}

/// `$XDG_DATA_HOME/tsuiyomi/local`, i.e. `~/.local/share/tsuiyomi/local`.
fn default_library_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("local"))
}

/// Load the config. `TSUIYOMI_LIBRARY` overrides the stored path.
pub fn load_config() -> Config {
    let mut config: Config = confy::load("tsuiyomi", "config").unwrap_or_else(|error| {
        eprintln!("failed to load config: {error}; using defaults");
        Config::default()
    });

    if let Some(path) = env::var_os("TSUIYOMI_LIBRARY") {
        config.library_path = Some(PathBuf::from(path));
    }

    config
}

/// Persist the config, including unknown keys preserved from the existing file.
pub fn save_config(config: &Config) {
    if let Err(error) = confy::store("tsuiyomi", "config", config) {
        eprintln!("failed to save config: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_and_preserves_unknown_config_keys() {
        let config: Config = toml::from_str(
            r#"
library_path = "/tmp/manga"
theme = "default"

[player]
external_mpv = ["mpv"]
"#,
        )
        .unwrap();

        assert_eq!(config.library_path, Some(PathBuf::from("/tmp/manga")));
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
        // A legacy `theme` value is now a known key; unknown names fall back.
        assert_eq!(config.theme, Theme::Unknown);
        assert_eq!(config.reader_zoom, 100);
        assert!(config.extra.contains_key("player"));

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("[player]"));
    }

    #[test]
    fn missing_keys_use_defaults() {
        let config: Config = toml::from_str("theme = \"default\"").unwrap();
        assert_eq!(config.library_path, Config::default().library_path);
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
        assert_eq!(config.reader_zoom, 100);
        assert_eq!(config.scroll_prefetch_behind, 2);
        assert_eq!(config.scroll_prefetch_ahead, 2);
        assert_eq!(config.scroll_cache_margin, 4);
    }

    #[test]
    fn theme_and_accent_roundtrip() {
        let config: Config = toml::from_str(
            r##"
theme = "tokyo_night"
accent = "#ffcc00"
fullscreen = true
"##,
        )
        .unwrap();
        assert_eq!(config.theme, Theme::TokyoNight);
        assert_eq!(config.accent.as_deref(), Some("#ffcc00"));
        assert!(config.fullscreen);

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("theme = \"tokyo_night\""));
        assert!(serialized.contains("accent = \"#ffcc00\""));
        assert!(serialized.contains("fullscreen = true"));
    }

    #[test]
    fn reading_mode_matches_mihon_names() {
        let config: Config = toml::from_str("reading_mode = \"left_to_right\"").unwrap();
        assert_eq!(config.reading_mode, ReadingMode::LeftToRight);

        let config: Config = toml::from_str("reading_mode = \"webtoon\"").unwrap();
        assert_eq!(config.reading_mode, ReadingMode::Webtoon);
    }
}
