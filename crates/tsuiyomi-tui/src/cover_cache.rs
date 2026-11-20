//! LRU cache for decoded cover thumbnails.
//!
//! Cover protocols keep their decoded image, so an entry-count-only cache can retain far
//! more memory than intended. This cache bounds both entries and estimated pixel bytes,
//! and touches entries when they are drawn so the visible grid is not evicted first.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
};

use image::DynamicImage;
use ratatui_image::protocol::StatefulProtocol;

/// Maximum number of cover protocols retained.
pub(crate) const COVER_CACHE_ENTRIES: usize = 256;
/// Estimated decoded-image budget for covers.
pub(crate) const COVER_CACHE_BYTES: usize = 128 * 1024 * 1024;
/// Longest edge kept for a cover. Generated covers use the same cap.
pub(crate) const COVER_MAX_EDGE: u32 = 512;

/// Shrink a cover to the largest useful edge before it enters the protocol cache.
pub(crate) fn fit_cover_image(image: DynamicImage) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    if width <= COVER_MAX_EDGE && height <= COVER_MAX_EDGE {
        image
    } else {
        image.thumbnail(COVER_MAX_EDGE, COVER_MAX_EDGE)
    }
}

/// Estimate the decoded image size held by a protocol.
pub(crate) fn image_bytes(image: &DynamicImage) -> usize {
    image.width() as usize * image.height() as usize * image.color().bytes_per_pixel() as usize
}

#[derive(Default)]
pub(crate) struct TileCoverCache {
    protocols: HashMap<PathBuf, StatefulProtocol>,
    sizes: HashMap<PathBuf, usize>,
    /// Least recently used first.
    order: VecDeque<PathBuf>,
    bytes: usize,
}

impl TileCoverCache {
    pub fn insert(&mut self, path: PathBuf, protocol: StatefulProtocol, bytes: usize) {
        self.insert_with_limits(
            path,
            protocol,
            bytes,
            COVER_CACHE_ENTRIES,
            COVER_CACHE_BYTES,
        );
    }

    fn insert_with_limits(
        &mut self,
        path: PathBuf,
        protocol: StatefulProtocol,
        bytes: usize,
        max_entries: usize,
        max_bytes: usize,
    ) {
        self.remove(&path);
        self.order.push_back(path.clone());
        self.sizes.insert(path.clone(), bytes);
        self.bytes = self.bytes.saturating_add(bytes);
        self.protocols.insert(path, protocol);

        while (self.protocols.len() > max_entries || self.bytes > max_bytes)
            && self.protocols.len() > 1
        {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.remove_entry(&oldest);
        }
    }

    pub fn contains_key(&self, path: &Path) -> bool {
        self.protocols.contains_key(path)
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut StatefulProtocol> {
        if !self.protocols.contains_key(path) {
            return None;
        }
        self.touch(path);
        self.protocols.get_mut(path)
    }

    pub fn remove(&mut self, path: &Path) -> Option<StatefulProtocol> {
        if let Some(position) = self.order.iter().position(|entry| entry == path) {
            self.order.remove(position);
        }
        self.remove_entry(path)
    }

    fn remove_entry(&mut self, path: &Path) -> Option<StatefulProtocol> {
        let protocol = self.protocols.remove(path)?;
        if let Some(bytes) = self.sizes.remove(path) {
            self.bytes = self.bytes.saturating_sub(bytes);
        }
        Some(protocol)
    }

    fn touch(&mut self, path: &Path) {
        if let Some(position) = self.order.iter().position(|entry| entry == path) {
            self.order.remove(position);
        }
        self.order.push_back(path.to_path_buf());
    }

    pub fn clear(&mut self) {
        self.protocols.clear();
        self.sizes.clear();
        self.order.clear();
        self.bytes = 0;
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.protocols.len()
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, Rgb, RgbImage};
    use ratatui_image::picker::Picker;

    use super::*;

    fn protocol() -> StatefulProtocol {
        Picker::halfblocks().new_resize_protocol(DynamicImage::ImageRgb8(RgbImage::from_pixel(
            2,
            2,
            Rgb([1, 2, 3]),
        )))
    }

    #[test]
    fn touches_make_an_entry_recent() {
        let mut cache = TileCoverCache::default();
        let a = PathBuf::from("a");
        let b = PathBuf::from("b");
        let c = PathBuf::from("c");
        cache.insert_with_limits(a.clone(), protocol(), 1, 2, 100);
        cache.insert_with_limits(b.clone(), protocol(), 1, 2, 100);
        assert!(cache.get_mut(&a).is_some());
        cache.insert_with_limits(c.clone(), protocol(), 1, 2, 100);

        assert!(cache.contains_key(&a));
        assert!(!cache.contains_key(&b));
        assert!(cache.contains_key(&c));
    }

    #[test]
    fn byte_budget_evicts_old_entries() {
        let mut cache = TileCoverCache::default();
        let a = PathBuf::from("a");
        let b = PathBuf::from("b");
        cache.insert_with_limits(a.clone(), protocol(), 60, 10, 100);
        cache.insert_with_limits(b.clone(), protocol(), 60, 10, 100);

        assert!(!cache.contains_key(&a));
        assert!(cache.contains_key(&b));
        assert_eq!(cache.bytes(), 60);
    }

    #[test]
    fn covers_are_downscaled_to_the_cache_edge() {
        let large = DynamicImage::ImageRgb8(RgbImage::from_pixel(2000, 1000, Rgb([1, 2, 3])));
        let fitted = fit_cover_image(large);
        assert!(fitted.width() <= COVER_MAX_EDGE);
        assert!(fitted.height() <= COVER_MAX_EDGE);

        let small = DynamicImage::ImageRgb8(RgbImage::from_pixel(10, 20, Rgb([1, 2, 3])));
        let fitted = fit_cover_image(small);
        assert_eq!((fitted.width(), fitted.height()), (10, 20));
    }

    #[test]
    fn image_bytes_matches_pixel_layout() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(10, 20, Rgb([1, 2, 3])));
        assert_eq!(image_bytes(&image), 10 * 20 * 3);
        assert_eq!(cache_len_after_insert(), (1, 600));
    }

    fn cache_len_after_insert() -> (usize, usize) {
        let mut cache = TileCoverCache::default();
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(10, 20, Rgb([1, 2, 3])));
        let bytes = image_bytes(&image);
        cache.insert(PathBuf::from("x"), protocol(), bytes);
        (cache.len(), cache.bytes())
    }
}
