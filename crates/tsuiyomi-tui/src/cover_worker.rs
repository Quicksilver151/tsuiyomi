use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::HostClient;

pub(crate) struct CoverRequest {
    pub source_id: String,
    pub url: String,
    pub memo: Option<String>,
    pub client: HostClient,
}

pub(crate) struct CoverResult {
    pub source_id: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
}

/// Fetches a missing thumbnail from a source's details page in the background.
pub(crate) struct CoverWorker {
    requests: Sender<CoverRequest>,
    results: Receiver<CoverResult>,
}

impl CoverWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<CoverRequest>();
        let (result_tx, result_rx) = mpsc::channel::<CoverResult>();

        thread::spawn(move || {
            for request in request_rx {
                let thumbnail_url = request
                    .client
                    .details(&request.source_id, &request.url, request.memo.as_deref())
                    .ok()
                    .and_then(|manga| manga.thumbnail_url);
                let _ = result_tx.send(CoverResult {
                    source_id: request.source_id,
                    url: request.url,
                    thumbnail_url,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: CoverRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<CoverResult> {
        self.results.try_recv().ok()
    }
}
