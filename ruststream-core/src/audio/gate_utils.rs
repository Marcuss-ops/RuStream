//! Gate utility functions for audio processing
//!
//! This module provides shared gate logic used across audio baking and assembly.
//! Gates are used to mute/unmute audio at specific time ranges.

use serde::{Deserialize, Serialize};

/// A range where audio should be muted (gate applied)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioGateRange {
    pub start_s: f64,
    pub end_s: f64,
}

/// Build FFmpeg gate expression from ranges.
/// 
/// # Arguments
/// * `ranges` - Gate ranges (start_s, end_s) tuples
/// * `invert` - If true, returns 1 during gate ranges and 0 outside (for base audio)
///              If false, returns 0 during gate ranges and 1 outside (for VO/music)
/// 
/// # Returns
/// FFmpeg filter expression string for use with volume='expr':eval=frame
/// 
/// # Examples
/// ```ignore
/// use ruststream_core::audio::gate_utils::build_gate_expr_from_ranges;
/// 
/// // Empty ranges: always on (1) or always off (0)
/// assert_eq!(build_gate_expr_from_ranges(&[], false), "1");
/// assert_eq!(build_gate_expr_from_ranges(&[], true), "0");
/// 
/// // Single range: muted during 0-5s
/// let expr = build_gate_expr_from_ranges(&[(0.0, 5.0)], false);
/// assert!(expr.contains("between(t"));
/// ```
pub fn build_gate_expr_from_ranges(ranges: &[(f64, f64)], invert: bool) -> String {
    if ranges.is_empty() {
        return if invert { "0".to_string() } else { "1".to_string() };
    }

    let parts: Vec<String> = ranges
        .iter()
        .filter(|(s, e)| e > s)
        .map(|(s, e)| format!("between(t\\,{:.6}\\,{:.6})", s, e))
        .collect();

    if parts.is_empty() {
        return if invert { "0".to_string() } else { "1".to_string() };
    }

    let expr = parts.join("+");
    if invert {
        format!("if({expr},1,0)")
    } else {
        format!("if({expr},0,1)")
    }
}

/// Build intro-only gate expression: 1 only in (0, intro_duration), 0 elsewhere
pub fn build_intro_only_gate_expr(intro_duration: f64) -> String {
    if intro_duration <= 0.01 {
        return "0".to_string();
    }
    format!("if(between(t\\,0\\,{:.6}),1,0)", intro_duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gate_expr_empty() {
        let ranges: Vec<(f64, f64)> = vec![];
        assert_eq!(build_gate_expr_from_ranges(&ranges, false), "1");
        assert_eq!(build_gate_expr_from_ranges(&ranges, true), "0");
    }

    #[test]
    fn test_build_gate_expr_single() {
        let ranges = vec![(0.0, 5.0)];
        let expr = build_gate_expr_from_ranges(&ranges, false);
        assert!(expr.contains("between(t"));
        assert!(expr.contains("0.000000"));
        assert!(expr.contains("5.000000"));
    }

    #[test]
    fn test_build_gate_expr_multiple() {
        let ranges = vec![
            (0.0, 5.0),
            (10.0, 15.0),
        ];
        let expr = build_gate_expr_from_ranges(&ranges, false);
        assert!(expr.contains("+"));
    }

    #[test]
    fn test_build_intro_only_gate_expr() {
        assert_eq!(build_intro_only_gate_expr(0.0), "0");
        assert_eq!(build_intro_only_gate_expr(0.005), "0");
        let expr = build_intro_only_gate_expr(5.0);
        assert!(expr.contains("between(t\\,0\\,5.000000)"));
    }
}
