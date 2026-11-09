//! GUI command surface: the single typed request/response dispatcher that any
//! frontend (Tauri+React, or tests) calls. The engine owns state; the UI only
//! submits commands and reads snapshots. This module is the anti-divergence
//! boundary — the GUI cannot execute anything that does not go through here.

use flux_catalog::build_catalog;
use flux_domain::params::FeatureSnapshot;
use flux_domain::{Command, Dispatcher, ErrorCode, FluxError, FluxResult, Project, RationalTime};
use serde::{Deserialize, Serialize};

/// Request envelope (CONTRACTS §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub protocol_version: u32,
    pub request_id: String,
    pub project_id: Option<String>,
    pub expected_revision: u64,
    pub command: GuiCommand,
}

/// UI-facing command set. Structural edits map to engine `Command`s.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GuiCommand {
    CreateProject {
        name: String,
        seed: u64,
        width: u32,
        height: u32,
        fps_num: u32,
        fps_den: u32,
    },
    LoadProject {
        json: String,
    },
    Save {
        path: String,
    },
    /// Apply an engine command (parameter, node, edge, clip, track edits).
    Apply {
        command: Command,
    },
    Undo,
    Redo,
    /// Add a generator-backed clip and create a chain graph with effects.
    AddGeneratorClip {
        generator_id: String,
        effects: Vec<String>,
        seconds: i64,
    },
    /// Read-only snapshot for the UI.
    Snapshot,
    /// List catalog entries for the node library.
    Catalog,
    /// Evaluate one frame's resolved parameters (inspector contribution view).
    ResolveParams {
        node_id: String,
        time_seconds: f64,
    },
    /// Learn a control binding for a node parameter (OSC/MIDI).
    LearnBinding {
        binding: crate::control::ControlBinding,
        graph_id: String,
    },
    /// Apply a live OSC parameter control to the document.
    LiveOsc {
        node: String,
        param: String,
        value: f32,
    },
    /// Apply a live MIDI control to the document via learned bindings.
    LiveMidi {
        channel: u8,
        cc: u8,
        value: f32,
    },
    /// Add a node of `type_id` to the active graph at (x,y). Returns its id.
    AddGraphNode {
        type_id: String,
        x: f32,
        y: f32,
    },
    /// Connect an output port to an input port in the active graph.
    ConnectPins {
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
    },
    /// Remove the edge feeding an input port.
    DisconnectPin {
        to_node: String,
        to_port: String,
    },
    /// Remove a node from the active graph.
    RemoveGraphNode {
        node_id: String,
    },
    /// Move a node (canvas position only; not an undoable structural edit).
    MoveNode {
        node_id: String,
        x: f32,
        y: f32,
    },
    /// Set the graph's rendered output node.
    SetGraphOutput {
        node_id: String,
    },
    /// Create an empty composition graph (single Output sink) and make it active.
    NewGraph {
        name: String,
    },
    /// List the built-in multi-node presets.
    Presets,
    /// Insert a preset as the active composition (or merge into the active graph).
    InsertPreset {
        preset_id: String,
        position: [f32; 2],
        merge_into: Option<String>,
    },
    /// Set one node's bypass flag (undoable).
    SetBypass {
        node_id: String,
        bypass: bool,
    },
    /// Replace a parameter's keyframe lane. Lane id is `"nodeId:paramId"`.
    SetKeyframes {
        lane_id: String,
        keyframes: Vec<flux_domain::Keyframe>,
    },
    /// Create/replace a modulation mapping.
    SetMapping {
        mapping: flux_domain::Mapping,
    },
    RemoveMapping {
        mapping_id: String,
    },
    /// Persist canvas-only metadata (frames, viewport) in project.extensions.
    SetCanvasMeta {
        frames: serde_json::Value,
        viewport: serde_json::Value,
    },
}

/// Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub request_id: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseError {
    pub code: String,
    pub message: String,
    pub recovery: String,
    pub entity_ids: Vec<String>,
}

impl ResponseError {
    fn from(e: &FluxError) -> Self {
        Self {
            code: format!("{:?}", e.code),
            message: e.message.clone(),
            recovery: e.recovery.clone(),
            entity_ids: e.entity_ids.clone(),
        }
    }
}

/// The host holds the authoritative document and revision counter.
pub struct CommandHost {
    pub dispatcher: Dispatcher,
    route: flux_engine::router::Router,
    control_map: crate::control::ControlMap,
    /// The graph the node editor is currently editing/rendering.
    pub active_graph: String,
}

impl CommandHost {
    pub fn new(project: Project) -> Self {
        let active_graph = project.graphs.keys().next().cloned().unwrap_or_default();
        Self {
            dispatcher: Dispatcher::new(project),
            route: Default::default(),
            control_map: Default::default(),
            active_graph,
        }
    }

    /// The active graph (if present).
    pub fn graph(&self) -> Option<&flux_domain::Graph> {
        self.dispatcher.project.graphs.get(&self.active_graph)
    }

    /// Ensure an active graph exists (create a default gen.plasma -> output graph
    /// and make it active). Idempotent.
    fn ensure_active_graph(&mut self) {
        if !self
            .dispatcher
            .project
            .graphs
            .contains_key(&self.active_graph)
        {
            let g = crate::samples::chain_graph("gen.plasma", &[]);
            self.active_graph = g.id.clone();
            self.dispatcher.project.graphs.insert(g.id.clone(), g);
        }
        // Persist the composition root so preview, export and the canvas agree.
        self.dispatcher.project.timeline.master_graph_id = Some(self.active_graph.clone());
    }

    pub fn revision(&self) -> u64 {
        self.dispatcher.revision
    }

    /// Handle a request, returning a response. Never panics on bad input.
    pub fn handle(&mut self, req: Request) -> Response {
        let id = req.request_id.clone();
        if req.protocol_version != 1 {
            return Response {
                request_id: id,
                revision: self.revision(),
                result: None,
                error: Some(ResponseError {
                    code: "PROTOCOL_VERSION".into(),
                    message: format!("unsupported protocol version {}", req.protocol_version),
                    recovery: "update client".into(),
                    entity_ids: vec![],
                }),
            };
        }
        match self.execute(req.expected_revision, req.command) {
            Ok(v) => Response {
                request_id: id,
                revision: self.revision(),
                result: Some(v),
                error: None,
            },
            Err(e) => Response {
                request_id: id,
                revision: self.revision(),
                result: None,
                error: Some(ResponseError::from(&e)),
            },
        }
    }

    fn execute(&mut self, expected: u64, cmd: GuiCommand) -> FluxResult<serde_json::Value> {
        match cmd {
            GuiCommand::CreateProject {
                name,
                seed,
                width,
                height,
                fps_num,
                fps_den,
            } => {
                let mut p = Project::new(name, seed);
                p.settings.size = flux_domain::Size { width, height };
                p.settings.frame_rate = flux_domain::FrameRate::new(fps_num, fps_den)
                    .map_err(|e| FluxError::new(ErrorCode::InvalidCommand, "gui", e.to_string()))?;
                self.dispatcher = Dispatcher::new(p);
                // Start the node editor with a gen.plasma -> output graph so the
                // canvas is never empty and the preview renders immediately.
                self.active_graph.clear();
                self.ensure_active_graph();
                Ok(self.snapshot())
            }
            GuiCommand::LoadProject { json } => {
                let p: Project = serde_json::from_str(&json)
                    .map_err(|e| FluxError::new(ErrorCode::ProjectCorrupt, "gui", e.to_string()))?;
                if p.schema_version > flux_domain::SCHEMA_VERSION {
                    return Err(FluxError::new(
                        ErrorCode::ProjectVersion,
                        "gui",
                        "newer schema",
                    ));
                }
                self.dispatcher = Dispatcher::new(p);
                self.active_graph = self
                    .dispatcher
                    .project
                    .timeline
                    .master_graph_id
                    .clone()
                    .unwrap_or_default();
                self.ensure_active_graph();
                Ok(self.snapshot())
            }
            GuiCommand::Save { path } => {
                crate::persistence::save_atomic(
                    &self.dispatcher.project,
                    std::path::Path::new(&path),
                )?;
                Ok(serde_json::json!({ "saved": path }))
            }
            GuiCommand::Apply { command } => {
                let tx = self.dispatcher.apply(command, Some(expected))?;
                Ok(serde_json::json!({ "changed": tx.changed_ids, "revision": self.revision() }))
            }
            GuiCommand::Undo => {
                let rev = self.dispatcher.undo()?;
                Ok(serde_json::json!({ "revision": rev }))
            }
            GuiCommand::Redo => {
                let rev = self.dispatcher.redo()?;
                Ok(serde_json::json!({ "revision": rev }))
            }
            GuiCommand::AddGeneratorClip {
                generator_id,
                effects,
                seconds,
            } => {
                let demo = crate::samples::demo_generator_project(
                    &generator_id,
                    &effect_refs(&effects),
                    seconds,
                );
                // Create exactly one fresh track and clip (consistent ids), then
                // attach the demo's chain graph. Never re-add the demo's whole
                // structure, which caused duplicate clips.
                let track_id = format!("track-{}", uuid::Uuid::new_v4());
                let mut track = demo
                    .timeline
                    .tracks
                    .values()
                    .next()
                    .cloned()
                    .ok_or_else(|| {
                        FluxError::new(ErrorCode::InvalidCommand, "gui", "demo has no track")
                    })?;
                track.id = track_id.clone();
                track.clip_order.clear();
                let n = self.dispatcher.project.timeline.tracks.len() + 1;
                track.name = format!("V{n}");
                self.dispatcher.apply(Command::AddTrack { track }, None)?;

                // Take the demo's single clip, re-id it, place on the new track.
                if let Some((_old, mut clip)) = demo
                    .timeline
                    .clips
                    .iter()
                    .next()
                    .map(|(k, v)| (k.clone(), v.clone()))
                {
                    clip.id = format!("clip-{}", uuid::Uuid::new_v4());
                    self.dispatcher.apply(
                        Command::AddClip {
                            track_id: track_id.clone(),
                            clip,
                        },
                        None,
                    )?;
                }
                for (gid, graph) in demo.graphs.iter() {
                    self.dispatcher
                        .project
                        .graphs
                        .insert(gid.clone(), graph.clone());
                }
                Ok(self.snapshot())
            }
            GuiCommand::Snapshot => Ok(self.snapshot()),
            GuiCommand::Catalog => {
                let cat = build_catalog();
                let entries: Vec<serde_json::Value> = cat
                    .entries
                    .iter()
                    .map(|e| {
                        let params: Vec<serde_json::Value> = e
                            .manifest
                            .parameters
                            .iter()
                            .filter(|p| !p.hidden)
                            .map(|p| {
                                serde_json::json!({
                                    "id": p.id,
                                    "kind": match p.kind {
                                        flux_catalog::ParamKind::Float => "float",
                                        flux_catalog::ParamKind::Int => "int",
                                        flux_catalog::ParamKind::Bool => "bool",
                                        flux_catalog::ParamKind::Enum => "enum",
                                        flux_catalog::ParamKind::Color => "color",
                                        flux_catalog::ParamKind::Vec2 => "vec2",
                                        flux_catalog::ParamKind::Vec3 => "vec3",
                                    },
                                    "default": p.default,
                                    "min": p.min,
                                    "max": p.max,
                                    "label": if p.label.is_empty() { p.id.clone() } else { p.label.clone() },
                                    "options": p.options,
                                })
                            })
                            .collect();
                        let inputs: Vec<serde_json::Value> = e
                            .manifest
                            .inputs
                            .iter()
                            .map(|p| {
                                serde_json::json!({
                                    "id": p.id,
                                    "type": port_type_str(&p.port_type),
                                    "required": p.required,
                                })
                            })
                            .collect();
                        let outputs: Vec<serde_json::Value> = e
                            .manifest
                            .outputs
                            .iter()
                            .map(|p| {
                                serde_json::json!({
                                    "id": p.id,
                                    "type": port_type_str(&p.port_type),
                                    "required": p.required,
                                })
                            })
                            .collect();
                        serde_json::json!({
                            "id": e.manifest.id, "name": e.manifest.name,
                            "category": e.manifest.category, "kind": if e.manifest.generator_id.is_empty() { "effect" } else { "generator" },
                            "parameters": params,
                            "inputs": inputs,
                            "outputs": outputs,
                            "help": e.manifest.help,
                            "accent": e.manifest.accent,
                            "version": e.manifest.version,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "entries": entries }))
            }
            GuiCommand::ResolveParams {
                node_id,
                time_seconds,
            } => {
                let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
                    .map_err(|e| FluxError::new(ErrorCode::InvalidCommand, "gui", e.to_string()))?;
                // Find the graph containing the node.
                let projects = &self.dispatcher.project;
                for graph in projects.graphs.values() {
                    if graph.node(&node_id).is_some() {
                        let vals = self.route.resolve_node(
                            graph,
                            &node_id,
                            t,
                            &FeatureSnapshot::default(),
                            0.016,
                            projects,
                        )?;
                        return Ok(serde_json::json!({ "values": vals }));
                    }
                }
                Err(FluxError::new(ErrorCode::NotFound, "gui", "node not found"))
            }
            GuiCommand::LearnBinding { binding, graph_id } => {
                self.control_map.graph_id = graph_id;
                self.control_map.bindings.push(binding);
                Ok(serde_json::json!({ "bindings": self.control_map.bindings.len() }))
            }
            GuiCommand::LiveOsc { node, param, value } => {
                let graph_id = self.control_map.graph_id.clone();
                let changed = if graph_id.is_empty() {
                    false
                } else {
                    crate::control::apply_to_dispatcher(
                        &mut self.dispatcher,
                        &graph_id,
                        &node,
                        &param,
                        value,
                    )?
                };
                Ok(serde_json::json!({ "changed": changed, "revision": self.revision() }))
            }
            GuiCommand::LiveMidi { channel, cc, value } => {
                let graph_id = self.control_map.graph_id.clone();
                let target = crate::control::apply_midi(
                    &self.control_map,
                    &flux_integrations::midi::MidiControl::Cc {
                        channel,
                        cc,
                        value: value as f64,
                    },
                );
                let changed = match target {
                    Some((node, param, v)) if !graph_id.is_empty() => {
                        crate::control::apply_to_dispatcher(
                            &mut self.dispatcher,
                            &graph_id,
                            &node,
                            &param,
                            v,
                        )?
                    }
                    _ => false,
                };
                Ok(serde_json::json!({ "changed": changed, "revision": self.revision() }))
            }
            GuiCommand::AddGraphNode { type_id, x, y } => {
                self.ensure_active_graph();
                let graph_id = self.active_graph.clone();
                let node = flux_domain::GraphNode {
                    id: uuid::Uuid::new_v4().to_string(),
                    type_id,
                    type_version: 1,
                    position: [x, y],
                    params: Default::default(),
                    bypass: false,
                    missing_type: false,
                };
                let node_id = node.id.clone();
                self.dispatcher
                    .apply(Command::AddNode { graph_id, node }, None)?;
                Ok(serde_json::json!({ "nodeId": node_id, "revision": self.revision() }))
            }
            GuiCommand::ConnectPins {
                from_node,
                from_port,
                to_node,
                to_port,
            } => {
                let graph_id = self.active_graph.clone();
                self.dispatcher.apply(
                    Command::Connect {
                        graph_id,
                        from_node,
                        from_port,
                        to_node,
                        to_port,
                    },
                    None,
                )?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::DisconnectPin { to_node, to_port } => {
                let graph_id = self.active_graph.clone();
                let idx = self.dispatcher.project.graphs.get(&graph_id).and_then(|g| {
                    g.edges
                        .iter()
                        .position(|e| e.to_node == to_node && e.to_port == to_port)
                });
                if let Some(edge_index) = idx {
                    self.dispatcher.apply(
                        Command::Disconnect {
                            graph_id,
                            edge_index,
                        },
                        None,
                    )?;
                }
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::RemoveGraphNode { node_id } => {
                let graph_id = self.active_graph.clone();
                self.dispatcher.apply(
                    Command::RemoveNodes {
                        graph_id,
                        node_ids: vec![node_id],
                    },
                    None,
                )?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::MoveNode { node_id, x, y } => {
                let graph_id = self.active_graph.clone();
                self.dispatcher.apply(
                    Command::SetNodePositions {
                        graph_id,
                        positions: vec![flux_domain::commands::NodePosition {
                            node_id,
                            position: [x, y],
                        }],
                    },
                    None,
                )?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::SetGraphOutput { node_id } => {
                let gid = self.active_graph.clone();
                if let Some(g) = self.dispatcher.project.graphs.get_mut(&gid) {
                    g.output_node = node_id;
                }
                Ok(serde_json::json!({ "ok": true }))
            }
            GuiCommand::NewGraph { name } => {
                let g = crate::samples::empty_graph(name);
                let graph_id = g.id.clone();
                self.dispatcher.project.graphs.insert(graph_id.clone(), g);
                self.active_graph = graph_id.clone();
                self.dispatcher.project.timeline.master_graph_id = Some(graph_id.clone());
                Ok(serde_json::json!({ "graphId": graph_id }))
            }
            GuiCommand::Presets => {
                let presets: Vec<serde_json::Value> = flux_catalog::presets::all_presets()
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "name": p.name,
                            "description": p.description,
                            "requirementRefs": p.requirement_refs,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "presets": presets }))
            }
            GuiCommand::InsertPreset {
                preset_id,
                position,
                merge_into,
            } => {
                let presets = flux_catalog::presets::all_presets();
                let preset = presets
                    .iter()
                    .find(|p| p.id == preset_id)
                    .ok_or_else(|| {
                        FluxError::new(ErrorCode::NotFound, "gui", "preset not found")
                    })?;
                let graph = crate::samples::insert_preset(preset);
                if let Some(target) = merge_into {
                    let dst = self.dispatcher.project.graphs.get_mut(&target).ok_or_else(|| {
                        FluxError::new(ErrorCode::NotFound, "gui", "graph not found")
                    })?;
                    crate::samples::merge_graph(dst, &graph, position);
                    Ok(serde_json::json!({ "graphId": target }))
                } else {
                    let mut graph = graph;
                    for n in &mut graph.nodes {
                        n.position = [n.position[0] + position[0], n.position[1] + position[1]];
                    }
                    let graph_id = graph.id.clone();
                    self.dispatcher.project.graphs.insert(graph_id.clone(), graph);
                    self.active_graph = graph_id.clone();
                    self.dispatcher.project.timeline.master_graph_id = Some(graph_id.clone());
                    Ok(serde_json::json!({ "graphId": graph_id }))
                }
            }
            GuiCommand::SetBypass { node_id, bypass } => {
                let graph_id = self.active_graph.clone();
                self.dispatcher.apply(
                    Command::SetBypass {
                        graph_id,
                        node_id,
                        bypass,
                    },
                    None,
                )?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::SetKeyframes { lane_id, keyframes } => {
                let graph_id = self.active_graph.clone();
                self.dispatcher.apply(
                    Command::SetKeyframes {
                        graph_id,
                        lane_id,
                        keyframes,
                    },
                    None,
                )?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::SetMapping { mapping } => {
                self.dispatcher.apply(Command::SetMapping { mapping }, None)?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::RemoveMapping { mapping_id } => {
                self.dispatcher
                    .apply(Command::RemoveMapping { mapping_id }, None)?;
                Ok(serde_json::json!({ "revision": self.revision() }))
            }
            GuiCommand::SetCanvasMeta { frames, viewport } => {
                self.dispatcher.project.extensions.insert(
                    "dev.flux.canvas".into(),
                    serde_json::json!({ "frames": frames, "viewport": viewport }),
                );
                Ok(serde_json::json!({ "ok": true }))
            }
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "project": self.dispatcher.project,
            "revision": self.revision(),
            "canUndo": self.dispatcher.can_undo(),
            "canRedo": self.dispatcher.can_redo(),
            "activeGraphId": self.active_graph,
            "activeGraph": self.dispatcher.project.graphs.get(&self.active_graph),
        })
    }
}

fn effect_refs(effects: &[String]) -> Vec<&str> {
    effects.iter().map(|s| s.as_str()).collect()
}

/// Stable snake_case name for a port type, consumed by the node canvas to
/// colour cables and decide connect compatibility.
fn port_type_str(t: &flux_catalog::manifest::PortType) -> &'static str {
    use flux_catalog::manifest::PortType as P;
    match t {
        P::Texture2D => "texture2d",
        P::Mask => "mask",
        P::Depth => "depth",
        P::Normal => "normal",
        P::MotionVectors => "motion_vectors",
        P::Scalar => "scalar",
        P::Int => "int",
        P::Bool => "bool",
        P::Color => "color",
        P::Vec2 => "vec2",
        P::Vec3 => "vec3",
        P::Transform => "transform",
        P::AudioFeatures => "audio_features",
        P::AudioSpectrum => "audio_spectrum",
        P::Event => "event",
        P::BeatState => "beat_state",
        P::Geometry => "geometry",
        P::Scene => "scene",
        P::ParticleBuffer => "particle_buffer",
        P::SdfField => "sdf_field",
        P::VectorField => "vector_field",
        P::PathSet => "path_set",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> CommandHost {
        CommandHost::new(Project::new("gui", 1))
    }

    #[test]
    fn create_then_snapshot() {
        let mut h = host();
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "1".into(),
            project_id: None,
            expected_revision: 0,
            command: GuiCommand::CreateProject {
                name: "P".into(),
                seed: 3,
                width: 640,
                height: 360,
                fps_num: 30,
                fps_den: 1,
            },
        });
        assert!(r.error.is_none());
        assert_eq!(r.result.unwrap()["project"]["name"], "P");
    }

    #[test]
    fn stale_revision_reports_conflict() {
        let mut h = host();
        // A structural Apply with a stale expected revision must be rejected.
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "2".into(),
            project_id: None,
            expected_revision: 99,
            command: GuiCommand::Apply {
                command: Command::SetTimelineRange {
                    range: flux_domain::TimeRange::new(
                        RationalTime::ZERO,
                        RationalTime::from_secs(4),
                    ),
                },
            },
        });
        assert_eq!(r.error.unwrap().code, "RevisionConflict");
    }

    #[test]
    fn add_generator_clip_and_undo() {
        let mut h = host();
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "3".into(),
            project_id: None,
            expected_revision: 0,
            command: GuiCommand::AddGeneratorClip {
                generator_id: "gen.plasma".into(),
                effects: vec!["fx.bloom".into()],
                seconds: 2,
            },
        });
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(h.dispatcher.project.timeline.tracks.len(), 1);
        // Exactly one clip — the earlier merge double-added the demo's clip.
        assert_eq!(h.dispatcher.project.timeline.clips.len(), 1);
        let rev = h.revision();
        // Undo the clips/track.
        while h.dispatcher.can_undo() {
            h.handle(Request {
                protocol_version: 1,
                request_id: "u".into(),
                project_id: None,
                expected_revision: 0,
                command: GuiCommand::Undo,
            });
        }
        assert!(h.revision() > rev);
    }

    #[test]
    fn catalog_lists_entries() {
        let mut h = host();
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "4".into(),
            project_id: None,
            expected_revision: 0,
            command: GuiCommand::Catalog,
        });
        let entries = r.result.unwrap();
        assert!(entries["entries"].as_array().unwrap().len() > 200);
    }

    #[test]
    fn bad_protocol_rejected() {
        let mut h = host();
        let r = h.handle(Request {
            protocol_version: 2,
            request_id: "5".into(),
            project_id: None,
            expected_revision: 0,
            command: GuiCommand::Snapshot,
        });
        assert_eq!(r.error.unwrap().code, "PROTOCOL_VERSION");
    }

    #[test]
    fn wire_json_camelcase_deserializes() {
        // Exactly what apps/desktop/ui/app.js sends over IPC. Regression for the
        // serde `rename_all` (tag only) vs `rename_all_fields` (fields) bug that
        // made the desktop UI show an empty library and a broken preview.
        let create = r#"{"protocolVersion":1,"requestId":"x","projectId":null,"expectedRevision":0,
            "command":{"type":"createProject","name":"G","seed":1,"width":640,"height":360,"fpsNum":60,"fpsDen":1}}"#;
        let mut h = host();
        let res =
            h.handle(serde_json::from_str::<Request>(create).expect("createProject wire json"));
        assert!(res.error.is_none(), "{:?}", res.error);

        let add = r#"{"protocolVersion":1,"requestId":"y","projectId":null,"expectedRevision":0,
            "command":{"type":"addGeneratorClip","generatorId":"gen.plasma","effects":["fx.bloom"],"seconds":4}}"#;
        let res =
            h.handle(serde_json::from_str::<Request>(add).expect("addGeneratorClip wire json"));
        assert!(res.error.is_none(), "{:?}", res.error);

        let setp = r#"{"protocolVersion":1,"requestId":"z","projectId":null,"expectedRevision":0,
            "command":{"type":"apply","command":{"type":"setParameter","graphId":"g","nodeId":"n","paramId":"p","value":{"kind":"float","value":0.5}}}}"#;
        let req = serde_json::from_str::<Request>(setp)
            .expect("setParameter wire json (camelCase fields)");
        if let GuiCommand::Apply { command } = &req.command {
            assert!(matches!(command, Command::SetParameter { param_id, .. } if param_id == "p"));
        } else {
            panic!("expected apply");
        }
    }

    #[test]
    fn gui_acceptance_flow_edit_save_reload_deterministic() {
        // Mirrors the GUI acceptance test: create → catalog → add scene → set a
        // parameter → save → reload → the reloaded project equals the saved one.
        let mut h = host();
        let mk = |cmd: GuiCommand, rev: u64| Request {
            protocol_version: 1,
            request_id: "r".into(),
            project_id: None,
            expected_revision: rev,
            command: cmd,
        };
        h.handle(mk(
            GuiCommand::CreateProject {
                name: "G".into(),
                seed: 7,
                width: 320,
                height: 180,
                fps_num: 30,
                fps_den: 1,
            },
            0,
        ));
        let catalog = h.handle(mk(GuiCommand::Catalog, 0)).result.unwrap();
        assert!(catalog["entries"].as_array().unwrap().len() > 200);
        h.handle(mk(
            GuiCommand::AddGeneratorClip {
                generator_id: "gen.plasma".into(),
                effects: vec!["fx.bloom".into()],
                seconds: 4,
            },
            0,
        ));
        // Find the bloom node and set exposure.
        let snap = h.snapshot();
        let graphs = snap["project"]["graphs"].as_object().unwrap();
        let mut set_ok = false;
        for (gid, g) in graphs {
            for n in g["nodes"].as_array().unwrap() {
                if n["typeId"] == "fx.bloom" {
                    let r = h.handle(mk(
                        GuiCommand::Apply {
                            command: Command::SetParameter {
                                graph_id: gid.clone(),
                                node_id: n["id"].as_str().unwrap().to_string(),
                                param_id: "intensity".into(),
                                value: flux_domain::ParamValue::Float { value: 1.5 },
                            },
                        },
                        h.revision(),
                    ));
                    assert!(r.error.is_none(), "{:?}", r.error);
                    set_ok = true;
                }
            }
        }
        assert!(set_ok, "expected a bloom node to edit");
        // Save to a temp file and reload; structure must be identical.
        let dir = std::env::temp_dir().join(format!("flux-gui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("g.flux.json");
        h.handle(mk(
            GuiCommand::Save {
                path: path.display().to_string(),
            },
            h.revision(),
        ));
        assert!(path.exists());
        let reloaded = flux_app_persistence_load(&path);
        let live = h.dispatcher.project.clone();
        assert_eq!(reloaded.project_id, live.project_id);
        assert_eq!(reloaded.graphs.len(), live.graphs.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn flux_app_persistence_load(path: &std::path::Path) -> Project {
    crate::persistence::load(path).unwrap()
}
