//! Zero-copy file transfer via Linux `splice()`.
//!
//! `splice()` moves data between file descriptors entirely in the kernel —
//! the bytes never enter userspace RAM. For large video files (1–10 GB clips),
//! this eliminates the read() → kernel buffer → write() cycle that `std::io::copy`
//! performs, saving both CPU cycles and memory bandwidth.
//!
//! # Usage context
//! Call `splice_concat` when:
//! - Output container format is the same as input (no re-encode needed)
//! - Clips are format-compatible (codec, resolution, frame rate match)
//! - The goal is purely to join the raw bitstream
//!
//! For re-encode or container remux, FFmpeg subprocess is still required.
//!
//! # Linux internals
//! `splice()` requires at least one end to be a pipe. This module creates a
//! kernel pipe pair as the intermediary and operates in SPLICE_F_MOVE mode
//! to allow the kernel to hand off pages instead of copying them.

#![cfg(target_os = "linux")]

use std::fs::File;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;

/// Pipe buffer size: 512 KB per splice() call.
/// Larger chunks reduce syscall overhead; this matches typical kernel pipe capacity.
const SPLICE_CHUNK: usize = 512 * 1024;

// ── Core splice loop ──────────────────────────────────────────────────────────

/// Internal: drain all data from `src_fd` into `dst_fd` via a kernel pipe.
///
/// The pipe [pipe_r, pipe_w] must already be open before calling this.
fn splice_all(src_fd: RawFd, dst_fd: RawFd, pipe_r: RawFd, pipe_w: RawFd) -> io::Result<u64> {
    let mut total = 0u64;

    loop {
        // Phase 1 — splice N bytes from source file → pipe write end
        let n_in = unsafe {
            libc::splice(
                src_fd,
                std::ptr::null_mut(),
                pipe_w,
                std::ptr::null_mut(),
                SPLICE_CHUNK,
                libc::SPLICE_F_MOVE | libc::SPLICE_F_MORE,
            )
        };

        if n_in < 0 {
            return Err(io::Error::last_os_error());
        }
        if n_in == 0 {
            break; // EOF on source
        }

        // Phase 2 — drain exactly n_in bytes from pipe read end → destination
        let mut remaining = n_in as usize;
        while remaining > 0 {
            let n_out = unsafe {
                libc::splice(
                    pipe_r,
                    std::ptr::null_mut(),
                    dst_fd,
                    std::ptr::null_mut(),
                    remaining,
                    libc::SPLICE_F_MOVE,
                )
            };
            if n_out <= 0 {
                return Err(io::Error::last_os_error());
            }
            remaining -= n_out as usize;
            total += n_out as u64;
        }
    }

    Ok(total)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Copy all bytes from `src` to `dst` using zero-copy `splice()`.
///
/// Returns the number of bytes transferred.
///
/// # Example
/// ```rust,ignore
/// let mut src = File::open("input.mp4")?;
/// let mut dst = File::create("output.mp4")?;
/// let bytes = splice_copy(&mut src, &mut dst)?;
/// ```
pub fn splice_copy(src: &mut File, dst: &mut File) -> io::Result<u64> {
    let src_fd = src.as_raw_fd();
    let dst_fd = dst.as_raw_fd();

    let mut pipe_fds: [i32; 2] = [0; 2];
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let [pipe_r, pipe_w] = pipe_fds;

    let result = splice_all(src_fd, dst_fd, pipe_r, pipe_w);

    // Always close pipe FDs even on error
    unsafe {
        libc::close(pipe_r);
        libc::close(pipe_w);
    }

    result
}

/// Concatenate multiple source files into a single output file using `splice()`.
///
/// Each source is appended to `dst` in order with zero userspace copies.
/// Ideal for raw stream-copy concat of format-compatible clips.
///
/// # Returns
/// Total bytes written to `dst`.
pub fn splice_concat(src_paths: &[&Path], dst: &mut File) -> io::Result<u64> {
    if src_paths.is_empty() {
        return Ok(0);
    }

    let dst_fd = dst.as_raw_fd();

    // Create one shared pipe for all files (amortises pipe creation cost)
    let mut pipe_fds: [i32; 2] = [0; 2];
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let [pipe_r, pipe_w] = pipe_fds;

    let mut total = 0u64;

    for path in src_paths {
        let src = File::open(path).map_err(|e| {
            unsafe { libc::close(pipe_r); libc::close(pipe_w); }
            e
        })?;

        let n = splice_all(src.as_raw_fd(), dst_fd, pipe_r, pipe_w)
            .map_err(|e| {
                unsafe { libc::close(pipe_r); libc::close(pipe_w); }
                e
            })?;

        total += n;
    }

    unsafe {
        libc::close(pipe_r);
        libc::close(pipe_w);
    }

    log::debug!("splice_concat: {} files, {} bytes", src_paths.len(), total);
    Ok(total)
}

/// Returns `true` if `splice()` is likely to be available on this kernel.
///
/// splice() requires Linux ≥ 2.6.17. All modern VPS kernels satisfy this.
pub fn splice_available() -> bool {
    // Always true on Linux — we compile this file only for Linux
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::NamedTempFile;

    fn make_temp_with(data: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(data).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_splice_copy_basic() {
        let src_file = make_temp_with(b"splice test data 1234567890");
        let mut dst_file = NamedTempFile::new().unwrap();

        let mut src = File::open(src_file.path()).unwrap();
        let mut dst = dst_file.reopen().unwrap();

        let n = splice_copy(&mut src, &mut dst).unwrap();
        assert_eq!(n, 27);

        let mut result = Vec::new();
        let mut dst_read = File::open(dst_file.path()).unwrap();
        dst_read.read_to_end(&mut result).unwrap();
        assert_eq!(result, b"splice test data 1234567890");
    }

    #[test]
    fn test_splice_copy_empty() {
        let src_file = make_temp_with(b"");
        let mut dst_file = NamedTempFile::new().unwrap();

        let mut src = File::open(src_file.path()).unwrap();
        let mut dst = dst_file.reopen().unwrap();

        let n = splice_copy(&mut src, &mut dst).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_splice_concat_multiple() {
        let f1 = make_temp_with(b"AAAA");
        let f2 = make_temp_with(b"BBBB");
        let f3 = make_temp_with(b"CCCC");
        let mut dst_file = NamedTempFile::new().unwrap();
        let mut dst = dst_file.reopen().unwrap();

        let paths = [f1.path(), f2.path(), f3.path()];
        let n = splice_concat(&paths, &mut dst).unwrap();
        assert_eq!(n, 12);

        let mut result = Vec::new();
        File::open(dst_file.path()).unwrap().read_to_end(&mut result).unwrap();
        assert_eq!(result, b"AAAABBBBCCCC");
    }

    #[test]
    fn test_splice_concat_empty_list() {
        let mut dst_file = NamedTempFile::new().unwrap();
        let mut dst = dst_file.reopen().unwrap();
        let n = splice_concat(&[], &mut dst).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_splice_available() {
        assert!(splice_available());
    }

    #[test]
    fn test_splice_concat_large() {
        // 1 MB of data across 4 files
        let chunk = vec![0xABu8; 256 * 1024];
        let files: Vec<NamedTempFile> = (0..4).map(|_| make_temp_with(&chunk)).collect();
        let paths: Vec<&Path> = files.iter().map(|f| f.path()).collect();
        let mut dst_file = NamedTempFile::new().unwrap();
        let mut dst = dst_file.reopen().unwrap();

        let n = splice_concat(&paths, &mut dst).unwrap();
        assert_eq!(n, 4 * 256 * 1024);
    }
}
