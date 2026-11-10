//! Manifest construction helpers shared by every effect/generator manifest.
//!
//! Manifests are defined in Rust so the catalog, uniform packing, UI, docs, and
//! tests are all generated from one source without hand-maintaining a second
//! schema. The catalog writer emits `catalog/effects/*.json` + `*.wgsl` for
//! inspection and drift checks.

use crate::manifest::*;

pub fn p_float(id: &str, label: &str, default: f64, min: f64, max: f64, unit: &str) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Float,
        default: serde_json::json!(default),
        min: Some(min),
        max: Some(max),
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: unit.into(),
        automatable: true,
        modulatable: true,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: vec![],
        label: label.into(),
        hidden: false,
    }
}

pub fn p_int(id: &str, label: &str, default: i64, min: i64, max: i64) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Int,
        default: serde_json::json!(default),
        min: Some(min as f64),
        max: Some(max as f64),
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: String::new(),
        automatable: true,
        modulatable: true,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: vec![],
        label: label.into(),
        hidden: false,
    }
}

pub fn p_bool(id: &str, label: &str, default: bool) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Bool,
        default: serde_json::json!(default),
        min: None,
        max: None,
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: String::new(),
        automatable: true,
        modulatable: true,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: vec![],
        label: label.into(),
        hidden: false,
    }
}

pub fn p_enum(id: &str, label: &str, default: &str, options: &[&str]) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Enum,
        default: serde_json::json!(default),
        min: None,
        max: None,
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: String::new(),
        automatable: true,
        modulatable: false,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: options.iter().map(|s| s.to_string()).collect(),
        label: label.into(),
        hidden: false,
    }
}

pub fn p_text(id: &str, label: &str, default: &str) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Text,
        default: serde_json::json!(default),
        min: None,
        max: None,
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: String::new(),
        automatable: false,
        modulatable: false,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: vec![],
        label: label.into(),
        hidden: false,
    }
}

pub fn p_color(id: &str, label: &str, rgba: [f64; 4]) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Color,
        default: serde_json::json!(rgba),
        min: None,
        max: None,
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: String::new(),
        automatable: true,
        modulatable: true,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: vec![],
        label: label.into(),
        hidden: false,
    }
}

pub fn p_vec2(
    id: &str,
    label: &str,
    default: [f64; 2],
    min: f64,
    max: f64,
    unit: &str,
) -> ParamDecl {
    ParamDecl {
        id: id.into(),
        kind: ParamKind::Vec2,
        default: serde_json::json!(default),
        min: Some(min),
        max: Some(max),
        soft_min: None,
        soft_max: None,
        scale: Scale::Linear,
        unit: unit.into(),
        automatable: true,
        modulatable: true,
        smoothing_ms: 0.0,
        state_impact: StateImpact::None,
        options: vec![],
        label: label.into(),
        hidden: false,
    }
}

pub fn source_port() -> PortDecl {
    PortDecl {
        id: "source".into(),
        port_type: PortType::Texture2D,
        required: true,
    }
}

pub fn mask_port() -> PortDecl {
    PortDecl {
        id: "mask".into(),
        port_type: PortType::Mask,
        required: false,
    }
}

pub fn color_output() -> PortDecl {
    PortDecl {
        id: "color".into(),
        port_type: PortType::Texture2D,
        required: true,
    }
}

/// The three extra ports every effect manifest gains for time, seed and
/// quality. These are documented as auto-injected by the ABI (uniforms), not
/// authored parameters.
pub fn standard_params() -> Vec<ParamDecl> {
    vec![p_float("wet", "Wet", 1.0, 0.0, 1.0, "")]
}

/// Build a fragment-effect manifest with the standard source/mask/mask ports.
pub fn effect(
    id: &str,
    name: &str,
    category: &str,
    shader: &str,
    quality: QualityClass,
    params: Vec<ParamDecl>,
    help: &str,
    requirement: &str,
) -> Manifest {
    // The common `wet` parameter is injected unless the caller already supplied
    // one (generator/CPU rows add their own labelled opacity control).
    let mut p = if params.iter().any(|d| d.id == "wet") {
        params
    } else {
        let mut base = standard_params();
        base.extend(params);
        base
    };
    // Guarantee manifest order: `wet` first for stable uniform slots.
    if let Some(pos) = p.iter().position(|d| d.id == "wet") {
        if pos != 0 {
            let wet = p.remove(pos);
            p.insert(0, wet);
        }
    }
    Manifest {
        id: id.into(),
        version: 1,
        requirement_ids: vec![requirement.into()],
        name: name.into(),
        category: category.into(),
        implementation: Implementation {
            kind: "fragment".into(),
            path: format!("{shader}.wgsl"),
            entry: "fs_main".into(),
        },
        inputs: vec![source_port(), mask_port()],
        outputs: vec![color_output()],
        parameters: p,
        time_model: TimeModel::Analytic,
        color_policy: "linear_premultiplied".into(),
        quality_class: quality,
        fixtures: vec![],
        help: help.into(),
        generator_id: String::new(),
        loop_status: String::new(),
        tags: vec![],
        accent: String::new(),
    }
}
