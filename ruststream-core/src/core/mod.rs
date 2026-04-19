//! Core module - Core pipeline orchestration and error types

pub mod errors;
pub mod audio_graph;
pub mod instrumentation;
pub mod timeline;
pub mod batch_scheduler;

// Re-export error types
pub use errors::{MediaError, MediaErrorCode, MediaResult};
pub use batch_scheduler::{Job, probe_scheduled, run_scheduled, ConcatJob, concat_scheduled};
