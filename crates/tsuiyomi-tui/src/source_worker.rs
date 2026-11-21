use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{HostClient, RemoteSource};

pub(crate) struct SourceRequest {
    pub generation: u64,
    pub client: HostClient,
}

pub(crate) struct SourceEvent {
    pub generation: u64,
    pub result: Result<Vec<RemoteSource>, String>,
}

/// Loads the host source list off the UI thread.
pub(crate) struct SourceWorker {
    requests: Sender<SourceRequest>,
    results: Receiver<SourceEvent>,
}

impl SourceWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<SourceRequest>();
        let (result_tx, result_rx) = mpsc::channel::<SourceEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result = request.client.sources().map_err(|error| error.to_string());
                let _ = result_tx.send(SourceEvent {
                    generation: request.generation,
                    result,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: SourceRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<SourceEvent> {
        self.results.try_recv().ok()
    }
}
