//! io_uring-based batch file statistics (Linux, optional feature).
//!
//! The standard probe batch uses rayon + sequential `statx()` per file to
//! compute cache keys (mtime + size). With io_uring, **all N** `statx()`
//! calls are submitted to the kernel in a single `io_uring_enter()` syscall,
//! reducing round-trips from O(N) to O(ceil(N/RING_SIZE)).
//!
//! # When to use
//! - Batch probing > 16 files (below that, rayon overhead dominates)
//! - SSD/NVMe storage (latency-sensitive; HDDs already use prefetch)
//! - VPS with io_uring kernel support (Linux ≥ 5.1)
//!
//! # Feature flag
//! Activate with: `cargo build --features io-uring`
//!
//! # Integration
//! ```rust,ignore
//! use ruststream_core::io::uring::stat_batch;
//!
//! let stats = stat_batch(&["/a.mp4", "/b.mp4", "/c.mp4"])?;
//! for stat in stats {
//!     let s = stat?;
//!     println!("size={} mtime={}", s.size_bytes, s.mtime_secs);
//! }
//! ```

#![cfg(all(target_os = "linux", feature = "io-uring"))]

use std::ffi::CString;
use std::io;
use io_uring::{opcode, types, IoUring};

/// Metadata extracted by a `statx()` call.
#[derive(Debug, Clone)]
pub struct FileStat {
    /// File size in bytes.
    pub size_bytes: u64,
    /// Last modification time (seconds since Unix epoch).
    pub mtime_secs: i64,
    /// Nanoseconds component of mtime.
    pub mtime_nsecs: u32,
}

/// Maximum submissions per io_uring batch.
/// Must be a power of two; 256 fits comfortably in memory.
const RING_SIZE: u32 = 256;

/// Run N `statx()` calls in parallel via a single io_uring submission batch.
///
/// All paths are submitted together; the kernel executes them concurrently
/// (subject to storage parallelism). Results are collected from the
/// completion queue in the same order as `paths`.
///
/// # Errors
/// - Returns `Err` if io_uring setup fails (kernel too old, RLIMIT_MEMLOCK).
/// - Individual file errors are returned as `Err` in the result `Vec`.
pub fn stat_batch(paths: &[&str]) -> io::Result<Vec<io::Result<FileStat>>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let ring_cap = paths.len().min(RING_SIZE as usize) as u32;
    let mut ring = IoUring::new(ring_cap)?;

    // CStrings + statx buffers must stay alive until completions are reaped
    let cstrings: Vec<CString> = paths
        .iter()
        .map(|p| CString::new(*p).unwrap_or_default())
        .collect();

    // SAFETY: zeroed statx is a valid initial state (all fields 0)
    let mut statxbufs: Vec<libc::statx> =
        (0..paths.len())
            .map(|_| unsafe { std::mem::zeroed::<libc::statx>() })
            .collect();

    let mut results: Vec<Option<io::Result<FileStat>>> = vec![None; paths.len()];

    // Process in chunks of RING_SIZE to handle batches larger than the ring
    let mut i = 0;
    while i < paths.len() {
        let chunk_end = (i + ring_cap as usize).min(paths.len());
        let chunk_len = chunk_end - i;

        // ── Submit ────────────────────────────────────────────────────────────
        {
            let mut sq = ring.submission();
            for j in i..chunk_end {
                let sqe = opcode::Statx::new(
                    types::Fd(libc::AT_FDCWD),
                    cstrings[j].as_ptr(),
                    // Cast: types::Statx is a type alias for libc::statx
                    &mut statxbufs[j] as *mut libc::statx as *mut types::Statx,
                )
                // AT_STATX_SYNC_AS_STAT: standard sync behaviour
                .flags(libc::AT_STATX_SYNC_AS_STAT as _)
                // Only fetch size and mtime — skip uid/gid/blocks/etc.
                .mask((libc::STATX_MTIME | libc::STATX_SIZE) as _)
                .build()
                // Encode the path index in user_data for result mapping
                .user_data(j as u64);

                // SAFETY: sqe is valid; sq not concurrently modified
                unsafe {
                    sq.push(&sqe).map_err(|_| {
                        io::Error::new(io::ErrorKind::Other, "io_uring: SQ full")
                    })?;
                }
            }
        } // sq guard dropped here — submission queue now owned by ring

        // ── Wait for all completions in this chunk ────────────────────────────
        ring.submit_and_wait(chunk_len)?;

        // ── Harvest completions ───────────────────────────────────────────────
        let cq = ring.completion();
        for cqe in cq {
            let idx = cqe.user_data() as usize;
            let res = cqe.result();

            results[idx] = Some(if res < 0 {
                Err(io::Error::from_raw_os_error(-res))
            } else {
                let sx = &statxbufs[idx];
                Ok(FileStat {
                    size_bytes:  sx.stx_size,
                    mtime_secs:  sx.stx_mtime.tv_sec,
                    mtime_nsecs: sx.stx_mtime.tv_nsec,
                })
            });
        }

        i = chunk_end;
    }

    Ok(results
        .into_iter()
        .map(|r| r.unwrap_or_else(|| Err(io::Error::new(io::ErrorKind::Other, "missing cqe"))))
        .collect())
}

/// Returns `true` if the current kernel supports io_uring (Linux ≥ 5.1).
///
/// Attempts to create a tiny ring to probe support; closes it immediately.
pub fn io_uring_available() -> bool {
    IoUring::new(2).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_stat_batch_empty() {
        let result = stat_batch(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_stat_batch_existing_file() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"io_uring test payload 1234567890").unwrap();
        let path_str = f.path().to_str().unwrap().to_string();

        let results = stat_batch(&[&path_str]).unwrap();
        assert_eq!(results.len(), 1);

        let stat = results.into_iter().next().unwrap().unwrap();
        assert_eq!(stat.size_bytes, 32);
        assert!(stat.mtime_secs > 0);
    }

    #[test]
    fn test_stat_batch_missing_file() {
        let results = stat_batch(&["/tmp/__nonexistent_uring_test__.mp4"]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[test]
    fn test_stat_batch_multiple() {
        let files: Vec<NamedTempFile> = (0..8).map(|i| {
            let mut f = NamedTempFile::new().unwrap();
            f.write_all(&vec![i as u8; (i + 1) * 1024]).unwrap();
            f
        }).collect();
        let path_strs: Vec<String> = files.iter()
            .map(|f| f.path().to_str().unwrap().to_string())
            .collect();
        let path_refs: Vec<&str> = path_strs.iter().map(|s| s.as_str()).collect();

        let results = stat_batch(&path_refs).unwrap();
        assert_eq!(results.len(), 8);

        for (i, result) in results.iter().enumerate() {
            let stat = result.as_ref().unwrap();
            assert_eq!(stat.size_bytes, (i + 1) as u64 * 1024);
        }
    }

    #[test]
    fn test_io_uring_available() {
        // Just check it doesn't panic — may return false on old kernels
        let _ = io_uring_available();
    }
}
