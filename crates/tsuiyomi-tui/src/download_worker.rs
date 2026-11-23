use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

use tsuiyomi_core::{download_chapter_with_cancel, ChapterDownload, Page};

pub(crate) struct DownloadRequest {
    pub label: String,
    pub root: PathBuf,
    pub source_name: String,
    pub series_title: String,
    pub chapter_name: String,
    pub scanlator: Option<String>,
    pub pages: Vec<Page>,
    pub as_cbz: bool,
    pub cancel: Arc<AtomicBool>,
}

pub(crate) struct DownloadResult {
    pub label: String,
    pub result: Result<PathBuf, String>,
}

/// Downloads chapters off the UI thread so reading and scrolling stay responsive.
pub(crate) struct DownloadWorker {
    requests: Sender<DownloadRequest>,
    results: Receiver<DownloadResult>,
}

impl DownloadWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<DownloadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<DownloadResult>();

        thread::spawn(move || {
            for request in request_rx {
                let result = download_chapter_with_cancel(
                    ChapterDownload {
                        root: &request.root,
                        source_name: &request.source_name,
                        series_title: &request.series_title,
                        chapter_name: &request.chapter_name,
                        scanlator: request.scanlator.as_deref(),
                        pages: &request.pages,
                        as_cbz: request.as_cbz,
                    },
                    || request.cancel.load(Ordering::Relaxed),
                )
                .map_err(|error| error.to_string());
                let _ = result_tx.send(DownloadResult {
                    label: request.label,
                    result,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: DownloadRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<DownloadResult> {
        self.results.try_recv().ok()
    }
}
