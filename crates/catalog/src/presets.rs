//! Preset recipes (EFFECTS 4) expressed as editable graph templates.
//! A preset is a node list plus edges; inserting it into a project creates real,
//! editable nodes with new IDs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetNode {
    pub type_id: String,
    /// Index into the graph's node list; edges reference these indices.
    pub index: usize,
    pub params: serde_json::Map<String, serde_json::Value>,
    pub position: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetEdge {
    pub from: usize,
    pub from_port: String,
    pub to: usize,
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub requirement_refs: Vec<String>,
    pub nodes: Vec<PresetNode>,
    pub edges: Vec<PresetEdge>,
    pub output: usize,
    pub macros: Vec<String>,
}

fn n(index: usize, type_id: &str, params: &[(&str, serde_json::Value)]) -> PresetNode {
    let mut map = serde_json::Map::new();
    for (k, v) in params {
        map.insert(k.to_string(), v.clone());
    }
    PresetNode {
        type_id: type_id.to_string(),
        index,
        params: map,
        position: [(index as f32 % 4.0) * 220.0, (index as f32 / 4.0) * 140.0],
    }
}

fn e(from: usize, to: usize) -> PresetEdge {
    PresetEdge { from, from_port: "color".into(), to, to_port: "source".into() }
}

fn chain(types: &[&str], ids: &str) -> Preset {
    let source = n(0, types[0], &[]);
    let mut nodes = vec![source];
    let mut edges = vec![];
    for (i, t) in types.iter().enumerate().skip(1) {
        nodes.push(n(i, t, &[]));
        edges.push(e(i - 1, i));
    }
    let output = types.len() - 1;
    nodes.push(n(output + 1, "flux.output", &[]));
    edges.push(e(output, output + 1));
    Preset {
        id: ids.into(),
        name: ids.rsplit('.').next().unwrap_or(ids).replace('_', " "),
        description: format!("Graph chain: {}", types.join(" -> ")),
        requirement_refs: vec!["FX001".into()],
        nodes,
        edges,
        output,
        macros: vec![],
    }
}

pub fn all_presets() -> Vec<Preset> {
    let mut v = Vec::new();

    // Bass Impact: blob -> chromatic metal -> bloom (bass->deformation via mapping)
    {
        let mut p = chain(&["flux.source", "gen.sdf_blob", "fx.chromatic_metal", "fx.bloom"], "preset.bass_impact");
        p.name = "Bass Impact".into();
        p.description = "Blob -> Chromatic metal -> Bloom. Bass drives deformation, onset triggers a pulse.".into();
        p.requirement_refs = vec!["FX001".into(), "G081".into(), "R014".into()];
        p.macros = vec!["bass->blob.radius".into(), "onset->bloom.intensity".into(), "treble->grain".into()];
        if let Some(node) = p.nodes.iter_mut().find(|n| n.type_id == "gen.sdf_blob") {
            node.params.insert("bassAmount".into(), serde_json::json!(1.2));
        }
        v.push(p);
    }
    v.push(chain(&["flux.source", "fx.chromatic", "fx.vhs", "fx.crt"], "preset.vhs_tracking"));
    v.push(chain(&["flux.source", "fx.crt", "fx.noise"], "preset.crt_phosphor"));
    v.push(chain(&["flux.source", "fx.color_adjustment", "fx.halftone", "fx.noise"], "preset.editorial_halftone"));
    v.push(chain(&["flux.source", "fx.print"], "preset.cmyk_print"));
    v.push(chain(&["flux.source", "fx.tone", "fx.print", "fx.noise"], "preset.risograph"));
    v.push(chain(&["flux.source", "fx.dither"], "preset.sierra_dither"));
    v.push(chain(&["flux.source", "fx.pixel_ops"], "preset.modular_pixel_dissolve"));
    v.push(chain(&["flux.source", "fx.technical"], "preset.industrial_ct"));
    v.push(chain(&["flux.source", "fx.gradient_map", "fx.bloom"], "preset.thermal_vision"));
    v.push(chain(&["flux.source", "fx.technical"], "preset.cad_wireframe"));
    v.push(chain(&["flux.source", "fx.technical"], "preset.technical_cutaway"));
    v.push(chain(&["flux.source", "fx.pattern_refraction"], "preset.reeded_glass"));
    v.push(chain(&["flux.source", "fx.pattern_refraction"], "preset.fluted_glass"));
    v.push(chain(&["flux.source", "fx.pattern_refraction"], "preset.microprism_glass"));
    v.push(chain(&["flux.source", "fx.chromatic_metal", "fx.materials"], "preset.liquid_chrome_metal"));
    v.push(chain(&["gen.water_caustic", "flux.color"], "preset.water_caustics"));
    v.push(chain(&["gen.fractal_noise"], "preset.fractal_noise"));
    v.push(chain(&["gen.clouds"], "preset.clouds"));
    v.push(chain(&["gen.nebula"], "preset.nebula"));
    v.push(chain(&["gen.mesh_gradient_fill"], "preset.mesh_gradient"));
    v.push(chain(&["gen.moire"], "preset.moire"));
    v.push(chain(&["gen.glowing_wave"], "preset.glowing_wave"));
    v.push(chain(&["gen.pattern_grid"], "preset.pattern_grid"));
    v.push(chain(&["gen.concentric"], "preset.concentric_patterns"));
    v.push(chain(&["gen.texture_field"], "preset.paper_texture"));
    v.push(chain(&["fx.technical"], "preset.blueprint"));
    v.push(chain(&["fx.text_graphic"], "preset.ascii_art"));
    v.push(chain(&["gen.topographic"], "preset.topographic_contours"));
    v.push(chain(&["fx.text_graphic"], "preset.shapes_mosaic"));
    v.push(chain(&["fx.text_graphic"], "preset.code_rain"));
    v.push(chain(&["flux.source", "fx.slice_shift", "fx.chromatic", "fx.glitch"], "preset.cyberpunk_glitch"));
    v.push(chain(&["flux.source", "fx.glow", "fx.bloom"], "preset.bloom_and_halo"));
    v.push(chain(&["gen.fractal_noise"], "preset.fractal_noise_template"));
    v.push(chain(&["gen.nebula"], "preset.nebula_template"));
    v.push(chain(&["gen.aurora"], "preset.aurora"));
    {
        let mut p = chain(
            &["flux.source", "gen.particle_emitter", "gen.particle_render", "fx.bloom"],
            "preset.flow_field_particles",
        );
        p.description = "Curl field -> particles -> trails -> bloom.".into();
        p.requirement_refs = vec!["G110".into(), "G112".into(), "FX001".into()];
        v.push(p);
    }
    v.push(chain(&["gen.sdf_blob", "fx.chromatic_metal", "fx.bloom"], "preset.liquid_blob"));
    v.push(chain(&["gen.sdf_blob", "gen.sdf_material", "fx.bloom"], "preset.metaball_glow"));
    v.push(chain(&["flux.source", "fx.crt", "fx.vhs"], "preset.crt_vhs_mix"));
    v.push(chain(&["gen.reaction_diffusion"], "preset.reaction_skin"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_catalog;

    #[test]
    fn preset_types_exist_in_catalog() {
        let catalog = build_catalog();
        for p in all_presets() {
            for node in &p.nodes {
                assert!(
                    catalog.get(&node.type_id).is_some() || node.type_id == "flux.output",
                    "preset {} references missing type {}",
                    p.id,
                    node.type_id
                );
            }
            assert!(p.nodes.iter().any(|n| n.index == p.output));
        }
    }

    #[test]
    fn preset_count_at_least_required() {
        // EFFECTS 4 requires at least 35 named recipes.
        assert!(all_presets().len() >= 35);
    }
}
