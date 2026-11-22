use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{
    best_match, deep_queries, latest_chapter_number, load_cached_story, HostClient, MigrationMatch,
    RemoteChapter, RemoteFavorite, RemoteManga, RemoteSource,
};

pub(crate) struct MigrationRequest {
    pub generation: u64,
    pub client: HostClient,
    pub favorites: Vec<RemoteFavorite>,
    pub targets: Vec<RemoteSource>,
    pub prioritize_chapters: bool,
}

pub(crate) struct MigrationEvent {
    pub generation: u64,
    pub matches: Vec<MigrationMatch>,
}

/// Matches saved manga against enabled sources off the UI thread.
pub(crate) struct MigrationWorker {
    requests: Sender<MigrationRequest>,
    results: Receiver<MigrationEvent>,
}

impl MigrationWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<MigrationRequest>();
        let (result_tx, result_rx) = mpsc::channel::<MigrationEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let mut matches = Vec::new();
                for favorite in request.favorites {
                    let old_chapters = load_cached_story(&favorite.source_id, &favorite.url)
                        .map(|story| story.chapters)
                        .or_else(|| {
                            request
                                .client
                                .chapters(&favorite.source_id, &favorite.url, None)
                                .ok()
                        })
                        .unwrap_or_default();
                    let mut best: Option<(RemoteSource, RemoteManga, f64)> = None;
                    let mut candidates: Vec<(RemoteSource, RemoteManga, f64)> = Vec::new();
                    for source in &request.targets {
                        if source.id == favorite.source_id {
                            continue;
                        }
                        let mut source_best: Option<(RemoteManga, f64)> = None;
                        let mut queries = vec![favorite.title.clone()];
                        queries.extend(deep_queries(&favorite.title));
                        queries.dedup();
                        for query in queries {
                            let Ok(page) = request.client.search(&source.id, &query, 1) else {
                                continue;
                            };
                            let candidates: Vec<(usize, String)> = page
                                .mangas
                                .iter()
                                .enumerate()
                                .map(|(index, manga)| (index, manga.title.clone()))
                                .collect();
                            if let Some((index, score)) = best_match(&favorite.title, candidates) {
                                let candidate = page.mangas[index].clone();
                                if source_best.as_ref().is_none_or(|(_, known)| score > *known) {
                                    source_best = Some((candidate, score));
                                }
                                if score >= 0.95 {
                                    break;
                                }
                            }
                        }
                        if let Some((manga, score)) = source_best {
                            let candidate = (source.clone(), manga, score);
                            if request.prioritize_chapters {
                                candidates.push(candidate.clone());
                            }
                            if best.as_ref().is_none_or(|(_, _, known)| score > *known) {
                                best = Some(candidate);
                            }
                        }
                    }

                    let mut matched = MigrationMatch {
                        old: favorite.clone(),
                        old_chapters,
                        target: None,
                        target_source: None,
                        target_chapters: Vec::new(),
                        score: 0.0,
                        error: Some("No match found".to_owned()),
                        skipped: false,
                    };
                    if request.prioritize_chapters {
                        // Mihon's prioritize-by-chapters mode searches every target
                        // source and keeps the candidate with the newest chapter.
                        let mut chosen: Option<(
                            RemoteSource,
                            RemoteManga,
                            f64,
                            Vec<RemoteChapter>,
                        )> = None;
                        let mut last_error = None;
                        for (source, target, score) in candidates {
                            match request.client.chapters(
                                &source.id,
                                &target.url,
                                target.memo.as_deref(),
                            ) {
                                Ok(chapters) if !chapters.is_empty() => {
                                    let newer = chosen.as_ref().is_none_or(|(_, _, _, known)| {
                                        latest_chapter_number(&chapters)
                                            > latest_chapter_number(known)
                                    });
                                    if newer {
                                        chosen = Some((source, target, score, chapters));
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => last_error = Some(error.to_string()),
                            }
                        }
                        match chosen {
                            Some((source, target, score, chapters)) => {
                                matched.target = Some(target);
                                matched.target_source = Some(source);
                                matched.target_chapters = chapters;
                                matched.score = score;
                                matched.error = None;
                            }
                            None => {
                                if let Some(error) = last_error {
                                    matched.error = Some(error);
                                }
                            }
                        }
                    } else if let Some((source, target, score)) = best {
                        match request.client.chapters(
                            &source.id,
                            &target.url,
                            target.memo.as_deref(),
                        ) {
                            Ok(chapters) if !chapters.is_empty() => {
                                matched.target = Some(target);
                                matched.target_source = Some(source);
                                matched.target_chapters = chapters;
                                matched.score = score;
                                matched.error = None;
                            }
                            Ok(_) => matched.error = Some("Target has no chapters".to_owned()),
                            Err(error) => matched.error = Some(error.to_string()),
                        }
                    }
                    matches.push(matched);
                }
                let _ = result_tx.send(MigrationEvent {
                    generation: request.generation,
                    matches,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: MigrationRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<MigrationEvent> {
        self.results.try_recv().ok()
    }
}
