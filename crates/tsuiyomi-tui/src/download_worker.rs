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
    pub key: String,
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
    pub key: String,
    pub label: String,
    pub result: Result<PathBuf, String>,
}

/// Downloads chapters off the UI thread so reading and scrolling stay responsive.
///
/// Each request runs on its own thread; the UI queue bounds how many are active.
pub(crate) struct DownloadWorker {
    results: Receiver<DownloadResult>,
    _result_tx: Sender<DownloadResult>,
}

impl DownloadWorker {
    pub fn spawn() -> Self {
        let (result_tx, result_rx) = mpsc::channel::<DownloadResult>();
        Self {
            results: result_rx,
            _result_tx: result_tx,
        }
    }

    pub fn request(&self, request: DownloadRequest) {
        let sender = self._result_tx.clone();
        thread::spawn(move || {
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
            let _ = sender.send(DownloadResult {
                key: request.key,
                label: request.label,
                result,
            });
        });
    }

    pub fn try_result(&self) -> Option<DownloadResult> {
        self.results.try_recv().ok()
    }
}
