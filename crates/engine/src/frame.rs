//! Frame context and the deterministic CPU evaluator used as the reference
//! path for export, tests and the preview fallback.
//!
//! The GPU renderer consumes the same `FrameContext`, manifests and parameter
//! values. Any behavior difference between CPU reference and GPU is a bug to be
//! found by the parity tests; the CPU path is never a second *product* engine.

use flux_domain::{ErrorCode, FluxError, FrameRate, Graph, Project, RationalTime};

/// The 96-byte frame ABI in Rust form (CONTRACTS 3). Field offsets must match
/// WGSL exactly. The render crate holds the bytemuck-derived mirror.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameUniform {
    pub dims: [u32; 4],
    pub sampling: [f32; 4],
    pub clock: [f32; 4],
    pub counters: [u32; 4],
    pub music: [f32; 4],
    pub controls: [f32; 4],
}

pub const FRAME_UNIFORM_SIZE: usize = 96;

impl FrameUniform {
    pub fn to_bytes(&self) -> [u8; 96] {
        let mut out = [0u8; 96];
        for (i, v) in self.dims.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in self.sampling.iter().enumerate() {
            let o = 16 + i * 4;
            out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in self.clock.iter().enumerate() {
            let o = 32 + i * 4;
            out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in self.counters.iter().enumerate() {
            let o = 48 + i * 4;
            out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in self.music.iter().enumerate() {
            let o = 64 + i * 4;
            out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in self.controls.iter().enumerate() {
            let o = 80 + i * 4;
            out[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioSnapshot {
    pub bands: [f32; 4],
    pub extra: [f32; 4],
    pub params: [f32; 4],
}

impl Default for AudioSnapshot {
    fn default() -> Self {
        Self {
            bands: [0.0; 4],
            extra: [0.0; 4],
            params: [0.0; 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MusicState {
    pub beat_phase: f32,
    pub bar_phase: f32,
    pub tempo: f32,
    pub beat_confidence: f32,
}

impl Default for MusicState {
    fn default() -> Self {
        Self {
            beat_phase: 0.0,
            bar_phase: 0.0,
            tempo: 120.0,
            beat_confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameContext {
    /// Exact project time.
    pub project_time: RationalTime,
    pub frame_index: u64,
    pub transport_epoch: u32,
    pub project_seed: u64,
    pub node_seed: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub preview_scale: [f32; 2],
    pub audio: AudioSnapshot,
    pub music: MusicState,
    /// Which class of evidence filled `audio` / `music`. Recorded on every
    /// frame so a live-fed frame can never be mistaken for a deterministic one.
    pub feature_source: flux_domain::params::FeatureSource,
    pub quality_scale: f32,
    pub sim_dt: f32,
    pub sim_interpolation: f32,
}

impl FrameContext {
    /// The document-only frame context: no audio evidence has been supplied, so
    /// the audio snapshot is the silent baseline and the recorded source says
    /// so. Callers that have audio mutate `ctx.audio`, `ctx.music`, and
    /// `ctx.feature_source` in place after this call — the seam is the field
    /// assignment, not a separate constructor.
    pub fn for_project(project: &Project, project_time: RationalTime, frame_index: u64) -> Self {
        let size = project.settings.size;
        Self {
            project_time,
            frame_index,
            transport_epoch: 0,
            project_seed: project.seed_u64(),
            node_seed: 0,
            output_width: size.width,
            output_height: size.height,
            source_width: size.width,
            source_height: size.height,
            preview_scale: [1.0, 1.0],
            audio: AudioSnapshot::default(),
            music: MusicState::default(),
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
            quality_scale: 1.0,
            sim_dt: project.settings.simulation_step.as_f64() as f32,
            sim_interpolation: 0.0,
        }
    }

    /// Time split for the 96-byte ABI: high = floor(seconds/1024)*1024 (used as
    /// a phase anchor), low = seconds - high. For shaders that sum low+high we
    /// keep the split but report full seconds in `low` to avoid float drift.
    pub fn time_split(&self) -> (f32, f32) {
        let secs = self.project_time.as_f64();
        let high = (secs / 1024.0).floor() * 1024.0;
        let low = secs - high;
        (low as f32, high as f32)
    }

    pub fn uniform(&self, wet: f32) -> FrameUniform {
        let (low, high) = self.time_split();
        let frame_lo = (self.frame_index & 0xFFFF_FFFF) as u32;
        let frame_hi = (self.frame_index >> 32) as u32;
        FrameUniform {
            dims: [
                self.output_width,
                self.output_height,
                self.source_width,
                self.source_height,
            ],
            sampling: [
                1.0 / self.output_width.max(1) as f32,
                1.0 / self.output_height.max(1) as f32,
                self.preview_scale[0],
                self.preview_scale[1],
            ],
            clock: [low, high, self.sim_dt, self.sim_interpolation],
            counters: [frame_lo, frame_hi, self.node_seed, self.transport_epoch],
            music: [
                self.music.beat_phase,
                self.music.bar_phase,
                self.music.tempo,
                self.music.beat_confidence,
            ],
            controls: [wet, self.quality_scale, self.sim_dt, 0.0],
        }
    }
}

/// Frame rate helpers for export.
pub fn export_frame_count(range: RationalTime, rate: FrameRate) -> u64 {
    let dur = range;
    let n = dur.div_ceil(RationalTime::frame_duration(rate));
    n.max(0) as u64
}

/// Frame start time for frame index.
pub fn frame_time(index: u64, rate: FrameRate) -> FluxErrorResult<RationalTime> {
    RationalTime::from_frames(index as i64, rate)
        .map_err(|e| FluxError::new(ErrorCode::InvalidCommand, "time", e.to_string()))
}

pub type FluxErrorResult<T> = Result<T, FluxError>;

/// A compiled runtime graph plus resolved parameters.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub node_id: String,
    pub type_id: String,
    pub params: std::collections::BTreeMap<String, flux_domain::ParamValue>,
    pub bypass: bool,
}

pub fn node_configs(graph: &Graph) -> Vec<NodeConfig> {
    graph
        .nodes
        .iter()
        .map(|n| NodeConfig {
            node_id: n.id.clone(),
            type_id: n.type_id.clone(),
            params: n.params.clone(),
            bypass: n.bypass,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_domain::Project;

    #[test]
    fn frame_uniform_is_96_bytes() {
        let u = FrameUniform {
            dims: [1920, 1080, 1920, 1080],
            sampling: [1.0 / 1920.0, 1.0 / 1080.0, 1.0, 1.0],
            clock: [1.5, 0.0, 0.008, 0.0],
            counters: [42, 0, 7, 0],
            music: [0.25, 0.5, 120.0, 0.8],
            controls: [1.0, 1.0, 0.008, 0.0],
        };
        let b = u.to_bytes();
        assert_eq!(b.len(), FRAME_UNIFORM_SIZE);
        // dims[1] at offset 4, counters[0] at 48
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 1080);
        assert_eq!(u32::from_le_bytes([b[48], b[49], b[50], b[51]]), 42);
    }

    #[test]
    fn export_counts_exact() {
        let fr = FrameRate::FPS_60;
        let d = RationalTime::new(10, 1).unwrap();
        assert_eq!(export_frame_count(d, fr), 600);
        let samples = d.div_round(RationalTime::new(1, 48000).unwrap());
        assert_eq!(samples, 480_000);
    }

    #[test]
    fn frame_start_times() {
        let fr = FrameRate::FPS_60;
        assert_eq!(frame_time(0, fr).unwrap(), RationalTime::ZERO);
        assert_eq!(frame_time(60, fr).unwrap(), RationalTime::from_secs(1));
    }

    #[test]
    fn context_carries_seeds() {
        let p = Project::new("x", 47019);
        let c = FrameContext::for_project(&p, RationalTime::ZERO, 0);
        assert_eq!(c.project_seed, 47019);
        assert_eq!(c.output_width, 1920);
    }

    #[test]
    fn time_split_reconstructs() {
        let p = Project::new("x", 1);
        let c = FrameContext::for_project(&p, RationalTime::new(12345, 1000).unwrap(), 10);
        let (low, high) = c.time_split();
        assert!(((low + high) as f64 - 12.345).abs() < 1e-4);
    }
}
