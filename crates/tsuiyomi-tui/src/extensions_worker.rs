use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{ExtensionInfo, HostClient, NotLoadedExtension};

pub(crate) struct ExtensionsRequest {
    pub generation: u64,
    pub client: HostClient,
}

pub(crate) struct ExtensionsSnapshot {
    pub installed: Vec<ExtensionInfo>,
    pub repos: Vec<String>,
    pub entries: Vec<ExtensionInfo>,
    pub not_loaded: Vec<NotLoadedExtension>,
    pub repo_errors: Vec<String>,
}

pub(crate) struct ExtensionsEvent {
    pub generation: u64,
    pub result: Result<ExtensionsSnapshot, String>,
}

/// Loads installed extensions, repository indexes, and not-loaded files off the UI thread.
pub(crate) struct ExtensionsWorker {
    requests: Sender<ExtensionsRequest>,
    results: Receiver<ExtensionsEvent>,
}

impl ExtensionsWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ExtensionsRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ExtensionsEvent>();

        thread::spawn(move || {
            for request in request_rx {
                let result = (|| {
                    let installed = request
                        .client
                        .installed_extensions()
                        .map_err(|error| error.to_string())?;
                    let repos = request.client.repos().map_err(|error| error.to_string())?;
                    let mut entries = Vec::new();
                    let mut repo_errors = Vec::new();
                    for repo in &repos {
                        match request.client.repo_extensions(repo) {
                            Ok(mut available) => entries.append(&mut available),
                            Err(error) => repo_errors.push(error.to_string()),
                        }
                    }
                    let not_loaded = request.client.not_loaded_extensions().unwrap_or_default();
                    Ok(ExtensionsSnapshot {
                        installed,
                        repos,
                        entries,
                        not_loaded,
                        repo_errors,
                    })
                })();
                let _ = result_tx.send(ExtensionsEvent {
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

    pub fn request(&self, request: ExtensionsRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<ExtensionsEvent> {
        self.results.try_recv().ok()
    }
}
