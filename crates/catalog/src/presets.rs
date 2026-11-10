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

fn chain(types: &[&str], ids: &str) -> Preset {
    let catalog = shared_catalog();
    let manifest = |t: &str| catalog.get(t).map(|e| &e.manifest);

    // Recipes were authored as "effect on footage". In the node editor there is
    // no footage, so a `flux.source` root is a passthrough of transparent black
    // and the whole chain renders black. Substitute a generator that gives the
    // recipe something real to work on, chosen from its id.
    let substitute: &str = if ids.contains("glass") || ids.contains("chrome") || ids.contains("thermal")
        || ids.contains("metal") || ids.contains("bloom")
    {
        "gen.mesh_gradient_fill"
    } else if ids.contains("technical") || ids.contains("wireframe") || ids.contains("blueprint")
        || ids.contains("industrial") || ids.contains("ascii") || ids.contains("code_rain")
        || ids.contains("mosaic")
    {
        // These modes derive structure from luminance, so they need a bright,
        // high-contrast field (a dark grid renders as black).
        "gen.texture_field"
    } else {
        "gen.plasma"
    };

    let types: Vec<&str> = types
        .iter()
        .map(|t| if *t == "flux.source" { substitute } else { *t })
        .collect();
    let types = types.as_slice();

    // If a recipe is a bare effect with nothing feeding it, prepend the seed
    // generator (handled by the auto-prepend below).

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut next = 0usize;

    // If the first node is an effect that needs an image input, prepend the seed
    // generator so the preset starts from something visible.
    let first = types.first().copied().unwrap_or("gen.solid");
    let first_needs_source = manifest(first)
        .map(|m| m.inputs.iter().any(|p| p.id == "source"))
        .unwrap_or(false);
    if first != "flux.source" && first_needs_source {
        nodes.push(n(next, substitute, &[]));
        next += 1;
    }

    let mut prev: Option<(usize, &str)> = if next == 1 {
        Some((0, substitute))
    } else {
        None
    };
    for t in types {
        let idx = next;
        next += 1;
        nodes.push(n(idx, t, &[]));

        if let Some((pidx, ptype)) = prev {
            // Only connect when the target has an input and the types match the
            // engine's strict port-type check. Generators have no `source` input,
            // so they start their own branch; a required input left unconnected is
            // caught by validation rather than guessed at here.
            if let (Some((out_id, out_ty)), Some((in_id, in_ty))) = (
                pick_output(manifest(ptype)),
                pick_input(manifest(t)),
            ) {
                if out_ty == in_ty {
                    edges.push(PresetEdge {
                        from: pidx,
                        from_port: out_id,
                        to: idx,
                        to_port: in_id,
                    });
                }
            }
        }
        prev = Some((idx, t));
    }

    let sink = next;
    nodes.push(n(sink, "flux.output", &[]));
    if let Some((pidx, ptype)) = prev {
        if let (Some((out_id, out_ty)), Some((in_id, in_ty))) =
            (pick_output(manifest(ptype)), pick_input(manifest("flux.output")))
        {
            if out_ty == in_ty {
                edges.push(PresetEdge {
                    from: pidx,
                    from_port: out_id,
                    to: sink,
                    to_port: in_id,
                });
            }
        }
    }

    Preset {
        id: ids.into(),
        name: ids.rsplit('.').next().unwrap_or(ids).replace('_', " "),
        description: format!("Graph chain: {}", types.join(" -> ")),
        requirement_refs: vec!["FX001".into()],
        nodes,
        edges,
        output: sink,
        macros: vec![],
    }
}

fn shared_catalog() -> &'static crate::Catalog {
    static C: std::sync::OnceLock<crate::Catalog> = std::sync::OnceLock::new();
    C.get_or_init(crate::build_catalog)
}

/// Preferred image output: `color`, else the first output.
fn pick_output(m: Option<&crate::Manifest>) -> Option<(String, crate::manifest::PortType)> {
    let m = m?;
    m.outputs
        .iter()
        .find(|p| p.id == "color")
        .or_else(|| m.outputs.first())
        .map(|p| (p.id.clone(), p.port_type))
}

/// Preferred image input: required `source`, then optional `source`, then `a`,
/// then the first input (so effects with a differently named image port work).
fn pick_input(m: Option<&crate::Manifest>) -> Option<(String, crate::manifest::PortType)> {
    let m = m?;
    m.inputs
        .iter()
        .find(|p| p.id == "source" && p.required)
        .or_else(|| m.inputs.iter().find(|p| p.id == "source"))
        .or_else(|| m.inputs.iter().find(|p| p.id == "a"))
        .or_else(|| m.inputs.first())
        .map(|p| (p.id.clone(), p.port_type))
}

pub fn all_presets() -> Vec<Preset> {
    let mut v = Vec::new();

    // Bass Impact: blob -> chromatic metal -> bloom (bass->deformation via mapping)
    {
        let mut p = chain(
            &[
                "gen.sdf_blob",
                "fx.chromatic_metal",
                "fx.bloom",
            ],
            "preset.bass_impact",
        );
        p.name = "Bass Impact".into();
        p.description =
            "Blob -> Chromatic metal -> Bloom. Bass drives deformation, onset triggers a pulse."
                .into();
        p.requirement_refs = vec!["FX001".into(), "G081".into(), "R014".into()];
        p.macros = vec![
            "bass->blob.radius".into(),
            "onset->bloom.intensity".into(),
            "treble->grain".into(),
        ];
        if let Some(node) = p.nodes.iter_mut().find(|n| n.type_id == "gen.sdf_blob") {
            node.params
                .insert("bassAmount".into(), serde_json::json!(1.2));
        }
        v.push(p);
    }
    v.push(chain(
        &["flux.source", "fx.chromatic", "fx.vhs", "fx.crt"],
        "preset.vhs_tracking",
    ));
    v.push(chain(
        &["flux.source", "fx.crt", "fx.noise"],
        "preset.crt_phosphor",
    ));
    v.push(chain(
        &[
            "flux.source",
            "fx.color_adjustment",
            "fx.halftone",
            "fx.noise",
        ],
        "preset.editorial_halftone",
    ));
    v.push(chain(&["flux.source", "fx.print"], "preset.cmyk_print"));
    v.push(chain(
        &["flux.source", "fx.tone", "fx.print", "fx.noise"],
        "preset.risograph",
    ));
    v.push(chain(&["flux.source", "fx.dither"], "preset.sierra_dither"));
    v.push(chain(
        &["flux.source", "fx.pixel_ops"],
        "preset.modular_pixel_dissolve",
    ));
    v.push(chain(
        &["flux.source", "fx.technical"],
        "preset.industrial_ct",
    ));
    v.push(chain(
        &["flux.source", "fx.gradient_map", "fx.bloom"],
        "preset.thermal_vision",
    ));
    v.push(chain(
        &["flux.source", "fx.technical"],
        "preset.cad_wireframe",
    ));
    v.push(chain(
        &["flux.source", "fx.technical"],
        "preset.technical_cutaway",
    ));
    v.push(chain(
        &["flux.source", "fx.pattern_refraction"],
        "preset.reeded_glass",
    ));
    v.push(chain(
        &["flux.source", "fx.pattern_refraction"],
        "preset.fluted_glass",
    ));
    v.push(chain(
        &["flux.source", "fx.pattern_refraction"],
        "preset.microprism_glass",
    ));
    v.push(chain(
        &["flux.source", "fx.chromatic_metal", "fx.materials"],
        "preset.liquid_chrome_metal",
    ));
    v.push(chain(
        &["gen.water_caustic", "flux.color"],
        "preset.water_caustics",
    ));
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
    v.push(chain(
        &["flux.source", "fx.slice_shift", "fx.chromatic", "fx.glitch"],
        "preset.cyberpunk_glitch",
    ));
    v.push(chain(
        &["flux.source", "fx.glow", "fx.bloom"],
        "preset.bloom_and_halo",
    ));
    v.push(chain(
        &["gen.fractal_noise"],
        "preset.fractal_noise_template",
    ));
    v.push(chain(&["gen.nebula"], "preset.nebula_template"));
    v.push(chain(&["gen.aurora"], "preset.aurora"));
    {
        let mut p = chain(
            &[
                "gen.particle_emitter",
                "gen.particle_render",
                "fx.bloom",
            ],
            "preset.flow_field_particles",
        );
        p.description = "Curl field -> particles -> trails -> bloom.".into();
        p.requirement_refs = vec!["G110".into(), "G112".into(), "FX001".into()];
        v.push(p);
    }
    v.push(chain(
        &["gen.sdf_blob", "fx.chromatic_metal", "fx.bloom"],
        "preset.liquid_blob",
    ));
    v.push(chain(
        &["gen.sdf_blob", "gen.sdf_material", "fx.bloom"],
        "preset.metaball_glow",
    ));
    v.push(chain(
        &["flux.source", "fx.crt", "fx.vhs"],
        "preset.crt_vhs_mix",
    ));
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
