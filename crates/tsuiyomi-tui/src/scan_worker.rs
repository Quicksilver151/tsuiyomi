use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{scan_library, Library};

pub(crate) struct ScanRequest {
    pub generation: u64,
    pub root: PathBuf,
}

pub(crate) struct ScanEvent {
    pub generation: u64,
    pub result: Result<Library, String>,
}

/// Scans the local library off the UI thread.
pub(crate) struct ScanWorker {
    requests: Sender<ScanRequest>,
    results: Receiver<ScanEvent>,
}

impl ScanWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ScanRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ScanEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result = scan_library(&request.root).map_err(|error| error.to_string());
                let _ = result_tx.send(ScanEvent {
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

    pub fn request(&self, request: ScanRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<ScanEvent> {
        self.results.try_recv().ok()
    }
}
