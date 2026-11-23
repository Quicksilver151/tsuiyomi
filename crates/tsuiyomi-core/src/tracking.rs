//! Minimal AniList tracker client.
//!
//! AniList uses a single GraphQL endpoint. Search is public; progress updates need a
//! user-provided OAuth token. The token is stored in the config file, so this is a
//! deliberate opt-in and never obtained through an embedded client secret.

use serde_json::{json, Value};

use crate::error::CoreError;

const ENDPOINT: &str = "https://graphql.anilist.co";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerMedia {
    /// `anilist` or `mal`.
    pub service: String,
    pub id: u64,
    pub title: String,
    pub chapters: Option<u32>,
}

pub fn search_anilist(query: &str) -> Result<Vec<TrackerMedia>, CoreError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let body = json!({
        "query": "query($search: String) { Page(perPage: 10) { media(search: $search, type: MANGA, sort: SEARCH_MATCH) { id title { romaji english native } chapters } } }",
        "variables": { "search": query },
    });
    let value = post(body, None)?;
    parse_search(&value)
}

pub fn update_anilist_progress(
    token: &str,
    media_id: u64,
    progress: u32,
    total: Option<u32>,
) -> Result<(), CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: ENDPOINT.to_owned(),
            message: "an AniList token is required for progress updates".to_owned(),
        });
    }
    let completed = total.is_some_and(|total| total > 0 && progress >= total);
    let status = if completed { "COMPLETED" } else { "CURRENT" };
    let body = json!({
        "query": "mutation($id: Int, $progress: Int, $status: MediaListStatus) { SaveMediaListEntry(mediaId: $id, progress: $progress, status: $status) { id progress status } }",
        "variables": { "id": media_id, "progress": progress, "status": status },
    });
    let value = post(body, Some(token))?;
    if value.get("errors").is_some() {
        return Err(CoreError::Network {
            url: ENDPOINT.to_owned(),
            message: graphql_error(&value),
        });
    }
    Ok(())
}

pub fn search_mal(client_id: &str, query: &str) -> Result<Vec<TrackerMedia>, CoreError> {
    let client_id = client_id.trim();
    let query = query.trim();
    if client_id.is_empty() || query.is_empty() {
        return Ok(Vec::new());
    }
    let url = "https://api.myanimelist.net/v2/manga";
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let response = agent
        .get(url)
        .set("X-MAL-CLIENT-ID", client_id)
        .query("q", query)
        .query("limit", "10")
        .query("fields", "id,title,num_chapters")
        .call()
        .map_err(|error| CoreError::Network {
            url: url.to_owned(),
            message: error.to_string(),
        })?;
    let value: Value = response.into_json().map_err(|error| CoreError::Network {
        url: url.to_owned(),
        message: error.to_string(),
    })?;
    Ok(value
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let node = item.get("node")?;
                    let id = node.get("id")?.as_u64()?;
                    let title = node.get("title")?.as_str()?.to_owned();
                    let chapters = node
                        .get("num_chapters")
                        .and_then(Value::as_u64)
                        .and_then(|chapters| u32::try_from(chapters).ok());
                    Some(TrackerMedia {
                        service: "mal".to_owned(),
                        id,
                        title,
                        chapters,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn update_mal_progress(
    token: &str,
    media_id: u64,
    progress: u32,
    total: Option<u32>,
) -> Result<(), CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: "https://api.myanimelist.net".to_owned(),
            message: "a MyAnimeList token is required for progress updates".to_owned(),
        });
    }
    let completed = total.is_some_and(|total| total > 0 && progress >= total);
    let status = if completed { "completed" } else { "reading" };
    let url = format!("https://api.myanimelist.net/v2/manga/{media_id}/my_list_status");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    agent
        .put(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .send_form(&[
            ("status", status),
            ("num_chapters_read", &progress.to_string()),
        ])
        .map_err(|error| CoreError::Network {
            url,
            message: error.to_string(),
        })?;
    Ok(())
}

fn post(body: Value, token: Option<&str>) -> Result<Value, CoreError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut request = agent.post(ENDPOINT).set("Content-Type", "application/json");
    if let Some(token) = token.filter(|token| !token.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = request
        .send_json(body)
        .map_err(|error| CoreError::Network {
            url: ENDPOINT.to_owned(),
            message: error.to_string(),
        })?;
    let value: Value = response.into_json().map_err(|error| CoreError::Network {
        url: ENDPOINT.to_owned(),
        message: error.to_string(),
    })?;
    if value.get("errors").is_some() {
        return Err(CoreError::Network {
            url: ENDPOINT.to_owned(),
            message: graphql_error(&value),
        });
    }
    Ok(value)
}

fn graphql_error(value: &Value) -> String {
    value
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(|error| error.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "AniList request failed".to_owned())
}

fn parse_search(value: &Value) -> Result<Vec<TrackerMedia>, CoreError> {
    let media = value
        .pointer("/data/Page/media")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(media
        .into_iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_u64()?;
            let title = item
                .pointer("/title/english")
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
                .or_else(|| item.pointer("/title/romaji").and_then(Value::as_str))
                .or_else(|| item.pointer("/title/native").and_then(Value::as_str))?
                .to_owned();
            let chapters = item
                .get("chapters")
                .and_then(Value::as_u64)
                .and_then(|chapters| u32::try_from(chapters).ok());
            Some(TrackerMedia {
                service: "anilist".to_owned(),
                id,
                title,
                chapters,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_anilist_search_results() {
        let value = json!({
            "data": { "Page": { "media": [
                { "id": 7, "title": { "romaji": "Berserk", "english": "Berserk" }, "chapters": 380 },
                { "id": 9, "title": { "romaji": "No English" }, "chapters": null }
            ] } }
        });
        let results = parse_search(&value).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 7);
        assert_eq!(results[0].service, "anilist");
        assert_eq!(results[0].title, "Berserk");
        assert_eq!(results[0].chapters, Some(380));
        assert_eq!(results[1].title, "No English");
    }

    #[test]
    fn reports_graphql_errors() {
        let value = json!({ "errors": [{ "message": "Invalid token" }] });
        assert_eq!(graphql_error(&value), "Invalid token");
    }
}
