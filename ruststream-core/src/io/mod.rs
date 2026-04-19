//! I/O module - Async and sync I/O operations.

pub mod sync_io;
pub mod subprocess;
pub mod prefetch;

#[cfg(target_os = "linux")]
pub mod affinity;
#[cfg(target_os = "linux")]
pub mod splice;
#[cfg(target_os = "linux")]
pub mod fallocate;
#[cfg(all(target_os = "linux", feature = "io-uring"))]
pub mod uring;

// Re-export
pub use sync_io::{read_file_bytes, open_noatime, read_noatime};
pub use subprocess::{FfmpegCommand, ffmpeg_available, ffmpeg_version, temp_dir, temp_file};
pub use prefetch::{prefetch_sequential as prefetch, prefetch_batch};

#[cfg(target_os = "linux")]
pub use affinity::pin_to_physical_core;
#[cfg(target_os = "linux")]
pub use splice::{splice_copy, splice_concat, splice_available};
#[cfg(target_os = "linux")]
pub use fallocate::{preallocate, advise_dontneed, advise_dontneed_batch, estimate_output_size};

#[cfg(all(target_os = "linux", feature = "io-uring"))]
pub use uring::{stat_batch, io_uring_available, FileStat};
