use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use tsuiyomi_core::{ensure_runtime, host_data_dir, HostProcess};

pub(crate) struct HostRequest {
    pub host_jar: Option<PathBuf>,
    pub flaresolverr_url: Option<String>,
    pub epoch: u64,
}

pub(crate) struct HostResult {
    pub epoch: u64,
    pub result: Result<HostProcess, String>,
}

/// Starts the bundled JVM host off the UI thread.
///
/// Opening Browse used to freeze until the JVM answered `/health` (a couple of
/// seconds). The Browse screen now renders immediately and this worker delivers the
/// process when it is ready.
pub(crate) struct HostWorker {
    requests: Sender<HostRequest>,
    results: Receiver<HostResult>,
}

impl HostWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<HostRequest>();
        let (result_tx, result_rx) = mpsc::channel::<HostResult>();

        thread::spawn(move || {
            for request in request_rx {
                let result = (|| {
                    let runtime = ensure_runtime(request.host_jar.as_deref())
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| {
                            "No extension host found. Put miwayomi.jar in the runtime cache \
                             or set host_jar in the config."
                                .to_owned()
                        })?;
                    let data_dir = host_data_dir()
                        .ok_or_else(|| "Cannot resolve the host data directory.".to_owned())?;
                    HostProcess::start(&runtime, &data_dir, request.flaresolverr_url.as_deref())
                        .map_err(|error| error.to_string())
                })();
                let _ = result_tx.send(HostResult {
                    epoch: request.epoch,
                    result,
                });
            }
        });

        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, request: HostRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<HostResult> {
        self.results.try_recv().ok()
    }
}
