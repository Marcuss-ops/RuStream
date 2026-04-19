//! Core module - Core pipeline orchestration and error types

pub mod errors;
pub mod audio_graph;
pub mod instrumentation;
pub mod timeline;

// Re-export error types
pub use errors::{MediaError, MediaErrorCode, MediaResult};
