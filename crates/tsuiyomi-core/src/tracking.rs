//! Minimal AniList tracker client.
//!
//! AniList uses a single GraphQL endpoint. Search is public; progress updates need a
//! user-provided OAuth token. The token is stored in the config file, so this is a
//! deliberate opt-in and never obtained through an embedded client secret.

use serde_json::{json, Value};

use crate::{config::TrackerAuth, error::CoreError};

const ENDPOINT: &str = "https://graphql.anilist.co";

/// Tracker list status in Mihon's portable spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerStatus {
    Reading,
    Completed,
    OnHold,
    Dropped,
    Planning,
}

impl TrackerStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "reading" | "current" => Some(Self::Reading),
            "completed" => Some(Self::Completed),
            "on_hold" | "paused" => Some(Self::OnHold),
            "dropped" => Some(Self::Dropped),
            "planning" | "plan_to_read" => Some(Self::Planning),
            _ => None,
        }
    }

    /// Stable id used in the config file.
    pub fn id(self) -> &'static str {
        match self {
            Self::Reading => "reading",
            Self::Completed => "completed",
            Self::OnHold => "on_hold",
            Self::Dropped => "dropped",
            Self::Planning => "planning",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Reading => "Reading",
            Self::Completed => "Completed",
            Self::OnHold => "On hold",
            Self::Dropped => "Dropped",
            Self::Planning => "Planning",
        }
    }

    fn anilist(self) -> &'static str {
        match self {
            Self::Reading => "CURRENT",
            Self::Completed => "COMPLETED",
            Self::OnHold => "PAUSED",
            Self::Dropped => "DROPPED",
            Self::Planning => "PLANNING",
        }
    }
}

/// Fields sent when creating or updating a tracker list entry.
#[derive(Debug, Clone, Default)]
pub struct TrackerEntry<'a> {
    pub progress: u32,
    pub total: Option<u32>,
    /// Explicit status; when unset, progress decides reading vs completed.
    pub status: Option<TrackerStatus>,
    /// 0–10 score, matching Mihon's UI.
    pub score: Option<f32>,
    /// `YYYY-MM-DD`.
    pub start_date: Option<&'a str>,
    pub finish_date: Option<&'a str>,
    pub private: Option<bool>,
}

impl TrackerEntry<'_> {
    fn effective_status(&self) -> TrackerStatus {
        self.status.unwrap_or_else(|| {
            if self
                .total
                .is_some_and(|total| total > 0 && self.progress >= total)
            {
                TrackerStatus::Completed
            } else {
                TrackerStatus::Reading
            }
        })
    }
}

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
    update_anilist_entry(
        token,
        media_id,
        &TrackerEntry {
            progress,
            total,
            ..TrackerEntry::default()
        },
    )
}

/// Update an AniList list entry with progress, status, score, dates, and privacy.
pub fn update_anilist_entry(
    token: &str,
    media_id: u64,
    entry: &TrackerEntry<'_>,
) -> Result<(), CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: ENDPOINT.to_owned(),
            message: "an AniList token is required for progress updates".to_owned(),
        });
    }
    let (query, variables) = anilist_entry_mutation(media_id, entry);
    let value = post(
        json!({ "query": query, "variables": variables }),
        Some(token),
    )?;
    if value.get("errors").is_some() {
        return Err(CoreError::Network {
            url: ENDPOINT.to_owned(),
            message: graphql_error(&value),
        });
    }
    Ok(())
}

/// Build the GraphQL mutation and variables for one entry. Only metadata present in
/// `entry` is included, so unrelated remote fields are not cleared.
fn anilist_entry_mutation(media_id: u64, entry: &TrackerEntry<'_>) -> (String, Value) {
    let mut declarations = vec!["$id: Int!", "$progress: Int", "$status: MediaListStatus"];
    let mut fields = vec!["progress: $progress", "status: $status"];
    let mut variables = json!({
        "id": media_id,
        "progress": entry.progress,
        "status": entry.effective_status().anilist(),
    });
    if let Some(score) = entry.score {
        declarations.push("$score: Int");
        fields.push("scoreRaw: $score");
        variables["score"] = json!((score.clamp(0.0, 10.0) * 10.0).round() as i64);
    }
    if let Some(date) = entry.start_date.and_then(parse_tracker_date) {
        declarations.push("$startedAt: FuzzyDateInput");
        fields.push("startedAt: $startedAt");
        variables["startedAt"] = date;
    }
    if let Some(date) = entry.finish_date.and_then(parse_tracker_date) {
        declarations.push("$finishedAt: FuzzyDateInput");
        fields.push("finishedAt: $finishedAt");
        variables["finishedAt"] = date;
    }
    if let Some(private) = entry.private {
        declarations.push("$private: Boolean");
        fields.push("private: $private");
        variables["private"] = json!(private);
    }
    (
        format!(
            "mutation({}) {{ SaveMediaListEntry(mediaId: $id, {}) {{ id progress status }} }}",
            declarations.join(", "),
            fields.join(", ")
        ),
        variables,
    )
}

fn parse_tracker_date(value: &str) -> Option<Value> {
    let mut parts = value.trim().split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(json!({ "year": year, "month": month, "day": day }))
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
    Ok(parse_mal_search(&value))
}

fn parse_mal_search(value: &Value) -> Vec<TrackerMedia> {
    value
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
        .unwrap_or_default()
}

pub fn update_mal_progress(
    token: &str,
    media_id: u64,
    progress: u32,
    total: Option<u32>,
) -> Result<(), CoreError> {
    update_mal_entry(
        token,
        media_id,
        &TrackerEntry {
            progress,
            total,
            ..TrackerEntry::default()
        },
    )
}

/// Update a MyAnimeList list entry with progress, status, score, and dates.
pub fn update_mal_entry(
    token: &str,
    media_id: u64,
    entry: &TrackerEntry<'_>,
) -> Result<(), CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: "https://api.myanimelist.net".to_owned(),
            message: "a MyAnimeList token is required for progress updates".to_owned(),
        });
    }
    let url = format!("https://api.myanimelist.net/v2/manga/{media_id}/my_list_status");
    let form = mal_entry_form(entry);
    let form: Vec<(&str, &str)> = form
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    agent
        .put(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .send_form(&form)
        .map_err(|error| CoreError::Network {
            url,
            message: error.to_string(),
        })?;
    Ok(())
}

fn mal_entry_form(entry: &TrackerEntry<'_>) -> Vec<(String, String)> {
    let mut form = vec![
        (
            "status".to_owned(),
            entry.effective_status().id().to_owned(),
        ),
        ("num_chapters_read".to_owned(), entry.progress.to_string()),
    ];
    if let Some(score) = entry.score {
        form.push((
            "score".to_owned(),
            format!("{}", score.clamp(0.0, 10.0).round() as i64),
        ));
    }
    if let Some(date) = entry.start_date {
        form.push(("start_date".to_owned(), date.trim().to_owned()));
    }
    if let Some(date) = entry.finish_date {
        form.push(("finish_date".to_owned(), date.trim().to_owned()));
    }
    form
}

const MANGAUPDATES: &str = "https://api.mangaupdates.com";

/// MangaUpdates list ids: reading, wish, complete, unfinished, on hold.
fn mangaupdates_list_id(status: TrackerStatus) -> i64 {
    match status {
        TrackerStatus::Reading => 0,
        TrackerStatus::Planning => 1,
        TrackerStatus::Completed => 2,
        TrackerStatus::Dropped => 3,
        TrackerStatus::OnHold => 4,
    }
}

/// Search MangaUpdates. The search endpoint does not need a session token.
pub fn search_mangaupdates(query: &str) -> Result<Vec<TrackerMedia>, CoreError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let body = json!({
        "search": query,
        "filter_types": ["drama cd", "novel"],
    });
    let value = mangaupdates_request("POST", "/v1/series/search", Some(&body), None)?;
    Ok(parse_mangaupdates_search(&value))
}

fn parse_mangaupdates_search(value: &Value) -> Vec<TrackerMedia> {
    value
        .get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|item| {
                    let record = item.get("record")?;
                    let id = record.get("series_id")?.as_u64()?;
                    let title = record
                        .get("title")
                        .and_then(Value::as_str)
                        .filter(|title| !title.trim().is_empty())?
                        .to_owned();
                    Some(TrackerMedia {
                        service: "mangaupdates".to_owned(),
                        id,
                        title,
                        chapters: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Log in with a MangaUpdates username and password, returning a session token.
pub fn login_mangaupdates(username: &str, password: &str) -> Result<TrackerAuth, CoreError> {
    let username = username.trim();
    if username.is_empty() || password.is_empty() {
        return Err(CoreError::Network {
            url: MANGAUPDATES.to_owned(),
            message: "a MangaUpdates username and password are required".to_owned(),
        });
    }
    let body = json!({ "username": username, "password": password });
    let value = mangaupdates_request("PUT", "/v1/account/login", Some(&body), None)?;
    let token = value
        .pointer("/context/session_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| CoreError::Network {
            url: MANGAUPDATES.to_owned(),
            message: "MangaUpdates did not return a session token".to_owned(),
        })?;
    Ok(TrackerAuth {
        access_token: token.to_owned(),
        refresh_token: None,
        expires_at: None,
        account_id: None,
    })
}

/// Update a MangaUpdates list entry and rating.
pub fn update_mangaupdates_entry(
    token: &str,
    media_id: u64,
    entry: &TrackerEntry<'_>,
) -> Result<(), CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: MANGAUPDATES.to_owned(),
            message: "a MangaUpdates session token is required".to_owned(),
        });
    }
    let body = json!([{
        "series": { "id": media_id },
        "list_id": mangaupdates_list_id(entry.effective_status()),
        "status": { "chapter": entry.progress },
    }]);
    mangaupdates_request("POST", "/v1/lists/series/update", Some(&body), Some(token))?;

    if let Some(score) = entry.score {
        let path = format!("/v1/series/{media_id}/rating");
        if score > 0.0 {
            mangaupdates_request("PUT", &path, Some(&json!({ "rating": score })), Some(token))?;
        } else {
            mangaupdates_request("DELETE", &path, None, Some(token))?;
        }
    }
    Ok(())
}

fn mangaupdates_request(
    method: &str,
    path: &str,
    body: Option<&Value>,
    token: Option<&str>,
) -> Result<Value, CoreError> {
    let url = format!("{MANGAUPDATES}{path}");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut request = match method {
        "PUT" => agent.put(&url),
        "POST" => agent.post(&url),
        "DELETE" => agent.delete(&url),
        _ => agent.get(&url),
    };
    request = request.set("Accept", "application/json");
    if let Some(token) = token.filter(|token| !token.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = if let Some(body) = body {
        request
            .set("Content-Type", "application/json")
            .send_json(body.clone())
    } else {
        request.call()
    }
    .map_err(|error| CoreError::Network {
        url: url.clone(),
        message: error.to_string(),
    })?;
    match response.into_json::<Value>() {
        Ok(value) => Ok(value),
        Err(_) => Ok(Value::Null),
    }
}

const SHIKIMORI: &str = "https://shikimori.one/api";

fn shikimori_status(status: TrackerStatus) -> &'static str {
    match status {
        TrackerStatus::Reading => "watching",
        TrackerStatus::Completed => "completed",
        TrackerStatus::OnHold => "on_hold",
        TrackerStatus::Dropped => "dropped",
        TrackerStatus::Planning => "planned",
    }
}

/// Search Shikimori's public manga catalog.
pub fn search_shikimori(query: &str) -> Result<Vec<TrackerMedia>, CoreError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let path = format!("/mangas?search={}&limit=10", encode_query(query));
    let value = shikimori_request("GET", &path, None, None)?;
    Ok(parse_shikimori_search(&value))
}

fn parse_shikimori_search(value: &Value) -> Vec<TrackerMedia> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_u64()?;
                    let title = item
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|title| !title.trim().is_empty())
                        .or_else(|| item.get("russian").and_then(Value::as_str))?
                        .to_owned();
                    let chapters = item
                        .get("chapters")
                        .and_then(Value::as_u64)
                        .and_then(|chapters| u32::try_from(chapters).ok());
                    Some(TrackerMedia {
                        service: "shikimori".to_owned(),
                        id,
                        title,
                        chapters,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Verify a pasted Shikimori access token and remember the account id.
pub fn login_shikimori(token: &str) -> Result<TrackerAuth, CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: SHIKIMORI.to_owned(),
            message: "a Shikimori access token is required".to_owned(),
        });
    }
    let value = shikimori_request("GET", "/users/whoami", None, Some(token))?;
    let account_id = value
        .get("id")
        .and_then(Value::as_u64)
        .map(|id| id.to_string())
        .ok_or_else(|| CoreError::Network {
            url: SHIKIMORI.to_owned(),
            message: "Shikimori did not return the current user".to_owned(),
        })?;
    Ok(TrackerAuth {
        access_token: token.to_owned(),
        refresh_token: None,
        expires_at: None,
        account_id: Some(account_id),
    })
}

/// Create or update a Shikimori manga list entry.
pub fn update_shikimori_entry(
    token: &str,
    account_id: &str,
    media_id: u64,
    entry: &TrackerEntry<'_>,
) -> Result<(), CoreError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(CoreError::Network {
            url: SHIKIMORI.to_owned(),
            message: "a Shikimori token is required".to_owned(),
        });
    }
    let path = format!(
        "/v2/user_rates?user_id={}&target_id={media_id}&target_type=Manga",
        encode_query(account_id)
    );
    let rates = shikimori_request("GET", &path, None, Some(token))?;
    let rate_id = rates
        .as_array()
        .and_then(|rates| rates.first())
        .and_then(|rate| rate.get("id"))
        .and_then(Value::as_u64);
    let status = shikimori_status(entry.effective_status());
    let score = entry.score.unwrap_or(0.0).clamp(0.0, 10.0).round() as i64;
    let (method, path, body) = if let Some(rate_id) = rate_id {
        (
            "PUT",
            format!("/v2/user_rates/{rate_id}"),
            json!({
                "user_rate": {
                    "chapters": entry.progress,
                    "score": score,
                    "status": status,
                }
            }),
        )
    } else {
        (
            "POST",
            "/v2/user_rates".to_owned(),
            json!({
                "user_rate": {
                    "user_id": account_id.parse::<u64>().unwrap_or(0),
                    "target_id": media_id,
                    "target_type": "Manga",
                    "chapters": entry.progress,
                    "score": score,
                    "status": status,
                }
            }),
        )
    };
    shikimori_request(method, &path, Some(&body), Some(token))?;
    Ok(())
}

fn shikimori_request(
    method: &str,
    path: &str,
    body: Option<&Value>,
    token: Option<&str>,
) -> Result<Value, CoreError> {
    let url = format!("{SHIKIMORI}{path}");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let mut request = match method {
        "PUT" => agent.put(&url),
        "POST" => agent.post(&url),
        _ => agent.get(&url),
    };
    request = request.set("Accept", "application/json");
    if let Some(token) = token.filter(|token| !token.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = if let Some(body) = body {
        request
            .set("Content-Type", "application/json")
            .send_json(body.clone())
    } else {
        request.call()
    }
    .map_err(|error| CoreError::Network {
        url: url.clone(),
        message: error.to_string(),
    })?;
    match response.into_json::<Value>() {
        Ok(value) => Ok(value),
        Err(_) => Ok(Value::Null),
    }
}

fn encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
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
    fn parses_myanimelist_search_results() {
        let value = json!({
            "data": [
                { "node": { "id": 2, "title": "Berserk", "num_chapters": 380 } },
                { "node": { "id": 4, "title": "No Chapters", "num_chapters": 0 } }
            ]
        });
        let results = parse_mal_search(&value);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].service, "mal");
        assert_eq!(results[0].id, 2);
        assert_eq!(results[0].chapters, Some(380));
    }

    #[test]
    fn anilist_mutation_includes_only_provided_metadata() {
        let entry = TrackerEntry {
            progress: 5,
            total: Some(5),
            score: Some(8.5),
            private: Some(true),
            ..TrackerEntry::default()
        };
        let (query, variables) = anilist_entry_mutation(42, &entry);
        assert!(query.contains("status: $status"));
        assert!(query.contains("scoreRaw: $score"));
        assert!(query.contains("private: $private"));
        assert!(!query.contains("startedAt"));
        assert_eq!(variables["id"], 42);
        assert_eq!(variables["progress"], 5);
        assert_eq!(variables["status"], "COMPLETED");
        assert_eq!(variables["score"], 85);
        assert_eq!(variables["private"], true);
    }

    #[test]
    fn mal_form_maps_status_score_and_dates() {
        let entry = TrackerEntry {
            progress: 3,
            total: Some(20),
            status: Some(TrackerStatus::OnHold),
            score: Some(7.4),
            start_date: Some("2024-02-03"),
            finish_date: None,
            private: None,
        };
        let form = mal_entry_form(&entry);
        assert!(form.contains(&("status".to_owned(), "on_hold".to_owned())));
        assert!(form.contains(&("score".to_owned(), "7".to_owned())));
        assert!(form.contains(&("start_date".to_owned(), "2024-02-03".to_owned())));
        assert!(!form.iter().any(|(key, _)| key == "finish_date"));
    }

    #[test]
    fn parses_dates_and_status_ids() {
        assert_eq!(
            parse_tracker_date("2024-02-03"),
            Some(json!({
                "year": 2024,
                "month": 2,
                "day": 3
            }))
        );
        assert!(parse_tracker_date("2024-13-03").is_none());
        assert_eq!(
            TrackerStatus::parse("on_hold").map(TrackerStatus::id),
            Some("on_hold")
        );
        assert_eq!(TrackerStatus::parse("PAUSED"), Some(TrackerStatus::OnHold));
    }

    #[test]
    fn parses_mangaupdates_search_results() {
        let value = json!({
            "results": [
                { "record": { "series_id": 42, "title": "Berserk" } },
                { "record": { "series_id": 7 } }
            ]
        });
        let results = parse_mangaupdates_search(&value);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 42);
        assert_eq!(results[0].service, "mangaupdates");
    }

    #[test]
    fn maps_mangaupdates_list_ids() {
        assert_eq!(mangaupdates_list_id(TrackerStatus::Reading), 0);
        assert_eq!(mangaupdates_list_id(TrackerStatus::Planning), 1);
        assert_eq!(mangaupdates_list_id(TrackerStatus::Completed), 2);
        assert_eq!(mangaupdates_list_id(TrackerStatus::Dropped), 3);
        assert_eq!(mangaupdates_list_id(TrackerStatus::OnHold), 4);
    }

    #[test]
    fn parses_shikimori_search_results() {
        let value = json!([
            { "id": 5, "name": "Berserk", "russian": "Берсерк", "chapters": 380 },
            { "id": 6, "russian": "Только русский" }
        ]);
        let results = parse_shikimori_search(&value);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 5);
        assert_eq!(results[0].service, "shikimori");
        assert_eq!(results[0].chapters, Some(380));
        assert_eq!(results[1].title, "Только русский");
    }

    #[test]
    fn maps_shikimori_statuses() {
        assert_eq!(shikimori_status(TrackerStatus::Reading), "watching");
        assert_eq!(shikimori_status(TrackerStatus::Completed), "completed");
        assert_eq!(shikimori_status(TrackerStatus::OnHold), "on_hold");
        assert_eq!(shikimori_status(TrackerStatus::Dropped), "dropped");
        assert_eq!(shikimori_status(TrackerStatus::Planning), "planned");
    }

    #[test]
    fn reports_graphql_errors() {
        let value = json!({ "errors": [{ "message": "Invalid token" }] });
        assert_eq!(graphql_error(&value), "Invalid token");
    }
}
