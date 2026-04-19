//! Audio module - Audio processing and mixing
//!
//! Provides SIMD-optimized audio processing kernels, gate utilities,
//! RAII buffer pool, and native Symphonia-based audio decoding.

pub mod audio_bake;
pub mod audio_mix;
pub mod audio_resample;
pub mod gate_utils;
pub mod hot_kernels;
pub mod buffer_pool;
pub mod native_decode;

// Re-export
pub use gate_utils::{build_gate_expr_from_ranges, build_intro_only_gate_expr, AudioGateRange};
pub use hot_kernels::{audio_mix, apply_volume};
pub use buffer_pool::{PooledBuffer, audio_mix_pooled, FRAME_SAMPLES, BLOCK_SAMPLES};
pub use native_decode::{DecodedAudio, decode_audio_file, native_decoding_available};
