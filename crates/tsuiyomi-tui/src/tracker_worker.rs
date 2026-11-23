use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{
    search_anilist, search_mal, tracker_login, tracker_refresh, update_anilist_progress,
    update_mal_progress, TrackerAuth, TrackerMedia, TrackerService,
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
    },
    Update {
        service: String,
        token: String,
        media_id: u64,
        progress: u32,
        total: Option<u32>,
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
                        TrackerEvent::Search {
                            generation,
                            result: results.map_err(|error| error.to_string()),
                        }
                    }
                    TrackerRequest::Update {
                        service,
                        token,
                        media_id,
                        progress,
                        total,
                    } => {
                        let result = if service == "mal" {
                            update_mal_progress(&token, media_id, progress, total)
                        } else {
                            update_anilist_progress(&token, media_id, progress, total)
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
