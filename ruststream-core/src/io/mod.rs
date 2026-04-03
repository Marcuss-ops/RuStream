//! I/O module - Async and sync I/O operations

pub mod sync_io;

// Re-export
pub use sync_io::read_file_bytes;
