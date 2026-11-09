//! Flux render: the authoritative wgpu renderer and its execution plan.

pub mod gpu;
pub mod graph_exec;
pub mod pipeline;
pub mod preview;

pub use gpu::{BackendPref, Gpu, GpuInfo};
pub use graph_exec::RuntimeGraph;
pub use pipeline::{AudioFeaturesGpu, EffectCompiler, FrameUniformGpu, NodeParamsBuffer};

#[cfg(test)]
mod tests;
