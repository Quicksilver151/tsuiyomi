use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

use ratatui_image::{picker::Picker, sliced::SlicedProtocol};
use tsuiyomi_core::Page;

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
