use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{HostClient, RemoteFavorite, RemoteManga};

pub(crate) enum FavoritesMutation {
    Add(RemoteManga),
    AddMany(Vec<RemoteManga>),
    Remove { source_id: String, url: String },
    RemoveMany(Vec<(String, String)>),
}

pub(crate) struct FavoritesRequest {
    pub generation: u64,
    pub client: HostClient,
    pub mutation: Option<FavoritesMutation>,
    /// Status to show after a successful mutation.
    pub label: Option<String>,
    /// Newly added manga, for pending category and cache cleanup.
    pub added: Vec<RemoteManga>,
}

pub(crate) struct FavoritesEvent {
    pub generation: u64,
    pub result: Result<Vec<RemoteFavorite>, String>,
    pub label: Option<String>,
    pub added: Vec<RemoteManga>,
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
                let mutation_error = match request.mutation {
                    None => None,
                    Some(FavoritesMutation::Add(manga)) => request
                        .client
                        .add_favorite(&manga)
                        .err()
                        .map(|error| error.to_string()),
                    Some(FavoritesMutation::AddMany(mangas)) => mangas.iter().find_map(|manga| {
                        request
                            .client
                            .add_favorite(manga)
                            .err()
                            .map(|error| error.to_string())
                    }),
                    Some(FavoritesMutation::Remove { source_id, url }) => request
                        .client
                        .remove_favorite(&source_id, &url)
                        .err()
                        .map(|error| error.to_string()),
                    Some(FavoritesMutation::RemoveMany(pairs)) => {
                        pairs.iter().find_map(|(source_id, url)| {
                            request
                                .client
                                .remove_favorite(source_id, url)
                                .err()
                                .map(|error| error.to_string())
                        })
                    }
                };
                let result = match mutation_error {
                    Some(error) => Err(error),
                    None => request
                        .client
                        .favorites()
                        .map_err(|error| error.to_string()),
                };
                let _ = result_tx.send(FavoritesEvent {
                    generation: request.generation,
                    result,
                    label: request.label,
                    added: request.added,
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
