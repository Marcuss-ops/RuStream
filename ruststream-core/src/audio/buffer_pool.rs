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

// ════════════════════════════════════════════════════════════════════════════
// AlignedF32Buffer — 64-byte aligned f32 storage for aligned SIMD loads
// ════════════════════════════════════════════════════════════════════════════
//
// Use this type when you need to guarantee that a buffer starts on a
// 64-byte cache-line boundary.  On AVX-512 this enables using
// `_mm512_load_ps` (aligned) instead of `_mm512_loadu_ps`, and eliminates
// any potential cache-line split on all x86_64/AArch64 microarchitectures.

use std::alloc::{alloc_zeroed, dealloc, Layout};

/// Cache-line size (64 bytes on all modern x86_64 / ARM64 CPUs).
pub const CACHE_LINE: usize = 64;

/// A heap-allocated `[f32]` guaranteed to start on a 64-byte cache-line boundary.
///
/// # Example
/// ```rust,ignore
/// let mut buf = AlignedF32Buffer::new(FRAME_SAMPLES);
/// audio_mix(buf.as_mut_slice(), &inputs, &volumes);
/// assert!(buf.is_aligned());
/// ```
pub struct AlignedF32Buffer {
    ptr: std::ptr::NonNull<f32>,
    len: usize,
    layout: Layout,
}

// SAFETY: AlignedF32Buffer owns its allocation exclusively.
unsafe impl Send for AlignedF32Buffer {}
unsafe impl Sync for AlignedF32Buffer {}

impl AlignedF32Buffer {
    /// Allocate a zeroed f32 buffer of `len` elements with 64-byte alignment.
    ///
    /// # Panics
    /// Panics if `len == 0` or if OOM.
    pub fn new(len: usize) -> Self {
        assert!(len > 0, "AlignedF32Buffer: len must be > 0");
        let base   = Layout::array::<f32>(len).expect("layout overflow");
        let layout = base.align_to(CACHE_LINE).expect("alignment overflow");
        let raw    = unsafe { alloc_zeroed(layout) } as *mut f32;
        let ptr    = std::ptr::NonNull::new(raw).expect("OOM");
        debug_assert_eq!(ptr.as_ptr() as usize % CACHE_LINE, 0);
        Self { ptr, len, layout }
    }

    /// `true` if the buffer pointer is 64-byte aligned (invariant: always true).
    #[inline(always)]
    pub fn is_aligned(&self) -> bool {
        self.ptr.as_ptr() as usize % CACHE_LINE == 0
    }

    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    #[inline] pub fn len(&self)      -> usize  { self.len }
    #[inline] pub fn is_empty(&self) -> bool   { self.len == 0 }
    #[inline] pub fn as_ptr(&self)   -> *const f32 { self.ptr.as_ptr() }
    #[inline] pub fn as_mut_ptr(&mut self) -> *mut f32 { self.ptr.as_ptr() }

    /// Zero-fill in place (f32 zero == 0x00000000).
    #[inline]
    pub fn zero_fill(&mut self) {
        unsafe { std::ptr::write_bytes(self.ptr.as_ptr(), 0, self.len) };
    }
}

impl Drop for AlignedF32Buffer {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr() as *mut u8, self.layout) };
    }
}

impl std::ops::Deref for AlignedF32Buffer {
    type Target = [f32];
    fn deref(&self) -> &Self::Target { self.as_slice() }
}

impl std::ops::DerefMut for AlignedF32Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target { self.as_mut_slice() }
}

#[cfg(test)]
mod aligned_tests {
    use super::*;

    #[test]
    fn test_aligned_allocation_frame() {
        let buf = AlignedF32Buffer::new(FRAME_SAMPLES);
        assert!(buf.is_aligned());
        assert_eq!(buf.len(), FRAME_SAMPLES);
    }

    #[test]
    fn test_aligned_allocation_block() {
        let buf = AlignedF32Buffer::new(BLOCK_SAMPLES);
        assert!(buf.is_aligned());
    }

    #[test]
    fn test_aligned_zeroed() {
        let buf = AlignedF32Buffer::new(64);
        assert!(buf.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_aligned_write_read() {
        let mut buf = AlignedF32Buffer::new(16);
        for (i, x) in buf.as_mut_slice().iter_mut().enumerate() {
            *x = i as f32;
        }
        assert!((buf[0] - 0.0).abs() < 1e-9);
        assert!((buf[15] - 15.0).abs() < 1e-9);
    }

    #[test]
    fn test_aligned_zero_fill() {
        let mut buf = AlignedF32Buffer::new(32);
        buf.as_mut_slice().fill(1.0);
        buf.zero_fill();
        assert!(buf.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_aligned_ptr_invariant_multiple_sizes() {
        for size in [16usize, 64, 1024, 9600, 48000] {
            let buf = AlignedF32Buffer::new(size);
            assert_eq!(
                buf.as_ptr() as usize % CACHE_LINE, 0,
                "size={size} not 64-byte aligned"
            );
        }
    }
}

