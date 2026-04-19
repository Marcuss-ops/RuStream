//! `fallocate()` pre-allocation and `POSIX_FADV_DONTNEED` post-processing.
//!
//! Two complementary syscalls to maximise I/O efficiency on Linux:
//!
//! ## fallocate — pre-allocate output files
//! Reserves contiguous disk blocks **before** writing begins. Writing within
//! the pre-allocated range never triggers block allocation stalls (which can
//! spike latency by 5–50 ms on a busy EXT4/XFS filesystem).
//!
//! ## POSIX_FADV_DONTNEED — free page cache after processing
//! After fully processing an input file, tell the kernel it can reclaim the
//! pages immediately. Critical on 512 MB VPS: without this, a 200 MB clip
//! stays in page cache while the next clip is being processed, starving the
//! allocator.
//!
//! Both calls are **best-effort**: errors from unsupported filesystems
//! (tmpfs, NFS, FUSE) are logged as debug and ignored.

#![cfg(target_os = "linux")]

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;

// ════════════════════════════════════════════════════════════════════════════
// fallocate — contiguous disk pre-allocation
// ════════════════════════════════════════════════════════════════════════════

/// Pre-allocate `size_bytes` of contiguous disk space for a new output file.
///
/// Creates (or truncates) the file at `path`, then calls `fallocate(2)` with
/// mode=0 (allocate-and-initialize). This reserves blocks on the filesystem
/// without creating sparse areas, so subsequent sequential writes are O(1)
/// in terms of block allocation.
///
/// # Returns
/// The open, writable `File` seeked to position 0 — ready for writing.
///
/// # Errors
/// Returns `Err` only for real I/O errors (permission, disk full, etc.).
/// Unsupported filesystem (`EOPNOTSUPP`, `ENOSYS`) is handled gracefully.
pub fn preallocate(path: &Path, size_bytes: u64) -> io::Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;

    if size_bytes == 0 {
        return Ok(file);
    }

    let fd = file.as_raw_fd();
    let ret = unsafe {
        libc::fallocate(
            fd,
            0,                       // mode=0: allocate and update file size
            0,                       // offset: start from byte 0
            size_bytes as libc::off_t,
        )
    };

    if ret != 0 {
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            // Filesystem does not support fallocate — not fatal
            Some(libc::EOPNOTSUPP) | Some(libc::ENOSYS) => {
                log::debug!("fallocate not supported ({}), skipping pre-allocation", err);
            }
            // ENOSPC: disk full — this IS a real error
            _ => return Err(err),
        }
    } else {
        log::debug!("fallocate: pre-allocated {} bytes for {:?}", size_bytes, path);
    }

    Ok(file)
}

/// Estimate an output file size from the sum of input sizes with a multiplier.
///
/// Useful when exact output size is unknown (e.g. transcoding). A multiplier
/// of `1.05` adds 5% safety margin for container overhead.
pub fn estimate_output_size(input_paths: &[&Path], multiplier: f64) -> u64 {
    let total: u64 = input_paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).map(|m| m.len()).ok())
        .sum();
    (total as f64 * multiplier.max(1.0)) as u64
}

// ════════════════════════════════════════════════════════════════════════════
// POSIX_FADV_DONTNEED — page cache eviction after processing
// ════════════════════════════════════════════════════════════════════════════

/// Signal the kernel that the entire file at `path` is no longer needed.
///
/// After this call, the kernel is free to evict all cached pages for the file
/// on the next memory pressure event. For 512 MB VPS environments this makes
/// a substantial difference when processing files sequentially: without it,
/// the first clip's 200 MB footprint stays hot while the second clip is probed.
///
/// This is a **hint** — the kernel may ignore it. No error is propagated.
pub fn advise_dontneed(path: &Path) {
    let fd_result = std::fs::File::open(path);
    let file = match fd_result {
        Ok(f) => f,
        Err(e) => {
            log::debug!("advise_dontneed: cannot open {:?}: {}", path, e);
            return;
        }
    };

    let size = match std::fs::metadata(path) {
        Ok(m) => m.len() as libc::off_t,
        Err(_) => 0,
    };

    let ret = unsafe {
        libc::posix_fadvise(
            file.as_raw_fd(),
            0,
            size,
            libc::POSIX_FADV_DONTNEED,
        )
    };

    if ret != 0 {
        log::debug!(
            "FADV_DONTNEED for {:?} failed: {}",
            path,
            io::Error::from_raw_os_error(ret)
        );
    } else {
        log::trace!("FADV_DONTNEED: evicted {} bytes of page cache for {:?}", size, path);
    }
}

/// Apply FADV_DONTNEED to a list of paths in parallel.
///
/// Call this after a batch probe or concat stage completes. Runs via rayon
/// so many files can be evicted concurrently on a multi-core VPS.
pub fn advise_dontneed_batch(paths: &[&Path]) {
    use rayon::prelude::*;
    paths.par_iter().for_each(|&p| advise_dontneed(p));
}

/// Apply FADV_DONTNEED to an already-open file descriptor (range version).
///
/// Use when you have the `File` handle and want to evict only a portion of the
/// file (e.g. the audio portion after baking it separately).
pub fn advise_dontneed_fd(file: &File, offset: u64, len: u64) {
    let ret = unsafe {
        libc::posix_fadvise(
            file.as_raw_fd(),
            offset as libc::off_t,
            len as libc::off_t,
            libc::POSIX_FADV_DONTNEED,
        )
    };
    if ret != 0 {
        log::debug!(
            "FADV_DONTNEED(fd, {}, {}) failed: {}",
            offset, len,
            io::Error::from_raw_os_error(ret)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_preallocate_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("output.mp4");
        let file = preallocate(&path, 1024 * 1024).unwrap(); // 1 MB
        assert!(path.exists());
        drop(file);
    }

    #[test]
    fn test_preallocate_zero_size() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.mp4");
        let _file = preallocate(&path, 0).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_preallocate_write_after() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("write.mp4");
        let mut file = preallocate(&path, 4096).unwrap();
        use std::io::Write;
        file.write_all(b"hello preallocated").unwrap();
        file.flush().unwrap();
    }

    #[test]
    fn test_estimate_output_size() {
        let mut f1 = NamedTempFile::new().unwrap();
        f1.write_all(&vec![0u8; 1000]).unwrap();
        let mut f2 = NamedTempFile::new().unwrap();
        f2.write_all(&vec![0u8; 2000]).unwrap();

        let size = estimate_output_size(&[f1.path(), f2.path()], 1.10);
        assert!(size >= 3300 && size <= 3310, "got {}", size);
    }

    #[test]
    fn test_advise_dontneed_existing() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&vec![1u8; 65536]).unwrap();
        // Must not panic
        advise_dontneed(f.path());
    }

    #[test]
    fn test_advise_dontneed_nonexistent() {
        // Must not panic even for missing file
        advise_dontneed(Path::new("/tmp/__nonexistent_advise__.mp4"));
    }

    #[test]
    fn test_advise_dontneed_batch() {
        let files: Vec<NamedTempFile> = (0..3).map(|i| {
            let mut f = NamedTempFile::new().unwrap();
            f.write_all(&vec![i as u8; 4096]).unwrap();
            f
        }).collect();
        let paths: Vec<&Path> = files.iter().map(|f| f.path()).collect();
        advise_dontneed_batch(&paths); // must not panic
    }

    #[test]
    fn test_advise_dontneed_fd() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&vec![0u8; 8192]).unwrap();
        let file = File::open(f.path()).unwrap();
        advise_dontneed_fd(&file, 0, 4096); // must not panic
    }
}
