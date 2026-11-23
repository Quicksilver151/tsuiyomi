//! Minimal AniList tracker client.
//!
//! AniList uses a single GraphQL endpoint. Search is public; progress updates need a
//! user-provided OAuth token. The token is stored in the config file, so this is a
//! deliberate opt-in and never obtained through an embedded client secret.

use serde_json::{json, Value};

use crate::error::CoreError;

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
    fn reports_graphql_errors() {
        let value = json!({ "errors": [{ "message": "Invalid token" }] });
        assert_eq!(graphql_error(&value), "Invalid token");
    }
}
