//! Catalog linter and generated-shader validity tests.

use crate::build_catalog;
use crate::manifest::ParamKind;
use crate::shader_gen;

fn check_no_duplicates() {
    let c = build_catalog();
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for e in &c.entries {
        *counts.entry(e.manifest.id.as_str()).or_default() += 1;
    }
    let dupes: Vec<(&str, usize)> = counts
        .iter()
        .filter(|(_, v)| **v > 1)
        .map(|(k, v)| (*k, *v))
        .collect();
    assert!(dupes.is_empty(), "duplicate manifest IDs: {dupes:?}");
}

#[test]
fn catalog_ids_are_unique() {
    check_no_duplicates();
}

#[test]
fn every_manifest_defaults_in_range() {
    let c = build_catalog();
    for e in &c.entries {
        for p in &e.manifest.parameters {
            match p.kind {
                ParamKind::Float => {
                    let v = p.default.as_f64().expect("float default");
                    if let Some(min) = p.min {
                        assert!(
                            v >= min - 1e-9,
                            "{} {} default {v} < min {min}",
                            e.manifest.id,
                            p.id
                        );
                    }
                    if let Some(max) = p.max {
                        assert!(
                            v <= max + 1e-9,
                            "{} {} default {v} > max {max}",
                            e.manifest.id,
                            p.id
                        );
                    }
                }
                ParamKind::Int => {
                    let v = p.default.as_i64().expect("int default");
                    if let (Some(min), Some(max)) = (p.min, p.max) {
                        assert!(
                            v as f64 >= min && v as f64 <= max,
                            "{} {} int out of range",
                            e.manifest.id,
                            p.id
                        );
                    }
                }
                ParamKind::Color => {
                    let arr = p.default.as_array().expect("color default");
                    assert_eq!(arr.len(), 4);
                }
                ParamKind::Enum => {
                    let v = p.default.as_str().expect("enum default");
                    assert!(
                        p.options.iter().any(|o| o == v),
                        "{} {} enum default {v} not in options",
                        e.manifest.id,
                        p.id
                    );
                }
                ParamKind::Vec2 => {
                    let arr = p.default.as_array().expect("vec2");
                    assert_eq!(arr.len(), 2);
                }
                ParamKind::Vec3 => {
                    let arr = p.default.as_array().expect("vec3");
                    assert_eq!(arr.len(), 3);
                }
                ParamKind::Bool => {
                    assert!(p.default.is_boolean());
                }
                ParamKind::Text => {
                    assert!(p.default.is_string());
                }
            }
        }
    }
}

#[test]
fn every_shader_generates_valid_structure() {
    let c = build_catalog();
    for e in &c.entries {
        if e.manifest.implementation.kind == "fragment" {
            let src = shader_gen::generate_shader(e);
            shader_gen::quick_wgsl_check(&src)
                .unwrap_or_else(|err| panic!("shader check failed for {}: {err}", e.manifest.id));
        }
    }
}

#[test]
fn all_28_effects_and_families_present() {
    let c = build_catalog();
    let ids: Vec<String> = c.entries.iter().map(|e| e.manifest.id.clone()).collect();
    for id in [
        "fx.bloom",
        "fx.bokeh_blur",
        "fx.channel_mixer",
        "fx.chromatic_metal",
        "fx.color_adjustment",
        "fx.colored_edges",
        "fx.dither",
        "preset.color_filter",
        "fx.gooey_merge",
        "fx.gradient_map",
        "fx.halftone",
        "fx.hatching",
        "fx.lens_distortion",
        "fx.outlines",
        "fx.pattern_refraction",
        "fx.pixel_stretch",
        "fx.pixelate",
        "fx.slice_shift",
        "fx.warp",
        "gen.clouds",
        "gen.concentric",
        "gen.fractal_noise",
        "gen.glowing_wave",
        "gen.mesh_gradient",
        "gen.moire",
        "gen.nebula",
        "gen.pattern_grid",
        "gen.water_caustic",
    ] {
        assert!(
            ids.contains(&id.to_string()),
            "missing requested effect {id}"
        );
    }
    for id in [
        "fx.tone",
        "fx.color_ops",
        "fx.noise",
        "fx.blur",
        "fx.glow",
        "fx.chromatic",
        "fx.vhs",
        "fx.crt",
        "fx.glitch",
        "fx.pixel_ops",
        "fx.smear",
        "fx.geometry",
        "fx.glass",
        "fx.materials",
        "fx.texture",
        "fx.print",
        "fx.technical",
        "fx.text_graphic",
        "fx.temporal",
        "fx.feedback",
        "fx.keys",
        "fx.masks",
        "fx.composite",
        "fx.transforms",
    ] {
        assert!(ids.contains(&id.to_string()), "missing family {id}");
    }
}

#[test]
fn required_generator_rows_present() {
    let c = build_catalog();
    let reqs: Vec<String> = c
        .entries
        .iter()
        .flat_map(|e| e.manifest.requirement_ids.iter().cloned())
        .collect();
    let required: Vec<String> = (1..=28)
        .map(|n| format!("G{n:03}"))
        .chain((30..=45).map(|n| format!("G{n:03}")))
        .chain((50..=68).map(|n| format!("G{n:03}")))
        .chain((70..=79).map(|n| format!("G{n:03}")))
        .chain((80..=89).map(|n| format!("G{n:03}")))
        .chain((90..=107).map(|n| format!("G{n:03}")))
        .chain((110..=120).map(|n| format!("G{n:03}")))
        .chain((130..=137).map(|n| format!("G{n:03}")))
        .chain((140..=148).map(|n| format!("G{n:03}")))
        .chain((150..=162).map(|n| format!("G{n:03}")))
        .collect();
    let cond: Vec<String> = (170..=177)
        .map(|n| format!("G{n:03}"))
        .chain((180..=185).map(|n| format!("G{n:03}")))
        .collect();
    for r in required {
        assert!(reqs.contains(&r), "missing required generator row {r}");
    }
    for r in cond {
        assert!(reqs.contains(&r), "missing conditional generator row {r}");
    }
}

#[test]
fn uniform_layout_matches_expected_size() {
    let c = build_catalog();
    for e in &c.entries {
        if e.manifest.implementation.kind != "fragment" {
            continue;
        }
        let (_, layout) = crate::pack::params_wgsl(&e.manifest.parameters);
        let bytes = layout.byte_size();
        assert_eq!(bytes % 16, 0);
        // No manifest should exceed a 4 KiB node uniform.
        assert!(
            bytes <= 4096,
            "{} node params too large: {bytes}",
            e.manifest.id
        );
        let numeric = e
            .manifest
            .parameters
            .iter()
            .filter(|p| p.kind != ParamKind::Text)
            .count();
        assert_eq!(layout.slots.len(), numeric);
    }
}

/// Pin the named catalog constants to their exact literal values, so renaming
/// or re-tuning one without updating the record is a build failure rather than
/// a silent drift. These are NOT std::f64::consts::PI or TAU — see the doc
/// comment at builders.rs:10-30 for why the literals are intentional.
#[test]
fn named_constants_are_pinned_to_their_exact_literals() {
    use crate::builders::{FULL_TURN_2DP, FULL_TURN_7DP, HALF_TURN_2DP, HALF_TURN_7DP};

    // Assert the literal expected numbers, not `>= 1` and not `> 0`.
    assert_eq!(
        HALF_TURN_7DP, 3.1415926,
        "HALF_TURN_7DP must equal 3.1415926 exactly; if this fails, the constant \
         was re-tuned without updating the record"
    );
    assert_eq!(
        FULL_TURN_7DP, 6.2831853,
        "FULL_TURN_7DP must equal 6.2831853 exactly; if this fails, the constant \
         was re-tuned without updating the record"
    );
    assert_eq!(
        HALF_TURN_2DP, 3.14,
        "HALF_TURN_2DP must equal 3.14 exactly; if this fails, the constant \
         was re-tuned without updating the record"
    );
    assert_eq!(
        FULL_TURN_2DP, 6.28,
        "FULL_TURN_2DP must equal 6.28 exactly; if this fails, the constant \
         was re-tuned without updating the record"
    );
}
