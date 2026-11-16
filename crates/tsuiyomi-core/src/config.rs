use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Application config, stored as TOML under the XDG config directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root of the Mihon `local/` library.
    pub library_path: Option<PathBuf>,
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
        assert!(config.library_path.is_none());
    }
}
