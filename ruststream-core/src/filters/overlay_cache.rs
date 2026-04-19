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
//! The cache lives in a global `DashMap`-like structure (backed by `parking_lot`
//! RwLock on a `HashMap`). It is bounded by `max_entries` to prevent unbounded
//! growth on servers handling thousands of different overlays.
//!
//! # Format
//! Pixel data is stored as raw `Vec<u8>` in **RGBA8** layout, with dimensions
//! stored alongside. Callers borrow `Arc<OverlayAsset>` — zero copies on hit.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use crate::probe::cache_key;

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
    pub entries: usize,
    pub total_bytes: usize,
}

/// Process-global overlay asset cache.
pub struct OverlayCache {
    store:       RwLock<HashMap<String, Arc<OverlayAsset>>>,
    max_entries: usize,
    hits:        std::sync::atomic::AtomicU64,
    misses:      std::sync::atomic::AtomicU64,
}

impl OverlayCache {
    /// Create a new cache with the default entry limit.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_ENTRIES)
    }

    /// Create a cache with a custom maximum entry count.
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            max_entries,
            hits:   std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get or decode an overlay asset.
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

        // Fast read path
        {
            let guard = self.store.read();
            if let Some(asset) = guard.get(&key) {
                self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                log::debug!("overlay cache HIT: {}", path);
                return Ok(Arc::clone(asset));
            }
        }

        // Cache miss — decode
        self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

        // Write — evict oldest entry if at capacity
        let mut guard = self.store.write();
        if guard.len() >= self.max_entries {
            // LRU-lite: remove any one entry to stay under limit
            if let Some(evict_key) = guard.keys().next().cloned() {
                guard.remove(&evict_key);
                log::debug!("overlay cache EVICT: {}", evict_key);
            }
        }
        guard.insert(key, Arc::clone(&asset));

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
        self.store.write().remove(&key);
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        self.store.write().clear();
    }

    /// Cache statistics.
    pub fn stats(&self) -> OverlayCacheStats {
        let guard = self.store.read();
        let total_bytes: usize = guard.values().map(|a| a.byte_size()).sum();
        OverlayCacheStats {
            hits:        self.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses:      self.misses.load(std::sync::atomic::Ordering::Relaxed),
            entries:     guard.len(),
            total_bytes,
        }
    }
}

impl Default for OverlayCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global singleton overlay cache (256 entries max).
pub fn global_overlay_cache() -> &'static OverlayCache {
    static CACHE: std::sync::OnceLock<OverlayCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(OverlayCache::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    fn make_fake_png(dir: &TempDir, name: &str) -> String {
        // Write a tiny valid 1x1 RGBA raw "asset" (not real PNG — decoder is mocked)
        let path = dir.path().join(name);
        std::fs::File::create(&path).unwrap()
            .write_all(&[137, 80, 78, 71]).unwrap(); // PNG magic bytes
        path.to_string_lossy().into_string()
    }

    fn mock_decoder(path: &str) -> crate::core::MediaResult<(u32, u32, Vec<u8>)> {
        let _ = path;
        // 2×2 RGBA: all red
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
        // Must be the same address
        assert!(std::ptr::eq(c1, c2));
    }
}
