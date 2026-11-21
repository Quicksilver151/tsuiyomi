use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{HostClient, RemoteMangaPage, RemoteSource, SourceFilter};

pub(crate) enum ResultListing {
    Popular,
    Latest,
    Search,
}

pub(crate) struct ResultRequest {
    pub generation: u64,
    pub client: HostClient,
    pub source: RemoteSource,
    pub listing: ResultListing,
    pub query: String,
    pub filters: Vec<SourceFilter>,
    pub page: i32,
    pub append: bool,
}

pub(crate) struct ResultEvent {
    pub generation: u64,
    pub append: bool,
    pub page: i32,
    pub result: Result<RemoteMangaPage, String>,
}

/// Fetches source result pages off the UI thread.
pub(crate) struct ResultWorker {
    requests: Sender<ResultRequest>,
    results: Receiver<ResultEvent>,
}

impl ResultWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ResultRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ResultEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result =
                    match request.listing {
                        ResultListing::Popular => {
                            request.client.popular(&request.source.id, request.page)
                        }
                        ResultListing::Latest => {
                            request.client.latest(&request.source.id, request.page)
                        }
                        ResultListing::Search if request.filters.is_empty() => request
                            .client
                            .search(&request.source.id, &request.query, request.page),
                        ResultListing::Search => request.client.search_filtered(
                            &request.source.id,
                            &request.query,
                            request.page,
                            &request.filters,
                        ),
                    }
                    .map_err(|error| error.to_string());
                let _ = result_tx.send(ResultEvent {
                    generation: request.generation,
                    append: request.append,
                    page: request.page,
                    result,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: ResultRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<ResultEvent> {
        self.results.try_recv().ok()
    }
}
