//! Unified Render Graph - Single API Contract for Rust 100% Engine
//!
//! This module defines the `RenderGraph` struct that serves as the single source of truth
//! for all media processing operations. It replaces scattered Python adapters with a
//! unified Rust pipeline.
//!
//! # Architecture
//!
//! ```text
//! RenderGraph (input)
//!   ├── MediaTimelinePlan (video/effects/overlays)
//!   ├── AudioGraphConfig (audio processing)
//!   └── RenderConfig (execution settings)
//!         │
//!         ▼
//!   process_render_graph()
//!         │
//!         ▼
//!   RenderResult (output)
//!   ├── artifact_path: String
//!   ├── metrics: RenderMetrics
//!   ├── reason_codes: Vec<String>
//!   └── drift: DriftMetrics
//! ```

pub mod component;
pub mod config;
pub mod graph;
pub mod metrics;
pub mod process;
pub mod reason;
pub mod result;
pub mod stages;

// Re-export key types for convenience
pub use component::{ComponentId, ComponentMetrics};
pub use config::{RenderConfig, RenderMode};
pub use graph::RenderGraph;
pub use metrics::RenderMetrics;
pub use process::{process_render_graph, run_audio_graph};
pub use reason::ReasonCode;
pub use result::RenderResult;