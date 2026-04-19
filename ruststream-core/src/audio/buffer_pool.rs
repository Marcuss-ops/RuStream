//! Audio buffer pool — reusable, zero-alloc f32 buffers for the hot path.
//!
//! The existing `BufferPool` in `hot_kernels` is thread-local.
//! This module wraps it in a size-class aware, ergonomic RAII guard so callers
//! don't forget to release buffers back to the pool.
//!
//! # Size classes
//! Three pre-allocated size classes cover all common audio block sizes:
//! - **Frame**  (1024 samples) — FFmpeg/Symphonia default decode block
//! - **Block**  (9600 samples) — 48 kHz × 200 ms bulk mixing
//! - **Large**  (48000 samples) — 48 kHz × 1 s, used by batch resamplers
//!
//! Each size class uses an independent thread-local stack so same-size acquire
//! and release are always O(1) with zero inter-thread synchronisation.

use super::hot_kernels::BufferPool;
use std::cell::RefCell;

/// Standard frame size for audio processing (matches typical FFmpeg output).
pub const FRAME_SAMPLES: usize = 1024;

/// Larger block for bulk mixing pipelines (48 kHz × 200 ms).
pub const BLOCK_SAMPLES: usize = 9600;

/// Large block for 1-second resampling windows (48 kHz × 1 s).
pub const LARGE_SAMPLES: usize = 48000;

// ── Thread-local size-class pools ────────────────────────────────────────────
//
// One independent pool per size class, per thread. Uses `RefCell` since
// thread_local is inherently single-threaded; no runtime synchronisation.

thread_local! {
    static FRAME_POOL_TL: RefCell<Vec<Vec<f32>>> = RefCell::new(Vec::with_capacity(8));
    static BLOCK_POOL_TL: RefCell<Vec<Vec<f32>>> = RefCell::new(Vec::with_capacity(4));
    static LARGE_POOL_TL: RefCell<Vec<Vec<f32>>> = RefCell::new(Vec::with_capacity(2));
}

// Maximum buffers cached per size class per thread.
const MAX_POOLED_PER_CLASS: usize = 8;

/// Acquire a frame-sized buffer from the thread-local pool.
#[inline]
fn tl_acquire(
    pool: &'static std::thread::LocalKey<RefCell<Vec<Vec<f32>>>>,
    size: usize,
) -> Vec<f32> {
    pool.with(|p| {
        if let Some(mut buf) = p.borrow_mut().pop() {
            // Reuse: zero-fill and resize to exact size
            buf.clear();
            buf.resize(size, 0.0f32);
            buf
        } else {
            vec![0.0f32; size]
        }
    })
}

/// Release a buffer back to the thread-local pool.
#[inline]
fn tl_release(
    pool: &'static std::thread::LocalKey<RefCell<Vec<Vec<f32>>>>,
    buf: Vec<f32>,
    expected_size: usize,
) {
    // Only recycle if capacity matches the pool's size class (±25% tolerance)
    let lo = expected_size.saturating_sub(expected_size / 4);
    let hi = expected_size + expected_size / 4;
    if buf.capacity() >= lo && buf.capacity() <= hi {
        pool.with(|p| {
            let mut pool = p.borrow_mut();
            if pool.len() < MAX_POOLED_PER_CLASS {
                pool.push(buf);
            }
        });
    }
}

// ── RAII guard ────────────────────────────────────────────────────────────────

/// Which size class a `PooledBuffer` belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SizeClass {
    Frame,
    Block,
    Large,
    /// Arbitrary size from the legacy `BufferPool` API.
    Custom {
        pool: &'static BufferPool,
    },
}

/// RAII guard that returns a buffer to the correct size-class pool on drop.
///
/// # Example
/// ```rust,ignore
/// let mut guard = PooledBuffer::acquire_frame();
/// audio_mix(guard.as_mut_slice(), &inputs, &volumes);
/// // buffer automatically released when `guard` drops
/// ```
pub struct PooledBuffer {
    inner: Vec<f32>,
    class: SizeClass,
}

impl PooledBuffer {
    /// Acquire a single-frame (1024-sample) buffer from the thread-local pool.
    #[inline]
    pub fn acquire_frame() -> Self {
        Self {
            inner: tl_acquire(&FRAME_POOL_TL, FRAME_SAMPLES),
            class: SizeClass::Frame,
        }
    }

    /// Acquire a bulk-block (9600-sample) buffer from the thread-local pool.
    #[inline]
    pub fn acquire_block() -> Self {
        Self {
            inner: tl_acquire(&BLOCK_POOL_TL, BLOCK_SAMPLES),
            class: SizeClass::Block,
        }
    }

    /// Acquire a large (48000-sample, 1-second) buffer from the thread-local pool.
    #[inline]
    pub fn acquire_large() -> Self {
        Self {
            inner: tl_acquire(&LARGE_POOL_TL, LARGE_SAMPLES),
            class: SizeClass::Large,
        }
    }

    /// Acquire a buffer of an arbitrary size from a caller-supplied pool.
    /// Legacy API — prefer the named constructors above.
    pub fn acquire_custom(pool: &'static BufferPool) -> Self {
        Self {
            inner: pool.acquire(),
            class: SizeClass::Custom { pool },
        }
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
        let buf = std::mem::replace(&mut self.inner, Vec::new());
        std::mem::forget(self);
        buf
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        let buf = std::mem::replace(&mut self.inner, Vec::new());
        if buf.is_empty() {
            return;
        }
        match self.class {
            SizeClass::Frame => tl_release(&FRAME_POOL_TL, buf, FRAME_SAMPLES),
            SizeClass::Block => tl_release(&BLOCK_POOL_TL, buf, BLOCK_SAMPLES),
            SizeClass::Large => tl_release(&LARGE_POOL_TL, buf, LARGE_SAMPLES),
            SizeClass::Custom { pool } => pool.release(buf),
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = [f32];
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

// ── Convenience mixing helper ─────────────────────────────────────────────────

/// Mix N audio inputs into `output` using SIMD kernels.
///
/// Identical semantics to `audio_mix` but documents that no heap allocation
/// occurs on warm calls when the caller manages buffers via `PooledBuffer`.
#[inline]
pub fn audio_mix_pooled(output: &mut [f32], inputs: &[&[f32]], volumes: &[f32]) {
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
    fn test_pooled_buffer_large() {
        let buf = PooledBuffer::acquire_large();
        assert_eq!(buf.len(), LARGE_SAMPLES);
    }

    #[test]
    fn test_pooled_buffer_reuse() {
        // Acquire, drop (returns to pool), acquire again — must not panic
        {
            let _buf = PooledBuffer::acquire_frame();
        }
        let _buf2 = PooledBuffer::acquire_frame();
        // Pool must have exactly 1 cached entry after the first drop
    }

    #[test]
    fn test_pooled_buffer_reuse_block() {
        {
            let _b = PooledBuffer::acquire_block();
        }
        let _b2 = PooledBuffer::acquire_block();
    }

    #[test]
    fn test_audio_mix_pooled_correctness() {
        let input = vec![0.5f32; FRAME_SAMPLES];
        let inputs: Vec<&[f32]> = vec![&input];
        let mut guard = PooledBuffer::acquire_frame();
        audio_mix_pooled(guard.as_mut_slice(), &inputs, &[1.0]);
        assert!((guard[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_into_inner_skips_pool() {
        let buf = PooledBuffer::acquire_frame();
        let raw = buf.into_inner();
        assert_eq!(raw.len(), FRAME_SAMPLES);
        // No panic, pool is just left with one fewer entry
    }

    #[test]
    fn test_multiple_size_classes_independent() {
        // Fill up frame pool, block pool should be unaffected
        for _ in 0..MAX_POOLED_PER_CLASS {
            drop(PooledBuffer::acquire_frame());
        }
        // Block pool is fresh
        let b = PooledBuffer::acquire_block();
        assert_eq!(b.len(), BLOCK_SAMPLES);
    }
}
