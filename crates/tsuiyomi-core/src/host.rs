//! Client for the bundled extension host.
//!
//! Mihon/Tachiyomi extensions are JVM bytecode. tsuiyomi does not load APKs itself; a
//! headless JVM service ([miwayomi](https://github.com/miwayomi/miwayomi)) loads them and
//! exposes sources, manga, chapters, pages, and extension management over a small REST
//! API. This module is the Rust side of that contract and contains no UI code.

use std::{io::Read, path::PathBuf, time::Duration};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::CoreError;

const TIMEOUT: Duration = Duration::from_secs(30);

/// A source exposed by the host: one per installed extension, or one per source in a
/// multi-source extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteSource {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub pkg: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
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
        "remote://{}/{}/{}",
        source_id,
        fnv1a(manga_url),
        fnv1a(chapter_url)
    ))
}

fn fnv1a(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

fn encode(value: &str) -> String {
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
}

impl ExtensionRow {
    fn into_info(self, repo_url: &str) -> ExtensionInfo {
        let sources: Vec<ExtensionSource> = self
            .sources
            .into_iter()
            .map(|source| ExtensionSource {
                name: source.name,
                lang: source.lang,
            })
            .collect();
        ExtensionInfo {
            name: self.name,
            pkg: self.pkg,
            lang: self.lang,
            version: self.version,
            apk: self.apk,
            installed: self.installed,
            nsfw: self.nsfw,
            source_count: sources.len(),
            sources,
            repo_url: repo_url.to_owned(),
        }
    }
}

/// Read an arbitrary URL into memory. Used for cover thumbnails and remote pages.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>, CoreError> {
    let response = ureq::get(url).call().map_err(|error| CoreError::Network {
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
    fn reads_manga_sources() {
        let (address, requests) = server(vec![
            r#"{"manga":[{"id":"1","name":"Demo","lang":"en","type":"manga"},{"id":"7","name":"MangaDex","lang":"en","type":"manga","pkg":"eu.kanade.tachiyomi.extension.en.mangadex"}],"anime":[]}"#,
        ]);
        let client = HostClient::new(address);

        let sources = client.sources().unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "7");
        assert_eq!(sources[0].display_name(), "MangaDex (EN)");
        assert!(requests.recv().unwrap().ends_with("/api/v1/sources"));
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
