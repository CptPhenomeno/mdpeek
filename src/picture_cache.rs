//! In-memory cache of rasterised pictures (SPEC §3.4.3, §4.13).
//!
//! Keyed by (source hash, pixel width, pixel height). This caches the RGBA
//! pixels, one level below `graphics::PictureSet`, which caches encoded
//! terminal protocols.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::UNIX_EPOCH;

use image::RgbaImage;

use crate::diagram::source_hash;

/// Default limit on stored pixel data: 256 MB (SPEC §4.13).
pub const DEFAULT_LIMIT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub source: u64,
    pub width: u32,
    pub height: u32,
}

struct Entry {
    image: Arc<RgbaImage>,
    last_used: u64,
}

struct Inner {
    limit: usize,
    bytes: usize,
    tick: u64,
    map: HashMap<CacheKey, Entry>,
}

fn cost(img: &RgbaImage) -> usize {
    img.width() as usize * img.height() as usize * 4
}

/// Thread-safe, cheaply cloneable handle to one shared cache. Values are
/// `Arc<RgbaImage>` so readers never copy pixel data. Eviction is
/// least-recently-used once the stored bytes (width x height x 4) exceed the
/// limit; both `get` and `insert` count as a use.
#[derive(Clone)]
pub struct PictureCache {
    inner: Arc<Mutex<Inner>>,
}

impl PictureCache {
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                limit: limit_bytes,
                bytes: 0,
                tick: 0,
                map: HashMap::new(),
            })),
        }
    }

    pub fn with_default_limit() -> Self {
        Self::new(DEFAULT_LIMIT_BYTES)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // The cache holds no invariant a panicking holder could break halfway
        // that matters more than staying usable, so recover from poisoning.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Looks up a picture and marks it most recently used.
    pub fn get(&self, key: &CacheKey) -> Option<Arc<RgbaImage>> {
        let mut g = self.lock();
        g.tick += 1;
        let tick = g.tick;
        let e = g.map.get_mut(key)?;
        e.last_used = tick;
        Some(Arc::clone(&e.image))
    }

    /// Stores a picture and returns the shared handle. A picture larger than
    /// the whole limit is returned but not cached, and evicts nothing.
    pub fn insert(&self, key: CacheKey, img: RgbaImage) -> Arc<RgbaImage> {
        let size = cost(&img);
        let image = Arc::new(img);
        let mut g = self.lock();
        if let Some(old) = g.map.remove(&key) {
            g.bytes -= cost(&old.image);
        }
        if size > g.limit {
            return image;
        }
        g.tick += 1;
        let last_used = g.tick;
        g.bytes += size;
        g.map.insert(
            key,
            Entry {
                image: Arc::clone(&image),
                last_used,
            },
        );
        while g.bytes > g.limit {
            let Some(oldest) = g
                .map
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| *k)
            else {
                break;
            };
            if let Some(e) = g.map.remove(&oldest) {
                g.bytes -= cost(&e.image);
            }
        }
        image
    }

    /// Returns the cached picture, or renders, stores and returns it. The
    /// lock is not held while rendering.
    pub fn get_or_render<E>(
        &self,
        key: CacheKey,
        render: impl FnOnce() -> Result<RgbaImage, E>,
    ) -> Result<Arc<RgbaImage>, E> {
        if let Some(hit) = self.get(&key) {
            return Ok(hit);
        }
        Ok(self.insert(key, render()?))
    }

    pub fn len(&self) -> usize {
        self.lock().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total pixel bytes currently stored.
    pub fn bytes(&self) -> usize {
        self.lock().bytes
    }
}

/// Source hash of a local image: FNV-1a (`source_hash`) of path, modification
/// time and byte size, so editing or replacing the file is a miss without
/// reading its content. `None` if the file cannot be stat-ed.
pub fn image_source_hash(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Some(source_hash(&format!(
        "{}\0{mtime}\0{}",
        path.to_string_lossy(),
        meta.len()
    )))
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
