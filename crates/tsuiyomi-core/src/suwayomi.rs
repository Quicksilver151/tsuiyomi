//! Suwayomi-Server GraphQL client.
//!
//! Suwayomi is the JVM side that runs Mihon/Tachiyomi extension APKs through
//! AndroidCompat and dex2jar. tsuiyomi never loads APK bytecode itself; it asks the
//! local server for sources, search results, chapters, pages, and extension state.

use std::{io::Read, time::Duration};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::CoreError;

const QUERY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteSource {
    pub id: i64,
    pub name: String,
    pub lang: String,
    pub display_name: String,
    pub supports_latest: bool,
    pub icon_url: Option<String>,
    pub content_warning: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteManga {
    pub id: i32,
    pub source_id: i64,
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genre: Vec<String>,
    pub status: String,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteMangaPage {
    pub mangas: Vec<RemoteManga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteChapter {
    pub id: i32,
    pub name: String,
    pub chapter_number: f32,
    pub scanlator: Option<String>,
    pub source_order: i32,
    pub page_count: i32,
    pub upload_date: i64,
    pub is_read: bool,
    pub last_page_read: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionStore {
    pub name: String,
    pub badge_label: String,
    pub signing_key: String,
    pub contact_website: String,
    pub index_url: String,
    pub is_legacy: bool,
    pub extension_list_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub pkg_name: String,
    pub version_name: String,
    pub version_code: i64,
    pub lang: String,
    pub is_installed: bool,
    pub has_update: bool,
    pub is_obsolete: bool,
    pub content_warning: String,
    pub icon_url: Option<String>,
    pub store_index_url: Option<String>,
}

#[derive(Clone)]
pub struct SuwayomiClient {
    base_url: String,
    agent: ureq::Agent,
}

impl std::fmt::Debug for SuwayomiClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SuwayomiClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Fetch bytes from a remote source/image URL.
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

impl SuwayomiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        Self {
            base_url,
            agent: ureq::AgentBuilder::new().timeout(QUERY_TIMEOUT).build(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn absolute_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{}/{}", self.base_url, path.trim_start_matches('/'))
        }
    }

    pub fn sources(&self) -> Result<Vec<RemoteSource>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            sources: Nodes<SourceNode>,
        }
        #[derive(Deserialize)]
        struct SourceNode {
            id: i64,
            name: String,
            lang: String,
            #[serde(rename = "displayName")]
            display_name: String,
            #[serde(rename = "supportsLatest")]
            supports_latest: bool,
            #[serde(rename = "iconUrl")]
            icon_url: String,
            #[serde(rename = "contentWarning")]
            content_warning: String,
        }

        let data: Data = self.graphql(
            r#"query Sources {
                sources {
                    nodes {
                        id
                        name
                        lang
                        displayName
                        supportsLatest
                        iconUrl
                        contentWarning
                    }
                }
            }"#,
            json!({}),
        )?;

        Ok(data
            .sources
            .nodes
            .into_iter()
            .map(|source| RemoteSource {
                id: source.id,
                name: source.name,
                lang: source.lang,
                display_name: source.display_name,
                supports_latest: source.supports_latest,
                icon_url: (!source.icon_url.is_empty())
                    .then(|| self.absolute_url(&source.icon_url)),
                content_warning: source.content_warning,
            })
            .collect())
    }

    pub fn search(
        &self,
        source_id: i64,
        query: &str,
        page: i32,
    ) -> Result<RemoteMangaPage, CoreError> {
        self.fetch_source_manga(source_id, "SEARCH", page, Some(query))
    }

    pub fn popular(&self, source_id: i64, page: i32) -> Result<RemoteMangaPage, CoreError> {
        self.fetch_source_manga(source_id, "POPULAR", page, None)
    }

    pub fn latest(&self, source_id: i64, page: i32) -> Result<RemoteMangaPage, CoreError> {
        self.fetch_source_manga(source_id, "LATEST", page, None)
    }

    fn fetch_source_manga(
        &self,
        source_id: i64,
        fetch_type: &str,
        page: i32,
        query: Option<&str>,
    ) -> Result<RemoteMangaPage, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "fetchSourceManga")]
            payload: Payload,
        }
        #[derive(Deserialize)]
        struct Payload {
            #[serde(rename = "hasNextPage")]
            has_next_page: bool,
            mangas: Vec<MangaNode>,
        }
        #[derive(Deserialize)]
        struct MangaNode {
            id: i32,
            #[serde(rename = "sourceId")]
            source_id: i64,
            title: String,
            author: Option<String>,
            artist: Option<String>,
            description: Option<String>,
            #[serde(default)]
            genre: Vec<String>,
            status: String,
            #[serde(rename = "thumbnailUrl")]
            thumbnail_url: Option<String>,
        }

        let data: Data = self.graphql(
            r#"mutation FetchSourceManga($input: FetchSourceMangaInput!) {
                fetchSourceManga(input: $input) {
                    hasNextPage
                    mangas {
                        id
                        sourceId
                        title
                        author
                        artist
                        description
                        genre
                        status
                        thumbnailUrl
                    }
                }
            }"#,
            json!({
                "input": {
                    "source": source_id,
                    "type": fetch_type,
                    "page": page,
                    "query": query.unwrap_or(""),
                }
            }),
        )?;

        Ok(RemoteMangaPage {
            has_next_page: data.payload.has_next_page,
            mangas: data
                .payload
                .mangas
                .into_iter()
                .map(|manga| RemoteManga {
                    id: manga.id,
                    source_id: manga.source_id,
                    title: manga.title,
                    author: manga.author,
                    artist: manga.artist,
                    description: manga.description,
                    genre: manga.genre,
                    status: manga.status,
                    thumbnail_url: manga
                        .thumbnail_url
                        .filter(|url| !url.is_empty())
                        .map(|url| self.absolute_url(&url)),
                })
                .collect(),
        })
    }

    pub fn chapters(&self, manga_id: i32) -> Result<Vec<RemoteChapter>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            chapters: Nodes<ChapterNode>,
        }
        #[derive(Deserialize)]
        struct ChapterNode {
            id: i32,
            name: String,
            #[serde(rename = "chapterNumber")]
            chapter_number: f32,
            scanlator: Option<String>,
            #[serde(rename = "sourceOrder")]
            source_order: i32,
            #[serde(rename = "pageCount")]
            page_count: i32,
            #[serde(rename = "uploadDate")]
            upload_date: i64,
            user: ChapterUser,
        }
        #[derive(Deserialize)]
        struct ChapterUser {
            #[serde(rename = "isRead")]
            is_read: bool,
            #[serde(rename = "lastPageRead")]
            last_page_read: i32,
        }

        let data: Data = self.graphql(
            r#"query Chapters($mangaId: Int!) {
                chapters(condition: { mangaId: $mangaId }) {
                    nodes {
                        id
                        name
                        chapterNumber
                        scanlator
                        sourceOrder
                        pageCount
                        uploadDate
                        user {
                            isRead
                            lastPageRead
                        }
                    }
                }
            }"#,
            json!({ "mangaId": manga_id }),
        )?;

        Ok(data
            .chapters
            .nodes
            .into_iter()
            .map(|chapter| RemoteChapter {
                id: chapter.id,
                name: chapter.name,
                chapter_number: chapter.chapter_number,
                scanlator: chapter.scanlator.filter(|value| !value.is_empty()),
                source_order: chapter.source_order,
                page_count: chapter.page_count,
                upload_date: chapter.upload_date,
                is_read: chapter.user.is_read,
                last_page_read: chapter.user.last_page_read,
            })
            .collect())
    }

    pub fn chapter_pages(&self, chapter_id: i32) -> Result<Vec<String>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "fetchChapterPages")]
            payload: Payload,
        }
        #[derive(Deserialize)]
        struct Payload {
            pages: Vec<String>,
        }

        let data: Data = self.graphql(
            r#"mutation FetchChapterPages($input: FetchChapterPagesInput!) {
                fetchChapterPages(input: $input) {
                    pages
                }
            }"#,
            json!({ "input": { "chapterId": chapter_id } }),
        )?;

        Ok(data
            .payload
            .pages
            .into_iter()
            .map(|page| self.absolute_url(&page))
            .collect())
    }

    pub fn extension_stores(&self) -> Result<Vec<ExtensionStore>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "extensionStores")]
            stores: Nodes<StoreNode>,
        }
        #[derive(Deserialize)]
        struct StoreNode {
            name: String,
            #[serde(rename = "badgeLabel")]
            badge_label: String,
            #[serde(rename = "signingKey")]
            signing_key: String,
            #[serde(rename = "contactWebsite")]
            contact_website: String,
            #[serde(rename = "indexUrl")]
            index_url: String,
            #[serde(rename = "isLegacy")]
            is_legacy: bool,
            #[serde(rename = "extensionListUrl")]
            extension_list_url: Option<String>,
        }

        let data: Data = self.graphql(
            r#"query ExtensionStores {
                extensionStores {
                    nodes {
                        name
                        badgeLabel
                        signingKey
                        contactWebsite
                        indexUrl
                        isLegacy
                        extensionListUrl
                    }
                }
            }"#,
            json!({}),
        )?;

        Ok(data
            .stores
            .nodes
            .into_iter()
            .map(|store| ExtensionStore {
                name: store.name,
                badge_label: store.badge_label,
                signing_key: store.signing_key,
                contact_website: store.contact_website,
                index_url: store.index_url,
                is_legacy: store.is_legacy,
                extension_list_url: store.extension_list_url,
            })
            .collect())
    }

    pub fn extensions(&self) -> Result<Vec<ExtensionInfo>, CoreError> {
        #[derive(Deserialize)]
        struct Data {
            extensions: Nodes<ExtensionNode>,
        }
        #[derive(Deserialize)]
        struct ExtensionNode {
            name: String,
            #[serde(rename = "pkgName")]
            pkg_name: String,
            #[serde(rename = "versionName")]
            version_name: String,
            #[serde(rename = "versionCodeLong")]
            version_code: i64,
            lang: String,
            #[serde(rename = "isInstalled")]
            is_installed: bool,
            #[serde(rename = "hasUpdate")]
            has_update: bool,
            #[serde(rename = "isObsolete")]
            is_obsolete: bool,
            #[serde(rename = "contentWarning")]
            content_warning: String,
            #[serde(rename = "iconUrl")]
            icon_url: String,
            #[serde(rename = "storeIndexUrl")]
            store_index_url: Option<String>,
        }

        let data: Data = self.graphql(
            r#"query Extensions {
                extensions {
                    nodes {
                        name
                        pkgName
                        versionName
                        versionCodeLong
                        lang
                        isInstalled
                        hasUpdate
                        isObsolete
                        contentWarning
                        iconUrl
                        storeIndexUrl
                    }
                }
            }"#,
            json!({}),
        )?;

        Ok(data
            .extensions
            .nodes
            .into_iter()
            .map(|extension| ExtensionInfo {
                name: extension.name,
                pkg_name: extension.pkg_name,
                version_name: extension.version_name,
                version_code: extension.version_code,
                lang: extension.lang,
                is_installed: extension.is_installed,
                has_update: extension.has_update,
                is_obsolete: extension.is_obsolete,
                content_warning: extension.content_warning,
                icon_url: (!extension.icon_url.is_empty())
                    .then(|| self.absolute_url(&extension.icon_url)),
                store_index_url: extension.store_index_url,
            })
            .collect())
    }

    pub fn add_extension_store(&self, index_url: &str) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "addExtensionStore")]
            _payload: Value,
        }
        let _: Data = self.graphql(
            r#"mutation AddExtensionStore($input: AddExtensionStoreInput!) {
                addExtensionStore(input: $input) {
                    extensionStore { indexUrl }
                }
            }"#,
            json!({ "input": { "indexUrl": index_url } }),
        )?;
        Ok(())
    }

    pub fn remove_extension_store(&self, index_url: &str) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "removeExtensionStore")]
            _payload: Value,
        }
        let _: Data = self.graphql(
            r#"mutation RemoveExtensionStore($input: RemoveExtensionStoreInput!) {
                removeExtensionStore(input: $input) {
                    extensionStore { indexUrl }
                }
            }"#,
            json!({ "input": { "indexUrl": index_url } }),
        )?;
        Ok(())
    }

    pub fn update_extension(
        &self,
        pkg_name: &str,
        install: bool,
        update: bool,
        uninstall: bool,
    ) -> Result<(), CoreError> {
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "updateExtension")]
            _payload: Value,
        }
        let _: Data = self.graphql(
            r#"mutation UpdateExtension($input: UpdateExtensionInput!) {
                updateExtension(input: $input) {
                    extension { pkgName }
                }
            }"#,
            json!({
                "input": {
                    "id": pkg_name,
                    "patch": {
                        "install": install,
                        "update": update,
                        "uninstall": uninstall,
                    }
                }
            }),
        )?;
        Ok(())
    }

    fn graphql<T: DeserializeOwned>(&self, query: &str, variables: Value) -> Result<T, CoreError> {
        let url = format!("{}/api/graphql", self.base_url);
        let response = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(json!({ "query": query, "variables": variables }))
            .map_err(|error| CoreError::Network {
                url: url.clone(),
                message: error.to_string(),
            })?;
        let response: GraphQlResponse<T> =
            response.into_json().map_err(|error| CoreError::Network {
                url: url.clone(),
                message: error.to_string(),
            })?;
        if let Some(errors) = response.errors {
            if !errors.is_empty() {
                return Err(CoreError::Network {
                    url,
                    message: errors
                        .into_iter()
                        .map(|error| error.message)
                        .collect::<Vec<_>>()
                        .join("; "),
                });
            }
        }
        response.data.ok_or_else(|| CoreError::Network {
            url,
            message: "GraphQL response contained no data".to_owned(),
        })
    }
}

#[derive(Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
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
        let server = Server::http("127.0.0.1:0").unwrap();
        let address = format!("http://{}", server.server_addr());
        let (body_tx, body_rx) = mpsc::channel();

        thread::spawn(move || {
            for response in responses {
                let Ok(mut request) = server.recv() else {
                    break;
                };
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let _ = body_tx.send(body);
                let _ = request.respond(
                    Response::from_string(response).with_header(
                        "Content-Type: application/json"
                            .parse::<tiny_http::Header>()
                            .unwrap(),
                    ),
                );
            }
        });

        (address, body_rx)
    }

    #[test]
    fn reads_sources_and_resolves_icons() {
        let (address, bodies) = server(vec![
            r#"{"data":{"sources":{"nodes":[{"id":7,"name":"MangaDex","lang":"en","displayName":"MangaDex","supportsLatest":true,"iconUrl":"/api/v1/extension/icon/x","contentWarning":"SAFE"}]}}}"#,
        ]);
        let client = SuwayomiClient::new(address);

        let sources = client.sources().unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, 7);
        assert!(sources[0]
            .icon_url
            .as_deref()
            .unwrap()
            .ends_with("/api/v1/extension/icon/x"));
        assert!(bodies.recv().unwrap().contains("Sources"));
    }

    #[test]
    fn searches_and_lists_chapters() {
        let (address, bodies) = server(vec![
            r#"{"data":{"fetchSourceManga":{"hasNextPage":false,"mangas":[{"id":3,"sourceId":7,"title":"Berserk","author":"Miura","artist":null,"description":"Dark","genre":["Action"],"status":"ONGOING","thumbnailUrl":"/api/v1/manga/3/thumbnail"}]}}}"#,
            r#"{"data":{"chapters":{"nodes":[{"id":11,"name":"Chapter 1","chapterNumber":1.0,"scanlator":null,"sourceOrder":0,"pageCount":20,"uploadDate":123,"user":{"isRead":false,"lastPageRead":0}}]}}}"#,
        ]);
        let client = SuwayomiClient::new(address);

        let page = client.search(7, "berserk", 1).unwrap();
        assert_eq!(page.mangas[0].title, "Berserk");
        assert!(page.mangas[0]
            .thumbnail_url
            .as_deref()
            .unwrap()
            .ends_with("/api/v1/manga/3/thumbnail"));

        let chapters = client.chapters(3).unwrap();
        assert_eq!(chapters[0].id, 11);
        assert_eq!(chapters[0].page_count, 20);
        assert!(bodies.recv().unwrap().contains("SEARCH"));
    }

    #[test]
    fn fetches_remote_page_urls() {
        let (address, _bodies) = server(vec![
            r#"{"data":{"fetchChapterPages":{"pages":["/api/v1/manga/3/chapter/0/page/0","/api/v1/manga/3/chapter/0/page/1"]}}}"#,
        ]);
        let client = SuwayomiClient::new(address);

        let pages = client.chapter_pages(11).unwrap();

        assert_eq!(pages.len(), 2);
        assert!(pages[0].starts_with("http://127.0.0.1:"));
        assert!(pages[0].ends_with("/api/v1/manga/3/chapter/0/page/0"));
    }

    #[test]
    fn reads_extension_stores_and_extensions() {
        let (address, bodies) = server(vec![
            r#"{"data":{"extensionStores":{"nodes":[{"name":"Keiyoushi","badgeLabel":"Kei","signingKey":"abc","contactWebsite":"https://example.org","indexUrl":"https://example.org/index.pb","isLegacy":false,"extensionListUrl":null}]}}}"#,
            r#"{"data":{"extensions":{"nodes":[{"name":"MangaDex","pkgName":"eu.kanade.tachiyomi.extension.en.mangadex","versionName":"1.5.0","versionCodeLong":15,"lang":"en","isInstalled":false,"hasUpdate":false,"isObsolete":false,"contentWarning":"SAFE","iconUrl":"/api/v1/extension/icon/x","storeIndexUrl":"https://example.org/index.pb"}]}}}"#,
        ]);
        let client = SuwayomiClient::new(address);

        let stores = client.extension_stores().unwrap();
        assert_eq!(stores[0].name, "Keiyoushi");
        let extensions = client.extensions().unwrap();
        assert_eq!(
            extensions[0].pkg_name,
            "eu.kanade.tachiyomi.extension.en.mangadex"
        );
        assert!(!extensions[0].is_installed);
        let _ = bodies;
    }

    #[test]
    fn installs_and_removes_extension_stores() {
        let (address, bodies) = server(vec![
            r#"{"data":{"addExtensionStore":{"extensionStore":{"indexUrl":"https://example.org/index.pb"}}}}"#,
            r#"{"data":{"removeExtensionStore":{"extensionStore":{"indexUrl":"https://example.org/index.pb"}}}}"#,
        ]);
        let client = SuwayomiClient::new(address);

        client
            .add_extension_store("https://example.org/index.pb")
            .unwrap();
        client
            .remove_extension_store("https://example.org/index.pb")
            .unwrap();

        assert!(bodies.recv().unwrap().contains("AddExtensionStore"));
    }

    #[test]
    fn surfaces_graphql_errors() {
        let (address, _bodies) = server(vec![
            r#"{"errors":[{"message":"extension not installed"}],"data":null}"#,
        ]);
        let client = SuwayomiClient::new(address);

        let error = client.sources().unwrap_err();

        assert!(error.to_string().contains("extension not installed"));
    }
}
