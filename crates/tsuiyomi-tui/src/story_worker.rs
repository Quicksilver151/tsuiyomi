use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{chapter::natural_cmp, HostClient, RemoteChapter, RemoteManga};

/// Refresh cached details and chapters without blocking the UI.
pub(crate) struct StoryRequest {
    pub key: String,
    pub manga: RemoteManga,
    pub client: HostClient,
}

pub(crate) struct StoryResult {
    pub key: String,
    pub manga: RemoteManga,
    pub chapters: Vec<RemoteChapter>,
    pub error: Option<String>,
}

pub(crate) struct StoryWorker {
    requests: Sender<StoryRequest>,
    results: Receiver<StoryResult>,
}

impl StoryWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<StoryRequest>();
        let (result_tx, result_rx) = mpsc::channel::<StoryResult>();

        thread::spawn(move || {
            for request in request_rx {
                let mut manga = request.manga;
                if let Ok(details) =
                    request
                        .client
                        .details(&manga.source_id, &manga.url, manga.memo.as_deref())
                {
                    manga.author = details.author;
                    manga.artist = details.artist;
                    manga.description = details.description;
                    manga.genre = details.genre;
                    manga.status = details.status;
                    manga.memo = details.memo;
                    if manga.thumbnail_url.is_none() {
                        manga.thumbnail_url = details.thumbnail_url;
                    }
                }

                let mut chapters = match request.client.chapters(
                    &manga.source_id,
                    &manga.url,
                    manga.memo.as_deref(),
                ) {
                    Ok(chapters) => chapters,
                    Err(error) => {
                        let _ = result_tx.send(StoryResult {
                            key: request.key,
                            manga,
                            chapters: Vec::new(),
                            error: Some(error.to_string()),
                        });
                        continue;
                    }
                };
                chapters.sort_by(|left, right| {
                    right
                        .chapter_number
                        .partial_cmp(&left.chapter_number)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| natural_cmp(&right.name, &left.name))
                });
                let _ = result_tx.send(StoryResult {
                    key: request.key,
                    manga,
                    chapters,
                    error: None,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: StoryRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<StoryResult> {
        self.results.try_recv().ok()
    }
}
