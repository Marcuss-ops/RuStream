//! Core module - Core pipeline orchestration and error types

pub mod errors;

// Re-export error types
pub use errors::{MediaError, MediaErrorCode, MediaResult};
