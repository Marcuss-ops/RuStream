//! Audio module - Audio processing and mixing
//!
//! Provides SIMD-optimized audio processing kernels and gate utilities.

pub mod audio_bake;
pub mod audio_mix;
pub mod audio_resample;
pub mod gate_utils;
pub mod hot_kernels;

// Re-export
pub use gate_utils::{build_gate_expr_from_ranges, build_intro_only_gate_expr, AudioGateRange};
pub use hot_kernels::{audio_mix, apply_volume};
