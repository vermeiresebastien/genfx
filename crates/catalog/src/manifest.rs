//! Catalog manifest schema (CONTRACTS 5).
//!
//! A manifest is the single source of truth for an effect/generator: it drives
//! the parameter UI, uniform packing, shader selection, docs, and tests.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortDecl {
    pub id: String,
    #[serde(rename = "type")]
    pub port_type: PortType,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParamKind {
    Float,
    Int,
    Bool,
    Enum,
    Color,
    Vec2,
    Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    #[default]
    Linear,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StateImpact {
    #[default]
    None,
    ResetState,
    Recompile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDecl {
    pub id: String,
    pub kind: ParamKind,
    pub default: serde_json::Value,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub soft_min: Option<f64>,
    #[serde(default)]
    pub soft_max: Option<f64>,
    #[serde(default)]
    pub scale: Scale,
    #[serde(default)]
    pub unit: String,
    #[serde(default = "default_true")]
    pub automatable: bool,
    #[serde(default = "default_true")]
    pub modulatable: bool,
    #[serde(default)]
    pub smoothing_ms: f64,
    #[serde(default)]
    pub state_impact: StateImpact,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    pub kind: String,
    pub path: String,
    #[serde(default = "fs_main")]
    pub entry: String,
}

fn fs_main() -> String {
    "fs_main".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeModel {
    Analytic,
    FixedStep,
    CanonicalFrame,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QualityClass {
    L,
    M,
    H,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: u32,
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    pub name: String,
    pub category: String,
    pub implementation: Implementation,
    #[serde(default)]
    pub inputs: Vec<PortDecl>,
    #[serde(default)]
    pub outputs: Vec<PortDecl>,
    #[serde(default)]
    pub parameters: Vec<ParamDecl>,
    pub time_model: TimeModel,
    #[serde(default)]
    pub color_policy: String,
    pub quality_class: QualityClass,
    #[serde(default)]
    pub fixtures: Vec<String>,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub generator_id: String,
    /// Generator-only: deterministic loop behaviour.
    #[serde(default)]
    pub loop_status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Extra CSS-friendly color for node UI (not an effect algorithm).
    #[serde(default)]
    pub accent: String,
}

impl Manifest {
    pub fn param(&self, id: &str) -> Option<&ParamDecl> {
        self.parameters.iter().find(|p| p.id == id)
    }

    /// Default parameter values in manifest order (used by uniform packing and
    /// by node creation).
    pub fn defaults(&self) -> Vec<(String, serde_json::Value)> {
        self.parameters.iter().map(|p| (p.id.clone(), p.default.clone())).collect()
    }
}

/// A discovered catalog entry combining the manifest and its source text.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub manifest: Manifest,
    pub source: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.manifest.id == id)
    }

    pub fn by_category(&self) -> std::collections::BTreeMap<String, Vec<&CatalogEntry>> {
        let mut map: std::collections::BTreeMap<String, Vec<&CatalogEntry>> = Default::default();
        for e in &self.entries {
            map.entry(e.manifest.category.clone()).or_default().push(e);
        }
        map
    }
}
