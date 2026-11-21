use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{HostClient, SourcePreferences};

pub(crate) struct PrefsRequest {
    pub generation: u64,
    pub source_id: String,
    pub client: HostClient,
}

pub(crate) struct PrefsEvent {
    pub generation: u64,
    pub source_id: String,
    pub result: Result<SourcePreferences, String>,
}

/// Loads configurable-source preferences off the UI thread.
pub(crate) struct PrefsWorker {
    requests: Sender<PrefsRequest>,
    results: Receiver<PrefsEvent>,
}

impl PrefsWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<PrefsRequest>();
        let (result_tx, result_rx) = mpsc::channel::<PrefsEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result = request
                    .client
                    .source_preferences(&request.source_id)
                    .map_err(|error| error.to_string());
                let _ = result_tx.send(PrefsEvent {
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

    pub fn request(&self, request: PrefsRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<PrefsEvent> {
        self.results.try_recv().ok()
    }
}
