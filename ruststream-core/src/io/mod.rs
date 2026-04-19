//! I/O module - Async and sync I/O operations

pub mod sync_io;
pub mod subprocess;
pub mod prefetch;

// Re-export
pub use sync_io::read_file_bytes;
pub use subprocess::{FfmpegCommand, ffmpeg_available, ffmpeg_version, temp_dir, temp_file};
pub use prefetch::{prefetch, prefetch_paths, prefetch_batch, cpu_prefetch};
