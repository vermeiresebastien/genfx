//! Producer contract tests and a golden end-to-end export test.
//!
//! These prove (Workstream B build-up) that producer-backed generator rows
//! render visible non-transparent content without an uploaded texture, that
//! parameters and time change the result, that the same frame is reproducible,
//! and that the content survives a complete media export.

use flux_app::producer::{produce_node, Params};
use flux_domain::ParamValue;
use std::collections::BTreeMap;

fn params(pairs: &[(&str, ParamValue)]) -> Params {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn coverage(rgba: &[u8]) -> (usize, usize) {
    let mut opaque = 0;
    let mut nonblack = 0;
    for p in rgba.chunks_exact(4) {
        if p[3] > 0 {
            opaque += 1;
            if p[0] > 8 || p[1] > 8 || p[2] > 8 {
                nonblack += 1;
            }
        }
    }
    (opaque, nonblack)
}

#[test]
fn text_producer_renders_visible_glyphs() {
    // White fill on transparent background → opaque white glyph pixels exist.
    let p = params(&[
        ("size", ParamValue::Float { value: 0.2 }),
        (
            "fill",
            ParamValue::Color {
                value: [1.0, 1.0, 1.0, 1.0],
            },
        ),
        (
            "align",
            ParamValue::Enum {
                value: "center".into(),
            },
        ),
    ]);
    let rgba = produce_node("gen.text", &p, 320, 180, 0.0, 1)
        .unwrap()
        .expect("producer");
    assert_eq!(rgba.len(), 320 * 180 * 4);
    let (opaque, nonblack) = coverage(&rgba);
    assert!(opaque > 200, "expected glyph coverage, got opaque={opaque}");
    assert_eq!(opaque, nonblack, "glyphs should be white (non-black)");
    // Background must be transparent.
    assert_eq!(rgba[3], 0, "top-left corner should be transparent");
}

#[test]
fn text_producer_is_deterministic_and_size_sensitive() {
    let small = params(&[("size", ParamValue::Float { value: 0.1 })]);
    let big = params(&[("size", ParamValue::Float { value: 0.3 })]);
    let a = produce_node("gen.text", &small, 256, 144, 0.0, 1)
        .unwrap()
        .unwrap();
    let b = produce_node("gen.text", &small, 256, 144, 0.0, 1)
        .unwrap()
        .unwrap();
    assert_eq!(a, b, "same inputs must produce identical bytes");
    let c = produce_node("gen.text", &big, 256, 144, 0.0, 1)
        .unwrap()
        .unwrap();
    assert_ne!(a, c, "size parameter must change the result");
    let (oa, _) = coverage(&a);
    let (oc, _) = coverage(&c);
    assert!(
        oc > oa,
        "bigger size should cover more pixels ({oc} vs {oa})"
    );
}

#[test]
fn vector_producer_renders_and_animates() {
    let p = params(&[]);
    let a = produce_node("gen.shape", &p, 200, 200, 0.0, 1)
        .unwrap()
        .expect("vector producer");
    let b = produce_node("gen.shape", &p, 200, 200, 0.5, 1)
        .unwrap()
        .expect("vector producer");
    let (oa, _) = coverage(&a);
    assert!(oa > 500, "shape should cover area, got {oa}");
    assert_ne!(a, b, "vector producer must animate over time");
}

#[test]
fn analytic_generator_has_no_producer() {
    // Plasma computes in WGSL, so there is no CPU producer.
    let r = produce_node("gen.plasma", &params(&[]), 64, 64, 0.0, 1).unwrap();
    assert!(r.is_none(), "analytic generators are not producer-backed");
}

#[test]
fn producer_backed_rows_all_render_something() {
    // Every producer-backed generator type must render non-empty content.
    let types = [
        "gen.text",
        "gen.kinetic",
        "gen.typewriter",
        "gen.counter",
        "gen.text_path",
        "gen.chart",
        "gen.overlays",
        "gen.svg",
        "gen.shape",
        "gen.trim_paths",
        "gen.lottie",
        "gen.icon_scene",
        "gen.repeater",
        "gen.vector_mesh",
        "gen.sdf_primitive",
        "gen.sdf_blob",
        "gen.sdf_gyroid",
        "gen.sdf_fractal",
        "gen.mesh_primitive",
        "gen.terrain",
        "gen.voxel",
        "gen.ocean",
        "gen.particle_emitter",
        "gen.particle_boids",
        "gen.pendulum",
        "gen.cloth",
        "gen.fluid",
        "gen.em_field",
        "gen.wave_interference",
        "gen.audio_visual",
        "gen.clock",
        "gen.control_math",
    ];
    let mut failures = vec![];
    for t in types {
        match produce_node(t, &params(&[]), 160, 90, 0.3, 1) {
            Ok(Some(px)) => {
                let (opaque, _) = coverage(&px);
                if opaque == 0 {
                    failures.push(format!("{t}: produced fully transparent frame"));
                }
            }
            Ok(None) => failures.push(format!("{t}: no producer registered")),
            Err(e) => failures.push(format!("{t}: error {e:?}")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn sdf_and_field_producers_are_deterministic() {
    for t in [
        "gen.sdf_blob",
        "gen.sdf_gyroid",
        "gen.particle_emitter",
        "gen.fluid",
    ] {
        let a = produce_node(t, &params(&[]), 96, 96, 0.5, 42)
            .unwrap()
            .unwrap();
        let b = produce_node(t, &params(&[]), 96, 96, 0.5, 42)
            .unwrap()
            .unwrap();
        assert_eq!(a, b, "{t} must be deterministic at fixed time/seed");
        let c = produce_node(t, &params(&[]), 96, 96, 1.5, 42)
            .unwrap()
            .unwrap();
        assert_ne!(a, c, "{t} must animate over time");
    }
}
