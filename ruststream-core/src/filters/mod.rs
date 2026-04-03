//! Filters module - FFmpeg filter builders
//!
//! Provides filter_complex string builders for FFmpeg.

/// Transition types
#[derive(Debug, Clone)]
pub enum TransitionType {
    Fade,
    WipeLeft,
    WipeRight,
    CircleCrop,
}

/// Build transition filter
pub fn build_transition(
    transition: &TransitionType,
    duration: f64,
    offset: f64,
) -> String {
    match transition {
        TransitionType::Fade => {
            format!("xfade=transition=fade:duration={}:offset={}", duration, offset)
        }
        TransitionType::WipeLeft => {
            format!("xfade=transition=wipeleft:duration={}:offset={}", duration, offset)
        }
        TransitionType::WipeRight => {
            format!("xfade=transition=wiperight:duration={}:offset={}", duration, offset)
        }
        TransitionType::CircleCrop => {
            format!("xfade=transition=circlecrop:duration={}:offset={}", duration, offset)
        }
    }
}
