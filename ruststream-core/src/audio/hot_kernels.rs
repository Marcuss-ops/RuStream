//! SIMD-optimized audio processing kernels
//!
//! Provides CPU feature detection and SIMD-accelerated audio operations.
//!
//! # Performance Optimizations
//!
//! - **Single-pass mixing**: fill + mix + clip combined into one loop (3x bandwidth reduction)
//! - **Fused clipping in SIMD**: clip during last input's FMA, eliminates separate clipping pass
//! - **Thread-local buffer pools**: zero lock contention for per-thread allocations
//! - **AVX-512 masked tail**: no scalar fallback for remaining elements
//! - **Rayon parallelism**: auto-parallelized for large buffers
//! - **AVX-512**: 512-bit vectors (16x f32 operations) + FMA
//! - **AVX2**: 256-bit vectors (8x f32 operations) + FMA
//! - **SSE4.1**: 128-bit vectors (4x f32 operations) + FMA
//! - **Scalar**: Fallback for unsupported CPUs (auto-vectorized by compiler)

// Allow unsafe code for SIMD intrinsics - safety is guaranteed by CPU feature detection
#![allow(unsafe_code)]

use rayon::prelude::*;
use std::cell::RefCell;
use std::sync::LazyLock;

// ============================================================================
// CPU Feature Detection (cached at startup)
// ============================================================================

/// CPU features detected at runtime
#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    pub has_avx512: bool,
    pub has_avx2: bool,
    pub has_sse41: bool,
    pub has_fma: bool,
    /// ARM64 NEON — always true on AArch64 (mandatory in the base ISA)
    pub has_neon: bool,
}

/// Detect CPU features (cached via LazyLock)
fn detect_cpu_features() -> CpuFeatures {
    #[cfg(target_arch = "x86_64")]
    {
        CpuFeatures {
            has_avx512: is_x86_feature_detected!("avx512f"),
            has_avx2: is_x86_feature_detected!("avx2"),
            has_sse41: is_x86_feature_detected!("sse4.1"),
            has_fma: is_x86_feature_detected!("fma"),
            has_neon: false,
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // NEON is mandatory on every AArch64 CPU — no runtime detection needed.
        CpuFeatures {
            has_avx512: false,
            has_avx2: false,
            has_sse41: false,
            has_fma: false,
            has_neon: true,
        }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        CpuFeatures {
            has_avx512: false,
            has_avx2: false,
            has_sse41: false,
            has_fma: false,
            has_neon: false,
        }
    }
}

/// Cached CPU features - detected once at startup
static CPU_FEATURES: LazyLock<CpuFeatures> = LazyLock::new(detect_cpu_features);

/// Get cached CPU features
#[inline]
pub fn cpu_features() -> CpuFeatures {
    *CPU_FEATURES
}

// ============================================================================
// Public API (single-pass optimized mixing)
// ============================================================================

/// Mix multiple audio streams with SIMD optimization.
///
/// Dispatch order (best → fallback):
/// - ARM64:  NEON   (always available, 4×f32, vfmaq_f32)
/// - x86_64: AVX-512 + FMA  (16×f32, masked tail)
/// - x86_64: AVX2   + FMA  (8×f32)
/// - x86_64: SSE4.1 + FMA  (4×f32)
/// - Any:    scalar (auto-vectorized by LLVM)
#[inline]
pub fn audio_mix(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    if output.is_empty() {
        return;
    }

    // Fast path: no inputs means just zero the buffer
    if inputs.is_empty() {
        output.fill(0.0);
        return;
    }

    // ── ARM64: NEON is mandatory on every AArch64 CPU ────────────────────────
    #[cfg(target_arch = "aarch64")]
    // SAFETY: NEON is part of the AArch64 base ISA — always present
    return unsafe { audio_mix_neon(output, inputs, volumes) };

    #[cfg(target_arch = "x86_64")]
    {
        let features = &*CPU_FEATURES;
        if features.has_avx512 && features.has_fma {
            // SAFETY: CPU feature verified via detection
            return unsafe { audio_mix_avx512(output, inputs, volumes) };
        } else if features.has_avx2 && features.has_fma {
            // SAFETY: CPU feature verified via detection
            return unsafe { audio_mix_avx2(output, inputs, volumes) };
        } else if features.has_sse41 && features.has_fma {
            // SAFETY: CPU feature verified via detection
            return unsafe { audio_mix_sse41(output, inputs, volumes) };
        }
    }

    // Scalar fallback with auto-vectorization
    #[cfg(not(target_arch = "aarch64"))]
    audio_mix_scalar(output, inputs, volumes)
}

/// Apply volume/gain with SIMD optimization.
#[inline]
pub fn apply_volume(buffer: &mut [f32], gain: f32) {
    if buffer.is_empty() {
        return;
    }

    // Fast path: unity gain is no-op
    if (gain - 1.0).abs() < 1e-6 {
        return;
    }

    // Fast path: zero gain
    if gain == 0.0 {
        buffer.fill(0.0);
        return;
    }

    // ── ARM64: NEON ───────────────────────────────────────────────────────────
    #[cfg(target_arch = "aarch64")]
    // SAFETY: NEON is part of the AArch64 base ISA
    return unsafe { apply_volume_neon(buffer, gain) };

    #[cfg(target_arch = "x86_64")]
    {
        let features = &*CPU_FEATURES;
        if features.has_avx512 && features.has_fma {
            return unsafe { apply_volume_avx512(buffer, gain) };
        } else if features.has_avx2 && features.has_fma {
            return unsafe { apply_volume_avx2(buffer, gain) };
        } else if features.has_sse41 && features.has_fma {
            return unsafe { apply_volume_sse41(buffer, gain) };
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    apply_volume_scalar(buffer, gain)
}

// ============================================================================
// Thread-Local Buffer Pool (zero lock contention)
// ============================================================================

thread_local! {
    static THREAD_BUFFERS: RefCell<Vec<Vec<f32>>> = RefCell::new(Vec::with_capacity(4));
}

/// Thread-safe buffer pool with zero lock contention
pub struct BufferPool {
    pool_size: usize,
}

impl BufferPool {
    /// Create new buffer pool with specified buffer size
    pub fn new(pool_size: usize) -> Self {
        Self { pool_size }
    }

    /// Acquire a buffer from the thread-local pool (or allocate new if empty)
    pub fn acquire(&self) -> Vec<f32> {
        THREAD_BUFFERS.with(|pool| {
            let mut buffers = pool.borrow_mut();
            if let Some(mut buffer) = buffers.pop() {
                buffer.resize(self.pool_size, 0.0f32);
                buffer
            } else {
                vec![0.0f32; self.pool_size]
            }
        })
    }

    /// Return a buffer to the thread-local pool for reuse
    pub fn release(&self, mut buffer: Vec<f32>) {
        if buffer.capacity() >= self.pool_size && buffer.capacity() <= self.pool_size * 4 {
            buffer.clear();
            THREAD_BUFFERS.with(|pool| {
                let mut buffers = pool.borrow_mut();
                if buffers.len() < 8 {
                    buffers.push(buffer);
                }
            });
        }
    }
}

// ============================================================================
// Scalar Implementations (single-pass, auto-vectorizable)
// ============================================================================

/// Single-pass scalar mixing: combines fill + mix + clip into ONE loop.
/// Compiler auto-vectorizes this with `-C target-cpu=native`.
fn audio_mix_scalar(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    const PARALLEL_THRESHOLD: usize = 32768;
    const CHUNK_SIZE: usize = 4096;

    if output.len() >= PARALLEL_THRESHOLD {
        output.par_chunks_mut(CHUNK_SIZE).for_each(|chunk| {
            mix_and_clip_chunk(chunk, inputs, volumes);
        });
    } else {
        mix_and_clip_chunk(output, inputs, volumes);
    }
}

/// Single-pass mix + clip for a chunk (zero extra allocations, zero extra passes)
#[inline]
fn mix_and_clip_chunk(chunk: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    const UNROLL: usize = 8;
    let len = chunk.len();
    let unroll_len = len / UNROLL * UNROLL;

    // Unrolled loop for better ILP
    for i in (0..unroll_len).step_by(UNROLL) {
        for j in 0..UNROLL {
            let idx = i + j;
            let mut sum: f32 = 0.0;
            for (input, &volume) in inputs.iter().zip(volumes.iter()) {
                if idx < input.len() {
                    sum += input[idx] * volume;
                }
            }
            // Branchless clamp
            chunk[idx] = if sum > 1.0 { 1.0 } else if sum < -1.0 { -1.0 } else { sum };
        }
    }

    // Tail
    for idx in unroll_len..len {
        let mut sum: f32 = 0.0;
        for (input, &volume) in inputs.iter().zip(volumes.iter()) {
            if idx < input.len() {
                sum += input[idx] * volume;
            }
        }
        chunk[idx] = if sum > 1.0 { 1.0 } else if sum < -1.0 { -1.0 } else { sum };
    }
}

/// Apply volume/gain to audio buffer (scalar with Rayon for large buffers)
fn apply_volume_scalar(buffer: &mut [f32], gain: f32) {
    const PARALLEL_THRESHOLD: usize = 65536;
    const CHUNK_SIZE: usize = 8192;

    if buffer.len() >= PARALLEL_THRESHOLD {
        buffer.par_chunks_mut(CHUNK_SIZE).for_each(|chunk| {
            for sample in chunk.iter_mut() {
                *sample *= gain;
            }
        });
    } else {
        for sample in buffer.iter_mut() {
            *sample *= gain;
        }
    }
}

// ============================================================================
// SSE4.1 Implementations (128-bit SIMD - 4x f32)
// OPTIMIZATION: Single-pass - clip fused into last input's mixing loop
// ============================================================================

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1", enable = "fma")]
unsafe fn audio_mix_sse41(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    use std::arch::x86_64::*;

    let len = output.len();
    let simd_len = len / 4 * 4;
    let last_input_idx = inputs.len().saturating_sub(1);

    for (input_idx, (input, &volume)) in inputs.iter().zip(volumes.iter()).enumerate() {
        let vol_vec = _mm_set1_ps(volume);

        if input_idx == 0 {
            for i in (0..simd_len).step_by(4) {
                let inp_vec = _mm_loadu_ps(&input[i]);
                let scaled = _mm_mul_ps(inp_vec, vol_vec);
                _mm_storeu_ps(&mut output[i], scaled);
            }
        } else if input_idx < last_input_idx {
            for i in (0..simd_len).step_by(4) {
                let inp_vec = _mm_loadu_ps(&input[i]);
                let out_vec = _mm_loadu_ps(&output[i]);
                let result = _mm_fmadd_ps(inp_vec, vol_vec, out_vec);
                _mm_storeu_ps(&mut output[i], result);
            }
        } else {
            // Last input: mix + clip in ONE pass (eliminates separate clip loop)
            let min_vec = _mm_set1_ps(-1.0);
            let max_vec = _mm_set1_ps(1.0);
            for i in (0..simd_len).step_by(4) {
                let inp_vec = _mm_loadu_ps(&input[i]);
                let out_vec = _mm_loadu_ps(&output[i]);
                let mixed = _mm_fmadd_ps(inp_vec, vol_vec, out_vec);
                let clamped = _mm_min_ps(_mm_max_ps(mixed, min_vec), max_vec);
                _mm_storeu_ps(&mut output[i], clamped);
            }
        }
    }

    // Tail: mix + clip for remaining elements
    let tail_len = len - simd_len;
    if tail_len > 0 {
        for i in simd_len..len {
            let mut sum: f32 = 0.0;
            for (input, &volume) in inputs.iter().zip(volumes.iter()) {
                if i < input.len() {
                    sum += input[i] * volume;
                }
            }
            output[i] = if sum > 1.0 { 1.0 } else if sum < -1.0 { -1.0 } else { sum };
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1", enable = "fma")]
unsafe fn apply_volume_sse41(buffer: &mut [f32], gain: f32) {
    use std::arch::x86_64::*;

    let len = buffer.len();
    let simd_len = len / 4 * 4;
    let gain_vec = _mm_set1_ps(gain);

    for i in (0..simd_len).step_by(4) {
        let vec = _mm_loadu_ps(&buffer[i]);
        let result = _mm_mul_ps(vec, gain_vec);
        _mm_storeu_ps(&mut buffer[i], result);
    }

    for sample in buffer.iter_mut().take(len).skip(simd_len) {
        *sample *= gain;
    }
}

// ============================================================================
// AVX2 Implementations (256-bit SIMD - 8x f32)
// OPTIMIZATION: Single-pass - clip fused into last input's mixing loop
// ============================================================================

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn audio_mix_avx2(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    use std::arch::x86_64::*;

    let len = output.len();
    let simd_len = len / 8 * 8;
    let last_input_idx = inputs.len().saturating_sub(1);

    // Prefetch distance: 8 AVX2 iterations × 8 f32 = 64 f32 = 256 bytes = 4 cache lines
    // Hides ~100-300 ns of DRAM latency on cold-cache large buffers.
    const PF_DIST: usize = 64;

    for (input_idx, (input, &volume)) in inputs.iter().zip(volumes.iter()).enumerate() {
        let vol_vec = _mm256_set1_ps(volume);

        if input_idx == 0 {
            for i in (0..simd_len).step_by(8) {
                // Software prefetch: bring next data into L1/L2 while working on current
                if i + PF_DIST < input.len() {
                    _mm_prefetch(input.as_ptr().add(i + PF_DIST) as *const i8, _MM_HINT_T0);
                }
                let inp_vec = _mm256_loadu_ps(&input[i]);
                let scaled = _mm256_mul_ps(inp_vec, vol_vec);
                _mm256_storeu_ps(&mut output[i], scaled);
            }
        } else if input_idx < last_input_idx {
            for i in (0..simd_len).step_by(8) {
                if i + PF_DIST < input.len() {
                    _mm_prefetch(input.as_ptr().add(i + PF_DIST) as *const i8, _MM_HINT_T0);
                }
                let inp_vec = _mm256_loadu_ps(&input[i]);
                let out_vec = _mm256_loadu_ps(&output[i]);
                let result = _mm256_fmadd_ps(inp_vec, vol_vec, out_vec);
                _mm256_storeu_ps(&mut output[i], result);
            }
        } else {
            // Last input: mix + clip in ONE pass
            let min_vec = _mm256_set1_ps(-1.0);
            let max_vec = _mm256_set1_ps(1.0);
            for i in (0..simd_len).step_by(8) {
                if i + PF_DIST < input.len() {
                    _mm_prefetch(input.as_ptr().add(i + PF_DIST) as *const i8, _MM_HINT_T0);
                }
                let inp_vec = _mm256_loadu_ps(&input[i]);
                let out_vec = _mm256_loadu_ps(&output[i]);
                let mixed = _mm256_fmadd_ps(inp_vec, vol_vec, out_vec);
                let clamped = _mm256_min_ps(_mm256_max_ps(mixed, min_vec), max_vec);
                _mm256_storeu_ps(&mut output[i], clamped);
            }
        }
    }

    // Tail
    let tail_len = len - simd_len;
    if tail_len > 0 {
        for i in simd_len..len {
            let mut sum: f32 = 0.0;
            for (input, &volume) in inputs.iter().zip(volumes.iter()) {
                if i < input.len() {
                    sum += input[i] * volume;
                }
            }
            output[i] = if sum > 1.0 { 1.0 } else if sum < -1.0 { -1.0 } else { sum };
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn apply_volume_avx2(buffer: &mut [f32], gain: f32) {
    use std::arch::x86_64::*;

    let len = buffer.len();
    let simd_len = len / 8 * 8;
    let gain_vec = _mm256_set1_ps(gain);

    for i in (0..simd_len).step_by(8) {
        let vec = _mm256_loadu_ps(&buffer[i]);
        let result = _mm256_mul_ps(vec, gain_vec);
        _mm256_storeu_ps(&mut buffer[i], result);
    }

    for sample in buffer.iter_mut().take(len).skip(simd_len) {
        *sample *= gain;
    }
}

// ============================================================================
// AVX-512 Implementations (512-bit SIMD - 16x f32)
// OPTIMIZATION: Single-pass + masked tail (no scalar fallback for remainders)
// ============================================================================

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f", enable = "fma")]
unsafe fn audio_mix_avx512(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    use std::arch::x86_64::*;

    let len = output.len();
    let simd_len = len / 16 * 16;
    let last_input_idx = inputs.len().saturating_sub(1);

    // Prefetch distance: 4 AVX-512 iterations × 16 f32 = 64 f32 = 256 bytes = 4 cache lines.
    // On a modern server with 64-byte cache lines this hides ~200-400 ns DRAM latency.
    const PF_DIST: usize = 64;

    for (input_idx, (input, &volume)) in inputs.iter().zip(volumes.iter()).enumerate() {
        let vol_vec = _mm512_set1_ps(volume);

        if input_idx == 0 {
            for i in (0..simd_len).step_by(16) {
                if i + PF_DIST < input.len() {
                    _mm_prefetch(input.as_ptr().add(i + PF_DIST) as *const i8, _MM_HINT_T0);
                }
                let inp_vec = _mm512_loadu_ps(&input[i]);
                let scaled = _mm512_mul_ps(inp_vec, vol_vec);
                _mm512_storeu_ps(&mut output[i], scaled);
            }
        } else if input_idx < last_input_idx {
            for i in (0..simd_len).step_by(16) {
                if i + PF_DIST < input.len() {
                    _mm_prefetch(input.as_ptr().add(i + PF_DIST) as *const i8, _MM_HINT_T0);
                }
                let inp_vec = _mm512_loadu_ps(&input[i]);
                let out_vec = _mm512_loadu_ps(&output[i]);
                let result = _mm512_fmadd_ps(inp_vec, vol_vec, out_vec);
                _mm512_storeu_ps(&mut output[i], result);
            }
        } else {
            // Last input: mix + clip in ONE pass
            let min_vec = _mm512_set1_ps(-1.0);
            let max_vec = _mm512_set1_ps(1.0);
            for i in (0..simd_len).step_by(16) {
                if i + PF_DIST < input.len() {
                    _mm_prefetch(input.as_ptr().add(i + PF_DIST) as *const i8, _MM_HINT_T0);
                }
                let inp_vec = _mm512_loadu_ps(&input[i]);
                let out_vec = _mm512_loadu_ps(&output[i]);
                let mixed = _mm512_fmadd_ps(inp_vec, vol_vec, out_vec);
                let clamped = _mm512_min_ps(_mm512_max_ps(mixed, min_vec), max_vec);
                _mm512_storeu_ps(&mut output[i], clamped);
            }
        }
    }

    // AVX-512 masked tail: handles 0-15 remaining elements without scalar loop
    let remaining = len - simd_len;
    if remaining > 0 {
        let mask = (1u16 << remaining) - 1;

        // Zero the tail first
        let zero = _mm512_setzero_ps();
        _mm512_mask_storeu_ps(&mut output[simd_len], mask, zero);

        // Accumulate each input with mask
        for (input, &volume) in inputs.iter().zip(volumes.iter()) {
            let vol_vec = _mm512_set1_ps(volume);
            let inp_vec = _mm512_maskz_loadu_ps(mask, &input[simd_len]);
            let out_vec = _mm512_loadu_ps(&output[simd_len]);
            let result = _mm512_fmadd_ps(inp_vec, vol_vec, out_vec);
            _mm512_storeu_ps(&mut output[simd_len], result);
        }

        // Clip tail with mask
        let min_vec = _mm512_set1_ps(-1.0);
        let max_vec = _mm512_set1_ps(1.0);
        let tail_vec = _mm512_loadu_ps(&output[simd_len]);
        let clamped = _mm512_min_ps(_mm512_max_ps(tail_vec, min_vec), max_vec);
        _mm512_mask_storeu_ps(&mut output[simd_len], mask, clamped);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f", enable = "fma")]
unsafe fn apply_volume_avx512(buffer: &mut [f32], gain: f32) {
    use std::arch::x86_64::*;

    let len = buffer.len();
    let simd_len = len / 16 * 16;
    let gain_vec = _mm512_set1_ps(gain);

    for i in (0..simd_len).step_by(16) {
        let vec = _mm512_loadu_ps(&buffer[i]);
        let result = _mm512_mul_ps(vec, gain_vec);
        _mm512_storeu_ps(&mut buffer[i], result);
    }

    // Masked tail
    let remaining = len - simd_len;
    if remaining > 0 {
        let mask = (1u16 << remaining) - 1;
        let vec = _mm512_maskz_loadu_ps(mask, &buffer[simd_len]);
        let result = _mm512_mul_ps(vec, gain_vec);
        _mm512_mask_storeu_ps(&mut buffer[simd_len], mask, result);
    }
}

// ============================================================================
// ARM64 NEON Implementations (128-bit SIMD — 4×f32 per register)
// NEON is mandatory on every AArch64 CPU (Apple Silicon, AWS Graviton,
// Ampere Altra, etc.).  vfmaq_f32 is the NEON equivalent of FMA.
// ============================================================================

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn audio_mix_neon(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    use std::arch::aarch64::*;

    let len = output.len();
    let simd_len = len / 4 * 4;
    let last_input_idx = inputs.len().saturating_sub(1);

    for (input_idx, (input, &volume)) in inputs.iter().zip(volumes.iter()).enumerate() {
        let vol_vec = vdupq_n_f32(volume);

        if input_idx == 0 {
            // First input: scale into output (no accumulate)
            for i in (0..simd_len).step_by(4) {
                let inp = vld1q_f32(input.as_ptr().add(i));
                let scaled = vmulq_f32(inp, vol_vec);
                vst1q_f32(output.as_mut_ptr().add(i), scaled);
            }
        } else if input_idx < last_input_idx {
            // Middle inputs: FMA accumulate
            for i in (0..simd_len).step_by(4) {
                let inp = vld1q_f32(input.as_ptr().add(i));
                let out = vld1q_f32(output.as_ptr().add(i));
                // vfmaq_f32(acc, a, b) = acc + a * b
                let result = vfmaq_f32(out, inp, vol_vec);
                vst1q_f32(output.as_mut_ptr().add(i), result);
            }
        } else {
            // Last input: FMA + clamp in ONE pass (fused mix+clip)
            let min_vec = vdupq_n_f32(-1.0);
            let max_vec = vdupq_n_f32(1.0);
            for i in (0..simd_len).step_by(4) {
                let inp = vld1q_f32(input.as_ptr().add(i));
                let out = vld1q_f32(output.as_ptr().add(i));
                let mixed = vfmaq_f32(out, inp, vol_vec);
                // clamp: max(min_vec, min(max_vec, mixed))
                let clamped = vminq_f32(vmaxq_f32(mixed, min_vec), max_vec);
                vst1q_f32(output.as_mut_ptr().add(i), clamped);
            }
        }
    }

    // Scalar tail (0-3 remaining samples)
    for idx in simd_len..len {
        let mut sum = 0.0f32;
        for (input, &vol) in inputs.iter().zip(volumes.iter()) {
            if idx < input.len() {
                sum += input[idx] * vol;
            }
        }
        output[idx] = sum.clamp(-1.0, 1.0);
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn apply_volume_neon(buffer: &mut [f32], gain: f32) {
    use std::arch::aarch64::*;

    let len = buffer.len();
    let simd_len = len / 4 * 4;
    let gain_vec = vdupq_n_f32(gain);

    for i in (0..simd_len).step_by(4) {
        let v = vld1q_f32(buffer.as_ptr().add(i));
        let result = vmulq_f32(v, gain_vec);
        vst1q_f32(buffer.as_mut_ptr().add(i), result);
    }

    // Scalar tail
    for sample in buffer.iter_mut().skip(simd_len) {
        *sample *= gain;
    }
}

// ============================================================================
// Gate / Utility Functions
// ============================================================================

/// Apply gate/mute to audio buffer
#[inline]
pub fn apply_gate(buffer: &mut [f32], start: usize, end: usize) {
    let end = end.min(buffer.len());
    if start < end {
        // Uses memset/SIMD fill internally - much faster than manual loop
        buffer[start..end].fill(0.0);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_features() {
        let features = cpu_features();
        #[cfg(target_arch = "x86_64")]
        {
            assert!(features.has_sse41 || features.has_avx2 || features.has_avx512);
        }
    }

    #[test]
    fn test_audio_mix() {
        let input1 = vec![0.5f32; 100];
        let input2 = vec![0.3f32; 100];
        let mut output = vec![0.0f32; 100];

        audio_mix(&mut output, &[&input1, &input2], &[1.0, 1.0]);

        assert!((output[0] - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_audio_mix_empty() {
        let mut output: Vec<f32> = vec![];
        audio_mix(&mut output, &[], &[]);
        assert!(output.is_empty());
    }

    #[test]
    fn test_audio_mix_no_inputs() {
        let mut output = vec![0.5f32; 100];
        audio_mix(&mut output, &[], &[]);
        assert!(output.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_audio_mix_clipping() {
        let input1 = vec![0.8f32; 100];
        let input2 = vec![0.7f32; 100];
        let mut output = vec![0.0f32; 100];

        audio_mix(&mut output, &[&input1, &input2], &[1.0, 1.0]);

        assert!((output[0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_apply_volume() {
        let mut buffer = vec![1.0f32; 100];
        apply_volume(&mut buffer, 0.5);
        assert!((buffer[0] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_apply_volume_unity() {
        let original = vec![0.5f32; 100];
        let mut buffer = original.clone();
        apply_volume(&mut buffer, 1.0);
        assert_eq!(buffer, original);
    }

    #[test]
    fn test_apply_volume_zero() {
        let mut buffer = vec![0.5f32; 100];
        apply_volume(&mut buffer, 0.0);
        assert!(buffer.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_apply_gate() {
        let mut buffer = vec![1.0f32; 100];
        apply_gate(&mut buffer, 20, 80);
        assert!(buffer[20..80].iter().all(|&v| v == 0.0));
        assert!(buffer[0..20].iter().all(|&v| v == 1.0));
        assert!(buffer[80..].iter().all(|&v| v == 1.0));
    }

    #[test]
    fn test_buffer_pool() {
        let pool = BufferPool::new(1024);
        let buf = pool.acquire();
        assert_eq!(buf.len(), 1024);
        pool.release(buf);
    }

    #[test]
    fn test_single_pass_correctness_large() {
        let input1 = vec![0.3f32; 48000];
        let input2 = vec![0.4f32; 48000];
        let input3 = vec![0.2f32; 48000];
        let mut output = vec![0.0f32; 48000];

        audio_mix(&mut output, &[&input1, &input2, &input3], &[1.0, 1.0, 1.0]);

        assert!((output[0] - 0.9).abs() < 0.001);
        assert!((output[47999] - 0.9).abs() < 0.001);
    }
}
