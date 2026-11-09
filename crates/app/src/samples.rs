//! Sample project generator: builds editable, deterministic demo projects that
//! exercise the generator, effect, and animation grids. Used by the CLI, the
//! editor's "New demo" action, and integration tests.

use flux_catalog::build_catalog;
use flux_catalog::presets::Preset;
use flux_domain::*;
use std::collections::BTreeMap;

/// A single generator clip on one track, with an effect chain applied through a
/// clip graph. Returns the project plus the graph used.
pub fn demo_generator_project(generator_id: &str, effects: &[&str], seconds: i64) -> Project {
    let mut p = Project::new(format!("Demo {}", generator_id), 12345);
    p.timeline.range = TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(seconds.max(1)));
    let track = Track {
        id: uuid::Uuid::new_v4().to_string(),
        name: "V1".into(),
        kind: TrackKind::Video,
        clip_order: vec![],
        locked: false,
        muted: false,
        solo: false,
        visible: true,
        gain_db: 0.0,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        graph_id: None,
    };
    let graph = chain_graph(generator_id, effects);
    let graph_id = graph.id.clone();
    p.graphs.insert(graph_id.clone(), graph);
    let clip = Clip {
        id: uuid::Uuid::new_v4().to_string(),
        name: generator_id.into(),
        source: ClipSource::GeneratorScene {
            generator_id: generator_id.into(),
        },
        range: TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(seconds.max(1))),
        source_range: TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(seconds.max(1))),
        rate: RationalTime::new(1, 1).unwrap(),
        loop_policy: LoopPolicy::Loop,
        graph_id: Some(graph_id),
        transform: Transform2D::default(),
        opacity: 1.0,
        audio: AudioControls::default(),
        linked_group: None,
    };
    let mut track = track;
    track.clip_order.push(clip.id.clone());
    p.timeline.tracks.insert(track.id.clone(), track);
    p.timeline.track_order.push("v1".into());
    // ensure track_order references the actual id
    p.timeline.track_order = vec![p.timeline.tracks.keys().next().unwrap().clone()];
    p.timeline.clips.insert(clip.id.clone(), clip);
    p
}

/// Build a generator → effects... → output graph with screen positions.
pub fn chain_graph(generator_id: &str, effects: &[&str]) -> Graph {
    let mut nodes = vec![];
    let mut edges = vec![];
    let mut prev = String::new();
    let mut index = 0;
    let mut add = |type_id: &str, pos: [f32; 2]| -> String {
        let id = uuid::Uuid::new_v4().to_string();
        nodes.push(GraphNode {
            id: id.clone(),
            type_id: type_id.into(),
            type_version: 1,
            position: pos,
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        });
        id
    };
    let g = add(generator_id, [0.0, 0.0]);
    prev = g;
    let mut x = 240.0;
    for e in effects {
        let n = add(e, [x, 0.0]);
        edges.push(GraphEdge {
            from_node: prev.clone(),
            from_port: "color".into(),
            to_node: n.clone(),
            to_port: "source".into(),
        });
        prev = n;
        x += 240.0;
    }
    let out = add("flux.output", [x, 0.0]);
    edges.push(GraphEdge {
        from_node: prev,
        from_port: "color".into(),
        to_node: out.clone(),
        to_port: "source".into(),
    });
    let _ = index;
    index += 1;
    Graph {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("{} chain", generator_id),
        nodes,
        edges,
        output_node: out,
        keyframes: BTreeMap::new(),
    }
}

/// Insert a preset into a project's graph map, materializing fresh node IDs.
pub fn insert_preset(preset: &Preset) -> Graph {
    let mut nodes = vec![];
    let mut id_map: BTreeMap<usize, String> = BTreeMap::new();
    for pn in &preset.nodes {
        let id = uuid::Uuid::new_v4().to_string();
        id_map.insert(pn.index, id.clone());
        let mut params = BTreeMap::new();
        for (k, v) in &pn.params {
            if let Ok(val) = serde_json::from_value::<ParamValue>(v.clone()) {
                params.insert(k.clone(), val);
            }
        }
        nodes.push(GraphNode {
            id,
            type_id: pn.type_id.clone(),
            type_version: 1,
            position: pn.position,
            params,
            bypass: false,
            missing_type: false,
        });
    }
    let edges = preset
        .edges
        .iter()
        .filter_map(|e| {
            Some(GraphEdge {
                from_node: id_map.get(&e.from)?.clone(),
                from_port: e.from_port.clone(),
                to_node: id_map.get(&e.to)?.clone(),
                to_port: e.to_port.clone(),
            })
        })
        .collect();
    let output_node = id_map.get(&preset.output).cloned().unwrap_or_default();
    Graph {
        id: preset.id.clone(),
        name: preset.name.clone(),
        nodes,
        edges,
        output_node,
        keyframes: BTreeMap::new(),
    }
}

/// Build a project with a single imported video asset on a video track and a
/// clip graph that presents the decoded frames through `fx.color_adjustment`.
pub fn demo_media_project(video_path: &std::path::Path, seconds: i64) -> Project {
    let worker = flux_media::MediaWorker::new().ok();
    let mut p = Project::new(
        format!(
            "Media {}",
            video_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("clip")
        ),
        777,
    );
    let dur = RationalTime::from_secs(seconds.max(1));
    p.timeline.range = TimeRange::new(RationalTime::ZERO, dur);

    let asset_id = if let Some(worker) = &worker {
        if let Ok(probe) = worker.probe(video_path) {
            let asset = worker.asset_from_probe(video_path, &probe).unwrap();
            let id = asset.id.clone();
            p.assets.insert(id.clone(), asset);
            id
        } else {
            "missing".into()
        }
    } else {
        "missing".into()
    };

    // Clip graph: source -> output (pure presentation of decoded frames).
    let graph = chain_graph("flux.source", &[]);
    // chain_graph starts with the given "generator" id as the first node; for
    // media that first node is flux.source which presents the decoded frame.
    let graph_id = graph.id.clone();
    p.graphs.insert(graph_id.clone(), graph);

    let clip = Clip {
        id: uuid::Uuid::new_v4().to_string(),
        name: "clip".into(),
        source: ClipSource::Asset { asset_id },
        range: TimeRange::new(RationalTime::ZERO, dur),
        source_range: TimeRange::new(RationalTime::ZERO, dur),
        rate: RationalTime::new(1, 1).unwrap(),
        loop_policy: LoopPolicy::Loop,
        graph_id: Some(graph_id),
        transform: Transform2D::default(),
        opacity: 1.0,
        audio: AudioControls::default(),
        linked_group: None,
    };
    let track_id = uuid::Uuid::new_v4().to_string();
    let track = Track {
        id: track_id.clone(),
        name: "V1".into(),
        kind: TrackKind::Video,
        clip_order: vec![clip.id.clone()],
        locked: false,
        muted: false,
        solo: false,
        visible: true,
        gain_db: 0.0,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        graph_id: None,
    };
    p.timeline.tracks.insert(track_id.clone(), track);
    p.timeline.track_order = vec![track_id];
    p.timeline.clips.insert(clip.id.clone(), clip);
    p
}

/// The list of demo compositions shipped with the app and used by tests.
pub fn demo_catalog() -> Vec<(String, String, Vec<String>)> {
    let c = build_catalog();
    let mut out = vec![];
    // One demo per generator row and per effect group.
    for e in &c.entries {
        if e.manifest.id.starts_with("gen.") && !e.manifest.id.contains("conditional") {
            out.push((e.manifest.id.clone(), e.manifest.name.clone(), vec![]));
        }
    }
    out
}
