use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

use image::DynamicImage;
use ratatui_image::{picker::Picker, sliced::SlicedProtocol};
use tsuiyomi_core::{page_dimensions, read_page, Page, PageSource};

use crate::app::{build_scroll_block, ScrollPageLayout};

pub(crate) struct BuildRequest {
    pub generation: u64,
    pub index: usize,
    pub page: Page,
    pub layout: ScrollPageLayout,
    pub picker: Picker,
    pub crop_borders: bool,
    pub grayscale: bool,
}

pub(crate) struct BuildResult {
    pub generation: u64,
    pub index: usize,
    pub result: Result<SlicedProtocol, String>,
}

/// Keeps recently decoded pages around.
///
/// A long webtoon strip becomes many scroll blocks that all belong to the same source
/// image. Without this cache every block downloads and decodes the whole strip again,
/// which is both slow and memory-hungry. Requests arrive in reading order, so a couple
/// of entries cover the overlapping page at a block boundary and a quick scroll back.
struct PageCache {
    capacity: usize,
    entries: Vec<(String, Arc<DynamicImage>)>,
}

impl PageCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Vec::new(),
        }
    }

    fn get(
        &mut self,
        page: &Page,
        crop_borders: bool,
        grayscale: bool,
    ) -> Result<Arc<DynamicImage>, String> {
        let key = format!("{crop_borders}:{grayscale}:{}", page_cache_key(page));
        if let Some(position) = self.entries.iter().position(|(entry, _)| entry == &key) {
            // Refresh the entry's position so it is the last one evicted.
            let entry = self.entries.remove(position);
            let image = Arc::clone(&entry.1);
            self.entries.push(entry);
            return Ok(image);
        }

        let bytes = read_page(page).map_err(|error| error.to_string())?;
        let image =
            image::load_from_memory(&bytes).map_err(|error| format!("{}: {error}", page.name))?;
        let image = Arc::new(crate::app::prepare_page_image(
            image,
            crop_borders,
            grayscale,
        ));
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((key, Arc::clone(&image)));
        Ok(image)
    }
}

fn page_cache_key(page: &Page) -> String {
    match &page.source {
        PageSource::File(path) => format!("file:{}", path.display()),
        PageSource::Archive { path, name, kind } => {
            format!("archive:{}:{name}:{kind:?}", path.display())
        }
        PageSource::Remote { url } => format!("remote:{url}"),
    }
}

/// The block range the UI currently cares about. The worker drops queued work
/// outside it so fast scrolling does not build content that is already off-screen.
struct ScrollWindow {
    first: AtomicUsize,
    last: AtomicUsize,
}

pub(crate) struct ScrollWorker {
    requests: Sender<BuildRequest>,
    results: Receiver<BuildResult>,
    generation: Arc<AtomicU64>,
    window: Arc<ScrollWindow>,
}

impl ScrollWorker {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<BuildRequest>();
        let (result_tx, result_rx) = mpsc::channel::<BuildResult>();
        let generation = Arc::new(AtomicU64::new(0));
        let window = Arc::new(ScrollWindow {
            first: AtomicUsize::new(0),
            last: AtomicUsize::new(usize::MAX),
        });
        let worker_generation = Arc::clone(&generation);
        let worker_window = Arc::clone(&window);

        thread::spawn(move || {
            let mut cache = PageCache::new(2);
            for request in request_rx {
                if !in_window(&request, &worker_generation, &worker_window) {
                    continue;
                }
                let Ok(image) = cache.get(&request.page, request.crop_borders, request.grayscale)
                else {
                    // Report the failure only if the request is still relevant.
                    if in_window(&request, &worker_generation, &worker_window) {
                        let _ = result_tx.send(BuildResult {
                            generation: request.generation,
                            index: request.index,
                            result: Err("could not load page".to_owned()),
                        });
                    }
                    continue;
                };
                // The page download above can take seconds; the viewport may have moved
                // on while it ran.
                if !in_window(&request, &worker_generation, &worker_window) {
                    continue;
                }
                let result =
                    build_scroll_block(&request.picker, &image, &request.page.name, request.layout);
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
            window,
        }
    }

    pub fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Relaxed);
    }

    /// Publish the block window the UI is currently interested in.
    pub fn set_window(&self, generation: u64, first: usize, last: usize) {
        self.generation.store(generation, Ordering::Relaxed);
        self.window.first.store(first, Ordering::Relaxed);
        self.window.last.store(last, Ordering::Relaxed);
    }

    pub fn request(&self, request: BuildRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_result(&self) -> Option<BuildResult> {
        self.results.try_recv().ok()
    }
}

fn in_window(request: &BuildRequest, generation: &AtomicU64, window: &ScrollWindow) -> bool {
    request.generation == generation.load(Ordering::Relaxed)
        && request.index >= window.first.load(Ordering::Relaxed)
        && request.index <= window.last.load(Ordering::Relaxed)
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
                for chunk in request.pages.chunks(2) {
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
