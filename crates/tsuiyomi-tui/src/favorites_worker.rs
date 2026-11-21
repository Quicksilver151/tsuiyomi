use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{HostClient, RemoteFavorite};

pub(crate) struct FavoritesRequest {
    pub generation: u64,
    pub client: HostClient,
}

pub(crate) struct FavoritesEvent {
    pub generation: u64,
    pub result: Result<Vec<RemoteFavorite>, String>,
}

/// Loads the host favorites table off the UI thread.
pub(crate) struct FavoritesWorker {
    requests: Sender<FavoritesRequest>,
    results: Receiver<FavoritesEvent>,
}

impl FavoritesWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<FavoritesRequest>();
        let (result_tx, result_rx) = mpsc::channel::<FavoritesEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result = request
                    .client
                    .favorites()
                    .map_err(|error| error.to_string());
                let _ = result_tx.send(FavoritesEvent {
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

    pub fn request(&self, request: FavoritesRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<FavoritesEvent> {
        self.results.try_recv().ok()
    }
}
