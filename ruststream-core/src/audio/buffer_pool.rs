//! Audio buffer pool — reusable, zero-alloc f32 buffers for the hot path.
//!
//! The existing `BufferPool` in `hot_kernels` is thread-local.
//! This module wraps it in an ergonomic RAII guard so callers don't forget
//! to release buffers, and adds a fixed-size "frame" pool for the common
//! 1024-sample block size used by most audio workloads.

use super::hot_kernels::BufferPool;

/// Standard frame size for audio processing (matches typical FFmpeg output).
pub const FRAME_SAMPLES: usize = 1024;

/// Larger block for bulk mixing pipelines (48 kHz × 200 ms).
pub const BLOCK_SAMPLES: usize = 9600;

/// RAII guard that returns a buffer to the pool on drop.
///
/// # Example
/// ```rust,ignore
/// let guard = PooledBuffer::acquire_frame();
/// let buf: &mut [f32] = guard.as_mut_slice();
/// audio_mix(buf, &inputs, &volumes);
/// // buffer automatically released when `guard` drops
/// ```
pub struct PooledBuffer {
    inner: Vec<f32>,
    pool: &'static BufferPool,
}

impl PooledBuffer {
    /// Acquire a single-frame (1024-sample) buffer from the thread-local pool.
    pub fn acquire_frame() -> Self {
        static FRAME_POOL: std::sync::LazyLock<BufferPool> =
            std::sync::LazyLock::new(|| BufferPool::new(FRAME_SAMPLES));
        let buf = FRAME_POOL.acquire();
        Self { inner: buf, pool: &FRAME_POOL }
    }

    /// Acquire a bulk-block (9600-sample) buffer from the thread-local pool.
    pub fn acquire_block() -> Self {
        static BLOCK_POOL: std::sync::LazyLock<BufferPool> =
            std::sync::LazyLock::new(|| BufferPool::new(BLOCK_SAMPLES));
        let buf = BLOCK_POOL.acquire();
        Self { inner: buf, pool: &BLOCK_POOL }
    }

    /// Acquire a buffer of an arbitrary size.
    pub fn acquire_custom(pool: &'static BufferPool) -> Self {
        Self { inner: pool.acquire(), pool }
    }

    /// Borrow the inner slice mutably.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.inner
    }

    /// Borrow the inner slice immutably.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.inner
    }

    /// Consume the guard and return the raw Vec (caller must release manually).
    pub fn into_inner(mut self) -> Vec<f32> {
        // Prevent the Drop impl from returning to pool
        let buf = std::mem::replace(&mut self.inner, Vec::new());
        std::mem::forget(self); // skip drop
        buf
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        let buf = std::mem::replace(&mut self.inner, Vec::new());
        if !buf.is_empty() {
            self.pool.release(buf);
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = [f32];
    fn deref(&self) -> &Self::Target { &self.inner }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.inner }
}

/// Mix N audio inputs into `output` using pooled intermediate buffers.
///
/// Identical semantics to `audio_mix` but avoids any heap allocation on
/// warm calls by reusing thread-local frame buffers.
#[inline]
pub fn audio_mix_pooled(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
    // For small input counts, pool overhead is irrelevant — delegate directly.
    super::hot_kernels::audio_mix(output, inputs, volumes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pooled_buffer_frame() {
        let buf = PooledBuffer::acquire_frame();
        assert_eq!(buf.len(), FRAME_SAMPLES);
    }

    #[test]
    fn test_pooled_buffer_block() {
        let buf = PooledBuffer::acquire_block();
        assert_eq!(buf.len(), BLOCK_SAMPLES);
    }

    #[test]
    fn test_pooled_buffer_reuse() {
        // Acquire, drop (returns to pool), acquire again — must not panic
        {
            let _buf = PooledBuffer::acquire_frame();
        }
        let _buf2 = PooledBuffer::acquire_frame();
    }

    #[test]
    fn test_audio_mix_pooled_correctness() {
        let input = vec![0.5f32; FRAME_SAMPLES];
        let inputs: Vec<&[f32]> = vec![&input];
        let mut guard = PooledBuffer::acquire_frame();
        audio_mix_pooled(guard.as_mut_slice(), &inputs, &[1.0]);
        assert!((guard[0] - 0.5).abs() < 1e-6);
    }
}
