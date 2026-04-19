//! CPU thread affinity — Linux `sched_setaffinity`.
//!
//! Pins rayon worker threads to specific physical cores at startup.
//! This prevents the OS scheduler from migrating threads between cores
//! (especially between hyperthreads on the same physical core), which
//! would thrash L1/L2 caches built up during audio mixing or probe batches.
//!
//! # How it works
//! Each rayon worker calls `pin_to_physical_core(thread_index, physical_cores)`
//! from its `start_handler`. An atomic counter assigns each thread a unique
//! core slot; all threads distribute evenly across physical cores.
//!
//! | Thread | physical_cores=4 | Assigned core |
//! |--------|------------------|---------------|
//! | 0      | 4                | 0             |
//! | 1      | 4                | 1             |
//! | 2      | 4                | 2             |
//! | 3      | 4                | 0 (wrap)      |
//!
//! # Side effects
//! Reduces context-switch overhead and cache-miss rate on CPU-bound
//! SIMD workloads (AVX-512 audio mix, fused concat).  For I/O-bound
//! probe batches the effect is minimal.

#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicUsize, Ordering};

/// Global monotonic counter — each call grabs a unique slot.
static CORE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Pin the calling thread to a physical CPU core.
///
/// Call this from rayon's `start_handler`; it runs inside the worker thread,
/// so `sched_setaffinity` targets the correct TID automatically.
///
/// # Arguments
/// - `_thread_idx` — rayon's 0-based thread index (informational)
/// - `physical_cores` — number of physical cores (from `num_cpus::get_physical()`)
///
/// # Thread safety
/// `CORE_COUNTER` is an atomic — safe to call from concurrent start_handlers.
pub fn pin_to_physical_core(_thread_idx: usize, physical_cores: usize) {
    if physical_cores == 0 {
        return;
    }

    // Round-robin assignment across physical cores
    let core = CORE_COUNTER.fetch_add(1, Ordering::Relaxed) % physical_cores;

    unsafe {
        let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut cpuset);
        libc::CPU_SET(core, &mut cpuset);

        // gettid via syscall — portable across all glibc versions
        let tid = libc::syscall(libc::SYS_gettid) as libc::pid_t;

        let ret = libc::sched_setaffinity(
            tid,
            std::mem::size_of::<libc::cpu_set_t>(),
            &cpuset,
        );

        if ret != 0 {
            log::warn!(
                "sched_setaffinity(core={}) failed: {}",
                core,
                std::io::Error::last_os_error()
            );
        } else {
            log::trace!("worker tid={} pinned to physical core {}", tid, core);
        }
    }
}

/// Reset the core counter (useful for tests that re-initialise the pool).
#[cfg(test)]
pub fn reset_counter() {
    CORE_COUNTER.store(0, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_increments() {
        reset_counter();
        // Simulate 4 threads on 2 physical cores
        let assignments: Vec<usize> = (0..4)
            .map(|_| CORE_COUNTER.fetch_add(1, Ordering::Relaxed) % 2)
            .collect();
        assert_eq!(assignments, vec![0, 1, 0, 1]);
    }

    #[test]
    fn test_pin_current_thread_succeeds() {
        let cpus = num_cpus::get_physical();
        // Should not panic or return error on a real Linux system
        pin_to_physical_core(0, cpus);
    }

    #[test]
    fn test_pin_zero_physical_cores_noop() {
        // Must not panic or crash
        pin_to_physical_core(0, 0);
    }
}
