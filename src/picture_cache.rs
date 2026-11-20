//! In-memory cache of rasterised pictures (SPEC §3.4.3, §4.13).
//!
//! Keyed by (source hash, pixel width, pixel height). This caches the RGBA
//! pixels, one level below `graphics::PictureSet`, which caches encoded
//! terminal protocols.

use std::path::Path;
use std::sync::{Arc, Mutex};

use image::RgbaImage;

/// Default limit on stored pixel data: 256 MB (SPEC §4.13).
pub const DEFAULT_LIMIT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub source: u64,
    pub width: u32,
    pub height: u32,
}

/// Thread-safe, cheaply cloneable handle to one shared cache.
#[derive(Clone)]
pub struct PictureCache {
    _inner: Arc<Mutex<()>>,
}

impl PictureCache {
    pub fn new(_limit_bytes: usize) -> Self {
        todo!()
    }
    pub fn with_default_limit() -> Self {
        todo!()
    }
    pub fn get(&self, _key: &CacheKey) -> Option<Arc<RgbaImage>> {
        todo!()
    }
    pub fn insert(&self, _key: CacheKey, _img: RgbaImage) -> Arc<RgbaImage> {
        todo!()
    }
    pub fn get_or_render<E>(
        &self,
        _key: CacheKey,
        _render: impl FnOnce() -> Result<RgbaImage, E>,
    ) -> Result<Arc<RgbaImage>, E> {
        todo!()
    }
    pub fn len(&self) -> usize {
        todo!()
    }
    pub fn is_empty(&self) -> bool {
        todo!()
    }
    pub fn bytes(&self) -> usize {
        todo!()
    }
}

/// Source hash of a local image: changes when path, mtime or size change.
pub fn image_source_hash(_path: &Path) -> Option<u64> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn img(w: u32, h: u32) -> RgbaImage {
        RgbaImage::new(w, h)
    }
    fn key(s: u64, w: u32, h: u32) -> CacheKey {
        CacheKey {
            source: s,
            width: w,
            height: h,
        }
    }

    #[test]
    fn same_key_returns_cached_without_rerendering() {
        let c = PictureCache::new(1 << 20);
        let calls = Cell::new(0);
        let render = || -> Result<RgbaImage, ()> {
            calls.set(calls.get() + 1);
            Ok(img(4, 4))
        };
        let a = c.get_or_render(key(1, 4, 4), render).unwrap();
        let b = c.get_or_render(key(1, 4, 4), render).unwrap();
        assert_eq!(calls.get(), 1);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn different_pixel_size_is_a_miss() {
        let c = PictureCache::new(1 << 20);
        c.insert(key(1, 4, 4), img(4, 4));
        assert!(c.get(&key(1, 4, 4)).is_some());
        assert!(c.get(&key(1, 8, 4)).is_none());
        assert!(c.get(&key(1, 4, 8)).is_none());
    }

    #[test]
    fn changed_source_is_a_miss() {
        let c = PictureCache::new(1 << 20);
        c.insert(key(1, 4, 4), img(4, 4));
        assert!(c.get(&key(2, 4, 4)).is_none());
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        std::fs::write(&p, b"one").unwrap();
        let h1 = image_source_hash(&p).unwrap();
        assert_eq!(h1, image_source_hash(&p).unwrap());
        std::fs::write(&p, b"longer content").unwrap();
        assert_ne!(h1, image_source_hash(&p).unwrap());
        assert!(image_source_hash(&dir.path().join("missing")).is_none());
    }

    #[test]
    fn shared_between_threads() {
        let c = PictureCache::new(1 << 20);
        let w = c.clone();
        std::thread::spawn(move || {
            w.insert(key(7, 2, 2), img(2, 2));
        })
        .join()
        .unwrap();
        assert!(c.get(&key(7, 2, 2)).is_some());
    }

    #[test]
    fn exceeding_limit_evicts_lru_first() {
        // each 4x4 image is 64 bytes; limit holds two
        let c = PictureCache::new(128);
        c.insert(key(1, 4, 4), img(4, 4));
        c.insert(key(2, 4, 4), img(4, 4));
        c.insert(key(3, 4, 4), img(4, 4));
        assert!(c.get(&key(1, 4, 4)).is_none());
        assert!(c.get(&key(2, 4, 4)).is_some());
        assert!(c.get(&key(3, 4, 4)).is_some());
        assert_eq!(c.bytes(), 128);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn lookup_refreshes_recency() {
        let c = PictureCache::new(128);
        c.insert(key(1, 4, 4), img(4, 4));
        c.insert(key(2, 4, 4), img(4, 4));
        assert!(c.get(&key(1, 4, 4)).is_some());
        c.insert(key(3, 4, 4), img(4, 4));
        assert!(c.get(&key(1, 4, 4)).is_some());
        assert!(c.get(&key(2, 4, 4)).is_none());
    }

    #[test]
    fn entry_larger_than_limit_is_returned_but_not_cached() {
        let c = PictureCache::new(128);
        c.insert(key(1, 4, 4), img(4, 4));
        let big = c.insert(key(2, 8, 8), img(8, 8)); // 256 bytes
        assert_eq!(big.width(), 8);
        assert!(c.get(&key(2, 8, 8)).is_none());
        // existing entries are not flushed by an oversize one
        assert!(c.get(&key(1, 4, 4)).is_some());
        assert_eq!(c.bytes(), 64);
    }
}
