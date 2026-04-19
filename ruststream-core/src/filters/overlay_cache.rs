//! Overlay asset cache — pre-rasterized PNG/image assets ready for compositing.
//!
//! Overlay images (watermarks, lower-thirds, logos) are often:
//! - Identical across hundreds of jobs
//! - Expensive to decode (PNG → raw RGBA) every time
//!
//! This module caches decoded RGBA pixel data keyed by (path + mtime + size)
//! so each asset is decoded **at most once per process lifetime**.
//!
//! # Memory model
//! The cache lives in a global `parking_lot::RwLock<HashMap>` with a
//! `VecDeque` insertion-order tracker for true LRU eviction.
//! Callers borrow `Arc<OverlayAsset>` — zero copies on hit.
//!
//! # Format
//! Pixel data is stored as raw `Vec<u8>` in **RGBA8** layout, with dimensions
//! stored alongside. Callers borrow `Arc<OverlayAsset>` — zero copies on hit.

use crate::probe::cache_key;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

/// Maximum number of overlay assets kept in memory simultaneously.
const DEFAULT_MAX_ENTRIES: usize = 256;

/// A decoded, ready-to-composite overlay asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAsset {
    /// Original path.
    pub path: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw RGBA8 pixel data (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
    /// Cache key used to look this entry up.
    pub cache_key: String,
}

impl OverlayAsset {
    /// Total byte size of the RGBA buffer.
    #[inline]
    pub fn byte_size(&self) -> usize {
        self.rgba.len()
    }

    /// Pixel count.
    #[inline]
    pub fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }
}

/// Cache hit/miss statistics.
#[derive(Debug, Clone, Default)]
pub struct OverlayCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
    pub total_bytes: usize,
}

/// Inner state guarded by a single RwLock.
struct CacheState {
    store: HashMap<String, Arc<OverlayAsset>>,
    /// LRU order: front = oldest (next eviction candidate), back = most recently used.
    order: VecDeque<String>,
}

impl CacheState {
    fn new() -> Self {
        Self {
            store: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Touch an existing key — moves it to the back (most-recently-used end).
    fn touch(&mut self, key: &str) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(key.to_string());
        }
    }

    /// Evict the least-recently-used entry. Returns `true` if something was evicted.
    fn evict_lru(&mut self) -> bool {
        if let Some(lru_key) = self.order.pop_front() {
            self.store.remove(&lru_key);
            log::debug!("overlay cache EVICT LRU: {}", lru_key);
            true
        } else {
            false
        }
    }

    /// Insert a new entry, evicting LRU if at capacity.
    fn insert(&mut self, key: String, asset: Arc<OverlayAsset>, max_entries: usize) -> bool {
        let evicted = if self.store.len() >= max_entries {
            self.evict_lru()
        } else {
            false
        };
        self.order.push_back(key.clone());
        self.store.insert(key, asset);
        evicted
    }
}

/// Process-global overlay asset cache with true LRU eviction.
pub struct OverlayCache {
    state: RwLock<CacheState>,
    max_entries: usize,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
    evictions: std::sync::atomic::AtomicU64,
}

impl OverlayCache {
    /// Create a new cache with the default entry limit.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_ENTRIES)
    }

    /// Create a cache with a custom maximum entry count.
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            state: RwLock::new(CacheState::new()),
            max_entries,
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
            evictions: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get or decode an overlay asset (LRU-aware).
    ///
    /// Returns `Arc<OverlayAsset>` — callers can hold the arc across frames
    /// without additional locking.
    ///
    /// Decoding is done via an injected `decoder` closure so callers can
    /// supply any PNG/WebP/JPEG decoder without a hard dependency here.
    pub fn get_or_decode<F>(
        &self,
        path: &str,
        decoder: F,
    ) -> crate::core::MediaResult<Arc<OverlayAsset>>
    where
        F: FnOnce(&str) -> crate::core::MediaResult<(u32, u32, Vec<u8>)>,
    {
        let key = cache_key(path);

        // ── Fast read path ────────────────────────────────────────────────────
        {
            let guard = self.state.read();
            if let Some(asset) = guard.store.get(&key) {
                self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                log::debug!("overlay cache HIT: {}", path);
                let asset = Arc::clone(asset);
                drop(guard);
                // Promote to MRU — needs write lock (do it outside read lock)
                self.state.write().touch(&key);
                return Ok(asset);
            }
        }

        // ── Cache miss — decode ───────────────────────────────────────────────
        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        log::debug!("overlay cache MISS: {}", path);

        if !Path::new(path).exists() {
            return Err(crate::core::MediaError::new(
                crate::core::MediaErrorCode::IoFileNotFound,
                format!("overlay asset not found: {}", path),
            ));
        }

        let (width, height, rgba) = decoder(path)?;
        let asset = Arc::new(OverlayAsset {
            path: path.to_string(),
            width,
            height,
            rgba,
            cache_key: key.clone(),
        });

        // ── Write with LRU eviction ───────────────────────────────────────────
        let mut guard = self.state.write();
        let evicted = guard.insert(key, Arc::clone(&asset), self.max_entries);
        if evicted {
            self.evictions
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        Ok(asset)
    }

    /// Pre-warm the cache for a list of overlay paths.
    ///
    /// Runs decoding in parallel via rayon. Errors are logged and skipped.
    pub fn prewarm<F>(&self, paths: &[&str], decoder: F)
    where
        F: Fn(&str) -> crate::core::MediaResult<(u32, u32, Vec<u8>)> + Sync,
    {
        use rayon::prelude::*;
        paths.par_iter().for_each(|&path| {
            if let Err(e) = self.get_or_decode(path, &decoder) {
                log::warn!("overlay prewarm failed for {}: {}", path, e);
            }
        });
    }

    /// Invalidate a single entry (e.g. after an asset is updated on disk).
    pub fn invalidate(&self, path: &str) {
        let key = cache_key(path);
        let mut guard = self.state.write();
        if guard.store.remove(&key).is_some() {
            if let Some(pos) = guard.order.iter().position(|k| k == &key) {
                guard.order.remove(pos);
            }
        }
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        let mut guard = self.state.write();
        guard.store.clear();
        guard.order.clear();
    }

    /// Cache statistics.
    pub fn stats(&self) -> OverlayCacheStats {
        let guard = self.state.read();
        let total_bytes: usize = guard.store.values().map(|a| a.byte_size()).sum();
        OverlayCacheStats {
            hits: self.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: self.misses.load(std::sync::atomic::Ordering::Relaxed),
            evictions: self.evictions.load(std::sync::atomic::Ordering::Relaxed),
            entries: guard.store.len(),
            total_bytes,
        }
    }
}

impl Default for OverlayCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global singleton overlay cache (256 entries max, LRU eviction).
pub fn global_overlay_cache() -> &'static OverlayCache {
    static CACHE: std::sync::OnceLock<OverlayCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(OverlayCache::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_fake_png(dir: &TempDir, name: &str) -> String {
        let path = dir.path().join(name);
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&[137, 80, 78, 71])
            .unwrap();
        path.to_string_lossy().into_string()
    }

    fn mock_decoder(path: &str) -> crate::core::MediaResult<(u32, u32, Vec<u8>)> {
        let _ = path;
        Ok((2, 2, vec![255, 0, 0, 255; 4]))
    }

    #[test]
    fn test_overlay_cache_miss_then_hit() {
        let cache = OverlayCache::new();
        let tmp = TempDir::new().unwrap();
        let path = make_fake_png(&tmp, "logo.png");

        let a1 = cache.get_or_decode(&path, mock_decoder).unwrap();
        let a2 = cache.get_or_decode(&path, mock_decoder).unwrap();

        assert_eq!(a1.width, 2);
        assert_eq!(a2.width, 2);

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn test_overlay_cache_lru_eviction() {
        // Create cache with capacity 2; insert 3 assets — first should be evicted
        let cache = OverlayCache::with_capacity(2);
        let tmp = TempDir::new().unwrap();

        let p1 = make_fake_png(&tmp, "a.png");
        let p2 = make_fake_png(&tmp, "b.png");
        let p3 = make_fake_png(&tmp, "c.png");

        cache.get_or_decode(&p1, mock_decoder).unwrap();
        cache.get_or_decode(&p2, mock_decoder).unwrap();
        // Touch p1 to make it MRU → p2 becomes LRU
        cache.get_or_decode(&p1, mock_decoder).unwrap();
        // Insert p3 → should evict p2 (LRU)
        cache.get_or_decode(&p3, mock_decoder).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn test_overlay_cache_clear() {
        let cache = OverlayCache::new();
        let tmp = TempDir::new().unwrap();
        let path = make_fake_png(&tmp, "logo2.png");
        cache.get_or_decode(&path, mock_decoder).unwrap();
        assert_eq!(cache.stats().entries, 1);
        cache.clear();
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn test_overlay_cache_nonexistent() {
        let cache = OverlayCache::new();
        #[cfg(windows)]
        let missing = "C:\\__missing__\\logo.png";
        #[cfg(not(windows))]
        let missing = "/tmp/__missing_overlay__.png";

        let result = cache.get_or_decode(missing, mock_decoder);
        assert!(result.is_err());
    }

    #[test]
    fn test_global_overlay_cache_singleton() {
        let c1 = global_overlay_cache();
        let c2 = global_overlay_cache();
        assert!(std::ptr::eq(c1, c2));
    }
}
