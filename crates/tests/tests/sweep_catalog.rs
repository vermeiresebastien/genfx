//! Sweep every catalog generator and effect: render it in isolation and classify
//! the result. This is the ground truth for "which nodes render nothing".
//!
//! Run with output:
//!   cargo test -p flux-tests --test sweep_catalog -- --nocapture --test-threads=1

use flux_app::export::PreviewRenderer;
use flux_domain::{Graph, GraphEdge, GraphNode, Project, RationalTime, TimeRange};
use std::collections::BTreeMap;

fn node(id: &str, type_id: &str) -> GraphNode {
    GraphNode {
        id: id.into(),
        type_id: type_id.into(),
        type_version: 1,
        position: [0.0, 0.0],
        params: BTreeMap::new(),
        bypass: false,
        missing_type: false,
    }
}

fn edge(from: &str, fp: &str, to: &str, tp: &str) -> GraphEdge {
    GraphEdge {
        from_node: from.into(),
        from_port: fp.into(),
        to_node: to.into(),
        to_port: tp.into(),
    }
}

/// Build the smallest legal graph: [Colorful source] -> node -> Output.
///
/// Effects are tested against a *colourful* generator, not `flux.source`: an
/// effect on a black input correctly renders black, so using the passthrough
/// would report every working effect as "blank".
pub fn build_probe_graph(type_id: &str) -> Option<Graph> {
    let catalog = flux_catalog::build_catalog();
    let entry = catalog.get(type_id)?;
    let m = &entry.manifest;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Any image-ish input (required or not) means the node needs a picture.
    let image_input = m
        .inputs
        .iter()
        .find(|p| {
            matches!(
                p.port_type,
                flux_catalog::manifest::PortType::Texture2D
                    | flux_catalog::manifest::PortType::Mask
            )
        })
        .map(|p| p.id.clone());

    let needs_source = image_input.is_some() && type_id != "flux.source";
    if needs_source {
        // A colourful generator so effects have real pixels to operate on.
        nodes.push(node("src", "gen.gradient"));
    }
    nodes.push(node("n", type_id));

    if needs_source {
        // Feed EVERY image input. Two-input blend nodes (fx.composite,
        // flux.blend) otherwise render the empty second input over the first.
        let mut i = 0;
        for p in &m.inputs {
            if matches!(
                p.port_type,
                flux_catalog::manifest::PortType::Texture2D
                    | flux_catalog::manifest::PortType::Mask
            ) {
                let src = if i == 0 { "src" } else { "src2" };
                if i == 1 {
                    // second, differently-coloured source
                    nodes.insert(1, node("src2", "gen.noise"));
                }
                edges.push(edge(src, "color", "n", &p.id));
                i += 1;
            }
        }
    }

    let out_port = m
        .outputs
        .iter()
        .find(|p| p.id == "color")
        .or_else(|| m.outputs.first())
        .map(|p| p.id.clone());

    nodes.push(node("out", "flux.output"));
    if let Some(op) = out_port {
        edges.push(edge("n", &op, "out", "source"));
    }

    Some(Graph {
        id: format!("probe-{}", type_id),
        name: type_id.into(),
        nodes,
        edges,
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    })
}

pub fn probe_project() -> Project {
    let mut p = Project::new("probe", 1);
    p.timeline.range = TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(4));
    p
}

/// Fraction of pixels with any visible colour. Alpha-only coverage counts too.
pub fn visible_fraction(px: &[u8]) -> f32 {
    if px.is_empty() {
        return 0.0;
    }
    let n = px.len() / 4;
    let mut vis = 0usize;
    for c in px.chunks(4) {
        if c[3] > 8 && (c[0] > 8 || c[1] > 8 || c[2] > 8) {
            vis += 1;
        }
    }
    vis as f32 / n as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Blank,
    Error,
}

pub fn classify(r: &mut PreviewRenderer, type_id: &str) -> (Verdict, String) {
    let Some(graph) = build_probe_graph(type_id) else {
        return (Verdict::Error, "not in catalog".into());
    };
    let p = probe_project();
    let mut best = 0.0f32;
    let mut last_err = String::new();

    // Two time points: some rows are legitimately empty at t=0.
    for t in [
        RationalTime::new(1, 2).unwrap(),
        RationalTime::new(2, 1).unwrap(),
    ] {
        match r.render_graph(&graph, &p, t, 64, 64) {
            Ok(px) => best = best.max(visible_fraction(&px)),
            Err(e) => last_err = format!("{:?}: {}", e.code, e.message),
        }
    }

    if !last_err.is_empty() && best == 0.0 {
        return (Verdict::Error, last_err);
    }
    if best < 0.01 {
        return (Verdict::Blank, format!("visible={:.4}", best));
    }
    (Verdict::Ok, format!("visible={:.3}", best))
}

#[test]
fn sweep_all_generators_and_effects() {
    let Ok(mut r) = PreviewRenderer::new() else {
        eprintln!("no GPU adapter; sweep skipped");
        return;
    };
    let catalog = flux_catalog::build_catalog();

    let mut by_cat: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut blanks: Vec<(String, String, String)> = Vec::new();
    let mut errors: Vec<(String, String, String)> = Vec::new();

    for entry in &catalog.entries {
        let id = &entry.manifest.id;
        let cat = entry.manifest.category.clone();
        // Sinks/sources have no image of their own.
        if matches!(
            id.as_str(),
            "flux.output" | "flux.source" | "flux.mask_input" | "flux.reroute"
        ) {
            continue;
        }
        let (v, detail) = classify(&mut r, id);
        let slot = by_cat.entry(cat.clone()).or_insert((0, 0, 0));
        match v {
            Verdict::Ok => slot.0 += 1,
            Verdict::Blank => {
                // `flux.feedback_read` has no input by design: it reads history
                // written by a paired `flux.feedback_write`. Alone it is empty.
                if id == "flux.feedback_read" {
                    slot.0 += 1;
                } else {
                    slot.1 += 1;
                    blanks.push((id.clone(), cat.clone(), detail));
                }
            }
            Verdict::Error => {
                slot.2 += 1;
                errors.push((id.clone(), cat.clone(), detail));
            }
        }
    }

    let total_ok: usize = by_cat.values().map(|v| v.0).sum();
    let total_blank: usize = by_cat.values().map(|v| v.1).sum();
    let total_err: usize = by_cat.values().map(|v| v.2).sum();

    eprintln!("\n================ CATALOG SWEEP ================");
    eprintln!(
        "{:<22} {:>4} {:>7} {:>6}",
        "CATEGORY", "OK", "BLANK", "ERROR"
    );
    for (cat, (o, b, e)) in &by_cat {
        eprintln!("{:<22} {:>4} {:>7} {:>6}", cat, o, b, e);
    }
    eprintln!("{:-<42}", "");
    eprintln!(
        "{:<22} {:>4} {:>7} {:>6}",
        "TOTAL", total_ok, total_blank, total_err
    );
    if !blanks.is_empty() {
        eprintln!("\n--- BLANK (renders nothing) ---");
        for (id, cat, d) in &blanks {
            eprintln!("  {:<28} [{:<16}] {}", id, cat, d);
        }
    }
    if !errors.is_empty() {
        eprintln!("\n--- ERROR ---");
        for (id, cat, d) in &errors {
            eprintln!("  {:<28} [{:<16}] {}", id, cat, d);
        }
    }
    eprintln!("===============================================\n");

    assert!(
        total_err == 0,
        "{} catalog node(s) failed to render; see the ERROR list above",
        total_err
    );
}
