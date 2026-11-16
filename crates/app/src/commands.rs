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
    /// Import a movie, GIF, or still image and make it the active composition source.
    ImportMedia {
        path: String,
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
    /// Turn the live audio capture on or off and say which endpoint to tap.
    ///
    /// `tap` is `"loopback"` or `"mic"`; `"off"` with `enabled: true` stops the
    /// capture rather than starting one that deliberately hears nothing. The
    /// command's `inputDb` always wins over `FLUX_LIVE_AUDIO_INPUT_DB`, because
    /// an explicit pick outranks an environment default; the ring depth, stream
    /// buffer and poll interval are machine properties and stay with the
    /// environment. Nothing here auto-adjusts: a quiet take stays quiet.
    ///
    /// Starting a capture is not a document edit and takes no revision — it
    /// touches no `Project` field. Live features reach the document only
    /// through [`GuiCommand::LiveOsc`], which does.
    SetLiveAudio {
        enabled: bool,
        tap: String,
        device_id: Option<String>,
        input_db: f32,
    },
    /// Read the live audio status slot: what the analyser last measured, and
    /// whether that reading can be trusted. Read-only, no revision.
    LiveAudioStatus,
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
    /// The live audio capture, when the owner has asked for one. Owned here
    /// rather than by the shell, so capture authority stays on the Rust side of
    /// the IPC boundary — the same reason `OscBridge` is not in the webview.
    live_audio: Option<crate::live_audio::LiveAudioBridge>,
    /// Why the last `SetLiveAudio` did not leave a running capture, kept so a
    /// read-out that shows nothing can also say something true about why.
    live_audio_error: Option<String>,
    /// Complaints about `FLUX_LIVE_AUDIO*` values that will not parse. Reported,
    /// never swallowed into a default.
    live_audio_problems: Vec<String>,
}

impl CommandHost {
    pub fn new(project: Project) -> Self {
        let active_graph = project.graphs.keys().next().cloned().unwrap_or_default();
        Self {
            dispatcher: Dispatcher::new(project),
            route: Default::default(),
            control_map: Default::default(),
            active_graph,
            live_audio: None,
            live_audio_error: None,
            live_audio_problems: Vec::new(),
        }
    }

    /// The live audio reading for a frontend that must not submit a command to
    /// read a status: the same slot [`GuiCommand::LiveAudioStatus`] returns,
    /// reached without touching the document or the revision counter.
    pub fn live_audio_status(&self) -> serde_json::Value {
        let snapshot = if !crate::live_audio::LIVE_AUDIO_COMPILED {
            crate::live_audio::LiveAudioSnapshot::not_compiled(
                "live audio capture is not in this build: it needs --features live-audio",
            )
        } else {
            match self.live_audio.as_ref() {
                Some(bridge) => bridge.status(),
                None => {
                    // No capture is running. Report the wire facts as absent
                    // rather than as zero, and the reason as a sentence — a
                    // read-out that shows `rms 0.0000` for a device nobody
                    // opened is the exact confusion this file exists to avoid.
                    let mut s = crate::live_audio::LiveAudioSnapshot::default();
                    s.compiled = crate::live_audio::LIVE_AUDIO_COMPILED;
                    s.running = false;
                    s.valid = false;
                    s.feature_source = flux_domain::params::FeatureSource::Live;
                    s.error = Some(self.live_audio_error.clone().unwrap_or_else(|| {
                        "live audio is off: no SetLiveAudio has run".to_string()
                    }));
                    s
                }
            }
        };
        let mut v = snapshot.to_json();
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "problems".into(),
                serde_json::json!(self.live_audio_problems),
            );
        }
        v
    }

    /// Start or stop the live capture. No partial success: a tap name that will
    /// not parse, a trim outside the accepted range, or a device that will not
    /// open is an error, and the capture that was running stays running and
    /// keeps its own settings.
    fn set_live_audio(
        &mut self,
        enabled: bool,
        tap: &str,
        device_id: Option<String>,
        input_db: f32,
    ) -> FluxResult<serde_json::Value> {
        use crate::live_audio::{LiveAudioBridge, LiveMode};
        if !crate::live_audio::LIVE_AUDIO_COMPILED {
            return Err(FluxError::new(
                ErrorCode::SdkMissing,
                "live-audio",
                "live audio capture is not in this build: it needs --features live-audio, \
                 and `cargo build -p flux-app --features live-audio`",
            ));
        }
        let (mut cfg, problems) = crate::live_audio::LiveAudioConfig::from_env();
        self.live_audio_problems = problems;
        if !input_db.is_finite() || !(-24.0..=24.0).contains(&input_db) {
            return Err(FluxError::new(
                ErrorCode::InvalidCommand,
                "live-audio",
                format!("inputDb {input_db} is not a trim in -24..=24 dB"),
            ));
        }
        let mode = LiveMode::parse(tap).ok_or_else(|| {
            FluxError::new(
                ErrorCode::InvalidCommand,
                "live-audio",
                format!("tap {tap:?} is not one of off|loopback|mic"),
            )
        })?;
        cfg.mode = if enabled { mode } else { LiveMode::Off };
        cfg.device_id = device_id;
        cfg.input_db = input_db;

        // Anything already running is stopped first, by the same path `Drop`
        // uses, so a restart cannot leave two captures fighting over one device
        // — and the ASIO rig has exactly one client.
        let restarted = if let Some(bridge) = self.live_audio.take() {
            let was_running = bridge.is_running();
            let _ = bridge.stop();
            was_running
        } else {
            false
        };

        if cfg.is_off() {
            self.live_audio = None;
            self.live_audio_error = None;
            return Ok(serde_json::json!({
                "enabled": false,
                "restarted": restarted,
                "problems": self.live_audio_problems,
                "status": self.live_audio_status(),
            }));
        }
        match LiveAudioBridge::start(&cfg) {
            Ok(bridge) => {
                let json = bridge.status().to_json();
                self.live_audio = Some(bridge);
                self.live_audio_error = None;
                Ok(serde_json::json!({
                    "enabled": true,
                    "restarted": restarted,
                    "problems": self.live_audio_problems,
                    "status": json,
                }))
            }
            Err(e) => {
                let detail = format!("{e:?}");
                self.live_audio = None;
                self.live_audio_error = Some(detail.clone());
                Err(FluxError::new(e.code, "live-audio", detail))
            }
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

    /// Which graph a live control edits: the one a `LearnBinding` named, or the
    /// graph the editor is on when nothing has been learned. Both the UI's own
    /// `LiveOsc` command and the UDP listener go through here so they cannot
    /// disagree about where an incoming parameter lands.
    fn live_target_graph(&self) -> String {
        if self.control_map.graph_id.is_empty() {
            self.active_graph.clone()
        } else {
            self.control_map.graph_id.clone()
        }
    }

    /// Apply one live parameter control to the authoritative document.
    ///
    /// The parameter is written as an override whether or not the node's catalog
    /// entry declares it (`Command::SetParameter` inserts without validating the
    /// id), so a mistyped address produces an inert parameter and an undo entry
    /// rather than an error. The learned binding table is the only whitelist.
    pub fn apply_live_param(
        &mut self,
        node: &str,
        param: &str,
        value: f32,
    ) -> crate::control::OscApplied {
        use crate::control::OscApplied;
        if !crate::control::is_allowed(&self.control_map, node, param) {
            return OscApplied::Unmapped;
        }
        let graph_id = self.live_target_graph();
        if graph_id.is_empty() {
            return OscApplied::Unmapped;
        }
        match crate::control::apply_to_dispatcher(
            &mut self.dispatcher,
            &graph_id,
            node,
            param,
            value,
        ) {
            Ok(true) => OscApplied::Parameter {
                node: node.to_string(),
                param: param.to_string(),
                value,
            },
            Ok(false) => OscApplied::Unmapped,
            Err(e) => OscApplied::Rejected(e.message.clone()),
        }
    }

    /// Route one received OSC control. Parameters reach the document; everything
    /// else is answered with the reason it cannot be, so the listener's counters
    /// say what arrived rather than quietly discarding it.
    pub fn apply_live_control(
        &mut self,
        ctrl: &flux_integrations::osc::OscControl,
    ) -> crate::control::OscApplied {
        use crate::control::OscApplied;
        use flux_integrations::osc::OscControl as C;
        match ctrl {
            C::Param { node, param, value } => self.apply_live_param(node, param, *value as f32),
            C::TransportPlay(_) => OscApplied::NotApplied(
                "transport: no Rust-owned playhead — the preview clock is the webview's rAF loop, and the document has no transport command",
            ),
            C::Seek { .. } => OscApplied::NotApplied(
                "seek: same as transport — there is no clock in the document to move",
            ),
            C::Trigger { .. } => {
                OscApplied::NotApplied("trigger: node triggers have no Command variant yet")
            }
            C::PresetRecall(_) => OscApplied::NotApplied(
                "preset recall: parse_address stores the whole address as the id and drops the argument that names the preset",
            ),
            C::Bridge(_) => OscApplied::NotApplied(
                "bridge features: decoded and counted, but a bridge packet never reaches \
                 the analyser — the analyser runs on the mixed media buffer, not on OSC",
            ),
        }
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
                                        flux_catalog::ParamKind::Text => "text",
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
                        // Document-only on purpose: this is a query about the
                        // document, so it must be answerable without decoding
                        // media. `render_single_frame` is where measured audio
                        // reaches a frame; see `crate::audio_features`.
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
                let changed = matches!(
                    self.apply_live_param(&node, &param, value),
                    crate::control::OscApplied::Parameter { .. }
                );
                Ok(serde_json::json!({ "changed": changed, "revision": self.revision() }))
            }
            GuiCommand::SetLiveAudio {
                enabled,
                tap,
                device_id,
                input_db,
            } => self.set_live_audio(enabled, &tap, device_id, input_db),
            GuiCommand::LiveAudioStatus => Ok(self.live_audio_status()),
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
            GuiCommand::ImportMedia { path } => {
                let path = std::path::PathBuf::from(path);
                if !path.is_file() {
                    return Err(FluxError::new(
                        ErrorCode::MediaMissing,
                        "media",
                        "file not found",
                    ));
                }
                let worker = flux_media::MediaWorker::new()?;
                worker.probe(&path)?;
                let imported = crate::samples::demo_media_project(&path, 8);
                let asset = imported.assets.values().next().cloned().ok_or_else(|| {
                    FluxError::new(
                        ErrorCode::MediaDecode,
                        "media",
                        "media asset was not created",
                    )
                })?;
                if asset.missing {
                    return Err(FluxError::new(
                        ErrorCode::MediaDecode,
                        "media",
                        "media asset could not be decoded",
                    ));
                }
                let graph_id = imported.graphs.keys().next().cloned().ok_or_else(|| {
                    FluxError::new(
                        ErrorCode::InvalidCommand,
                        "media",
                        "media graph was not created",
                    )
                })?;
                let graph = imported.graphs.get(&graph_id).cloned().unwrap();
                let track = imported.timeline.tracks.values().next().cloned().unwrap();
                let clip = imported.timeline.clips.values().next().cloned().unwrap();
                self.dispatcher
                    .project
                    .assets
                    .insert(asset.id.clone(), asset);
                self.dispatcher
                    .project
                    .graphs
                    .insert(graph_id.clone(), graph);
                self.dispatcher
                    .project
                    .timeline
                    .tracks
                    .insert(track.id.clone(), track.clone());
                self.dispatcher
                    .project
                    .timeline
                    .track_order
                    .push(track.id.clone());
                self.dispatcher
                    .project
                    .timeline
                    .clips
                    .insert(clip.id.clone(), clip);
                self.dispatcher.project.timeline.range = imported.timeline.range;
                self.active_graph = graph_id.clone();
                self.dispatcher.project.timeline.master_graph_id = Some(graph_id.clone());
                Ok(self.snapshot())
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
                let preset = presets.iter().find(|p| p.id == preset_id).ok_or_else(|| {
                    FluxError::new(ErrorCode::NotFound, "gui", "preset not found")
                })?;
                let graph = crate::samples::insert_preset(preset);
                if let Some(target) = merge_into {
                    let dst = self
                        .dispatcher
                        .project
                        .graphs
                        .get_mut(&target)
                        .ok_or_else(|| {
                            FluxError::new(ErrorCode::NotFound, "gui", "graph not found")
                        })?;
                    crate::samples::merge_graph(dst, &graph, position);
                    Ok(serde_json::json!({ "graphId": target }))
                } else {
                    let mut graph = graph;
                    // Presets carry a stable recipe id; give each insert a unique
                    // graph id so repeated inserts don't overwrite each other.
                    graph.id = format!("graph-{}", uuid::Uuid::new_v4());
                    for n in &mut graph.nodes {
                        n.position = [n.position[0] + position[0], n.position[1] + position[1]];
                    }
                    let graph_id = graph.id.clone();
                    self.dispatcher
                        .project
                        .graphs
                        .insert(graph_id.clone(), graph);
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
                self.dispatcher
                    .apply(Command::SetMapping { mapping }, None)?;
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

    /// The webview submits these as JSON, so the wire names are part of the
    /// contract, not the Rust spelling: `setLiveAudio` with camelCase fields and
    /// `liveAudioStatus` with none. Renaming a variant or a field in Rust
    /// without renaming it here reddens this first.
    #[test]
    fn the_live_audio_commands_have_the_json_names_the_webview_submits() {
        let cmd: GuiCommand = serde_json::from_str(
            r#"{"type":"setLiveAudio","enabled":true,"tap":"loopback","deviceId":null,"inputDb":6.0}"#,
        )
        .expect("`setLiveAudio` must parse from the documented envelope");
        match cmd {
            GuiCommand::SetLiveAudio {
                enabled,
                tap,
                device_id,
                input_db,
            } => {
                assert!(enabled);
                assert_eq!(tap, "loopback");
                assert_eq!(device_id, None);
                assert!(
                    (input_db - 6.0).abs() < 1e-6,
                    "inputDb did not survive: {input_db}"
                );
            }
            other => panic!("wrong variant deserialised: {other:?}"),
        }
        let status: GuiCommand =
            serde_json::from_str(r#"{"type":"liveAudioStatus"}"#).expect("`liveAudioStatus`");
        assert!(matches!(status, GuiCommand::LiveAudioStatus));
        assert_eq!(
            serde_json::to_string(&GuiCommand::LiveAudioStatus).unwrap(),
            r#"{"type":"liveAudioStatus"}"#
        );
    }

    /// A refused `SetLiveAudio` costs the document nothing: no revision, no
    /// half-started capture, no stored error pretending to be state. The
    /// refusal is `SdkMissing` in a build compiled without the feature and
    /// `InvalidCommand` for a tap name that is not a tap — either way it comes
    /// back as an error with a code, never `Ok` carrying zeros that read as a
    /// quiet room.
    #[test]
    fn a_refused_live_audio_command_costs_the_document_nothing() {
        let mut h = host();
        let before = h.revision();
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "live-refuse".into(),
            project_id: None,
            expected_revision: before,
            command: GuiCommand::SetLiveAudio {
                enabled: true,
                tap: "auto".into(),
                device_id: None,
                input_db: 0.0,
            },
        });
        let err = r.error.expect("a tap that is not a tap must be refused");
        assert!(
            matches!(err.code.as_str(), "SdkMissing" | "InvalidCommand"),
            "unexpected refusal code {:?}",
            err.code
        );
        assert_eq!(h.revision(), before, "a refused capture costs no revision");
        assert!(
            h.live_audio.is_none(),
            "a refused capture must not be stored"
        );
        assert_eq!(h.live_audio_error, None, "the refusal is in the response");
        let st = h.live_audio_status();
        assert_eq!(st["running"], serde_json::json!(false));
        assert_eq!(st["valid"], serde_json::json!(false));
    }

    /// The shipping build has no capture apparatus and the status says so in
    /// words instead of reporting a level of zero. This is the equivalence check
    /// for `--no-default-features`: the answer to a live-audio question must be
    /// a named absence, not a silent pass.
    #[cfg(not(feature = "live-audio"))]
    #[test]
    fn the_status_names_the_missing_apparatus_instead_of_a_quiet_room() {
        let h = host();
        assert!(!crate::live_audio::LIVE_AUDIO_COMPILED);
        let st = h.live_audio_status();
        assert_eq!(st["compiled"], serde_json::json!(false));
        assert_eq!(st["running"], serde_json::json!(false));
        assert_eq!(st["valid"], serde_json::json!(false));
        let err = st["error"].as_str().expect("a reason must be given");
        assert!(
            err.contains("live-audio"),
            "the reason must name the feature: {err}"
        );
        assert_eq!(st["featureSource"], serde_json::json!("live"));
    }

    /// The same refusal with a *valid* tap. In a build compiled without the
    /// feature there is no apparatus to ask for, and `SetLiveAudio` must answer
    /// `SdkMissing` instead of starting a capture that hears nothing. This is
    /// the branch the shipping-build equivalence rests on, so it is tested
    /// apart from the bad-name case above — which would still pass if the
    /// feature check were deleted, and this one would not.
    #[cfg(not(feature = "live-audio"))]
    #[test]
    fn the_shipping_build_refuses_a_valid_live_audio_request() {
        let mut h = host();
        let before = h.revision();
        // A refusal must be a refusal, not an attempt that failed: `start`
        // claims the shared sidecar on its way in, so a build with no apparatus
        // should never get that far. Clear the file first, because another test
        // in this binary may have written it.
        let _ = std::fs::remove_file(crate::live_audio::status_file());
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "live-sdk".into(),
            project_id: None,
            expected_revision: before,
            command: GuiCommand::SetLiveAudio {
                enabled: true,
                tap: "loopback".into(),
                device_id: None,
                input_db: 0.0,
            },
        });
        let err = r.error.expect("no capture apparatus means no capture");
        assert_eq!(err.code, "SdkMissing");
        assert!(err.message.contains("live-audio"), "{}", err.message);
        assert!(h.live_audio.is_none(), "a refusal must not store a bridge");
        assert_eq!(h.revision(), before, "a refusal costs no revision");
        assert!(
            crate::live_audio::read_status_file().is_none(),
            "a refused command must not claim the live-audio sidecar"
        );
    }

    /// With the apparatus compiled in, a status taken before any command is
    /// `off` with a reason — not an empty device list, which is exactly what a
    /// silent room would also look like (D036). No device is opened to prove
    /// this, and none may be: a refused trim is rejected before `open` runs.
    #[cfg(feature = "live-audio")]
    #[test]
    fn an_untouched_live_status_is_off_with_a_reason_not_zeroed() {
        let mut h = host();
        assert!(crate::live_audio::LIVE_AUDIO_COMPILED);
        let st = h.live_audio_status();
        assert_eq!(st["compiled"], serde_json::json!(true));
        assert_eq!(st["running"], serde_json::json!(false));
        assert_eq!(st["valid"], serde_json::json!(false));
        assert!(
            st["error"].as_str().unwrap_or("").contains("SetLiveAudio"),
            "the read-out must say why nothing is running: {:?}",
            st["error"]
        );
        let before = h.revision();
        let r = h.handle(Request {
            protocol_version: 1,
            request_id: "live-trim".into(),
            project_id: None,
            expected_revision: before,
            command: GuiCommand::SetLiveAudio {
                enabled: true,
                tap: "loopback".into(),
                device_id: None,
                input_db: 40.0,
            },
        });
        let err = r
            .error
            .expect("40 dB is not a trim, it is a replacement gain");
        assert_eq!(err.code, "InvalidCommand");
        assert!(
            h.live_audio.is_none(),
            "a refused trim must not open a device"
        );
        assert_eq!(h.revision(), before);
    }
}

fn flux_app_persistence_load(path: &std::path::Path) -> Project {
    crate::persistence::load(path).unwrap()
}
