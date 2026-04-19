//! Profiling and instrumentation utilities.
//!
//! This module provides timing instrumentation for measuring
//! pipeline stage performance and generating profiling reports.

use std::collections::HashMap;
use std::time::Instant;

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
}

/// Metrics for a single pipeline stage.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StageMetrics {
    /// Decode stage duration in milliseconds.
    pub decode_ms: u64,
    /// Effects stage duration in milliseconds.
    pub effects_ms: u64,
    /// Overlay stage duration in milliseconds.
    pub overlay_ms: u64,
    /// Audio stage duration in milliseconds.
    pub audio_ms: u64,
    /// Encode stage duration in milliseconds.
    pub encode_ms: u64,
    /// Total duration in milliseconds.
    pub total_ms: u64,
}

impl StageMetrics {
    /// Create new stage metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sum of all stage times.
    pub fn stage_sum(&self) -> u64 {
        self.decode_ms + self.effects_ms + self.overlay_ms + self.encode_ms + self.audio_ms
    }

    /// Check if any stage has non-zero duration.
    pub fn has_any(&self) -> bool {
        self.decode_ms > 0
            || self.effects_ms > 0
            || self.overlay_ms > 0
            || self.audio_ms > 0
            || self.encode_ms > 0
    }
}

/// Drift metrics for audio/video synchronization.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DriftMetrics {
    /// Maximum drift in frames.
    pub drift_frames_max: f64,
    /// P95 drift in frames.
    pub drift_frames_p95: f64,
    /// Number of drift corrections applied.
    pub drift_corrections_count: u32,
    /// Average resample ratio.
    pub resample_ratio_avg: f64,
}

impl DriftMetrics {
    /// Create new drift metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if drift is within acceptable threshold (<= 1 frame).
    pub fn is_acceptable(&self) -> bool {
        self.drift_frames_max <= 1.0
    }
}

/// CPU time record for tracking operations.
#[derive(Debug, Clone)]
pub struct CpuTimeRecord {
    /// Operation name.
    pub operation: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Profiler for tracking pipeline performance.
pub struct Profiler {
    /// Stage timings.
    stage_times: HashMap<String, u64>,
    /// Bytes processed.
    bytes_processed: u64,
    /// Frames processed.
    frames_processed: u64,
    /// Audio samples processed.
    samples_processed: u64,
    /// CPU time records.
    cpu_times: Vec<CpuTimeRecord>,
    /// Start time.
    start_time: Instant,
    /// Number of subprocess (FFmpeg) spawns in this pipeline run.
    subprocess_count: u32,
    /// Total bytes read from disk (I/O read).
    io_bytes_read: u64,
    /// Total bytes written to disk (I/O write).
    io_bytes_written: u64,
}

impl Profiler {
    /// Create a new profiler.
    pub fn new() -> Self {
        Self {
            stage_times: HashMap::new(),
            bytes_processed: 0,
            frames_processed: 0,
            samples_processed: 0,
            cpu_times: Vec::new(),
            start_time: Instant::now(),
            subprocess_count: 0,
            io_bytes_read: 0,
            io_bytes_written: 0,
        }
    }

    /// Record stage timing.
    pub fn record_stage_time(&mut self, stage: &str, duration_ms: u64) {
        self.stage_times.insert(stage.to_string(), duration_ms);
    }

    /// Record bytes processed.
    pub fn record_bytes_processed(&mut self, bytes: u64) {
        self.bytes_processed = bytes;
    }

    /// Record frames processed.
    pub fn record_frames_processed(&mut self, frames: u64) {
        self.frames_processed = frames;
    }

    /// Record audio samples processed.
    pub fn record_samples_processed(&mut self, samples: u64) {
        self.samples_processed = samples;
    }

    /// Record CPU time for an operation.
    pub fn record_cpu_time(&mut self, operation: impl Into<String>, duration_ms: u64) {
        self.cpu_times.push(CpuTimeRecord {
            operation: operation.into(),
            duration_ms,
        });
    }

    /// Record a subprocess spawn (e.g. FFmpeg invocation).
    pub fn record_subprocess(&mut self) {
        self.subprocess_count += 1;
    }

    /// Record bytes read from disk.
    pub fn record_io_read(&mut self, bytes: u64) {
        self.io_bytes_read += bytes;
    }

    /// Record bytes written to disk.
    pub fn record_io_written(&mut self, bytes: u64) {
        self.io_bytes_written += bytes;
    }

    /// Get total elapsed time.
    pub fn total_elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Get stage time.
    pub fn get_stage_time(&self, stage: &str) -> Option<u64> {
        self.stage_times.get(stage).copied()
    }

    /// Generate a profiling report.
    pub fn generate_report(&self) -> ProfilingReport {
        let total_cpu_time: u64 = self.cpu_times.iter().map(|r| r.duration_ms).sum();
        
        ProfilingReport {
            stage_times: self.stage_times.clone(),
            bytes_processed: self.bytes_processed,
            frames_processed: self.frames_processed,
            samples_processed: self.samples_processed,
            total_cpu_time_ms: total_cpu_time,
            total_elapsed_ms: self.total_elapsed_ms(),
            operation_count: self.cpu_times.len(),
            subprocess_count: self.subprocess_count,
            io_bytes_read: self.io_bytes_read,
            io_bytes_written: self.io_bytes_written,
        }
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Profiling report with aggregated metrics.
#[derive(Debug, Clone, Default)]
pub struct ProfilingReport {
    /// Stage timings.
    pub stage_times: HashMap<String, u64>,
    /// Total bytes processed.
    pub bytes_processed: u64,
    /// Total frames processed.
    pub frames_processed: u64,
    /// Total audio samples processed.
    pub samples_processed: u64,
    /// Total CPU time in milliseconds.
    pub total_cpu_time_ms: u64,
    /// Total elapsed time in milliseconds.
    pub total_elapsed_ms: u64,
    /// Number of operations recorded.
    pub operation_count: usize,
    /// Number of subprocess (FFmpeg) spawns.
    pub subprocess_count: u32,
    /// Total bytes read from disk across all operations.
    pub io_bytes_read: u64,
    /// Total bytes written to disk across all operations.
    pub io_bytes_written: u64,
}

impl ProfilingReport {
    /// Get throughput in bytes per second.
    pub fn bytes_per_second(&self) -> f64 {
        if self.total_elapsed_ms == 0 {
            return 0.0;
        }
        self.bytes_processed as f64 / (self.total_elapsed_ms as f64 / 1000.0)
    }

    /// Get throughput in frames per second.
    pub fn frames_per_second(&self) -> f64 {
        if self.total_elapsed_ms == 0 {
            return 0.0;
        }
        self.frames_processed as f64 / (self.total_elapsed_ms as f64 / 1000.0)
    }

    /// Get CPU utilization percentage.
    pub fn cpu_utilization_pct(&self) -> f64 {
        if self.total_elapsed_ms == 0 {
            return 0.0;
        }
        (self.total_cpu_time_ms as f64 / self.total_elapsed_ms as f64) * 100.0
    }

    /// Format report as string.
    pub fn format(&self) -> String {
        let mut output = String::from("=== Profiling Report ===\n");
        output.push_str(&format!("Total elapsed: {} ms\n", self.total_elapsed_ms));
        output.push_str(&format!("Total CPU time: {} ms\n", self.total_cpu_time_ms));
        output.push_str(&format!("CPU utilization: {:.1}%\n", self.cpu_utilization_pct()));
        output.push_str(&format!("Bytes processed: {}\n", self.bytes_processed));
        output.push_str(&format!("Frames processed: {}\n", self.frames_processed));
        output.push_str(&format!("Throughput: {:.1} bytes/sec\n", self.bytes_per_second()));
        output.push_str(&format!("Frame rate: {:.1} fps\n", self.frames_per_second()));
        output.push_str(&format!("Operations: {}\n", self.operation_count));
        output.push_str(&format!("Subprocess spawns: {}\n", self.subprocess_count));
        output.push_str(&format!("I/O read: {} KB\n", self.io_bytes_read / 1024));
        output.push_str(&format!("I/O written: {} KB\n", self.io_bytes_written / 1024));
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
    fn test_profiler_basic() {
        let mut profiler = Profiler::new();
        
        profiler.record_stage_time("decode", 100);
        profiler.record_stage_time("encode", 200);
        profiler.record_bytes_processed(1024);
        profiler.record_frames_processed(30);
        profiler.record_cpu_time("mix_audio", 50);

        assert_eq!(profiler.get_stage_time("decode"), Some(100));
        assert_eq!(profiler.get_stage_time("encode"), Some(200));
        assert_eq!(profiler.get_stage_time("invalid"), None);

        let report = profiler.generate_report();
        assert_eq!(report.bytes_processed, 1024);
        assert_eq!(report.frames_processed, 30);
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
        profiler.record_bytes_processed(1000);

        let report = profiler.generate_report();
        let formatted = report.format();

        assert!(formatted.contains("Profiling Report"));
        assert!(formatted.contains("test"));
        assert!(formatted.contains("100 ms"));
    }

    #[test]
    fn test_profiler_defaults() {
        let profiler = Profiler::default();
        assert_eq!(profiler.total_elapsed_ms(), 0);
        assert!(profiler.generate_report().stage_times.is_empty());
    }

    #[test]
    fn test_drift_metrics() {
        let drift = DriftMetrics::new();
        assert!((drift.drift_frames_max - 0.0).abs() < f64::EPSILON);
        assert!((drift.resample_ratio_avg - 0.0).abs() < f64::EPSILON);
    }
}
