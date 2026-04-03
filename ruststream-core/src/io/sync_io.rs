//! Sync I/O operations

use std::path::Path;
use std::fs;

/// Read file bytes
pub fn read_file_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    fs::read(path)
}

/// Write file bytes
pub fn write_file_bytes(path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::write(path, data)
}
