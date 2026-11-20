//! Extension merge and update rules.
//!
//! The host reports installed extensions and available store entries separately. Merging
//! them must preserve the installed version while remembering the store's version, or
//! update detection becomes impossible.

use crate::host::ExtensionInfo;

/// Merge installed extensions with every repository entry.
///
/// Installed rows win for identity and version; repository rows contribute the APK,
/// repository URL, available version, and any missing language/source count. Entries
/// that are not installed are appended as available extensions.
pub fn merge_extensions(
    mut installed: Vec<ExtensionInfo>,
    repo_entries: impl IntoIterator<Item = ExtensionInfo>,
) -> Vec<ExtensionInfo> {
    for entry in repo_entries {
        if let Some(known) = installed
            .iter_mut()
            .find(|known| !known.pkg.is_empty() && known.pkg == entry.pkg)
        {
            if !entry.version.is_empty() {
                known.available_version = Some(entry.version);
            }
            known.repo_url = entry.repo_url;
            known.apk = entry.apk;
            known.source_count = known.source_count.max(entry.source_count);
            if known.lang.is_empty() {
                known.lang = entry.lang;
            }
        } else {
            let mut entry = entry;
            if entry.available_version.is_none() && !entry.version.is_empty() {
                entry.available_version = Some(entry.version.clone());
            }
            installed.push(entry);
        }
    }

    installed.sort_by(|left, right| {
        right
            .installed
            .cmp(&left.installed)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    installed
}

#[cfg(test)]
mod tests {
    use crate::host::{ExtensionInfo, ExtensionSource};

    use super::*;

    fn extension(pkg: &str, version: &str, installed: bool) -> ExtensionInfo {
        ExtensionInfo {
            name: pkg.to_owned(),
            pkg: pkg.to_owned(),
            lang: "en".to_owned(),
            version: version.to_owned(),
            apk: format!("{pkg}.apk"),
            installed,
            nsfw: false,
            source_count: 1,
            sources: vec![ExtensionSource {
                name: pkg.to_owned(),
                lang: "en".to_owned(),
            }],
            repo_url: String::new(),
            available_version: None,
        }
    }

    #[test]
    fn merge_preserves_installed_version_and_records_update() {
        let installed = vec![extension("pkg.a", "1.0", true)];
        let mut available = extension("pkg.a", "1.2", false);
        available.repo_url = "https://example.org/index.min.json".to_owned();

        let merged = merge_extensions(installed, [available]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].version, "1.0");
        assert_eq!(merged[0].available_version.as_deref(), Some("1.2"));
        assert!(merged[0].update_available());
    }

    #[test]
    fn up_to_date_extensions_do_not_report_updates() {
        let installed = vec![extension("pkg.a", "1.2", true)];
        let available = extension("pkg.a", "1.2", false);

        let merged = merge_extensions(installed, [available]);

        assert!(!merged[0].update_available());
    }

    #[test]
    fn uninstalled_store_entries_remain_available() {
        let merged = merge_extensions(Vec::new(), [extension("pkg.b", "2.0", false)]);

        assert_eq!(merged.len(), 1);
        assert!(!merged[0].installed);
        assert_eq!(merged[0].available_version.as_deref(), Some("2.0"));
    }
}
