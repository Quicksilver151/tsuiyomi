use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{HostClient, SourceFilter};

pub(crate) struct FiltersRequest {
    pub generation: u64,
    pub source_id: String,
    pub client: HostClient,
}

pub(crate) struct FiltersEvent {
    pub generation: u64,
    pub source_id: String,
    pub result: Result<Vec<SourceFilter>, String>,
}

/// Loads configurable source filters off the UI thread.
pub(crate) struct FiltersWorker {
    requests: Sender<FiltersRequest>,
    results: Receiver<FiltersEvent>,
}

impl FiltersWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<FiltersRequest>();
        let (result_tx, result_rx) = mpsc::channel::<FiltersEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result = request
                    .client
                    .filters(&request.source_id)
                    .map_err(|error| error.to_string());
                let _ = result_tx.send(FiltersEvent {
                    generation: request.generation,
                    source_id: request.source_id,
                    result,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: FiltersRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<FiltersEvent> {
        self.results.try_recv().ok()
    }
}
