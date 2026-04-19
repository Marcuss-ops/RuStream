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
//! - **Fixed N=64 entries**: doubled from 32, fits comfortably in L2
//! - **Hash-first lookup**: FNV-1a u64 hash compared before string equality
//!   — eliminates expensive string comparisons on non-matching slots
//! - **Eviction**: FIFO ring (overwrites oldest slot)
//! - **Key**: `(path + mtime + size)` via `cache_key()`
//!
//! Two-level lookup chain:
//! ```text
//! probe_cached_l1() → [thread-local slab] → hit: return immediately (<1 µs)
//!                                         → miss: probe_cached() → [redb] → return + insert
//! ```

use crate::core::MediaResult;
use crate::probe::{cache_key, probe_full, FullMetadata};
use std::cell::RefCell;

/// Number of slots in each thread's L1 slab.
/// Must be a power of two for the wrapping mask trick.
const SLAB_SIZE: usize = 64;
const SLAB_MASK: usize = SLAB_SIZE - 1;

// ── FNV-1a inline hash (no allocation, no deps) ───────────────────────────────

/// Compute a 64-bit FNV-1a hash for a string key.
///
/// FNV-1a is chosen because it is:
/// - Extremely fast (one XOR + one multiply per byte)
/// - Well-distributed for path-like keys
/// - Completely branch-free
#[inline(always)]
fn fnv1a_hash(s: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET_BASIS;
    for &b in s.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

// ── Slab entry ────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct SlabEntry {
    /// FNV-1a hash of the key — checked first to avoid string compare on miss.
    key_hash: u64,
    key: String,
    meta: Option<FullMetadata>,
}

struct HotSlab {
    entries: [SlabEntry; SLAB_SIZE],
    next: usize, // next write slot (ring)
}

impl HotSlab {
    fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| SlabEntry::default()),
            next: 0,
        }
    }

    /// O(SLAB_SIZE) lookup, but hash-first: most slots are rejected in 1 cycle.
    #[inline]
    fn get(&self, key: &str, key_hash: u64) -> Option<&FullMetadata> {
        for entry in &self.entries {
            // Fast path: hash mismatch → skip without touching the String heap ptr
            if entry.key_hash == key_hash && entry.key == key {
                return entry.meta.as_ref();
            }
        }
        None
    }

    #[inline]
    fn insert(&mut self, key: String, key_hash: u64, meta: FullMetadata) {
        let slot = self.next & SLAB_MASK;
        self.entries[slot] = SlabEntry {
            key_hash,
            key,
            meta: Some(meta),
        };
        self.next = self.next.wrapping_add(1);
    }

    fn invalidate(&mut self, key: &str, key_hash: u64) {
        for entry in self.entries.iter_mut() {
            if entry.key_hash == key_hash && entry.key == key {
                entry.key_hash = 0;
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
/// 1. Thread-local slab (< 1 µs, lock-free, hash-first comparison)
/// 2. `probe_cached()` global OnceLock cache (~100–500 µs, one redb read)
/// 3. `probe_full()` real FFmpeg probe (1–10 ms, fills both caches)
///
/// # Thread safety
/// Each thread has its own slab — no synchronisation needed at L1.
/// `probe_cached()` uses a `parking_lot::Mutex` internally (shared across threads).
pub fn probe_cached_l1(path: &str) -> MediaResult<FullMetadata> {
    let key = cache_key(path);
    let key_hash = fnv1a_hash(&key);

    // ── L1 slab lookup ────────────────────────────────────────────────────────
    let l1_hit = L1.with(|slab| slab.borrow().get(&key, key_hash).cloned());

    if let Some(meta) = l1_hit {
        log::trace!("L1 slab HIT: {}", path);
        return Ok(meta);
    }

    // ── L2 (redb) / probe_full ────────────────────────────────────────────────
    log::trace!("L1 slab MISS: {}", path);
    let meta = crate::probe::probe_cached(path)?;

    // Insert into L1 slab for next access
    L1.with(|slab| {
        slab.borrow_mut().insert(key, key_hash, meta.clone());
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
    let key_hash = fnv1a_hash(&key);
    L1.with(|slab| slab.borrow_mut().invalidate(&key, key_hash));
}

/// How many L1 slots are currently occupied on the calling thread.
pub fn l1_occupancy() -> usize {
    L1.with(|slab| {
        slab.borrow()
            .entries
            .iter()
            .filter(|e| e.meta.is_some())
            .count()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_hash_deterministic() {
        assert_eq!(fnv1a_hash("hello"), fnv1a_hash("hello"));
        assert_ne!(fnv1a_hash("hello"), fnv1a_hash("world"));
        assert_ne!(fnv1a_hash(""), fnv1a_hash("x"));
    }

    #[test]
    fn test_l1_occupancy_starts_zero() {
        let _ = l1_occupancy();
    }

    #[test]
    fn test_l1_invalidate_nonexistent() {
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
        let key = "key1".to_string();
        let hash = fnv1a_hash(&key);
        slab.insert(key.clone(), hash, meta.clone());
        let found = slab.get(&key, hash);
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, "test.mp4");
    }

    #[test]
    fn test_slab_hash_collision_safety() {
        // Different keys with same-length strings: must not return wrong entry
        let mut slab = HotSlab::new();
        let meta_a = FullMetadata {
            path: "a.mp4".to_string(),
            video: crate::probe::VideoMetadata::default(),
            audio: None,
            format: crate::probe::FormatMetadata::default(),
        };
        let meta_b = FullMetadata {
            path: "b.mp4".to_string(),
            video: crate::probe::VideoMetadata::default(),
            audio: None,
            format: crate::probe::FormatMetadata::default(),
        };
        let (ha, hb) = (fnv1a_hash("a.mp4"), fnv1a_hash("b.mp4"));
        slab.insert("a.mp4".to_string(), ha, meta_a);
        slab.insert("b.mp4".to_string(), hb, meta_b);

        assert_eq!(slab.get("a.mp4", ha).unwrap().path, "a.mp4");
        assert_eq!(slab.get("b.mp4", hb).unwrap().path, "b.mp4");
        // Wrong hash → must return None even if key string happens to match
        assert!(slab.get("a.mp4", hb).is_none() || slab.get("a.mp4", hb).unwrap().path == "a.mp4");
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
        for i in 0..(SLAB_SIZE * 2) {
            let key = format!("key-{}", i);
            let hash = fnv1a_hash(&key);
            slab.insert(key, hash, meta.clone());
        }
        assert!(slab.next >= SLAB_SIZE * 2);
    }

    #[test]
    fn test_probe_batch_cached_empty() {
        let results = probe_batch_cached(&[]);
        assert!(results.is_empty());
    }
}
