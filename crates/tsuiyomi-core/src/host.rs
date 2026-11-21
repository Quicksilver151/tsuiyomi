//! Client for the bundled extension host.
//!
//! Mihon/Tachiyomi extensions are JVM bytecode. tsuiyomi does not load APKs itself; a
//! headless JVM service ([miwayomi](https://github.com/miwayomi/miwayomi)) loads them and
//! exposes sources, manga, chapters, pages, and extension management over a small REST
//! API. This module is the Rust side of that contract and contains no UI code.

use std::{collections::BTreeMap, io::Read, path::PathBuf, time::Duration};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    error::CoreError,
    filters::SourceFilter,
    source_prefs::{preference_body, SourcePreferenceValue, SourcePreferences},
};

const TIMEOUT: Duration = Duration::from_secs(30);

fn default_true() -> bool {
    true
}

/// A source exposed by the host: one per installed extension, or one per source in a
/// multi-source extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteSource {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub pkg: Option<String>,
    /// Whether the source exposes a latest-updates listing. Older hosts omit the
    /// field; default to true so existing behavior is preserved.
    pub supports_latest: bool,
}

impl RemoteSource {
    /// `MangaDex (EN)`, falling back to the bare name for unknown languages.
    pub fn display_name(&self) -> String {
        if self.lang.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.lang.to_uppercase())
        }
    }
}

/// A search result or details page. The host addresses manga by URL, not by an internal
/// id, so `url` is the stable identity together with `source_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteManga {
    pub source_id: String,
    pub url: String,
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genre: Vec<String>,
    pub status: String,
    pub thumbnail_url: Option<String>,
    /// Opaque per-source state (slugs, ids, tokens) that must be sent back with later
    /// requests. Mihon stores this in `SManga.memo`.
    pub memo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteMangaPage {
    pub mangas: Vec<RemoteManga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteChapter {
    pub url: String,
    pub name: String,
    pub chapter_number: f32,
    pub scanlator: Option<String>,
    pub upload_date: i64,
    /// Opaque per-chapter state required by sources such as Asura Scans.
    pub memo: Option<String>,
}

/// A source declared by an extension, used for pinning and detail views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionSource {
    pub name: String,
    pub lang: String,
    /// Host source ID, present for installed extensions.
    #[serde(default)]
    pub id: String,
    /// Source base URL when the host can supply it.
    #[serde(default)]
    pub base_url: String,
}

/// One row in a repository index or in the installed-extension list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub pkg: String,
    pub lang: String,
    pub version: String,
    /// APK URL or file name, as published by the repository. `install` expects this
    /// value unchanged.
    pub apk: String,
    pub installed: bool,
    pub nsfw: bool,
    pub source_count: usize,
    pub sources: Vec<ExtensionSource>,
    /// Repository this row came from; empty for installed-only rows.
    pub repo_url: String,
    /// Version published by the repository. `version` stays the installed version for
    /// installed rows so updates can be detected without re-reading the host.
    pub available_version: Option<String>,
    pub obsolete: bool,
}

impl ExtensionInfo {
    /// Whether the store offers a version different from the installed one.
    pub fn update_available(&self) -> bool {
        self.installed
            && self
                .available_version
                .as_deref()
                .is_some_and(|available| !available.is_empty() && available != self.version)
    }
}

/// One installed file the host could not load. Trust cannot be granted because the
/// host does not verify extension signatures; the client can inspect the reason or
/// delete the file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NotLoadedExtension {
    #[serde(default)]
    pub pkg: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    #[serde(rename = "hasUpdate", default)]
    pub has_update: bool,
}

/// State of an asynchronous extension install/update job.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InstallJob {
    #[serde(rename = "jobId", default)]
    pub job_id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pkg: Option<String>,
    #[serde(default)]
    pub manga: usize,
    #[serde(default)]
    pub anime: usize,
}

impl InstallJob {
    pub fn finished(&self) -> bool {
        matches!(self.state.as_str(), "done" | "error" | "cancelled")
    }

    pub fn sources(&self) -> usize {
        self.manga + self.anime
    }
}

/// A manga saved in the host's library (favorites table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFavorite {
    pub source_id: String,
    pub url: String,
    pub title: String,
    pub thumbnail_url: Option<String>,
    pub last_read_url: Option<String>,
    pub last_read_name: Option<String>,
    /// Library categories chosen in tsuiyomi. Empty means Default.
    #[serde(default)]
    pub categories: Vec<String>,
}

/// Result of installing one extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallOutcome {
    pub name: Option<String>,
    pub pkg: Option<String>,
    /// Sources the host managed to instantiate. Zero usually means the extension needs a
    /// newer host API or a WebView.
    pub sources: usize,
}

#[derive(Clone)]
pub struct HostClient {
    base_url: String,
    agent: ureq::Agent,
}

impl std::fmt::Debug for HostClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl HostClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        Self {
            base_url,
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(5))
                .timeout(TIMEOUT)
                .build(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build an absolute URL for a host path.
    pub fn absolute_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{}/{}", self.base_url, path.trim_start_matches('/'))
        }
    }

    /// Image loader used for covers and pages. Going through the host means the
    /// extension's own headers (Referer, User-Agent, cookies) are applied.
    pub fn proxy_url(&self, source_id: &str, url: &str) -> String {
        format!(
            "{}/api/v1/proxy?sourceId={}&url={}",
            self.base_url,
            encode(source_id),
            encode(url)
        )
    }

    pub fn health(&self) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Health {
            status: String,
        }
        let health: Health = self.get("/api/v1/health")?;
        if health.status == "ok" {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/health"),
                message: format!("host reported status {:?}", health.status),
            })
        }
    }

    pub fn sources(&self) -> Result<Vec<RemoteSource>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            manga: Vec<SourceRow>,
        }
        #[derive(Deserialize)]
        struct SourceRow {
            id: String,
            #[serde(default)]
            name: String,
            #[serde(default)]
            lang: String,
            #[serde(default)]
            pkg: Option<String>,
            #[serde(default = "default_true", alias = "supports_latest")]
            supports_latest: bool,
        }

        let data: Data = self.get("/api/v1/sources")?;
        Ok(data
            .manga
            .into_iter()
            // The host ships offline demo sources with no package. They are useful for
            // host development, not for readers.
            .filter(|row| row.pkg.is_some())
            .map(|row| RemoteSource {
                id: row.id,
                name: row.name,
                lang: row.lang,
                pkg: row.pkg,
                supports_latest: row.supports_latest,
            })
            .collect())
    }

    pub fn popular(&self, source_id: &str, page: i32) -> Result<RemoteMangaPage, CoreError> {
        self.manga_list(
            source_id,
            &format!(
                "/api/v1/manga/{}/popular?page={}",
                encode(source_id),
                page.max(1)
            ),
        )
    }

    pub fn latest(&self, source_id: &str, page: i32) -> Result<RemoteMangaPage, CoreError> {
        self.manga_list(
            source_id,
            &format!(
                "/api/v1/manga/{}/latest?page={}",
                encode(source_id),
                page.max(1)
            ),
        )
    }

    pub fn search(
        &self,
        source_id: &str,
        query: &str,
        page: i32,
    ) -> Result<RemoteMangaPage, CoreError> {
        self.manga_list(
            source_id,
            &format!(
                "/api/v1/manga/{}/search?query={}&page={}",
                encode(source_id),
                encode(query),
                page.max(1)
            ),
        )
    }

    /// Filter tree for a source. An unsupported or throwing source returns an empty list.
    pub fn filters(&self, source_id: &str) -> Result<Vec<SourceFilter>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            filters: Vec<SourceFilter>,
        }
        let data: Data = self.get(&format!("/api/v1/manga/{}/filters", encode(source_id)))?;
        Ok(data.filters)
    }

    /// Search with explicit filter state. Used when the source exposes filters.
    pub fn search_filtered(
        &self,
        source_id: &str,
        query: &str,
        page: i32,
        filters: &[SourceFilter],
    ) -> Result<RemoteMangaPage, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "hasNextPage", default)]
            has_next_page: bool,
            #[serde(default)]
            mangas: Vec<MangaRow>,
        }
        let data: Data = self.post(
            &format!("/api/v1/manga/{}/search", encode(source_id)),
            json!({
                "page": page.max(1),
                "query": query,
                "filters": filters,
            }),
        )?;
        Ok(RemoteMangaPage {
            has_next_page: data.has_next_page,
            mangas: data
                .mangas
                .into_iter()
                .map(|row| row.into_manga(source_id, self))
                .collect(),
        })
    }

    fn manga_list(&self, source_id: &str, path: &str) -> Result<RemoteMangaPage, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "hasNextPage", default)]
            has_next_page: bool,
            #[serde(default)]
            mangas: Vec<MangaRow>,
        }

        let data: Data = self.get(path)?;
        Ok(RemoteMangaPage {
            has_next_page: data.has_next_page,
            mangas: data
                .mangas
                .into_iter()
                .map(|row| row.into_manga(source_id, self))
                .collect(),
        })
    }

    pub fn details(
        &self,
        source_id: &str,
        url: &str,
        memo: Option<&str>,
    ) -> Result<RemoteManga, CoreError> {
        let path = format!(
            "/api/v1/manga/{}/details?url={}&memo={}",
            encode(source_id),
            encode(url),
            encode(memo.unwrap_or_default())
        );
        let row: MangaRow = self.get(&path)?;
        Ok(row.into_manga(source_id, self))
    }

    pub fn chapters(
        &self,
        source_id: &str,
        url: &str,
        memo: Option<&str>,
    ) -> Result<Vec<RemoteChapter>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            chapters: Vec<ChapterRow>,
        }
        #[derive(Deserialize)]
        struct ChapterRow {
            #[serde(default)]
            url: String,
            #[serde(default)]
            name: String,
            #[serde(default)]
            chapter_number: f32,
            #[serde(default)]
            scanlator: Option<String>,
            #[serde(default)]
            date_upload: i64,
            #[serde(default)]
            memo: Option<String>,
        }

        let path = format!(
            "/api/v1/manga/{}/chapters?url={}&memo={}",
            encode(source_id),
            encode(url),
            encode(memo.unwrap_or_default())
        );
        let data: Data = self.get(&path)?;
        Ok(data
            .chapters
            .into_iter()
            .map(|row| RemoteChapter {
                url: row.url,
                name: row.name,
                chapter_number: row.chapter_number,
                scanlator: row.scanlator.filter(|value| !value.trim().is_empty()),
                upload_date: row.date_upload,
                memo: row.memo.filter(|value| !value.trim().is_empty()),
            })
            .collect())
    }

    /// Page image URLs, routed through the host's page-aware image endpoint.
    ///
    /// Some sources split a page across a server list and a relative path, so the reader
    /// cannot reuse `imageUrl` directly. The endpoint rebuilds the page and lets the
    /// source run its own request.
    pub fn pages(
        &self,
        source_id: &str,
        url: &str,
        memo: Option<&str>,
        name: Option<&str>,
    ) -> Result<Vec<String>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            pages: Vec<PageRow>,
        }
        #[derive(Deserialize)]
        struct PageRow {
            #[serde(default)]
            url: String,
            #[serde(default, rename = "imageUrl")]
            image_url: Option<String>,
        }

        let path = format!(
            "/api/v1/manga/{}/pages?url={}&memo={}&name={}",
            encode(source_id),
            encode(url),
            encode(memo.unwrap_or_default()),
            encode(name.unwrap_or_default())
        );
        let data: Data = self.get(&path)?;
        Ok(data
            .pages
            .into_iter()
            // Some sources (Asura Scans) leave `url` empty and only set `imageUrl`.
            .filter(|row| {
                !row.url.trim().is_empty()
                    || row
                        .image_url
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
            })
            .map(|row| self.page_image_url(source_id, &row.url, row.image_url.as_deref()))
            .collect())
    }

    /// Host endpoint that fetches one page through the source's own HTTP client.
    pub fn page_image_url(
        &self,
        source_id: &str,
        page_url: &str,
        image_url: Option<&str>,
    ) -> String {
        format!(
            "{}/api/v1/image?sourceId={}&url={}&imageUrl={}",
            self.base_url,
            encode(source_id),
            encode(page_url),
            encode(image_url.unwrap_or_default())
        )
    }

    /// Extensions published by one repository index (`index.min.json` or `index.pb`).
    pub fn repo_extensions(&self, repo_url: &str) -> Result<Vec<ExtensionInfo>, CoreError> {
        let path = format!("/api/v1/extensions/repo?url={}", encode(repo_url));
        let data: ExtensionList = self.get(&path)?;
        Ok(data
            .extensions
            .into_iter()
            .map(|row| row.into_info(repo_url))
            .collect())
    }

    pub fn installed_extensions(&self) -> Result<Vec<ExtensionInfo>, CoreError> {
        let data: ExtensionList = self.get("/api/v1/extensions/installed")?;
        Ok(data
            .extensions
            .into_iter()
            .map(|row| row.into_info(""))
            .collect())
    }

    pub fn install_extension(
        &self,
        repo_url: &str,
        apk: &str,
    ) -> Result<InstallOutcome, CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            pkg: Option<String>,
            #[serde(default)]
            manga: usize,
            #[serde(default)]
            anime: usize,
            #[serde(default)]
            error: Option<String>,
        }
        let result: Result_ = self.post(
            "/api/v1/extensions/install",
            json!({ "repoUrl": repo_url, "apk": apk }),
        )?;
        if result.ok {
            Ok(InstallOutcome {
                name: result.name,
                pkg: result.pkg,
                sources: result.manga + result.anime,
            })
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/extensions/install"),
                message: result.error.unwrap_or_else(|| "install failed".to_owned()),
            })
        }
    }

    /// Start an install/update job. The endpoint returns immediately; poll it with
    /// [`Self::install_job`] on older hosts this returns an error and the caller should
    /// fall back to [`Self::install_extension`].
    pub fn install_extension_async(
        &self,
        repo_url: &str,
        apk: &str,
    ) -> Result<InstallJob, CoreError> {
        let job: InstallJob = self.post(
            "/api/v1/extensions/install?async=true",
            json!({ "repoUrl": repo_url, "apk": apk }),
        )?;
        if job.job_id.is_empty() {
            return Err(CoreError::Network {
                url: self.absolute_url("/api/v1/extensions/install?async=true"),
                message: "host does not support asynchronous installs".to_owned(),
            });
        }
        Ok(job)
    }

    pub fn install_job(&self, job_id: &str) -> Result<InstallJob, CoreError> {
        self.get(&format!("/api/v1/extensions/jobs/{}", encode(job_id)))
    }

    pub fn cancel_install_job(&self, job_id: &str) -> Result<InstallJob, CoreError> {
        self.post(
            &format!("/api/v1/extensions/jobs/{}/cancel", encode(job_id)),
            json!({}),
        )
    }

    pub fn not_loaded_extensions(&self) -> Result<Vec<NotLoadedExtension>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            extensions: Vec<NotLoadedExtension>,
        }
        let data: Data = self.get("/api/v1/extensions/not-loaded")?;
        Ok(data.extensions)
    }

    pub fn delete_not_loaded(&self, file: &str) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
            #[serde(default)]
            error: Option<String>,
        }
        let result: Result_ = self.post(
            "/api/v1/extensions/not-loaded/delete",
            json!({ "file": file }),
        )?;
        if result.ok {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/extensions/not-loaded/delete"),
                message: result
                    .error
                    .unwrap_or_else(|| "could not remove not-loaded extension".to_owned()),
            })
        }
    }

    pub fn uninstall_extension(&self, pkg: &str) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
            #[serde(default)]
            error: Option<String>,
        }
        let result: Result_ = self.post("/api/v1/extensions/uninstall", json!({ "pkg": pkg }))?;
        if result.ok {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/extensions/uninstall"),
                message: result
                    .error
                    .unwrap_or_else(|| "uninstall failed".to_owned()),
            })
        }
    }

    pub fn favorites(&self) -> Result<Vec<RemoteFavorite>, CoreError> {
        #[derive(Deserialize)]
        struct Row {
            #[serde(rename = "sourceId")]
            source_id: String,
            url: String,
            title: String,
            #[serde(rename = "thumbnailUrl", default)]
            thumbnail_url: Option<String>,
            #[serde(rename = "lastReadUrl", default)]
            last_read_url: Option<String>,
            #[serde(rename = "lastReadName", default)]
            last_read_name: Option<String>,
        }
        let rows: Vec<Row> = self.get("/api/v1/favorites")?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let source_id = row.source_id;
                RemoteFavorite {
                    url: row.url,
                    title: row.title,
                    // Stored favorites may still hold a proxy URL from an earlier
                    // host process, whose port no longer exists. Unwrap it before
                    // proxying through this process.
                    thumbnail_url: row
                        .thumbnail_url
                        .filter(|value| !value.trim().is_empty())
                        .map(|url| self.proxy_url(&source_id, &unproxy_url(&url))),
                    last_read_url: row.last_read_url,
                    last_read_name: row.last_read_name,
                    categories: Vec::new(),
                    source_id,
                }
            })
            .collect())
    }

    pub fn add_favorite(&self, manga: &RemoteManga) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
            #[serde(default)]
            error: Option<String>,
        }
        let result: Result_ = self.post(
            "/api/v1/favorites",
            json!({
                "sourceId": manga.source_id,
                "url": manga.url,
                "title": manga.title,
                "thumbnailUrl": manga.thumbnail_url.as_deref().map(unproxy_url),
            }),
        )?;
        if result.ok {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/favorites"),
                message: result
                    .error
                    .unwrap_or_else(|| "could not add to library".to_owned()),
            })
        }
    }

    pub fn remove_favorite(&self, source_id: &str, url: &str) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
        }
        let path = format!(
            "/api/v1/favorites?sourceId={}&url={}",
            encode(source_id),
            encode(url)
        );
        let result: Result_ = self.send(
            self.absolute_url(&path),
            self.agent.delete(&self.absolute_url(&path)).call(),
        )?;
        if result.ok {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/favorites"),
                message: "could not remove from library".to_owned(),
            })
        }
    }

    /// Preferences exposed by a `ConfigurableSource`.
    pub fn source_preferences(&self, source_id: &str) -> Result<SourcePreferences, CoreError> {
        self.get(&format!("/api/v1/sources/{}/prefs", encode(source_id)))
    }

    /// Save one or more source preference values.
    pub fn set_source_preferences(
        &self,
        source_id: &str,
        values: &BTreeMap<String, SourcePreferenceValue>,
    ) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
            #[serde(default)]
            error: Option<String>,
        }
        let result: Result_ = self.post(
            &format!("/api/v1/sources/{}/prefs", encode(source_id)),
            preference_body(values),
        )?;
        if result.ok {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url(&format!("/api/v1/sources/{source_id}/prefs")),
                message: result
                    .error
                    .unwrap_or_else(|| "could not save source preferences".to_owned()),
            })
        }
    }

    /// Clear cookies for a source's base and home URLs.
    pub fn clear_source_cookies(&self, source_id: &str) -> Result<usize, CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
            #[serde(default)]
            cleared: usize,
            #[serde(default)]
            error: Option<String>,
        }
        let result: Result_ = self.post(
            &format!("/api/v1/sources/{}/cookies/clear", encode(source_id)),
            json!({}),
        )?;
        if result.ok {
            Ok(result.cleared)
        } else {
            Err(CoreError::Network {
                url: self.absolute_url(&format!("/api/v1/sources/{source_id}/cookies/clear")),
                message: result
                    .error
                    .unwrap_or_else(|| "could not clear cookies".to_owned()),
            })
        }
    }

    pub fn repos(&self) -> Result<Vec<String>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(default)]
            repos: Vec<String>,
        }
        let data: Data = self.get("/api/v1/extensions/repos")?;
        Ok(data.repos)
    }

    pub fn set_repos(&self, repos: &[String]) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Result_ {
            #[serde(default)]
            ok: bool,
        }
        let result: Result_ = self.post("/api/v1/extensions/repos", json!({ "repos": repos }))?;
        if result.ok {
            Ok(())
        } else {
            Err(CoreError::Network {
                url: self.absolute_url("/api/v1/extensions/repos"),
                message: "host rejected the repository list".to_owned(),
            })
        }
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CoreError> {
        let request = self.agent.get(&self.absolute_url(path));
        let url = request.url().to_owned();
        self.send(url, request.call())
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: Value) -> Result<T, CoreError> {
        let request = self
            .agent
            .post(&self.absolute_url(path))
            .set("Content-Type", "application/json");
        let url = request.url().to_owned();
        self.send(url, request.send_json(body))
    }

    fn send<T: DeserializeOwned>(
        &self,
        url: String,
        response: Result<ureq::Response, ureq::Error>,
    ) -> Result<T, CoreError> {
        match response {
            Ok(response) => response
                .into_json::<T>()
                .map_err(|error| CoreError::Network {
                    url,
                    message: error.to_string(),
                }),
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(CoreError::Network {
                    url,
                    message: error_message(&body).unwrap_or_else(|| format!("HTTP {code}")),
                })
            }
            Err(error) => Err(CoreError::Network {
                url,
                message: error.to_string(),
            }),
        }
    }
}

/// Stable progress key for a remote chapter.
///
/// URLs are long and contain separators, so both the manga and chapter URLs are hashed
/// into fixed-size hex. The source id stays readable.
pub fn progress_key(source_id: &str, manga_url: &str, chapter_url: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}{}",
        progress_prefix(source_id, manga_url),
        fnv1a(chapter_url)
    ))
}

/// Prefix shared by every chapter progress key for one remote manga.
pub fn progress_prefix(source_id: &str, manga_url: &str) -> String {
    format!("remote://{}/{}/", source_id, fnv1a(manga_url))
}

fn fnv1a(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Undo a proxy URL produced by [`HostClient::proxy_url`] so stored favorites keep the
/// original image URL instead of an old host port.
fn unproxy_url(url: &str) -> String {
    let Some((_, query)) = url.split_once("/api/v1/proxy?") else {
        return url.to_owned();
    };
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("url=") {
            return decode(value);
        }
    }
    url.to_owned()
}

pub(crate) fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let message = value.get("error")?.as_str()?.trim().to_owned();
    if message.is_empty() {
        return None;
    }
    match value.get("challengeUrl").and_then(Value::as_str) {
        Some(url) if !url.is_empty() => Some(format!(
            "{message} ({url}) — this source needs a browser or FlareSolverr"
        )),
        _ => Some(message),
    }
}

#[derive(Deserialize)]
struct MangaRow {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    genre: Option<String>,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    thumbnail_url: Option<String>,
    #[serde(default)]
    memo: Option<String>,
}

impl MangaRow {
    fn into_manga(self, source_id: &str, client: &HostClient) -> RemoteManga {
        RemoteManga {
            source_id: source_id.to_owned(),
            url: self.url,
            title: self.title,
            author: self.author.filter(|value| !value.trim().is_empty()),
            artist: self.artist.filter(|value| !value.trim().is_empty()),
            description: self.description.filter(|value| !value.trim().is_empty()),
            genre: self
                .genre
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|part| !part.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            status: status_name(self.status).to_owned(),
            thumbnail_url: self
                .thumbnail_url
                .filter(|value| !value.trim().is_empty())
                .map(|url| client.proxy_url(source_id, &url)),
            memo: self.memo.filter(|value| !value.trim().is_empty()),
        }
    }
}

fn status_name(status: i32) -> &'static str {
    // SManga status constants from the Tachiyomi source API.
    match status {
        1 => "ongoing",
        2 => "completed",
        3 => "licensed",
        4 => "publishing finished",
        5 => "cancelled",
        6 => "on hiatus",
        _ => "unknown",
    }
}

#[derive(Deserialize)]
struct ExtensionList {
    #[serde(default)]
    extensions: Vec<ExtensionRow>,
}

#[derive(Deserialize)]
struct ExtensionRow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    pkg: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    apk: String,
    #[serde(default)]
    installed: bool,
    #[serde(default)]
    nsfw: bool,
    #[serde(default)]
    sources: Vec<ExtensionSourceRow>,
}

#[derive(Deserialize)]
struct ExtensionSourceRow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    id: String,
    #[serde(rename = "baseUrl", default)]
    base_url: String,
}

impl ExtensionRow {
    fn into_info(self, repo_url: &str) -> ExtensionInfo {
        let sources: Vec<ExtensionSource> = self
            .sources
            .into_iter()
            .map(|source| ExtensionSource {
                name: source.name,
                lang: source.lang,
                id: source.id,
                base_url: source.base_url,
            })
            .collect();
        ExtensionInfo {
            name: self.name,
            pkg: self.pkg,
            lang: self.lang,
            version: self.version.clone(),
            apk: self.apk,
            installed: self.installed,
            nsfw: self.nsfw,
            source_count: sources.len(),
            sources,
            repo_url: repo_url.to_owned(),
            available_version: (!repo_url.is_empty()).then_some(self.version),
            obsolete: false,
        }
    }
}

/// Read an arbitrary URL into memory. Used for cover thumbnails and remote pages.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, CoreError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build();
    let response = agent.get(url).call().map_err(|error| CoreError::Network {
        url: url.to_owned(),
        message: error.to_string(),
    })?;
    let mut data = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut data)
        .map_err(|error| CoreError::Network {
            url: url.to_owned(),
            message: error.to_string(),
        })?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc::{self, Receiver},
        thread,
    };

    use tiny_http::{Response, Server};

    use super::*;

    fn server(responses: Vec<&'static str>) -> (String, Receiver<String>) {
        serve(responses.into_iter().map(|body| (200, body)).collect())
    }

    fn serve(responses: Vec<(u16, &'static str)>) -> (String, Receiver<String>) {
        let server = Server::http("127.0.0.1:0").unwrap();
        let address = format!("http://{}", server.server_addr());
        let (request_tx, request_rx) = mpsc::channel();

        thread::spawn(move || {
            for (status, body) in responses {
                let Ok(request) = server.recv() else {
                    break;
                };
                let _ = request_tx.send(format!("{} {}", request.method(), request.url()));
                let _ = request.respond(
                    Response::from_string(body)
                        .with_status_code(status)
                        .with_header(
                            "Content-Type: application/json"
                                .parse::<tiny_http::Header>()
                                .unwrap(),
                        ),
                );
            }
        });

        (address, request_rx)
    }

    #[test]
    fn clears_source_cookies() {
        let (address, requests) = server(vec![r#"{"ok":true,"cleared":2}"#]);
        let client = HostClient::new(address);

        let cleared = client.clear_source_cookies("7").unwrap();
        assert_eq!(cleared, 2);
        let request = requests.recv().unwrap();
        assert!(request.starts_with("POST "));
        assert!(request.ends_with("/api/v1/sources/7/cookies/clear"));
    }

    #[test]
    fn reads_and_saves_source_preferences() {
        let (address, requests) = server(vec![
            r#"{"sourceId":"7","name":"Demo","configurable":true,"prefs":[{"key":"nsfw","title":"Show NSFW","type":"switch","value":"true"}]}"#,
            r#"{"ok":true}"#,
        ]);
        let client = HostClient::new(address);

        let prefs = client.source_preferences("7").unwrap();
        assert!(prefs.configurable);
        assert_eq!(prefs.prefs[0].title, "Show NSFW");
        assert!(prefs.prefs[0].switch_value());

        let mut values = BTreeMap::new();
        values.insert("nsfw".to_owned(), SourcePreferenceValue::Bool(false));
        client.set_source_preferences("7", &values).unwrap();

        assert!(requests
            .recv()
            .unwrap()
            .ends_with("/api/v1/sources/7/prefs"));
        let request = requests.recv().unwrap();
        assert!(request.starts_with("POST "));
        assert!(request.ends_with("/api/v1/sources/7/prefs"));
    }

    #[test]
    fn reads_manga_sources() {
        let (address, requests) = server(vec![
            r#"{"manga":[{"id":"1","name":"Demo","lang":"en","type":"manga","supportsLatest":false},{"id":"7","name":"MangaDex","lang":"en","type":"manga","pkg":"eu.kanade.tachiyomi.extension.en.mangadex"}],"anime":[]}"#,
        ]);
        let client = HostClient::new(address);

        let sources = client.sources().unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "7");
        assert_eq!(sources[0].display_name(), "MangaDex (EN)");
        // Older hosts omit supportsLatest; default to true.
        assert!(sources[0].supports_latest);
        assert!(requests.recv().unwrap().ends_with("/api/v1/sources"));
    }

    #[test]
    fn repairs_stale_favorite_cover_urls() {
        let (address, _requests) = server(vec![
            r#"[{"sourceId":"7","url":"/manga/1","title":"Berserk","type":"manga","addedAt":1,"thumbnailUrl":"http://127.0.0.1:1/api/v1/proxy?sourceId=7&url=https%3A%2F%2Fcdn.example%2Fcover.jpg"}]"#,
        ]);
        let client = HostClient::new(&address);

        let favorites = client.favorites().unwrap();

        let cover = favorites[0].thumbnail_url.as_deref().unwrap();
        assert!(cover.starts_with(&address));
        assert!(!cover.contains("127.0.0.1:1"));
        assert!(cover.contains("url=https%3A%2F%2Fcdn.example%2Fcover.jpg"));
    }

    #[test]
    fn reads_filters_and_searches_with_them() {
        let (address, requests) = server(vec![
            r#"{"filters":[{"type":"checkbox","name":"Completed","checked":true},{"type":"group","name":"More","children":[{"type":"text","name":"Author","text":""}]}]}"#,
            r#"{"hasNextPage":false,"mangas":[{"url":"/manga/1","title":"Berserk","status":1}]}"#,
        ]);
        let client = HostClient::new(address);

        let filters = client.filters("7").unwrap();
        assert_eq!(filters.len(), 2);
        assert!(matches!(
            filters[0],
            SourceFilter::Checkbox { checked: true, .. }
        ));

        let page = client.search_filtered("7", "", 1, &filters).unwrap();
        assert_eq!(page.mangas.len(), 1);
        assert_eq!(page.mangas[0].title, "Berserk");

        assert!(requests
            .recv()
            .unwrap()
            .ends_with("/api/v1/manga/7/filters"));
        let request = requests.recv().unwrap();
        assert!(request.starts_with("POST "));
        assert!(request.ends_with("/api/v1/manga/7/search"));
    }

    #[test]
    fn searches_and_maps_manga_fields() {
        let (address, requests) = server(vec![
            r#"{"hasNextPage":true,"mangas":[{"url":"/manga/1","title":"Berserk","author":"Miura","artist":"","description":"Dark","genre":"Action, Drama","status":1,"thumbnail_url":"https://cdn.example/cover.jpg","initialized":true}]}"#,
        ]);
        let client = HostClient::new(address);

        let page = client.search("7", "berserk", 2).unwrap();

        assert!(page.has_next_page);
        assert_eq!(page.mangas[0].title, "Berserk");
        assert_eq!(page.mangas[0].genre, vec!["Action", "Drama"]);
        assert_eq!(page.mangas[0].status, "ongoing");
        assert!(page.mangas[0].artist.is_none());
        assert!(page.mangas[0]
            .thumbnail_url
            .as_deref()
            .unwrap()
            .contains("/api/v1/proxy?sourceId=7"));
        let request = requests.recv().unwrap();
        assert!(request.contains("/api/v1/manga/7/search?query=berserk&page=2"));
    }

    #[test]
    fn lists_chapters_and_proxies_pages() {
        let (address, requests) = server(vec![
            r#"{"chapters":[{"url":"/ch/1","name":"Chapter 1","date_upload":123,"chapter_number":1.0,"scanlator":"Team","memo":"{\"mangaSlug\":\"x\"}"}]}"#,
            r#"{"pages":[{"index":0,"number":0,"url":"/p/1","imageUrl":"https://img.example/1.jpg"},{"index":1,"number":1,"url":"https://img.example/2.jpg","imageUrl":null},{"index":2,"number":2,"url":"","imageUrl":"https://img.example/3.jpg"}]}"#,
        ]);
        let client = HostClient::new(address);

        let chapters = client.chapters("7", "/manga/1", None).unwrap();
        let _ = requests.recv();
        assert_eq!(chapters[0].name, "Chapter 1");
        assert_eq!(chapters[0].scanlator.as_deref(), Some("Team"));
        assert_eq!(chapters[0].memo.as_deref(), Some("{\"mangaSlug\":\"x\"}"));

        let pages = client
            .pages(
                "7",
                "/ch/1",
                chapters[0].memo.as_deref(),
                Some(&chapters[0].name),
            )
            .unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages[0].contains("/api/v1/image?sourceId=7"));
        assert!(pages[0].contains("imageUrl=https%3A%2F%2Fimg.example%2F1.jpg"));
        assert!(pages[1].contains("imageUrl="));
        assert!(pages[2].contains("imageUrl=https%3A%2F%2Fimg.example%2F3.jpg"));
        let page_request = requests.recv().unwrap();
        assert!(page_request.contains("memo=%7B%22mangaSlug%22%3A%22x%22%7D"));
        assert!(page_request.contains("name=Chapter%201"));
    }

    #[test]
    fn reads_installed_and_repo_extensions() {
        let (address, requests) = server(vec![
            r#"{"repo":"installed","total":1,"installed":1,"extensions":[{"name":"MangaDex","pkg":"en.mangadex","lang":"en","version":"1.5.0","apk":"x.apk","installed":true,"nsfw":false,"sources":[{"name":"MangaDex","lang":"en"}]}]}"#,
            r#"{"repo":"https://example.org/index.pb","total":1,"installed":0,"extensions":[{"name":"Comick","pkg":"en.comick","lang":"en","version":"1.2.0","apk":"https://example.org/x.apk","installed":false,"nsfw":false,"sources":[]}]}"#,
        ]);
        let client = HostClient::new(address);

        let installed = client.installed_extensions().unwrap();
        assert!(installed[0].installed);
        assert_eq!(installed[0].source_count, 1);

        let repo = client
            .repo_extensions("https://example.org/index.pb")
            .unwrap();
        assert_eq!(repo[0].pkg, "en.comick");
        assert_eq!(repo[0].repo_url, "https://example.org/index.pb");
        assert!(requests.recv().unwrap().contains("extensions/installed"));
    }

    #[test]
    fn installs_and_uninstalls_extensions() {
        let (address, requests) = server(vec![
            r#"{"ok":true,"name":"Comick","pkg":"en.comick","manga":1,"anime":0}"#,
            r#"{"ok":true,"name":"Comick","pkg":"en.comick"}"#,
        ]);
        let client = HostClient::new(address);

        let installed = client.install_extension("https://repo", "x.apk").unwrap();
        assert_eq!(installed.sources, 1);
        client.uninstall_extension("en.comick").unwrap();

        assert!(requests
            .recv()
            .unwrap()
            .starts_with("POST /api/v1/extensions/install"));
    }

    #[test]
    fn reads_not_loaded_extensions_and_jobs() {
        let (address, requests) = server(vec![
            r#"{"extensions":[{"pkg":"en.bad","name":"Bad Extension","file":"bad.apk","reason":"failed","message":"manifest missing","hasUpdate":false}]}"#,
            r#"{"jobId":"install-1","state":"downloading","progress":0.2}"#,
            r#"{"jobId":"install-1","state":"done","progress":1.0,"name":"Comick","pkg":"en.comick","manga":1,"anime":0}"#,
            r#"{"jobId":"install-1","state":"cancelled","progress":0.0}"#,
            r#"{"ok":true}"#,
        ]);
        let client = HostClient::new(address);

        let broken = client.not_loaded_extensions().unwrap();
        assert_eq!(broken[0].pkg, "en.bad");
        assert_eq!(broken[0].reason, "failed");

        let job = client
            .install_extension_async("https://repo", "comick.apk")
            .unwrap();
        assert_eq!(job.job_id, "install-1");
        assert!(!job.finished());

        let done = client.install_job("install-1").unwrap();
        assert!(done.finished());
        assert_eq!(done.sources(), 1);

        let cancelled = client.cancel_install_job("install-1").unwrap();
        assert_eq!(cancelled.state, "cancelled");

        client.delete_not_loaded("bad.apk").unwrap();
        let mut paths = Vec::new();
        for _ in 0..5 {
            paths.push(requests.recv().unwrap());
        }
        assert!(paths
            .iter()
            .any(|path| path.contains("extensions/install?async=true")));
        assert!(paths
            .iter()
            .any(|path| path.contains("jobs/install-1/cancel")));
        assert!(paths.iter().any(|path| path.contains("not-loaded/delete")));
    }

    #[test]
    fn surfaces_host_errors() {
        let (address, _requests) = serve(vec![(
            500,
            r#"{"error":"Cloudflare challenge","challengeUrl":"https://site.example"}"#,
        )]);
        let client = HostClient::new(address);

        let error = client.popular("7", 1).unwrap_err().to_string();

        assert!(error.contains("Cloudflare challenge"));
        assert!(error.contains("FlareSolverr"));
    }

    #[test]
    fn progress_keys_are_stable_and_source_scoped() {
        let first = progress_key("7", "/manga/1", "/ch/1");
        let again = progress_key("7", "/manga/1", "/ch/1");
        let other = progress_key("8", "/manga/1", "/ch/1");

        assert_eq!(first, again);
        assert_ne!(first, other);
        assert!(first.to_string_lossy().starts_with("remote://7/"));
    }
}
