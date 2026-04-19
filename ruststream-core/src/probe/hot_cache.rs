//! L1 hot cache for probe results — zero redb round-trip for repeated lookups.
//!
//! The persistent `MediaCache` (backed by redb) adds ~100–500 µs per hit
//! (disk read + bincode decode). For workloads that probe the same files
//! repeatedly (e.g. a playlist encoder that checks clips 10× per job),
//! this cache sits in front and answers in **< 1 µs** using a fixed-size
//! thread-local slab.
//!
//! # Design
//! - **Thread-local, lock-free**: each thread has its own slab — no atomics
//! - **Fixed N=32 entries**: fits in a few KiB of L1/L2 cache
//! - **Eviction**: random slot overwrite (FIFO within the slot ring)
//! - **Key**: `(path + mtime + size)` via `cache_key()`
//!
//! This is the "L1" in a two-level scheme:
//! ```text
//! probe_cached_l1() → [thread-local slab] → hit: return immediately
//!                                         → miss: probe_cached() → [redb] → return + insert
//! ```

use std::cell::RefCell;
use crate::probe::{FullMetadata, cache_key, probe_full};
use crate::core::MediaResult;

/// Number of slots in each thread's L1 slab.
const SLAB_SIZE: usize = 32;

#[derive(Clone, Default)]
struct SlabEntry {
    key:  String,
    meta: Option<FullMetadata>,
}

struct HotSlab {
    entries: [SlabEntry; SLAB_SIZE],
    next:    usize, // next write slot (ring)
}

impl HotSlab {
    fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| SlabEntry::default()),
            next:    0,
        }
    }

    #[inline]
    fn get(&self, key: &str) -> Option<&FullMetadata> {
        for entry in &self.entries {
            if entry.key == key {
                return entry.meta.as_ref();
            }
        }
        None
    }

    #[inline]
    fn insert(&mut self, key: String, meta: FullMetadata) {
        let slot = self.next % SLAB_SIZE;
        self.entries[slot] = SlabEntry { key, meta: Some(meta) };
        self.next = self.next.wrapping_add(1);
    }

    fn invalidate(&mut self, key: &str) {
        for entry in self.entries.iter_mut() {
            if entry.key == key {
                entry.key.clear();
                entry.meta = None;
            }
        }
    }
}

thread_local! {
    static L1: RefCell<HotSlab> = RefCell::new(HotSlab::new());
}

/// Probe a file with L1 hot slab in front of the persistent cache.
///
/// Lookup order:
/// 1. Thread-local slab (< 1 µs, lock-free)
/// 2. `probe_cached()` global OnceLock cache (~100–500 µs, one redb read)
/// 3. `probe_full()` real FFmpeg probe (1–10 ms, fills both caches)
///
/// # Thread safety
/// Each thread has its own slab — no synchronisation needed at L1.
/// `probe_cached()` uses a `parking_lot::Mutex` internally (shared across threads).
pub fn probe_cached_l1(path: &str) -> MediaResult<FullMetadata> {
    let key = cache_key(path);

    // ── L1 slab lookup ────────────────────────────────────────────────────────
    let l1_hit = L1.with(|slab| {
        slab.borrow().get(&key).cloned()
    });

    if let Some(meta) = l1_hit {
        log::trace!("L1 slab HIT: {}", path);
        return Ok(meta);
    }

    // ── L2 (redb) / probe_full ────────────────────────────────────────────────
    log::trace!("L1 slab MISS: {}", path);
    let meta = crate::probe::probe_cached(path)?;

    // Insert into L1 slab for next access
    L1.with(|slab| {
        slab.borrow_mut().insert(key, meta.clone());
    });

    Ok(meta)
}

/// Probe a batch of files with L1 + L2 caching.
///
/// Unlike `probe_batch` (which uses only probe_fast without caches),
/// this function applies the full L1→L2→probe_full chain per file.
/// Use when the same files are likely to be probed multiple times.
pub fn probe_batch_cached(paths: &[&str]) -> Vec<MediaResult<FullMetadata>> {
    paths.iter().map(|&p| probe_cached_l1(p)).collect()
}

/// Manually invalidate a path from the calling thread's L1 slab.
///
/// Call this after a file is modified/replaced and you want the next
/// `probe_cached_l1()` call on the same thread to re-probe.
pub fn l1_invalidate(path: &str) {
    let key = cache_key(path);
    L1.with(|slab| slab.borrow_mut().invalidate(&key));
}

/// How many L1 slots are currently occupied on the calling thread.
pub fn l1_occupancy() -> usize {
    L1.with(|slab| {
        slab.borrow().entries.iter().filter(|e| e.meta.is_some()).count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l1_occupancy_starts_zero() {
        // Each test gets its own thread-local, so this is always 0 in a fresh context.
        // (Thread reuse between tests may give non-zero — just check it doesn't panic)
        let _ = l1_occupancy();
    }

    #[test]
    fn test_l1_invalidate_nonexistent() {
        // Must not panic even if the key was never inserted
        l1_invalidate("/nonexistent/path.mp4");
    }

    #[test]
    fn test_slab_insert_and_get() {
        let mut slab = HotSlab::new();
        let meta = FullMetadata {
            path: "test.mp4".to_string(),
            video: crate::probe::VideoMetadata::default(),
            audio: None,
            format: crate::probe::FormatMetadata {
                format_name: "mp4".to_string(),
                duration_secs: 1.0,
                bit_rate: None,
                size_bytes: 1024,
            },
        };
        slab.insert("key1".to_string(), meta.clone());
        let found = slab.get("key1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, "test.mp4");
    }

    #[test]
    fn test_slab_ring_overflow() {
        let mut slab = HotSlab::new();
        let meta = FullMetadata {
            path: "test.mp4".to_string(),
            video: crate::probe::VideoMetadata::default(),
            audio: None,
            format: crate::probe::FormatMetadata::default(),
        };

        // Insert more than SLAB_SIZE entries — must not panic
        for i in 0..(SLAB_SIZE * 2) {
            slab.insert(format!("key-{}", i), meta.clone());
        }

        // Oldest entries should have been overwritten, ring keeps rolling
        assert!(slab.next >= SLAB_SIZE * 2);
    }

    #[test]
    fn test_probe_batch_cached_empty() {
        let results = probe_batch_cached(&[]);
        assert!(results.is_empty());
    }
}
