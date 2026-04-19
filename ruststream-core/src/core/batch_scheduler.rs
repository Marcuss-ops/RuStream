//! Prioritized batch job scheduler.
//!
//! Sorts jobs by estimated cost (file size / duration) in ascending order
//! before dispatching via rayon. This maximises throughput by letting many
//! small jobs complete early while large jobs overlap with them.
//!
//! # Why size-ascending order?
//!
//! With N=8 threads and jobs of sizes [1s, 1s, 1s, 1s, 1s, 1s, 1s, 60s]:
//! - Random order: last thread finishes at t=60, others idle most of the time.
//! - Size-ascending: all 7 small jobs finish first (~1s), then one thread
//!   handles the 60s job while others are free for new work.
//!
//! For batch probe/concat this cuts average job latency significantly.

use rayon::prelude::*;
use std::path::Path;
use crate::core::MediaResult;
use crate::probe::{probe_fast, FullMetadata};

// ── Job types ────────────────────────────────────────────────────────────────

/// A generic job with an estimated cost hint used for scheduling.
pub struct Job<T> {
    /// Estimated cost in bytes (lower = scheduled first).
    pub cost_hint: u64,
    /// Payload to process.
    pub payload: T,
}

impl<T> Job<T> {
    pub fn new(cost_hint: u64, payload: T) -> Self {
        Self { cost_hint, payload }
    }

    /// Create a job where cost is derived from file size on disk.
    pub fn from_path(path: impl Into<String>) -> Job<String> {
        let p: String = path.into();
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX);
        Job { cost_hint: size, payload: p }
    }
}

// ── Probe scheduler ──────────────────────────────────────────────────────────

/// Probe a list of file paths in priority order (smallest files first).
///
/// Returns results in the **same order as the input** (not scheduling order).
/// Errors per-file are individual.
///
/// # Arguments
/// - `paths`: list of file paths
/// - `full`: if true use `probe_full` (slower but gives width/height/fps),
///           if false use `probe_fast` (recommended for scheduling)
pub fn probe_scheduled(paths: &[&str], full: bool) -> Vec<MediaResult<FullMetadata>> {
    if paths.is_empty() {
        return Vec::new();
    }

    // Build (original_index, cost_hint, path) triples
    let mut indexed: Vec<(usize, u64, &str)> = paths
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX);
            (i, size, p)
        })
        .collect();

    // Sort ascending by estimated cost
    indexed.sort_unstable_by_key(|&(_, cost, _)| cost);

    // Run in parallel in cost order
    let mut scheduled_results: Vec<(usize, MediaResult<FullMetadata>)> = indexed
        .into_par_iter()
        .map(|(orig_idx, _, path)| {
            let result = if full {
                crate::probe::probe_full(path)
            } else {
                probe_fast(path)
            };
            (orig_idx, result)
        })
        .collect();

    // Restore original order
    scheduled_results.sort_unstable_by_key(|&(idx, _)| idx);
    scheduled_results.into_iter().map(|(_, r)| r).collect()
}

/// Generic scheduled batch executor.
///
/// Sorts jobs by `cost_hint`, executes `f` on each payload in parallel,
/// and returns results in the **original input order**.
pub fn run_scheduled<T, R, F>(mut jobs: Vec<Job<T>>, f: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync + Send,
{
    if jobs.is_empty() {
        return Vec::new();
    }

    // Attach original indices before sorting
    let mut indexed: Vec<(usize, u64, T)> = jobs
        .drain(..)
        .enumerate()
        .map(|(i, j)| (i, j.cost_hint, j.payload))
        .collect();

    indexed.sort_unstable_by_key(|&(_, cost, _)| cost);

    let mut results: Vec<(usize, R)> = indexed
        .into_par_iter()
        .map(|(orig_idx, _, payload)| (orig_idx, f(payload)))
        .collect();

    results.sort_unstable_by_key(|&(idx, _)| idx);
    results.into_iter().map(|(_, r)| r).collect()
}

// ── Concat scheduler ─────────────────────────────────────────────────────────

/// A concat job for the scheduler.
#[derive(Debug, Clone)]
pub struct ConcatJob {
    /// Input video paths (in order).
    pub inputs: Vec<String>,
    /// Output path.
    pub output: String,
}

/// Schedule and run multiple independent concat jobs in parallel.
///
/// Jobs are sorted by total input bytes (ascending) so small jobs complete
/// first, freeing threads for other work.
pub fn concat_scheduled(
    jobs: Vec<ConcatJob>,
    allow_stream_copy: bool,
) -> Vec<crate::core::MediaResult<bool>> {
    use crate::video::ConcatConfig;
    use crate::video::concat_videos;

    let sched_jobs: Vec<Job<ConcatJob>> = jobs
        .into_iter()
        .map(|j| {
            let total_bytes: u64 = j.inputs.iter()
                .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
                .sum();
            Job::new(total_bytes, j)
        })
        .collect();

    run_scheduled(sched_jobs, move |cj| {
        let config = ConcatConfig {
            inputs: cj.inputs,
            output: cj.output,
            allow_stream_copy,
            ..Default::default()
        };
        concat_videos(&config).map_err(|e| {
            crate::core::MediaError::new(crate::core::MediaErrorCode::ConcatFailed, e)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_scheduled_empty() {
        let r = probe_scheduled(&[], false);
        assert!(r.is_empty());
    }

    #[test]
    fn test_run_scheduled_preserves_order() {
        let jobs: Vec<Job<usize>> = (0..10usize)
            .rev() // reverse cost so scheduler sorts them
            .map(|i| Job::new(i as u64, i))
            .collect();

        let results = run_scheduled(jobs, |x| x * 2);
        // Results must be in original insertion order (0..10 reversed → 9,8,...,0)
        // and doubled
        let expected: Vec<usize> = (0..10usize).rev().map(|i| i * 2).collect();
        assert_eq!(results, expected);
    }

    #[test]
    fn test_concat_scheduled_empty() {
        let r = concat_scheduled(vec![], true);
        assert!(r.is_empty());
    }
}
