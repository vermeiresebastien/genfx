//! Proves the DAG executor renders an explicit graph (node editor) with PARALLEL
//! branches: two solid generators (red + blue) merged by a two-input composite.
//! If the second input were ignored, the result would be pure red.

use flux_app::export::PreviewRenderer;
use flux_domain::{Graph, GraphEdge, GraphNode, ParamValue, Project, RationalTime};
use std::collections::BTreeMap;

fn node(id: &str, type_id: &str, params: BTreeMap<String, ParamValue>) -> GraphNode {
    GraphNode {
        id: id.into(),
        type_id: type_id.into(),
        type_version: 1,
        position: [0.0, 0.0],
        params,
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

fn solid(color: [f64; 4]) -> BTreeMap<String, ParamValue> {
    let mut p = BTreeMap::new();
    p.insert("color".to_string(), ParamValue::Color { value: color });
    p
}

#[test]
fn parallel_branches_merge_through_composite() {
    let mut cp = BTreeMap::new();
    cp.insert("blend".to_string(), ParamValue::Int { value: 1 }); // 1 = add
    cp.insert("opacity".to_string(), ParamValue::Float { value: 1.0 });

    let graph = Graph {
        id: "parallel-test".into(),
        name: "parallel".into(),
        nodes: vec![
            node("red", "gen.solid", solid([1.0, 0.0, 0.0, 1.0])),
            node("blue", "gen.solid", solid([0.0, 0.0, 1.0, 1.0])),
            node("comp", "fx.composite", cp),
            node("out", "flux.output", BTreeMap::new()),
        ],
        edges: vec![
            edge("red", "color", "comp", "a"),
            edge("blue", "color", "comp", "b"),
            edge("comp", "color", "out", "source"),
        ],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    };

    let project = Project::new("parallel", 1);
    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            eprintln!("no GPU; skipping");
            return;
        }
    };
    let px = r
        .render_graph(&graph, &project, RationalTime::new(1, 1).unwrap(), 64, 64)
        .expect("render graph");

    let w = 64usize;
    let i = (32 * w + 32) * 4;
    let (red, _g, blue) = (px[i], px[i + 1], px[i + 2]);
    assert!(red > 60, "red branch missing (r={red})");
    assert!(
        blue > 60,
        "blue branch missing (b={blue}) — composite ignored its second input"
    );
}

#[test]
fn series_chain_renders_non_blank() {
    let graph = Graph {
        id: "series-test".into(),
        name: "series".into(),
        nodes: vec![
            node("gen", "gen.solid", solid([0.9, 0.2, 0.1, 1.0])),
            node("fx", "fx.bloom", BTreeMap::new()),
            node("out", "flux.output", BTreeMap::new()),
        ],
        edges: vec![
            edge("gen", "color", "fx", "source"),
            edge("fx", "color", "out", "source"),
        ],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    };
    let project = Project::new("series", 1);
    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => return,
    };
    let px = r
        .render_graph(&graph, &project, RationalTime::new(1, 1).unwrap(), 64, 64)
        .expect("render");
    let nonwhite = px
        .chunks(4)
        .filter(|c| c[0] != 255 || c[1] != 255 || c[2] != 255)
        .count();
    assert!(
        nonwhite > (64 * 64) / 4,
        "series chain rendered blank/white"
    );
    // Red should dominate (bloom of a red solid stays reddish).
    let i = (32 * 64 + 32) * 4;
    assert!(px[i] > 100, "expected reddish output (r={})", px[i]);
}

#[test]
fn graph_edit_changes_output() {
    // Same graph id, different param → cache key must change and output differ.
    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => return,
    };
    let project = Project::new("edit", 1);
    let make = |c: [f64; 4]| Graph {
        id: "edit-test".into(),
        name: "e".into(),
        nodes: vec![
            node("g", "gen.solid", solid(c)),
            node("o", "flux.output", BTreeMap::new()),
        ],
        edges: vec![edge("g", "color", "o", "source")],
        output_node: "o".into(),
        keyframes: BTreeMap::new(),
    };
    let a = r
        .render_graph(
            &make([1.0, 0.0, 0.0, 1.0]),
            &project,
            RationalTime::new(0, 1).unwrap(),
            32,
            32,
        )
        .unwrap();
    let b = r
        .render_graph(
            &make([0.0, 0.0, 1.0, 1.0]),
            &project,
            RationalTime::new(0, 1).unwrap(),
            32,
            32,
        )
        .unwrap();
    let i = (16 * 32 + 16) * 4;
    assert!(a[i] > 100 && a[i + 2] < 100, "first should be red");
    assert!(
        b[i + 2] > 100 && b[i] < 100,
        "second should be blue (cache must invalidate)"
    );
}

#[test]
fn gui_command_flow_builds_renderable_graph() {
    // Drive the exact CommandHost command flow the node editor uses.
    use flux_app::commands::{CommandHost, GuiCommand, Request};
    let mk = |cmd: GuiCommand| Request {
        protocol_version: 1,
        request_id: "t".into(),
        project_id: None,
        expected_revision: 0,
        command: cmd,
    };
    let mut h = CommandHost::new(Project::new("gui", 1));
    h.handle(mk(GuiCommand::CreateProject {
        name: "G".into(),
        seed: 1,
        width: 320,
        height: 180,
        fps_num: 30,
        fps_den: 1,
    }));
    // Default project already has a plasma -> output graph.
    assert!(
        h.graph().is_some(),
        "createProject must seed a working graph"
    );
    let g0 = h.graph().unwrap().clone();
    assert!(g0.nodes.iter().any(|n| n.type_id == "gen.plasma"));
    assert!(g0.nodes.iter().any(|n| n.type_id == "flux.output"));

    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => return,
    };
    let before = r
        .render_graph(
            &g0,
            &h.dispatcher.project,
            RationalTime::new(1, 1).unwrap(),
            64,
            64,
        )
        .unwrap();

    // Add a solid node and rewire it to the output (what the UI does on click).
    let res = h.handle(mk(GuiCommand::AddGraphNode {
        type_id: "gen.solid".into(),
        x: 10.0,
        y: 10.0,
    }));
    let new_id = res.result.unwrap()["nodeId"].as_str().unwrap().to_string();
    let out = h
        .graph()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.type_id == "flux.output")
        .unwrap()
        .id
        .clone();
    h.handle(mk(GuiCommand::DisconnectPin {
        to_node: out.clone(),
        to_port: "source".into(),
    }));
    h.handle(mk(GuiCommand::ConnectPins {
        from_node: new_id,
        from_port: "color".into(),
        to_node: out,
        to_port: "source".into(),
    }));
    let g1 = h.graph().unwrap().clone();
    let after = r
        .render_graph(
            &g1,
            &h.dispatcher.project,
            RationalTime::new(1, 1).unwrap(),
            64,
            64,
        )
        .unwrap();
    // The two frames must differ (different generator driving the output).
    let same = before.iter().zip(after.iter()).all(|(a, b)| a == b);
    assert!(
        !same,
        "rewiring the output to a new generator must change the preview"
    );
}
