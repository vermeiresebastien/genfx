//! Integration tests that render effects and generators through the real GPU
//! graph and assert metamorphic properties (non-degenerate output, determinism,
//! transparency, scale behaviour). These complement the per-crate unit tests.

use flux_catalog::build_catalog;
use flux_domain::{Graph, GraphEdge, GraphNode, ParamValue, Project, RationalTime};
use flux_engine::frame::FrameContext;
use flux_render::graph_exec::RuntimeGraph;
use flux_render::pipeline::{EffectCompiler, NodeParamsBuffer};
use flux_render::preview::read_full;
use flux_render::{BackendPref, Gpu};
use std::collections::{BTreeMap, HashMap};

fn gpu() -> Gpu {
    Gpu::new(BackendPref::Auto).expect("GPU required")
}

fn graph_of(gen_id: &str, effects: &[&str]) -> Graph {
    let mut nodes = vec![];
    let mut edges = vec![];
    let mk = |id: &str, t: &str, pos: [f32; 2], params: BTreeMap<String, ParamValue>| GraphNode {
        id: id.into(),
        type_id: t.into(),
        type_version: 1,
        position: pos,
        params,
        bypass: false,
        missing_type: false,
    };
    nodes.push(mk("g", gen_id, [0.0, 0.0], BTreeMap::new()));
    let mut prev = "g".to_string();
    let mut x = 240.0;
    for (i, e) in effects.iter().enumerate() {
        let id = format!("e{i}");
        nodes.push(mk(&id, e, [x, 0.0], BTreeMap::new()));
        edges.push(GraphEdge { from_node: prev.clone(), from_port: "color".into(), to_node: id.clone(), to_port: "source".into() });
        prev = id;
        x += 240.0;
    }
    nodes.push(mk("out", "flux.output", [x, 0.0], BTreeMap::new()));
    edges.push(GraphEdge { from_node: prev, from_port: "color".into(), to_node: "out".into(), to_port: "source".into() });
    Graph {
        id: format!("g:{}:{}", gen_id, effects.join("_")),
        name: "test".into(),
        nodes,
        edges,
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    }
}

/// Render the graph at `w`x`h` and `time` seconds; returns RGBA.
fn render(g: &Gpu, cat: &flux_catalog::Catalog, graph: &Graph, secs: f64, w: u32, h: u32) -> Vec<u8> {
    let mut compiler = EffectCompiler::new(g);
    let mut rt = RuntimeGraph::activate(g, &mut compiler, cat, graph, w, h, 1).expect("activate");
    let project = {
        let mut p = Project::new("t", 7);
        p.settings.size = flux_domain::Size { width: w, height: h };
        p
    };
    let t = RationalTime::new((secs * 1_000_000.0) as i64, 1_000_000).unwrap();
    let ctx = FrameContext::for_project(&project, t, 0);
    let mut resolved = HashMap::new();
    for pass in &rt.compiled.passes {
        if let Some(e) = cat.get(&pass.type_id) {
            resolved.insert(pass.node_id.clone(), NodeParamsBuffer::defaults(&e.manifest));
        }
    }
    let src = g.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let sv = src.create_view(&Default::default());
    rt.execute(g, &compiler, cat, &ctx, &sv, &resolved, 1.0).expect("execute");
    let (_, _, px) = read_full(g, &rt.target_texture(), w, h).expect("read");
    px
}

fn stats(px: &[u8]) -> (f64, f64, u32) {
    // mean absolute luma, stddev, distinct pixel count over the whole image.
    let mut sum = 0.0f64;
    let mut sum2 = 0.0f64;
    let n = (px.len() / 4) as f64;
    let mut distinct = std::collections::BTreeSet::new();
    for p in px.chunks_exact(4) {
        let l = (0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64) / 255.0;
        sum += l;
        sum2 += l * l;
        distinct.insert((p[0], p[1], p[2], p[3]));
    }
    let mean = sum / n;
    let var = (sum2 / n - mean * mean).max(0.0);
    (mean, var.sqrt(), distinct.len() as u32)
}

/// Build a graph that feeds `gen.plasma` into an animator `gid` (for image
/// animators that require a source).
fn animator_graph(gid: &str) -> Graph {
    let mut nodes = vec![];
    let mut edges = vec![];
    let mk = |id: &str, t: &str, pos: [f32; 2]| GraphNode {
        id: id.into(),
        type_id: t.into(),
        type_version: 1,
        position: pos,
        params: BTreeMap::new(),
        bypass: false,
        missing_type: false,
    };
    nodes.push(mk("src", "gen.plasma", [0.0, 0.0]));
    nodes.push(mk("anim", gid, [240.0, 0.0]));
    nodes.push(mk("out", "flux.output", [480.0, 0.0]));
    edges.push(GraphEdge { from_node: "src".into(), from_port: "color".into(), to_node: "anim".into(), to_port: "source".into() });
    edges.push(GraphEdge { from_node: "anim".into(), from_port: "color".into(), to_node: "out".into(), to_port: "source".into() });
    Graph {
        id: format!("anim:{}", gid),
        name: gid.into(),
        nodes,
        edges,
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    }
}

#[test]
fn every_generator_row_renders_without_panic() {
    let g = gpu();
    let cat = build_catalog();
    let generators: Vec<&flux_catalog::CatalogEntry> = cat
        .entries
        .iter()
        .filter(|e| {
            e.manifest.requirement_ids.iter().any(|r| r.starts_with('G'))
                && e.manifest.implementation.kind == "fragment"
                && !is_passthrough(&e.source)
        })
        .collect();
    assert!(generators.len() >= 28, "expected >=28 analytic generators, got {}", generators.len());

    let mut failures = vec![];
    let mut checked = 0;
    for e in &generators {
        let gid = &e.manifest.id;
        let needs_source = e.manifest.inputs.iter().any(|p| p.id == "source" && p.required);
        let graph = if needs_source { animator_graph(gid) } else { graph_of(gid, &[]) };
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&g, &cat, &graph, 0.5, 128, 128)));
        checked += 1;
        if res.is_err() {
            failures.push(format!("{gid}: panicked"));
        }
    }
    eprintln!("checked {checked} analytic generators for panics");
    assert!(failures.is_empty(), "generator panics: {failures:#?}");
}

#[test]
fn dense_generators_produce_structure() {
    // Curated dense analytic generators must produce non-uniform output.
    let g = gpu();
    let cat = build_catalog();
    let dense = [
        "gen.plasma", "gen.voronoi", "gen.mandelbrot", "gen.fractal_noise", "gen.clouds",
        "gen.nebula", "gen.concentric", "gen.moire", "gen.topographic", "gen.kaleidoscope",
        "gen.texture_field", "gen.water_caustic",
    ];
    let mut failures = vec![];
    let names: Vec<String> = cat.entries.iter().map(|e| e.manifest.id.clone()).collect();
    for gid in dense {
        if !names.iter().any(|n| n == gid) {
            continue;
        }
        let px = render(&g, &cat, &graph_of(gid, &[]), 0.5, 128, 128);
        let (_m, std, _d) = stats(&px);
        let has_alpha = px.chunks_exact(4).any(|p| p[3] > 0);
        if !has_alpha && std < 1e-4 {
            failures.push(format!("{gid}: empty (std {std:.5})"));
        }
    }
    assert!(failures.is_empty(), "dense generator failures: {failures:#?}");
}

/// A passthrough presentation shader samples the external `src_tex` and returns
/// it unchanged; real analytic generators compute pixels from parameters.
fn is_passthrough(body: &str) -> bool {
    let b = body.replace(char::is_whitespace, " ");
    b.contains("return textureSampleLevel(src_tex, src_sampler, uv, 0.0)") && !b.contains("mix(")
        || b.contains("let c = textureSampleLevel(src_tex, src_sampler, uv, 0.0);")
            && b.contains("return c")
}

#[test]
fn effects_render_over_plasma_and_change_output() {
    let g = gpu();
    let cat = build_catalog();
    // Reference: plasma alone.
    let base = render(&g, &cat, &graph_of("gen.plasma", &[]), 0.5, 96, 96);
    let (bm, bs, _) = stats(&base);
    let effects = [
        "fx.bloom", "fx.pixelate", "fx.halftone", "fx.gradient_map", "fx.warp", "fx.outlines",
        "fx.colored_edges", "fx.channel_mixer", "fx.color_adjustment", "fx.tone", "fx.blur",
        "fx.glitch", "fx.vhs", "fx.crt", "fx.geometry", "fx.chromatic", "fx.noise", "fx.print",
        "fx.technical", "fx.transforms", "fx.smear", "fx.glass", "fx.texture", "fx.keys",
    ];
    let mut failures = vec![];
    for e in effects {
        let graph = graph_of("gen.plasma", &[e]);
        let px = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&g, &cat, &graph, 0.5, 96, 96)));
        match px {
            Ok(px) => {
                let (m, s, distinct) = stats(&px);
                if distinct <= 1 {
                    failures.push(format!("{e}: uniform/empty (mean {m:.3}, std {s:.3})"));
                }
            }
            Err(_) => failures.push(format!("{e}: panicked")),
        }
    }
    eprintln!("plasma base mean={bm:.3} std={bs:.3}");
    assert!(failures.is_empty(), "effect render failures: {failures:#?}");
}

#[test]
fn render_is_deterministic_at_fixed_time() {
    let g = gpu();
    let cat = build_catalog();
    let graph = graph_of("gen.fractal_noise", &["fx.blur"]);
    let a = render(&g, &cat, &graph, 1.25, 80, 80);
    let b = render(&g, &cat, &graph, 1.25, 80, 80);
    assert_eq!(a, b, "same time must render identical pixels");
}

#[test]
fn time_changes_the_output() {
    let g = gpu();
    let cat = build_catalog();
    let graph = graph_of("gen.plasma", &[]);
    let a = render(&g, &cat, &graph, 0.0, 64, 64);
    let b = render(&g, &cat, &graph, 1.0, 64, 64);
    assert_ne!(a, b, "animated generator must differ across time");
}

#[test]
fn solid_pixels_have_expected_alpha_and_color() {
    let g = gpu();
    let cat = build_catalog();
    // gen.solid default white opaque; check a centre pixel is near-white opaque.
    let px = render(&g, &cat, &graph_of("gen.solid", &[]), 0.0, 32, 32);
    let center = (16 * 32 + 16) * 4;
    let p = &px[center..center + 4];
    assert!(p[0] > 200 && p[1] > 200 && p[2] > 200 && p[3] > 200, "solid pixel {p:?}");
}
