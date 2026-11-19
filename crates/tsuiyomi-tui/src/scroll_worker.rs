use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

use ratatui_image::{picker::Picker, sliced::SlicedProtocol};
use tsuiyomi_core::{page_dimensions, Page};

use crate::app::{build_scroll_protocol, ScrollPageLayout};

pub(crate) struct BuildRequest {
    pub generation: u64,
    pub index: usize,
    pub page: Page,
    pub layout: ScrollPageLayout,
    pub picker: Picker,
}

pub(crate) struct BuildResult {
    pub generation: u64,
    pub index: usize,
    pub result: Result<SlicedProtocol, String>,
}

pub(crate) struct ScrollWorker {
    requests: Sender<BuildRequest>,
    results: Receiver<BuildResult>,
    generation: Arc<AtomicU64>,
}

impl ScrollWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<BuildRequest>();
        let (result_tx, result_rx) = mpsc::channel::<BuildResult>();
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&generation);

        thread::spawn(move || {
            for request in request_rx {
                if request.generation != worker_generation.load(Ordering::Relaxed) {
                    continue;
                }
                let result = build_scroll_protocol(&request.picker, &request.page, request.layout);
                if request.generation == worker_generation.load(Ordering::Relaxed) {
                    let _ = result_tx.send(BuildResult {
                        generation: request.generation,
                        index: request.index,
                        result,
                    });
                }
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
            generation,
        }
    }

    pub fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Relaxed);
    }

    pub fn request(&self, request: BuildRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<BuildResult> {
        self.results.try_recv().ok()
    }
}

pub(crate) struct SizeRequest {
    pub generation: u64,
    pub chapter: PathBuf,
    pub pages: Vec<Page>,
}

pub(crate) struct SizeResult {
    pub generation: u64,
    pub chapter: PathBuf,
    pub sizes: Vec<(u32, u32)>,
}

/// Measures page dimensions off the UI thread. Scroll mode needs every page's size
/// before it can lay the chapter out, and remote pages can take seconds to inspect.
pub(crate) struct SizeWorker {
    requests: Sender<SizeRequest>,
    results: Receiver<SizeResult>,
}

impl SizeWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<SizeRequest>();
        let (result_tx, result_rx) = mpsc::channel::<SizeResult>();

        thread::spawn(move || {
            for request in request_rx {
                let mut sizes = Vec::with_capacity(request.pages.len());
                for chunk in request.pages.chunks(8) {
                    let batch: Vec<Option<(u32, u32)>> = std::thread::scope(|scope| {
                        let handles: Vec<_> = chunk
                            .iter()
                            .map(|page| scope.spawn(move || page_dimensions(page).ok()))
                            .collect();
                        handles
                            .into_iter()
                            .map(|handle| handle.join().unwrap_or(None))
                            .collect()
                    });
                    for size in batch {
                        let size = size
                            .or_else(|| sizes.last().copied())
                            .unwrap_or((800, 1200));
                        sizes.push(size);
                    }
                }
                let _ = result_tx.send(SizeResult {
                    generation: request.generation,
                    chapter: request.chapter,
                    sizes,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: SizeRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<SizeResult> {
        self.results.try_recv().ok()
    }
}
