//! Document model: project, settings, assets, timeline, graphs, mappings.
//!
//! Schema version 2 is the authoritative project format. Maps use BTreeMap so
//! serialized key order is deterministic. Runtime handles (GPU resources, open
//! sockets, device IDs) never appear here.

use crate::time::{FrameRate, RationalTime, TimeRange};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 2;
/// Initial hard limit for project duration. 7 days in seconds.
pub const MAX_PROJECT_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const HD_1080: Size = Size {
        width: 1920,
        height: 1080,
    };
    pub fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelAspect {
    pub num: u32,
    pub den: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSpec {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkingColor {
    #[serde(rename = "linear_rec709")]
    LinearRec709,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub size: Size,
    pub frame_rate: FrameRate,
    pub pixel_aspect: PixelAspect,
    pub audio: AudioSpec,
    pub working_color: WorkingColor,
    pub seed: String,
    pub simulation_step: RationalTime,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            size: Size::HD_1080,
            frame_rate: FrameRate::FPS_60,
            pixel_aspect: PixelAspect { num: 1, den: 1 },
            audio: AudioSpec {
                sample_rate: 48_000,
                channels: 2,
            },
            working_color: WorkingColor::LinearRec709,
            seed: "0".into(),
            simulation_step: RationalTime::new(1, 120).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Video,
    Image,
    Audio,
    Svg,
    Lottie,
    ImageSequence,
    Gltf,
    Ply,
    Csv,
}

/// Static, serialized description of an imported asset. File-system paths are
/// resolved relative to the project; fingerprint checks happen at load/export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    pub original_path: String,
    pub relative_path: Option<String>,
    /// Cheap fingerprint: "$size-$mtime_nanos"; full SHA-256 optional.
    pub fingerprint: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub duration: Option<RationalTime>,
    pub streams: Vec<AssetStream>,
    pub rotation_deg: i32,
    pub sample_aspect: PixelAspect,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetStream {
    pub index: i32,
    pub kind: String,
    pub codec: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortType {
    Texture2D,
    Mask,
    Depth,
    Normal,
    MotionVectors,
    Scalar,
    Int,
    Bool,
    Color,
    Vec2,
    Vec3,
    Transform,
    AudioFeatures,
    AudioSpectrum,
    Event,
    BeatState,
    Geometry,
    Scene,
    ParticleBuffer,
    SdfField,
    VectorField,
    PathSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub id: String,
    pub port_type: PortType,
    pub required: bool,
}

/// User-authored value. Integers are i32, colors are linear RGB+straight alpha.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParamValue {
    Float { value: f64 },
    Int { value: i32 },
    Bool { value: bool },
    Enum { value: String },
    Text { value: String },
    Color { value: [f64; 4] },
    Vec2 { value: [f64; 2] },
    Vec3 { value: [f64; 3] },
    Asset { asset_id: String },
    Path { path_set_id: String },
}

impl ParamValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParamValue::Float { value } => Some(*value),
            ParamValue::Int { value } => Some(*value as f64),
            ParamValue::Bool { value } => Some(if *value { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_f64_vec(&self) -> Option<Vec<f64>> {
        match self {
            ParamValue::Float { value } => Some(vec![*value]),
            ParamValue::Int { value } => Some(vec![*value as f64]),
            ParamValue::Bool { value } => Some(vec![if *value { 1.0 } else { 0.0 }]),
            ParamValue::Color { value } => Some(value.to_vec()),
            ParamValue::Vec2 { value } => Some(value.to_vec()),
            ParamValue::Vec3 { value } => Some(value.to_vec()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Interpolation {
    Step,
    Linear,
    Cubic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    pub time: RationalTime,
    pub value: ParamValue,
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub type_id: String,
    pub type_version: u32,
    #[serde(default)]
    pub position: [f32; 2],
    #[serde(default)]
    pub params: BTreeMap<String, ParamValue>,
    #[serde(default)]
    pub bypass: bool,
    /// Missing type records are preserved and shown as visible errors rather
    /// than silently substituted.
    #[serde(default)]
    pub missing_type: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub id: String,
    pub name: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Node ID whose `out` port is the rendered result.
    pub output_node: String,
    #[serde(default)]
    pub keyframes: BTreeMap<String, Vec<Keyframe>>,
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: &str) -> Option<&mut GraphNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    pub fn incoming(&self, node: &str, port: &str) -> Option<&GraphEdge> {
        self.edges
            .iter()
            .find(|e| e.to_node == node && e.to_port == port)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClipSource {
    Asset {
        asset_id: String,
    },
    GeneratorScene {
        generator_id: String,
    },
    LiveSource {
        source_id: String,
    },
    FreezeFrame {
        asset_id: String,
        time: RationalTime,
    },
    RecordedTake {
        take_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform2D {
    /// Position in normalized composition coordinates, center-based.
    pub position: [f64; 2],
    pub scale: [f64; 2],
    pub rotation: f64,
    /// Anchor in normalized source coordinates.
    pub anchor: [f64; 2],
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            position: [0.5, 0.5],
            scale: [1.0, 1.0],
            rotation: 0.0,
            anchor: [0.5, 0.5],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioControls {
    pub gain_db: f64,
    pub pan: f64,
    pub fade_in: RationalTime,
    pub fade_out: RationalTime,
    pub muted: bool,
}

impl Default for AudioControls {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            pan: 0.0,
            fade_in: RationalTime::ZERO,
            fade_out: RationalTime::ZERO,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub name: String,
    pub source: ClipSource,
    /// Timeline placement. Generator clips default to 8s of finite duration.
    pub range: TimeRange,
    /// Source window. For non-looping sources this is clamped by trim.
    pub source_range: TimeRange,
    /// Rational playback rate; reverse is a negative rate.
    pub rate: RationalTime,
    #[serde(default)]
    pub loop_policy: LoopPolicy,
    #[serde(default)]
    pub graph_id: Option<String>,
    #[serde(default)]
    pub transform: Transform2D,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub audio: AudioControls,
    #[serde(default)]
    pub linked_group: Option<String>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoopPolicy {
    #[default]
    None,
    Loop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackKind {
    Video,
    Audio,
    Adjustment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlendMode {
    Normal,
    Add,
    Screen,
    Multiply,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    #[serde(default)]
    pub clip_order: Vec<String>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default = "default_blend")]
    pub blend_mode: BlendMode,
    #[serde(default)]
    pub graph_id: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_blend() -> BlendMode {
    BlendMode::Normal
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub range: TimeRange,
    #[serde(default)]
    pub track_order: Vec<String>,
    #[serde(default)]
    pub tracks: BTreeMap<String, Track>,
    #[serde(default)]
    pub clips: BTreeMap<String, Clip>,
    #[serde(default)]
    pub master_graph_id: Option<String>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            range: TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(8)),
            track_order: vec![],
            tracks: BTreeMap::new(),
            clips: BTreeMap::new(),
            master_graph_id: None,
        }
    }
}

impl Timeline {
    /// Ordered clips of a track.
    pub fn track_clips(&self, track_id: &str) -> Vec<&Clip> {
        self.tracks
            .get(track_id)
            .map(|t| {
                t.clip_order
                    .iter()
                    .filter_map(|id| self.clips.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Tracks in bottom-to-top stored order.
    pub fn ordered_tracks(&self) -> Vec<&Track> {
        self.track_order
            .iter()
            .filter_map(|id| self.tracks.get(id))
            .collect()
    }
}

/// A modulation mapping from a source feature to one node parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub id: String,
    pub node_id: String,
    pub param_id: String,
    pub source: ModulationSource,
    /// Raw input range used to normalize the source.
    pub in_min: f64,
    pub in_max: f64,
    pub amount: f64,
    pub fold: FoldMode,
    pub order: i32,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub attack_ms: f64,
    #[serde(default)]
    pub release_ms: f64,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FoldMode {
    Add,
    Multiply,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModulationSource {
    Feature {
        feature: FeatureId,
    },
    Constant {
        value: f64,
    },
    Lfo {
        shape: LfoShape,
        frequency: f64,
        phase: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatureId {
    Rms,
    Peak,
    Bass,
    LowMid,
    Mid,
    HighMid,
    Treble,
    Centroid,
    Flux,
    Onset,
    BeatPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LfoShape {
    Sine,
    Triangle,
    Saw,
    Square,
    SampleHold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub id: String,
    pub name: String,
    pub time: RationalTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub project_id: String,
    pub name: String,
    pub created_utc: String,
    pub modified_utc: String,
    pub settings: ProjectSettings,
    #[serde(default)]
    pub assets: BTreeMap<String, Asset>,
    #[serde(default)]
    pub timeline: Timeline,
    #[serde(default)]
    pub graphs: BTreeMap<String, Graph>,
    #[serde(default)]
    pub mappings: BTreeMap<String, Mapping>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// Vendor extension metadata preserved verbatim across save/load.
    #[serde(default)]
    pub extensions: serde_json::Map<String, serde_json::Value>,
}

impl Project {
    pub fn new(name: impl Into<String>, seed: u64) -> Self {
        let now = now_utc();
        Self {
            schema_version: SCHEMA_VERSION,
            project_id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            created_utc: now.clone(),
            modified_utc: now,
            settings: ProjectSettings {
                seed: seed.to_string(),
                ..Default::default()
            },
            assets: BTreeMap::new(),
            timeline: Timeline::default(),
            graphs: BTreeMap::new(),
            mappings: BTreeMap::new(),
            markers: vec![],
            extensions: serde_json::Map::new(),
        }
    }

    pub fn seed_u64(&self) -> u64 {
        self.settings.seed.parse().unwrap_or(0)
    }

    pub fn duration(&self) -> RationalTime {
        self.timeline.range.duration
    }
}

pub fn now_utc_pub() -> String {
    now_utc()
}

fn now_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // RFC3339 UTC without pulling a date library: fixed-epoch fallback if absurd.
    let (y, mo, d, h, mi, s) = civil_from_unix(secs as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Days-from-civil algorithm (Howard Hinnant), public domain.
fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if mo <= 2 { y + 1 } else { y };
    (
        y,
        mo,
        d,
        (rem / 3600) as u32,
        ((rem % 3600) / 60) as u32,
        (rem % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_key_order_is_deterministic() {
        let mut a = Project::new("A", 1);
        let mut b = Project::new("A", 1);
        for (p, id) in [(&mut a, "zzz"), (&mut b, "aaa")] {
            p.assets.insert(
                id.to_string(),
                Asset {
                    id: id.to_string(),
                    kind: AssetKind::Image,
                    name: id.into(),
                    original_path: "x".into(),
                    relative_path: None,
                    fingerprint: "0".into(),
                    size_bytes: 0,
                    sha256: None,
                    duration: None,
                    streams: vec![],
                    rotation_deg: 0,
                    sample_aspect: PixelAspect { num: 1, den: 1 },
                    missing: false,
                },
            );
        }
        a.extensions
            .insert("vendor".into(), serde_json::json!({"k": 1}));
        b.extensions
            .insert("vendor".into(), serde_json::json!({"k": 1}));
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        assert!(ja.contains(r#""assets":{"zzz""#));
        assert!(jb.contains(r#""assets":{"aaa""#));
    }

    #[test]
    fn default_settings_match_contract() {
        let s = ProjectSettings::default();
        assert_eq!(
            s.size,
            Size {
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(s.frame_rate, FrameRate::FPS_60);
        assert_eq!(s.audio.sample_rate, 48_000);
        assert_eq!(s.simulation_step, RationalTime::new(1, 120).unwrap());
    }
}
