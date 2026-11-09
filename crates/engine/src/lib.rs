//! Flux engine: graph validation/compilation, timeline evaluation, control
//! routing, frame ABI, and the deterministic render plan.
//!
//! The engine is UI-agnostic. The desktop shell, the GPU renderer and the
//! offline exporter all consume the same plan.

pub mod frame;
pub mod graph;
pub mod pod;
pub mod router;
pub mod timeline;

pub use frame::{
    export_frame_count, frame_time, AudioSnapshot, FrameContext, FrameUniform, MusicState,
};
pub use graph::{compile, validate, CompiledGraph, Registry};
pub use router::{pack_params, ResolvedParams, Router};
pub use timeline::{
    export_range, map_source_time, render_items, visible_clips, RenderItem, VisibleClip,
};
