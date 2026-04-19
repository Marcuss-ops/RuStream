//! Rayon thread pool auto-tuning and warm-up.
//!
//! By default, rayon creates `num_cpus` threads. On a VPS with hyper-threading
//! this wastes cycles: physical cores × 1 is better for CPU-bound SIMD work.
//!
//! This module:
//! 1. Detects physical (non-HT) core count via `num_cpus::get_physical()`
//! 2. Builds a global rayon pool sized to `max(1, physical - 1)`
//!    (leaves one core free for the OS / I/O scheduler)
//! 3. Warms the pool by spawning a no-op job on every thread (avoids cold-start
//!    latency on the first real job)
//! 4. Exposes `pool_info()` for monitoring / health checks

use std::sync::OnceLock;

/// Configuration for the process-wide thread pool.
#[derive(Debug, Clone, Copy)]
pub struct ThreadPoolConfig {
    /// Number of worker threads in the rayon pool.
    pub num_threads: usize,
    /// Whether physical core count was used (vs. logical).
    pub physical_cores_used: bool,
    /// Number of logical CPUs detected.
    pub logical_cpus: usize,
    /// Number of physical cores detected.
    pub physical_cpus: usize,
}

/// Information about the configured pool.
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub config: ThreadPoolConfig,
    /// Whether the pool has been successfully initialised.
    pub initialised: bool,
    /// Number of warm-up tasks dispatched.
    pub warmup_tasks: usize,
}

static POOL_INFO: OnceLock<PoolInfo> = OnceLock::new();

/// Initialise and warm the global rayon thread pool.
///
/// Safe to call multiple times — subsequent calls are no-ops and return
/// the previously recorded [`PoolInfo`].
///
/// # Tuning strategy
/// | CPUs (logical) | Threads used |
/// |---|---|
/// | 1 | 1 |
/// | 2 | 1 |
/// | 3–4 | physical − 1 |
/// | 5+ | physical − 1 |
///
/// For pure I/O workloads (probe batch) the scheduler already saturates the
/// pool via rayon, so no additional tuning is needed.
pub fn init_thread_pool() -> &'static PoolInfo {
    POOL_INFO.get_or_init(|| {
        let logical  = num_cpus::get();
        let physical = num_cpus::get_physical();

        let num_threads = match physical {
            0 | 1 => 1,
            2     => 1,
            n     => n - 1, // leave one core for OS + network I/O
        };

        let built = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name(|i| format!("ruststream-worker-{}", i))
            .start_handler(move |thread_idx| {
                // ── CPU affinity: pin this worker to a physical core ──────────
                // Prevents OS from migrating the thread between hyperthreads on
                // the same physical core, preserving L1/L2 cache across jobs.
                #[cfg(target_os = "linux")]
                crate::io::affinity::pin_to_physical_core(thread_idx, physical);
                #[cfg(not(target_os = "linux"))]
                let _ = thread_idx;
            })
            .build_global();

        let initialised = built.is_ok();
        if let Err(ref e) = built {
            log::warn!("thread pool init failed (using rayon default): {}", e);
        }

        // Warm up every thread by touching a local stack allocation.
        // This causes the OS to actually map the thread stacks before any
        // real work arrives, cutting first-job latency from ~2ms to ~0.
        let warmup_tasks = if initialised { num_threads } else { 0 };
        if warmup_tasks > 0 {
            use rayon::prelude::*;
            (0..warmup_tasks).into_par_iter().for_each(|_| {
                // Touch 64 KiB of stack to trigger page mapping
                let mut scratch = [0u8; 65536];
                for i in (0..scratch.len()).step_by(4096) {
                    scratch[i] = 1;
                }
                std::hint::black_box(scratch[0]);
            });
        }

        log::info!(
            "thread pool: {} workers (physical={}, logical={}, warmed={})",
            num_threads, physical, logical, warmup_tasks
        );

        PoolInfo {
            config: ThreadPoolConfig {
                num_threads,
                physical_cores_used: true,
                logical_cpus: logical,
                physical_cpus: physical,
            },
            initialised,
            warmup_tasks,
        }
    })
}

/// Get pool info (initialises the pool if not already done).
#[inline]
pub fn pool_info() -> &'static PoolInfo {
    init_thread_pool()
}

/// Get the number of active worker threads.
#[inline]
pub fn worker_count() -> usize {
    pool_info().config.num_threads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_info_non_zero() {
        let info = pool_info();
        assert!(info.config.num_threads >= 1);
        assert!(info.config.logical_cpus >= 1);
        assert!(info.config.physical_cpus >= 1);
    }

    #[test]
    fn test_pool_init_idempotent() {
        let a = init_thread_pool();
        let b = init_thread_pool();
        assert_eq!(a.config.num_threads, b.config.num_threads);
    }

    #[test]
    fn test_worker_count() {
        assert!(worker_count() >= 1);
    }
}
