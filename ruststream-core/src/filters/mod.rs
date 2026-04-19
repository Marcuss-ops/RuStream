//! Filters module - FFmpeg filter builders and overlay asset cache.

pub mod overlay_cache;

pub use overlay_cache::{OverlayAsset, OverlayCache, OverlayCacheStats, global_overlay_cache};


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
