//! Sync I/O operations — with Linux-specific fast-open extensions.

use std::path::Path;
use std::fs;

/// Read file bytes.
pub fn read_file_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    fs::read(path)
}

/// Write file bytes.
pub fn write_file_bytes(path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::write(path, data)
}

// ── Linux-specific: O_NOATIME ─────────────────────────────────────────────────

/// Open a file for reading with `O_NOATIME` (Linux-only).
///
/// `O_NOATIME` prevents the kernel from updating the access-time (atime)
/// inode field on every `open()`. For workloads that process thousands of
/// clips, this eliminates thousands of metadata writes to the filesystem,
/// reducing write amplification on SSD/NVMe and pressure on ext4 journaling.
///
/// # Fallback
/// If the caller does not own the file (EPERM), the flag is silently ignored
/// and the file is opened normally — so this function never fails solely
/// because of `O_NOATIME` permission.
///
/// On non-Linux platforms this is identical to `File::open`.
pub fn open_noatime(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::os::unix::{ffi::OsStrExt, io::FromRawFd};

        let cpath = CString::new(path.as_os_str().as_bytes())?;

        let fd = unsafe {
            libc::open(
                cpath.as_ptr(),
                libc::O_RDONLY | libc::O_NOATIME | libc::O_CLOEXEC,
            )
        };

        if fd >= 0 {
            return Ok(unsafe { std::fs::File::from_raw_fd(fd) });
        }

        let err = std::io::Error::last_os_error();
        // EPERM = noatime requires file ownership; fall back silently
        if err.raw_os_error() == Some(libc::EPERM) {
            return std::fs::File::open(path);
        }
        Err(err)
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::fs::File::open(path)
    }
}

/// Read a file's bytes with `O_NOATIME` on Linux.
///
/// Equivalent to [`read_file_bytes`] but skips atime updates.
pub fn read_noatime(path: &Path) -> std::io::Result<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;
        let mut file = open_noatime(path)?;
        let mut buf = Vec::with_capacity(
            std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(4096),
        );
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }
    #[cfg(not(target_os = "linux"))]
    read_file_bytes(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_write_roundtrip() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello rust").unwrap();
        let path = f.path().to_path_buf();
        let data = read_file_bytes(&path).unwrap();
        assert_eq!(data, b"hello rust");
    }

    #[test]
    fn test_open_noatime_exists() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"data").unwrap();
        // Must open successfully (fallback graceful on permission issues)
        let result = open_noatime(f.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_noatime_nonexistent() {
        let result = open_noatime(Path::new("/tmp/__nonexistent_rs__.mp4"));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_noatime() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"noatime content").unwrap();
        let data = read_noatime(f.path()).unwrap();
        assert_eq!(data, b"noatime content");
    }
}
