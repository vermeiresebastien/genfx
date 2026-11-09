//! Node-editor regressions: catalog port export, undoable node moves, keyframe
//! resolution through the router, master-graph precedence, and real bypass.

use flux_app::commands::{CommandHost, GuiCommand, Request};
use flux_app::export::PreviewRenderer;
use flux_app::samples;
use flux_domain::commands::NodePosition;
use flux_domain::params::FeatureSnapshot;
use flux_domain::{
    AudioControls, BlendMode, Clip, ClipSource, Command, Dispatcher, Graph, GraphEdge, GraphNode,
    Interpolation, Keyframe, LoopPolicy, ParamValue, Project, RationalTime, TimeRange, Track,
    TrackKind, Transform2D,
};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn rt(secs: i64) -> RationalTime {
    RationalTime::new(secs, 1).unwrap()
}

fn req(command: GuiCommand) -> Request {
    Request {
        protocol_version: 1,
        request_id: "t".into(),
        project_id: None,
        expected_revision: 0,
        command,
    }
}

fn solid_graph(id: &str, color: [f64; 4]) -> Graph {
    let mut params = BTreeMap::new();
    params.insert("color".to_string(), ParamValue::Color { value: color });
    let mut out_params = BTreeMap::new();
    out_params.insert("wet".to_string(), ParamValue::Float { value: 1.0 });
    Graph {
        id: id.into(),
        name: id.into(),
        nodes: vec![
            GraphNode {
                id: "src".into(),
                type_id: "gen.solid".into(),
                type_version: 1,
                position: [0.0, 0.0],
                params,
                bypass: false,
                missing_type: false,
            },
            GraphNode {
                id: "out".into(),
                type_id: "flux.output".into(),
                type_version: 1,
                position: [240.0, 0.0],
                params: out_params,
                bypass: false,
                missing_type: false,
            },
        ],
        edges: vec![GraphEdge {
            from_node: "src".into(),
            from_port: "color".into(),
            to_node: "out".into(),
            to_port: "source".into(),
        }],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    }
}

fn add_clip_using(p: &mut Project, graph_id: &str) {
    let track_id = "track-1".to_string();
    let clip_id = "clip-1".to_string();
    let clip = Clip {
        id: clip_id.clone(),
        name: "clip".into(),
        source: ClipSource::GeneratorScene {
            generator_id: "gen.plasma".into(),
        },
        range: TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(8)),
        source_range: TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(8)),
        rate: RationalTime::new(1, 1).unwrap(),
        loop_policy: LoopPolicy::Loop,
        graph_id: Some(graph_id.into()),
        transform: Transform2D::default(),
        opacity: 1.0,
        audio: AudioControls::default(),
        linked_group: None,
    };
    let track = Track {
        id: track_id.clone(),
        name: "V1".into(),
        kind: TrackKind::Video,
        clip_order: vec![clip_id.clone()],
        locked: false,
        muted: false,
        solo: false,
        visible: true,
        gain_db: 0.0,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        graph_id: None,
    };
    p.timeline.clips.insert(clip_id, clip);
    p.timeline.tracks.insert(track_id.clone(), track);
    p.timeline.track_order = vec![track_id];
}

fn center(px: &[u8], w: u32, h: u32) -> (u8, u8, u8, u8) {
    let i = (((h / 2) * w + w / 2) * 4) as usize;
    (px[i], px[i + 1], px[i + 2], px[i + 3])
}

#[test]
fn every_preset_validates() {
    let catalog = flux_catalog::build_catalog();
    let reg = flux_engine::graph::Registry::new(&catalog);
    let mut failures: Vec<String> = Vec::new();

    for preset in flux_catalog::presets::all_presets() {
        let mut host = CommandHost::new(Project::new("t", 1));
        let res = host.handle(req(GuiCommand::InsertPreset {
            preset_id: preset.id.clone(),
            position: [0.0, 0.0],
            merge_into: None,
        }));
        if let Some(e) = &res.error {
            failures.push(format!("{}: insert failed {e:?}", preset.id));
            continue;
        }

        let snap = host.handle(req(GuiCommand::Snapshot)).result.expect("snapshot");
        let g: Graph = serde_json::from_value(snap["activeGraph"].clone()).expect("graph");
        for i in flux_engine::graph::validate(&g, &reg) {
            if format!("{:?}", i.code).contains("SdkMissing") {
                continue;
            }
            failures.push(format!("{}: {:?}: {}", preset.id, i.code, i.message));
        }
    }

    assert!(failures.is_empty(), "invalid presets:\n{}", failures.join("\n"));
}

#[test]
fn insert_preset_creates_wired_graph_with_params() {
    let mut host = CommandHost::new(Project::new("t", 1));
    host.handle(req(GuiCommand::CreateProject {
        name: "T".into(),
        seed: 1,
        width: 640,
        height: 360,
        fps_num: 60,
        fps_den: 1,
    }));

    let res = host.handle(req(GuiCommand::InsertPreset {
        preset_id: "preset.bass_impact".into(),
        position: [40.0, 60.0],
        merge_into: None,
    }));
    assert!(res.error.is_none(), "insert preset failed: {:?}", res.error);

    let snap = host.handle(req(GuiCommand::Snapshot)).result.expect("snapshot");
    let g = &snap["activeGraph"];
    let nodes = g["nodes"].as_array().expect("nodes");
    assert!(nodes.len() >= 4, "preset graph should have all nodes");
    assert!(g["edges"].as_array().unwrap().len() >= 3, "preset edges");

    let out = g["outputNode"].as_str().expect("output node id");
    assert!(
        nodes.iter().any(|n| n["id"] == out),
        "output node must exist in the graph"
    );
    let out_ty = nodes
        .iter()
        .find(|n| n["id"] == out)
        .and_then(|n| n["typeId"].as_str())
        .unwrap();
    assert_eq!(out_ty, "flux.output", "preset output should be the sink");

    // The preset authors `bassAmount` as a plain JSON number; it must survive as
    // a tagged ParamValue or the preset silently loses its look.
    let blob = nodes
        .iter()
        .find(|n| n["typeId"] == "gen.sdf_blob")
        .expect("sdf blob node");
    let p = &blob["params"]["bassAmount"];
    assert_eq!(p["kind"], "float", "param converted to ParamValue");
    assert_eq!(p["value"], 1.2);
}

#[test]
fn master_graph_export_writes_video() {
    let _g = gpu_lock();
    let mut p = Project::new("export", 1);
    p.settings.size = flux_domain::Size { width: 160, height: 90 };
    let g = samples::chain_graph("gen.plasma", &["fx.bloom"]);
    p.timeline.master_graph_id = Some(g.id.clone());
    p.graphs.insert(g.id.clone(), g);
    p.timeline.range = TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(1));

    let out = std::env::temp_dir().join(format!(
        "flux-export-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let res = flux_app::export::export_project(
        &p,
        &flux_app::export::ExportOptions {
            out_dir: out.clone(),
            project_name: "export".into(),
            codec: flux_media::EncodeCodec::H264,
            width: 160,
            height: 90,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .expect("export");

    let video = res.video.expect("a video file must be produced (not only PNGs)");
    assert!(video.exists(), "video missing: {}", video.display());
    let meta = std::fs::metadata(&video).unwrap();
    assert!(meta.len() > 0, "video is empty");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn create_project_snapshot_has_starter_graph() {
    let mut host = CommandHost::new(Project::new("t", 1));
    let res = host.handle(req(GuiCommand::CreateProject {
        name: "T".into(),
        seed: 1,
        width: 1280,
        height: 720,
        fps_num: 60,
        fps_den: 1,
    }));
    let snap = res.result.expect("snapshot");
    let g = &snap["activeGraph"];
    assert!(g.is_object(), "activeGraph missing: {snap}");
    assert_eq!(g["nodes"].as_array().unwrap().len(), 2, "starter graph nodes");
    assert_eq!(g["edges"].as_array().unwrap().len(), 1, "starter graph edges");
    assert_eq!(
        snap["activeGraphId"].as_str().unwrap(),
        g["id"].as_str().unwrap()
    );
    assert!(snap["project"]["timeline"]["masterGraphId"].is_string());
}

#[test]
fn catalog_export_includes_ports() {
    let mut host = CommandHost::new(Project::new("t", 1));
    let res = host.handle(req(GuiCommand::Catalog));
    assert!(res.error.is_none(), "catalog failed: {:?}", res.error);
    let entries = res.result.unwrap()["entries"].as_array().unwrap().clone();
    assert!(!entries.is_empty());

    for e in &entries {
        assert!(e["inputs"].is_array(), "entry {} missing inputs", e["id"]);
        assert!(e["outputs"].is_array(), "entry {} missing outputs", e["id"]);
    }

    let comp = entries
        .iter()
        .find(|e| e["id"] == "fx.composite")
        .expect("fx.composite present");
    let inputs = comp["inputs"].as_array().unwrap();
    let a = inputs.iter().find(|p| p["id"] == "a").unwrap();
    let b = inputs.iter().find(|p| p["id"] == "b").unwrap();
    assert_eq!(a["type"], "texture2d");
    assert_eq!(a["required"], true);
    assert_eq!(b["required"], false);
}

#[test]
fn set_node_positions_roundtrips() {
    let g = samples::chain_graph("gen.plasma", &[]);
    let graph_id = g.id.clone();
    let node_id = g.nodes[0].id.clone();
    let old = g.nodes[0].position;

    let mut p = Project::new("t", 1);
    p.graphs.insert(graph_id.clone(), g);
    let mut d = Dispatcher::new(p);

    d.apply(
        Command::SetNodePositions {
            graph_id: graph_id.clone(),
            positions: vec![NodePosition {
                node_id: node_id.clone(),
                position: [111.0, 222.0],
            }],
        },
        None,
    )
    .unwrap();
    assert_eq!(
        d.project.graphs[&graph_id].node(&node_id).unwrap().position,
        [111.0, 222.0]
    );

    d.undo().unwrap();
    assert_eq!(
        d.project.graphs[&graph_id].node(&node_id).unwrap().position,
        old
    );
}

#[test]
fn keyframes_resolve_through_router() {
    let mut g = samples::chain_graph("gen.plasma", &["fx.blur"]);
    let node = g
        .nodes
        .iter()
        .find(|n| n.type_id == "fx.blur")
        .expect("blur node")
        .clone();
    let cat = flux_catalog::build_catalog();
    let decl = cat
        .get("fx.blur")
        .unwrap()
        .manifest
        .parameters
        .iter()
        .find(|p| p.id != "wet" && p.min.is_some() && p.max.is_some())
        .expect("a ranged parameter")
        .clone();

    let lane = format!("{}:{}", node.id, decl.id);
    let lo = decl.min.unwrap();
    let hi = decl.max.unwrap();
    g.keyframes.insert(
        lane,
        vec![
            Keyframe {
                time: rt(0),
                value: ParamValue::Float { value: lo },
                interpolation: Interpolation::Linear,
            },
            Keyframe {
                time: rt(10),
                value: ParamValue::Float { value: hi },
                interpolation: Interpolation::Linear,
            },
        ],
    );

    let mut p = Project::new("t", 1);
    p.graphs.insert(g.id.clone(), g.clone());
    let mut router = flux_engine::router::Router::default();
    let vals = router
        .resolve_node(
            &g,
            &node.id,
            rt(5),
            &FeatureSnapshot::default(),
            0.016,
            &p,
        )
        .expect("resolve");
    let v = *vals.get(&decl.id).expect("param resolved");
    let mid = (lo + hi) / 2.0;
    assert!(
        (v - mid).abs() <= (hi - lo) * 0.15 + 0.01,
        "expected midpoint ~{mid}, got {v}"
    );
}

#[test]
fn master_graph_precedes_topmost_clip() {
    let _g = gpu_lock();
    let Ok(mut renderer) = PreviewRenderer::new() else {
        eprintln!("no GPU; skipping");
        return;
    };

    let red = solid_graph("master", [1.0, 0.0, 0.0, 1.0]);
    let blue = solid_graph("clip-graph", [0.0, 0.0, 1.0, 1.0]);

    let mut p = Project::new("t", 1);
    p.graphs.insert(red.id.clone(), red.clone());
    p.graphs.insert(blue.id.clone(), blue.clone());
    p.timeline.master_graph_id = Some(red.id.clone());
    add_clip_using(&mut p, &blue.id);

    let t = rt(1);
    let px = renderer.render(&p, t, 64, 64).expect("render");
    let (r, g, b, _) = center(&px, 64, 64);
    assert!(r > 60 && b < 60, "master graph must win (got r={r} g={g} b={b})");

    // Without a master graph the timeline's top-most clip drives the frame.
    p.timeline.master_graph_id = None;
    let px = renderer.render(&p, t, 64, 64).expect("render");
    let (r, g, b, _) = center(&px, 64, 64);
    assert!(b > 60 && r < 60, "clip graph must win (got r={r} g={g} b={b})");
}

#[test]
fn bypass_copies_input() {
    let _g = gpu_lock();
    let Ok(mut renderer) = PreviewRenderer::new() else {
        eprintln!("no GPU; skipping");
        return;
    };

    let mut src_params = BTreeMap::new();
    src_params.insert(
        "color".to_string(),
        ParamValue::Color { value: [1.0, 0.0, 0.0, 1.0] },
    );
    let mut tone_params = BTreeMap::new();
    tone_params.insert("mode".to_string(), ParamValue::Int { value: 3 }); // invert
    let mut out_params = BTreeMap::new();
    out_params.insert("wet".to_string(), ParamValue::Float { value: 1.0 });

    let graph = Graph {
        id: "bypass-test".into(),
        name: "bypass".into(),
        nodes: vec![
            GraphNode {
                id: "src".into(),
                type_id: "gen.solid".into(),
                type_version: 1,
                position: [0.0, 0.0],
                params: src_params,
                bypass: false,
                missing_type: false,
            },
            GraphNode {
                id: "tone".into(),
                type_id: "fx.tone".into(),
                type_version: 1,
                position: [220.0, 0.0],
                params: tone_params,
                bypass: true,
                missing_type: false,
            },
            GraphNode {
                id: "out".into(),
                type_id: "flux.output".into(),
                type_version: 1,
                position: [440.0, 0.0],
                params: out_params,
                bypass: false,
                missing_type: false,
            },
        ],
        edges: vec![
            GraphEdge {
                from_node: "src".into(),
                from_port: "color".into(),
                to_node: "tone".into(),
                to_port: "source".into(),
            },
            GraphEdge {
                from_node: "tone".into(),
                from_port: "color".into(),
                to_node: "out".into(),
                to_port: "source".into(),
            },
        ],
        output_node: "out".into(),
        keyframes: BTreeMap::new(),
    };

    let mut p = Project::new("t", 1);
    p.graphs.insert(graph.id.clone(), graph.clone());
    p.timeline.master_graph_id = Some(graph.id.clone());

    let px = renderer.render(&p, rt(1), 64, 64).expect("render");
    let (r, g, b, _) = center(&px, 64, 64);
    assert!(
        r > 80 && g < 80 && b < 80,
        "bypassed node must pass the red input through (got r={r} g={g} b={b})"
    );
}
