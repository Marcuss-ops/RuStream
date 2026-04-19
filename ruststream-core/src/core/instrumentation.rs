//! Profiling and instrumentation utilities.
//!
//! This module provides timing instrumentation for measuring
//! pipeline stage performance and generating profiling reports.
//!
//! # Hot-path design
//! Counters updated on every audio frame or every decoded sample use
//! **lock-free `AtomicU64`** — zero contention, no cache-line ping-pong.
//! Stage-level timing (updated once per pipeline stage) still uses a `Mutex<Profiler>`
//! but the flush is batched: callers accumulate into a thread-local `Vec`
//! and flush with a single lock acquisition.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ── Fast atomic counters (hot path, lock-free) ────────────────────────────────

/// Process-global, lock-free performance counters.
///
/// Updated via `Relaxed` ordering — they are read only in summaries,
/// not used for synchronisation.
pub struct FastCounters {
    pub samples_processed: AtomicU64,
    pub frames_processed: AtomicU64,
    pub bytes_processed: AtomicU64,
    pub subprocess_count: AtomicU64,
    pub io_bytes_read: AtomicU64,
    pub io_bytes_written: AtomicU64,
}

impl FastCounters {
    const fn new() -> Self {
        Self {
            samples_processed: AtomicU64::new(0),
            frames_processed: AtomicU64::new(0),
            bytes_processed: AtomicU64::new(0),
            subprocess_count: AtomicU64::new(0),
            io_bytes_read: AtomicU64::new(0),
            io_bytes_written: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn add_samples(&self, n: u64) {
        self.samples_processed.fetch_add(n, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_frames(&self, n: u64) {
        self.frames_processed.fetch_add(n, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_bytes(&self, n: u64) {
        self.bytes_processed.fetch_add(n, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_subprocess(&self) {
        self.subprocess_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_io_read(&self, n: u64) {
        self.io_bytes_read.fetch_add(n, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn add_io_written(&self, n: u64) {
        self.io_bytes_written.fetch_add(n, Ordering::Relaxed);
    }
}

/// Process-global fast counters — accessible from any thread without locking.
pub static FAST_COUNTERS: FastCounters = FastCounters::new();

// ── Stage timer ───────────────────────────────────────────────────────────────

/// Stage timing information.
#[derive(Debug, Clone)]
pub struct StageTimer {
    /// Stage name.
    pub name: String,
    /// Start time.
    start: Option<Instant>,
    /// Elapsed time in milliseconds (if stopped).
    pub elapsed_ms: Option<u64>,
}

impl StageTimer {
    /// Start a new timer.
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Some(Instant::now()),
            elapsed_ms: None,
        }
    }

    /// Stop the timer and return elapsed time.
    pub fn stop(self, profiler: &std::sync::Mutex<Profiler>) -> std::time::Duration {
        let elapsed = self.start.map(|s| s.elapsed()).unwrap_or_default();
        let elapsed_ms = elapsed.as_millis() as u64;

        if let Ok(mut p) = profiler.lock() {
            p.record_stage_time(&self.name, elapsed_ms);
        }

        elapsed
    }

    /// Stop the timer and batch-record into a local buffer (lock-free staging).
    ///
    /// Call `Profiler::flush_batch` with the buffer to commit in one lock.
    pub fn stop_batched(self, batch: &mut Vec<(String, u64)>) -> std::time::Duration {
        let elapsed = self.start.map(|s| s.elapsed()).unwrap_or_default();
        batch.push((self.name, elapsed.as_millis() as u64));
        elapsed
    }
}

// ── Stage metrics ─────────────────────────────────────────────────────────────

/// Metrics for a single pipeline stage.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StageMetrics {
    pub decode_ms: u64,
    pub effects_ms: u64,
    pub overlay_ms: u64,
    pub audio_ms: u64,
    pub encode_ms: u64,
    pub total_ms: u64,
}

impl StageMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stage_sum(&self) -> u64 {
        self.decode_ms + self.effects_ms + self.overlay_ms + self.encode_ms + self.audio_ms
    }

    pub fn has_any(&self) -> bool {
        self.decode_ms > 0
            || self.effects_ms > 0
            || self.overlay_ms > 0
            || self.audio_ms > 0
            || self.encode_ms > 0
    }
}

// ── Drift metrics ─────────────────────────────────────────────────────────────

/// Drift metrics for audio/video synchronization.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DriftMetrics {
    pub drift_frames_max: f64,
    pub drift_frames_p95: f64,
    pub drift_corrections_count: u32,
    pub resample_ratio_avg: f64,
}

impl DriftMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_acceptable(&self) -> bool {
        self.drift_frames_max <= 1.0
    }
}

// ── CPU time record ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CpuTimeRecord {
    pub operation: String,
    pub duration_ms: u64,
}

// ── Profiler ─────────────────────────────────────────────────────────────────

/// Stage-level profiler (Mutex-guarded — locked once per stage, not per sample).
///
/// For per-sample/per-frame counting use [`FAST_COUNTERS`] instead.
pub struct Profiler {
    stage_times: HashMap<String, u64>,
    cpu_times: Vec<CpuTimeRecord>,
    start_time: Instant,
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            stage_times: HashMap::new(),
            cpu_times: Vec::new(),
            start_time: Instant::now(),
        }
    }

    /// Record stage timing.
    pub fn record_stage_time(&mut self, stage: &str, duration_ms: u64) {
        self.stage_times.insert(stage.to_string(), duration_ms);
    }

    /// Record CPU time for an operation.
    pub fn record_cpu_time(&mut self, operation: impl Into<String>, duration_ms: u64) {
        self.cpu_times.push(CpuTimeRecord {
            operation: operation.into(),
            duration_ms,
        });
    }

    /// Flush a batch of stage timings with a single lock acquisition.
    ///
    /// Callers should accumulate into a `Vec<(String, u64)>` via
    /// `StageTimer::stop_batched` and then call this once.
    pub fn flush_batch(&mut self, batch: Vec<(String, u64)>) {
        for (stage, ms) in batch {
            self.stage_times.insert(stage, ms);
        }
    }

    // Delegated hot-path helpers — these just forward to the global atomics.
    // Keeping the API compatible so existing callers don't break.

    #[inline(always)]
    pub fn record_bytes_processed(&self, bytes: u64) {
        FAST_COUNTERS.add_bytes(bytes);
    }

    #[inline(always)]
    pub fn record_frames_processed(&self, frames: u64) {
        FAST_COUNTERS.add_frames(frames);
    }

    #[inline(always)]
    pub fn record_samples_processed(&self, samples: u64) {
        FAST_COUNTERS.add_samples(samples);
    }

    #[inline(always)]
    pub fn record_subprocess(&self) {
        FAST_COUNTERS.add_subprocess();
    }

    #[inline(always)]
    pub fn record_io_read(&self, bytes: u64) {
        FAST_COUNTERS.add_io_read(bytes);
    }

    #[inline(always)]
    pub fn record_io_written(&self, bytes: u64) {
        FAST_COUNTERS.add_io_written(bytes);
    }

    pub fn total_elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn get_stage_time(&self, stage: &str) -> Option<u64> {
        self.stage_times.get(stage).copied()
    }

    /// Generate a profiling report (reads global counters atomically).
    pub fn generate_report(&self) -> ProfilingReport {
        let total_cpu_time: u64 = self.cpu_times.iter().map(|r| r.duration_ms).sum();

        ProfilingReport {
            stage_times: self.stage_times.clone(),
            bytes_processed: FAST_COUNTERS.bytes_processed.load(Ordering::Relaxed),
            frames_processed: FAST_COUNTERS.frames_processed.load(Ordering::Relaxed),
            samples_processed: FAST_COUNTERS.samples_processed.load(Ordering::Relaxed),
            total_cpu_time_ms: total_cpu_time,
            total_elapsed_ms: self.total_elapsed_ms(),
            operation_count: self.cpu_times.len(),
            subprocess_count: FAST_COUNTERS.subprocess_count.load(Ordering::Relaxed) as u32,
            io_bytes_read: FAST_COUNTERS.io_bytes_read.load(Ordering::Relaxed),
            io_bytes_written: FAST_COUNTERS.io_bytes_written.load(Ordering::Relaxed),
        }
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

// ── Profiling report ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ProfilingReport {
    pub stage_times: HashMap<String, u64>,
    pub bytes_processed: u64,
    pub frames_processed: u64,
    pub samples_processed: u64,
    pub total_cpu_time_ms: u64,
    pub total_elapsed_ms: u64,
    pub operation_count: usize,
    pub subprocess_count: u32,
    pub io_bytes_read: u64,
    pub io_bytes_written: u64,
}

impl ProfilingReport {
    pub fn bytes_per_second(&self) -> f64 {
        if self.total_elapsed_ms == 0 {
            return 0.0;
        }
        self.bytes_processed as f64 / (self.total_elapsed_ms as f64 / 1000.0)
    }

    pub fn frames_per_second(&self) -> f64 {
        if self.total_elapsed_ms == 0 {
            return 0.0;
        }
        self.frames_processed as f64 / (self.total_elapsed_ms as f64 / 1000.0)
    }

    pub fn cpu_utilization_pct(&self) -> f64 {
        if self.total_elapsed_ms == 0 {
            return 0.0;
        }
        (self.total_cpu_time_ms as f64 / self.total_elapsed_ms as f64) * 100.0
    }

    pub fn format(&self) -> String {
        let mut output = String::from("=== Profiling Report ===\n");
        output.push_str(&format!("Total elapsed: {} ms\n", self.total_elapsed_ms));
        output.push_str(&format!("Total CPU time: {} ms\n", self.total_cpu_time_ms));
        output.push_str(&format!(
            "CPU utilization: {:.1}%\n",
            self.cpu_utilization_pct()
        ));
        output.push_str(&format!("Bytes processed: {}\n", self.bytes_processed));
        output.push_str(&format!("Frames processed: {}\n", self.frames_processed));
        output.push_str(&format!(
            "Throughput: {:.1} bytes/sec\n",
            self.bytes_per_second()
        ));
        output.push_str(&format!(
            "Frame rate: {:.1} fps\n",
            self.frames_per_second()
        ));
        output.push_str(&format!("Operations: {}\n", self.operation_count));
        output.push_str(&format!("Subprocess spawns: {}\n", self.subprocess_count));
        output.push_str(&format!("I/O read: {} KB\n", self.io_bytes_read / 1024));
        output.push_str(&format!(
            "I/O written: {} KB\n",
            self.io_bytes_written / 1024
        ));
        output.push_str("\nStage Times:\n");

        let mut stages: Vec<_> = self.stage_times.iter().collect();
        stages.sort_by_key(|(_, &time)| std::cmp::Reverse(time));
        for (stage, time) in stages {
            output.push_str(&format!("  {}: {} ms\n", stage, time));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_stage_timer() {
        let profiler = Mutex::new(Profiler::new());
        let timer = StageTimer::start("test_stage");
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.stop(&profiler);
        assert!(elapsed.as_millis() >= 10);
    }

    #[test]
    fn test_stage_timer_batched() {
        let profiler = Mutex::new(Profiler::new());
        let mut batch = Vec::new();
        let t1 = StageTimer::start("decode");
        let t2 = StageTimer::start("encode");
        t1.stop_batched(&mut batch);
        t2.stop_batched(&mut batch);
        // One lock acquisition for two timers
        profiler.lock().unwrap().flush_batch(batch);
        assert!(profiler.lock().unwrap().get_stage_time("decode").is_some());
        assert!(profiler.lock().unwrap().get_stage_time("encode").is_some());
    }

    #[test]
    fn test_fast_counters_no_lock() {
        FAST_COUNTERS.add_samples(1024);
        FAST_COUNTERS.add_frames(1);
        // Just tests that it doesn't panic — no lock taken
        assert!(FAST_COUNTERS.samples_processed.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn test_profiler_basic() {
        let mut profiler = Profiler::new();
        profiler.record_stage_time("decode", 100);
        profiler.record_stage_time("encode", 200);
        profiler.record_cpu_time("mix_audio", 50);

        assert_eq!(profiler.get_stage_time("decode"), Some(100));
        assert_eq!(profiler.get_stage_time("encode"), Some(200));
        assert_eq!(profiler.get_stage_time("invalid"), None);

        let report = profiler.generate_report();
        assert_eq!(report.operation_count, 1);
    }

    #[test]
    fn test_stage_metrics() {
        let metrics = StageMetrics {
            decode_ms: 100,
            effects_ms: 50,
            overlay_ms: 30,
            audio_ms: 200,
            encode_ms: 150,
            total_ms: 530,
        };
        assert!(metrics.has_any());
        assert_eq!(metrics.total_ms, 530);
    }

    #[test]
    fn test_profiling_report_format() {
        let mut profiler = Profiler::new();
        profiler.record_stage_time("test", 100);
        let report = profiler.generate_report();
        let formatted = report.format();
        assert!(formatted.contains("Profiling Report"));
        assert!(formatted.contains("test"));
        assert!(formatted.contains("100 ms"));
    }

    #[test]
    fn test_drift_metrics() {
        let drift = DriftMetrics::new();
        assert!((drift.drift_frames_max - 0.0).abs() < f64::EPSILON);
    }
}
