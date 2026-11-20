use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
};

use tsuiyomi_core::{HostClient, RemoteManga, RemoteSource};

pub(crate) enum GlobalSearchEvent {
    Group {
        generation: u64,
        source: RemoteSource,
        mangas: Vec<RemoteManga>,
    },
    Error {
        generation: u64,
        source: RemoteSource,
        message: String,
    },
    Finished {
        generation: u64,
    },
}

/// Runs source searches concurrently and streams each source's outcome back to the UI.
///
/// A new request bumps the generation and stops old workers between sources, so typing a
/// new query cannot let stale results overtake the current search.
pub(crate) struct GlobalSearchWorker {
    generation: Arc<AtomicU64>,
    results: Sender<GlobalSearchEvent>,
    receiver: Receiver<GlobalSearchEvent>,
}

impl GlobalSearchWorker {
    pub fn spawn() -> Self {
        let (results, receiver) = mpsc::channel();
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            results,
            receiver,
        }
    }

    pub fn request(
        &self,
        generation: u64,
        query: String,
        sources: Vec<RemoteSource>,
        client: HostClient,
    ) {
        self.generation.store(generation, Ordering::SeqCst);
        let current_generation = Arc::clone(&self.generation);
        let results = self.results.clone();

        thread::spawn(move || {
            let queue = Arc::new(Mutex::new(sources.into_iter().collect::<Vec<_>>()));
            let workers = queue.lock().map(|queue| queue.len()).unwrap_or(0).min(5);
            thread::scope(|scope| {
                for _ in 0..workers {
                    let queue = Arc::clone(&queue);
                    let client = client.clone();
                    let query = query.clone();
                    let results = results.clone();
                    let current_generation = Arc::clone(&current_generation);
                    scope.spawn(move || loop {
                        if current_generation.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        let source = queue.lock().ok().and_then(|mut queue| queue.pop());
                        let Some(source) = source else {
                            return;
                        };
                        let event = match client.search(&source.id, &query, 1) {
                            Ok(page) => GlobalSearchEvent::Group {
                                generation,
                                source,
                                mangas: page.mangas,
                            },
                            Err(error) => GlobalSearchEvent::Error {
                                generation,
                                source,
                                message: error.to_string(),
                            },
                        };
                        let _ = results.send(event);
                    });
                }
            });
            let _ = results.send(GlobalSearchEvent::Finished { generation });
        });
    }

    pub fn try_event(&self) -> Option<GlobalSearchEvent> {
        self.receiver.try_recv().ok()
    }
}
