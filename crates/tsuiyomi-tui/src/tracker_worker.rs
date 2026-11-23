use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{
    login_bangumi, login_mangaupdates, login_shikimori, search_anilist, search_bangumi, search_mal,
    search_mangaupdates, search_shikimori, tracker_login, tracker_refresh, update_anilist_entry,
    update_bangumi_entry, update_mal_entry, update_mangaupdates_entry, update_shikimori_entry,
    TrackerAuth, TrackerEntry, TrackerMedia, TrackerService, TrackerStatus,
};

pub(crate) enum TrackerRequest {
    Login {
        service: String,
        client_id: String,
        client_secret: Option<String>,
    },
    Refresh {
        service: String,
        client_id: String,
        client_secret: Option<String>,
        refresh_token: String,
    },
    Search {
        generation: u64,
        query: String,
        mal_client_id: Option<String>,
        mangaupdates: bool,
        shikimori: bool,
        bangumi: bool,
    },
    MangaUpdatesLogin {
        username: String,
        password: String,
    },
    ShikimoriLogin {
        token: String,
    },
    BangumiLogin {
        token: String,
    },
    Update {
        service: String,
        token: String,
        media_id: u64,
        progress: u32,
        total: Option<u32>,
        status: Option<String>,
        score: Option<f32>,
        start_date: Option<String>,
        finish_date: Option<String>,
        private: Option<bool>,
        account_id: Option<String>,
    },
}

pub(crate) enum TrackerEvent {
    Auth {
        service: String,
        result: Result<TrackerAuth, String>,
    },
    Search {
        generation: u64,
        result: Result<Vec<TrackerMedia>, String>,
    },
    Update {
        result: Result<(), String>,
        media_id: u64,
        progress: u32,
    },
}

pub(crate) struct TrackerWorker {
    requests: Sender<TrackerRequest>,
    results: Receiver<TrackerEvent>,
}

impl TrackerWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<TrackerRequest>();
        let (result_tx, result_rx) = mpsc::channel::<TrackerEvent>();
        thread::spawn(move || {
            for request in request_rx {
                let event = match request {
                    TrackerRequest::Login {
                        service,
                        client_id,
                        client_secret,
                    } => {
                        let result = TrackerService::parse(&service)
                            .ok_or_else(|| format!("unknown tracker service: {service}"))
                            .and_then(|service| {
                                tracker_login(service, &client_id, client_secret.as_deref())
                                    .map_err(|error| error.to_string())
                            });
                        TrackerEvent::Auth { service, result }
                    }
                    TrackerRequest::Refresh {
                        service,
                        client_id,
                        client_secret,
                        refresh_token,
                    } => {
                        let result = TrackerService::parse(&service)
                            .ok_or_else(|| format!("unknown tracker service: {service}"))
                            .and_then(|service| {
                                tracker_refresh(
                                    service,
                                    &client_id,
                                    client_secret.as_deref(),
                                    &refresh_token,
                                )
                                .map_err(|error| error.to_string())
                            });
                        TrackerEvent::Auth { service, result }
                    }
                    TrackerRequest::Search {
                        generation,
                        query,
                        mal_client_id,
                        mangaupdates,
                        shikimori,
                        bangumi,
                    } => {
                        let mut results = search_anilist(&query);
                        if let Some(client_id) = mal_client_id.filter(|id| !id.trim().is_empty()) {
                            match search_mal(&client_id, &query) {
                                Ok(mut mal) => match results.as_mut() {
                                    Ok(anilist) => anilist.append(&mut mal),
                                    Err(_) => results = Ok(mal),
                                },
                                Err(error) => {
                                    if results.is_err() {
                                        results = Err(error);
                                    }
                                }
                            }
                        }
                        if mangaupdates {
                            match search_mangaupdates(&query) {
                                Ok(mut found) => match results.as_mut() {
                                    Ok(all) => all.append(&mut found),
                                    Err(_) => results = Ok(found),
                                },
                                Err(error) => {
                                    if results.is_err() {
                                        results = Err(error);
                                    }
                                }
                            }
                        }
                        if shikimori {
                            match search_shikimori(&query) {
                                Ok(mut found) => match results.as_mut() {
                                    Ok(all) => all.append(&mut found),
                                    Err(_) => results = Ok(found),
                                },
                                Err(error) => {
                                    if results.is_err() {
                                        results = Err(error);
                                    }
                                }
                            }
                        }
                        if bangumi {
                            match search_bangumi(&query) {
                                Ok(mut found) => match results.as_mut() {
                                    Ok(all) => all.append(&mut found),
                                    Err(_) => results = Ok(found),
                                },
                                Err(error) => {
                                    if results.is_err() {
                                        results = Err(error);
                                    }
                                }
                            }
                        }
                        TrackerEvent::Search {
                            generation,
                            result: results.map_err(|error| error.to_string()),
                        }
                    }
                    TrackerRequest::MangaUpdatesLogin { username, password } => {
                        let result = login_mangaupdates(&username, &password)
                            .map_err(|error| error.to_string());
                        TrackerEvent::Auth {
                            service: "mangaupdates".to_owned(),
                            result,
                        }
                    }
                    TrackerRequest::ShikimoriLogin { token } => {
                        let result = login_shikimori(&token).map_err(|error| error.to_string());
                        TrackerEvent::Auth {
                            service: "shikimori".to_owned(),
                            result,
                        }
                    }
                    TrackerRequest::BangumiLogin { token } => {
                        let result = login_bangumi(&token).map_err(|error| error.to_string());
                        TrackerEvent::Auth {
                            service: "bangumi".to_owned(),
                            result,
                        }
                    }
                    TrackerRequest::Update {
                        service,
                        token,
                        media_id,
                        progress,
                        total,
                        status,
                        score,
                        start_date,
                        finish_date,
                        private,
                        account_id,
                    } => {
                        let entry = TrackerEntry {
                            progress,
                            total,
                            status: status.as_deref().and_then(TrackerStatus::parse),
                            score,
                            start_date: start_date.as_deref(),
                            finish_date: finish_date.as_deref(),
                            private,
                        };
                        let result = match service.as_str() {
                            "mal" => update_mal_entry(&token, media_id, &entry),
                            "mangaupdates" => update_mangaupdates_entry(&token, media_id, &entry),
                            "shikimori" => update_shikimori_entry(
                                &token,
                                account_id.as_deref().unwrap_or_default(),
                                media_id,
                                &entry,
                            ),
                            "bangumi" => update_bangumi_entry(&token, media_id, &entry),
                            _ => update_anilist_entry(&token, media_id, &entry),
                        };
                        TrackerEvent::Update {
                            result: result.map_err(|error| error.to_string()),
                            media_id,
                            progress,
                        }
                    }
                };
                let _ = result_tx.send(event);
            }
        });
        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: TrackerRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<TrackerEvent> {
        self.results.try_recv().ok()
    }
}
