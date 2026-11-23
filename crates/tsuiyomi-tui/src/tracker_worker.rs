use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{search_anilist, update_anilist_progress, AniListMedia};

pub(crate) enum TrackerRequest {
    Search {
        generation: u64,
        query: String,
    },
    Update {
        token: String,
        media_id: u64,
        progress: u32,
        total: Option<u32>,
    },
}

pub(crate) enum TrackerEvent {
    Search {
        generation: u64,
        result: Result<Vec<AniListMedia>, String>,
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
                    TrackerRequest::Search { generation, query } => TrackerEvent::Search {
                        generation,
                        result: search_anilist(&query).map_err(|error| error.to_string()),
                    },
                    TrackerRequest::Update {
                        token,
                        media_id,
                        progress,
                        total,
                    } => TrackerEvent::Update {
                        result: update_anilist_progress(&token, media_id, progress, total)
                            .map_err(|error| error.to_string()),
                        media_id,
                        progress,
                    },
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
