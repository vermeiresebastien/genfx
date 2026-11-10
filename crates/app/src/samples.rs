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
    let catalog = build_catalog();
    let mut nodes = vec![];
    let mut id_map: BTreeMap<usize, String> = BTreeMap::new();
    for pn in &preset.nodes {
        let id = uuid::Uuid::new_v4().to_string();
        id_map.insert(pn.index, id.clone());
        let kind_of = |param: &str| -> Option<flux_catalog::ParamKind> {
            catalog
                .get(&pn.type_id)
                .and_then(|e| e.manifest.param(param).map(|p| p.kind))
        };
        let mut params = BTreeMap::new();
        for (k, v) in &pn.params {
            // Presets store authored values as plain JSON; the document uses the
            // tagged `ParamValue` form. Convert using the manifest's param kind.
            let val = match serde_json::from_value::<ParamValue>(v.clone()) {
                Ok(already) => Some(already),
                Err(_) => json_param_value(kind_of(k), v),
            };
            if let Some(val) = val {
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
    let mut edges: Vec<GraphEdge> = preset
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

    // Presets are user-editable data. Keep the graph renderable even when a
    // recipe omitted its final sink edge: the last non-output image node is the
    // only sensible provider for the required Output.source input.
    let output_is_wired = edges
        .iter()
        .any(|e| e.to_node == output_node && e.to_port == "source");
    if !output_is_wired {
        if let Some(last) = preset
            .nodes
            .iter()
            .filter(|pn| pn.index != preset.output && pn.type_id != "flux.output")
            .max_by_key(|pn| pn.index)
            .and_then(|pn| id_map.get(&pn.index))
        {
            edges.push(GraphEdge {
                from_node: last.clone(),
                from_port: "color".into(),
                to_node: output_node.clone(),
                to_port: "source".into(),
            });
        }
    }

    Graph {
        id: preset.id.clone(),
        name: preset.name.clone(),
        nodes,
        edges,
        output_node,
        keyframes: BTreeMap::new(),
    }
}

/// Convert a preset's plain JSON value into the document's `ParamValue`,
/// using the manifest kind when known and inferring from the JSON shape otherwise.
fn json_param_value(
    kind: Option<flux_catalog::ParamKind>,
    v: &serde_json::Value,
) -> Option<ParamValue> {
    use flux_catalog::ParamKind as K;
    match kind {
        Some(K::Int) => v.as_i64().map(|x| ParamValue::Int { value: x as i32 }),
        Some(K::Enum) => v
            .as_i64()
            .map(|x| ParamValue::Int { value: x as i32 })
            .or_else(|| v.as_str().map(|_| ParamValue::Int { value: 0 })),
        Some(K::Text) => v.as_str().map(|x| ParamValue::Text { value: x.into() }),
        Some(K::Bool) => v.as_bool().map(|x| ParamValue::Bool { value: x }),
        Some(K::Color) => json_array(v).and_then(|a| {
            (a.len() == 4).then(|| ParamValue::Color {
                value: [a[0], a[1], a[2], a[3]],
            })
        }),
        Some(K::Vec2) => json_array(v).and_then(|a| {
            (a.len() == 2).then(|| ParamValue::Vec2 {
                value: [a[0], a[1]],
            })
        }),
        Some(K::Vec3) => json_array(v).and_then(|a| {
            (a.len() == 3).then(|| ParamValue::Vec3 {
                value: [a[0], a[1], a[2]],
            })
        }),
        Some(K::Float) => v.as_f64().map(|x| ParamValue::Float { value: x }),
        None => {
            // Infer from the JSON shape when the type/param is unknown.
            if let Some(x) = v.as_f64() {
                Some(ParamValue::Float { value: x })
            } else if let Some(b) = v.as_bool() {
                Some(ParamValue::Bool { value: b })
            } else if let Some(a) = json_array(v) {
                match a.len() {
                    2 => Some(ParamValue::Vec2 { value: [a[0], a[1]] }),
                    3 => Some(ParamValue::Vec3 {
                        value: [a[0], a[1], a[2]],
                    }),
                    4 => Some(ParamValue::Color {
                        value: [a[0], a[1], a[2], a[3]],
                    }),
                    _ => None,
                }
            } else {
                None
            }
        }
    }
}

fn json_array(v: &serde_json::Value) -> Option<Vec<f64>> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
}

/// The starting master composition graph for a fresh project: one generator
/// wired straight into Output, so the canvas opens with something live.
pub fn master_graph_start() -> Graph {
    chain_graph("gen.plasma", &[])
}

/// An empty composition graph: a single Output sink, ready to be wired up.
pub fn empty_graph(name: impl Into<String>) -> Graph {
    let out = uuid::Uuid::new_v4().to_string();
    Graph {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        nodes: vec![GraphNode {
            id: out.clone(),
            type_id: "flux.output".into(),
            type_version: 1,
            position: [320.0, 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        }],
        edges: vec![],
        output_node: out,
        keyframes: BTreeMap::new(),
    }
}

/// Merge `src`'s nodes and edges into `dst`, offsetting positions and rewiring
/// `src`'s output into `dst`'s output node. Used when a preset is dropped onto an
/// existing graph.
pub fn merge_graph(dst: &mut Graph, src: &Graph, offset: [f32; 2]) {
    let existing_output = dst
        .edges
        .iter()
        .find(|e| e.to_node == dst.output_node && e.to_port == "source")
        .cloned();
    let mut id_map: BTreeMap<String, String> = BTreeMap::new();
    for n in &src.nodes {
        let id = uuid::Uuid::new_v4().to_string();
        id_map.insert(n.id.clone(), id.clone());
        let mut node = n.clone();
        node.id = id;
        node.position = [n.position[0] + offset[0], n.position[1] + offset[1]];
        dst.nodes.push(node);
    }
    let mut mapped_src_output = None;
    for e in &src.edges {
        if let (Some(from), Some(to)) = (id_map.get(&e.from_node), id_map.get(&e.to_node)) {
            if e.to_node == src.output_node && e.to_port == "source" {
                mapped_src_output = Some(from.clone());
            }
            dst.edges.push(GraphEdge {
                from_node: from.clone(),
                from_port: e.from_port.clone(),
                to_node: to.clone(),
                to_port: e.to_port.clone(),
            });
        }
    }
    // Preserve the existing composition and combine it with the dragged
    // preset through the real two-input compositor. The compositor remains a
    // normal editable node, so its Photoshop-style mode and opacity are
    // available in the inspector after the drop.
    if let Some(preset_provider) = mapped_src_output {
        dst.edges.retain(|e| !(e.to_node == dst.output_node && e.to_port == "source"));
        if let Some(existing) = existing_output {
            let blend_id = uuid::Uuid::new_v4().to_string();
            dst.nodes.push(GraphNode {
                id: blend_id.clone(),
                type_id: "fx.composite".into(),
                type_version: 1,
                position: [offset[0] + 260.0, offset[1] + 180.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            });
            dst.edges.push(GraphEdge {
                from_node: existing.from_node,
                from_port: existing.from_port,
                to_node: blend_id.clone(),
                to_port: "a".into(),
            });
            dst.edges.push(GraphEdge {
                from_node: preset_provider,
                from_port: "color".into(),
                to_node: blend_id.clone(),
                to_port: "b".into(),
            });
            dst.edges.push(GraphEdge {
                from_node: blend_id,
                from_port: "color".into(),
                to_node: dst.output_node.clone(),
                to_port: "source".into(),
            });
        } else {
            dst.edges.push(GraphEdge {
                from_node: preset_provider,
                from_port: "color".into(),
                to_node: dst.output_node.clone(),
                to_port: "source".into(),
            });
        }
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
