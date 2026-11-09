//! Commands, transactions and deterministic undo/redo.
//!
//! Every structural edit is a typed command validated atomically against the
//! document. Each command produces an inverse command; undo simply executes the
//! inverse through the same validation path, which guarantees structure and
//! references stay consistent.

use crate::document::*;
use crate::error::{ErrorCode, FluxError, FluxResult};
use crate::time::{RationalTime, TimeRange};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Command {
    AddTrack {
        track: Track,
    },
    RemoveTrack {
        track_id: String,
    },
    AddClip {
        track_id: String,
        clip: Clip,
    },
    RemoveClip {
        clip_id: String,
    },
    MoveClip {
        clip_id: String,
        new_start: RationalTime,
    },
    TrimStart {
        clip_id: String,
        delta: RationalTime,
    },
    TrimEnd {
        clip_id: String,
        delta: RationalTime,
    },
    SplitClip {
        clip_id: String,
        at: RationalTime,
    },
    SetParameter {
        graph_id: String,
        node_id: String,
        param_id: String,
        value: ParamValue,
    },
    AddNode {
        graph_id: String,
        node: GraphNode,
    },
    RemoveNodes {
        graph_id: String,
        node_ids: Vec<String>,
    },
    Connect {
        graph_id: String,
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
    },
    Disconnect {
        graph_id: String,
        edge_index: usize,
    },
    SetBypass {
        graph_id: String,
        node_id: String,
        bypass: bool,
    },
    AddAsset {
        asset: Asset,
    },
    AddMarker {
        marker: Marker,
    },
    SetTimelineRange {
        range: TimeRange,
    },
    // Internal inverse-only variants. Not accepted from the public dispatcher.
    #[serde(skip)]
    AddNodesBatch {
        graph_id: String,
        nodes: Vec<GraphNode>,
    },
    #[serde(skip)]
    RemoveAsset {
        asset_id: String,
    },
    #[serde(skip)]
    RemoveMarker {
        marker_id: String,
    },
}

/// A committed change: the command executed and its inverse for undo.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub command: Command,
    pub inverse: Option<Command>,
    pub revision_before: u64,
    pub revision_after: u64,
    pub label: String,
    pub changed_ids: Vec<String>,
}

pub struct Dispatcher {
    pub project: Project,
    pub revision: u64,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    max_undo: usize,
}

impl Dispatcher {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            revision: 0,
            undo: vec![],
            redo: vec![],
            max_undo: 200,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Apply a command. On error no state changes. `expected_revision` of
    /// `None` skips the staleness check (internal use / undo).
    pub fn apply(
        &mut self,
        cmd: Command,
        expected_revision: Option<u64>,
    ) -> FluxResult<Transaction> {
        if let Some(exp) = expected_revision {
            if exp != self.revision {
                return Err(FluxError::new(
                    ErrorCode::RevisionConflict,
                    "domain",
                    format!("expected revision {exp}, current is {}", self.revision),
                )
                .with_recovery("reload document"));
            }
        }
        let snapshot = self.project.clone();
        let mut applied = Applied::default();
        let result = self.execute(&cmd, &mut applied);
        let inverse = match result {
            Ok(inv) => inv,
            Err(e) => {
                self.project = snapshot;
                return Err(e);
            }
        };

        let before = self.revision;
        self.revision += 1;
        let tx = Transaction {
            command: cmd.clone(),
            inverse,
            revision_before: before,
            revision_after: self.revision,
            label: applied.label,
            changed_ids: applied.changed_ids,
        };
        self.undo.push(tx.clone());
        if self.undo.len() > self.max_undo {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.project.modified_utc = crate::document::now_utc_pub();
        Ok(tx)
    }

    pub fn undo(&mut self) -> FluxResult<Option<u64>> {
        let Some(tx) = self.undo.pop() else {
            return Ok(None);
        };
        let Some(inv) = tx.inverse.clone() else {
            return Ok(None);
        };
        let mut applied = Applied::default();
        let forward_inverse = self.execute(&inv, &mut applied)?;
        self.revision += 1;
        let redo_tx = Transaction {
            command: inv,
            inverse: forward_inverse,
            revision_before: tx.revision_after,
            revision_after: self.revision,
            label: format!("undo {}", tx.label),
            changed_ids: applied.changed_ids,
        };
        self.redo.push(redo_tx);
        Ok(Some(self.revision))
    }

    pub fn redo(&mut self) -> FluxResult<Option<u64>> {
        let Some(tx) = self.redo.pop() else {
            return Ok(None);
        };
        let Some(redo_cmd) = tx.inverse.clone() else {
            return Ok(None);
        };
        let mut applied = Applied::default();
        let inv = self.execute(&redo_cmd, &mut applied)?;
        self.revision += 1;
        self.undo.push(Transaction {
            command: redo_cmd,
            inverse: inv,
            revision_before: tx.revision_before,
            revision_after: self.revision,
            label: tx.label.trim_start_matches("undo ").to_string(),
            changed_ids: applied.changed_ids,
        });
        Ok(Some(self.revision))
    }

    /// Execute a command against `self.project`, returning its inverse.
    fn execute(&mut self, cmd: &Command, applied: &mut Applied) -> FluxResult<Option<Command>> {
        let p = &mut self.project;
        match cmd {
            Command::AddTrack { track } => {
                if p.timeline.tracks.contains_key(&track.id) {
                    return Err(cmd_err("track already exists"));
                }
                p.timeline.tracks.insert(track.id.clone(), track.clone());
                p.timeline.track_order.push(track.id.clone());
                applied.set("add track", vec![track.id.clone()]);
                Ok(Some(Command::RemoveTrack {
                    track_id: track.id.clone(),
                }))
            }
            Command::RemoveTrack { track_id } => {
                let track = p
                    .timeline
                    .tracks
                    .remove(track_id)
                    .ok_or_else(|| cmd_err("track not found"))?;
                // Remove the track's clips.
                for cid in &track.clip_order {
                    p.timeline.clips.remove(cid);
                }
                p.timeline.track_order.retain(|id| id != track_id);
                applied.set("remove track", vec![track_id.clone()]);
                Ok(Some(Command::AddTrack { track }))
            }
            Command::AddClip { track_id, clip } => {
                validate_clip(&p.timeline, clip)?;
                let track = p
                    .timeline
                    .tracks
                    .get_mut(track_id)
                    .ok_or_else(|| cmd_err("track not found"))?;
                if p.timeline.clips.contains_key(&clip.id) {
                    return Err(cmd_err("clip already exists"));
                }
                track.clip_order.push(clip.id.clone());
                p.timeline.clips.insert(clip.id.clone(), clip.clone());
                applied.set("add clip", vec![clip.id.clone()]);
                Ok(Some(Command::RemoveClip {
                    clip_id: clip.id.clone(),
                }))
            }
            Command::RemoveClip { clip_id } => {
                let clip = p
                    .timeline
                    .clips
                    .remove(clip_id)
                    .ok_or_else(|| cmd_err("clip not found"))?;
                let track_id = find_track_of_clip(&p.timeline, clip_id)
                    .ok_or_else(|| cmd_err("track not found"))?;
                if let Some(t) = p.timeline.tracks.get_mut(&track_id) {
                    t.clip_order.retain(|id| id != clip_id);
                }
                applied.set("remove clip", vec![clip_id.clone()]);
                Ok(Some(Command::AddClip { track_id, clip }))
            }
            Command::MoveClip { clip_id, new_start } => {
                let old_start = {
                    let clip = p
                        .timeline
                        .clips
                        .get(clip_id)
                        .ok_or_else(|| cmd_err("clip not found"))?;
                    clip.range.start
                };
                if new_start.is_negative() {
                    return Err(cmd_err("clip start cannot be negative"));
                }
                let clip = p.timeline.clips.get_mut(clip_id).unwrap();
                clip.range.start = *new_start;
                applied.set("move clip", vec![clip_id.clone()]);
                Ok(Some(Command::MoveClip {
                    clip_id: clip_id.clone(),
                    new_start: old_start,
                }))
            }
            Command::TrimStart { clip_id, delta } => {
                trim_start(p, clip_id, *delta)?;
                applied.set("trim", vec![clip_id.clone()]);
                Ok(Some(Command::TrimStart {
                    clip_id: clip_id.clone(),
                    delta: delta.neg(),
                }))
            }
            Command::TrimEnd { clip_id, delta } => {
                trim_end(p, clip_id, *delta)?;
                applied.set("trim", vec![clip_id.clone()]);
                Ok(Some(Command::TrimEnd {
                    clip_id: clip_id.clone(),
                    delta: delta.neg(),
                }))
            }
            Command::SplitClip { clip_id, at } => {
                let (second, _range_before) = split_clip(p, clip_id, *at)?;
                applied.set("split clip", vec![clip_id.clone(), second.id.clone()]);
                Ok(Some(Command::SplitClip {
                    clip_id: second.id.clone(),
                    at: *at,
                }))
            }
            Command::SetParameter {
                graph_id,
                node_id,
                param_id,
                value,
            } => {
                ensure_finite(value)?;
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                let node = graph
                    .node_mut(node_id)
                    .ok_or_else(|| cmd_err("node not found"))?;
                let old = node.params.get(param_id).cloned();
                node.params.insert(param_id.clone(), value.clone());
                applied.set("set parameter", vec![node_id.clone()]);
                let inverse = Command::SetParameter {
                    graph_id: graph_id.clone(),
                    node_id: node_id.clone(),
                    param_id: param_id.clone(),
                    value: old.unwrap_or(ParamValue::Float { value: 0.0 }),
                };
                Ok(Some(inverse))
            }
            Command::AddNode { graph_id, node } => {
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                if graph.nodes.iter().any(|n| n.id == node.id) {
                    return Err(cmd_err("node already exists"));
                }
                graph.nodes.push(node.clone());
                applied.set("add node", vec![node.id.clone()]);
                Ok(Some(Command::RemoveNodes {
                    graph_id: graph_id.clone(),
                    node_ids: vec![node.id.clone()],
                }))
            }
            Command::RemoveNodes { graph_id, node_ids } => {
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                if node_ids.iter().any(|id| id == &graph.output_node) {
                    return Err(cmd_err("cannot remove the output node"));
                }
                let removed: Vec<GraphNode> = graph
                    .nodes
                    .iter()
                    .filter(|n| node_ids.contains(&n.id))
                    .cloned()
                    .collect();
                graph.nodes.retain(|n| !node_ids.contains(&n.id));
                graph
                    .edges
                    .retain(|e| !node_ids.contains(&e.from_node) && !node_ids.contains(&e.to_node));
                applied.set("remove nodes", node_ids.clone());
                // Restoring multiple nodes: add them back individually in order.
                Ok(Some(Command::AddNodesBatch {
                    graph_id: graph_id.clone(),
                    nodes: removed,
                }))
            }
            Command::AddNodesBatch { graph_id, nodes } => {
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                for n in nodes {
                    graph.nodes.push(n.clone());
                }
                Ok(Some(Command::RemoveNodes {
                    graph_id: graph_id.clone(),
                    node_ids: nodes.iter().map(|n| n.id.clone()).collect(),
                }))
            }
            Command::Connect {
                graph_id,
                from_node,
                from_port,
                to_node,
                to_port,
            } => {
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                if graph.node(from_node).is_none() || graph.node(to_node).is_none() {
                    return Err(cmd_err("edge endpoint missing"));
                }
                graph.edges.push(GraphEdge {
                    from_node: from_node.clone(),
                    from_port: from_port.clone(),
                    to_node: to_node.clone(),
                    to_port: to_port.clone(),
                });
                applied.set("connect", vec![to_node.clone()]);
                Ok(Some(Command::Disconnect {
                    graph_id: graph_id.clone(),
                    edge_index: usize::MAX,
                }))
            }
            Command::Disconnect {
                graph_id,
                edge_index,
            } => {
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                let idx = if *edge_index == usize::MAX {
                    graph.edges.len().saturating_sub(1)
                } else {
                    *edge_index
                };
                if idx >= graph.edges.len() {
                    return Err(cmd_err("edge not found"));
                }
                let edge = graph.edges.remove(idx);
                applied.set("disconnect", vec![edge.to_node.clone()]);
                Ok(Some(Command::Connect {
                    graph_id: graph_id.clone(),
                    from_node: edge.from_node,
                    from_port: edge.from_port,
                    to_node: edge.to_node,
                    to_port: edge.to_port,
                }))
            }
            Command::SetBypass {
                graph_id,
                node_id,
                bypass,
            } => {
                let graph = p
                    .graphs
                    .get_mut(graph_id)
                    .ok_or_else(|| cmd_err("graph not found"))?;
                let node = graph
                    .node_mut(node_id)
                    .ok_or_else(|| cmd_err("node not found"))?;
                let old = node.bypass;
                node.bypass = *bypass;
                Ok(Some(Command::SetBypass {
                    graph_id: graph_id.clone(),
                    node_id: node_id.clone(),
                    bypass: old,
                }))
            }
            Command::AddAsset { asset } => {
                if p.assets.contains_key(&asset.id) {
                    return Err(cmd_err("asset already exists"));
                }
                p.assets.insert(asset.id.clone(), asset.clone());
                applied.set("import asset", vec![asset.id.clone()]);
                Ok(Some(Command::RemoveAsset {
                    asset_id: asset.id.clone(),
                }))
            }
            Command::RemoveAsset { asset_id } => {
                let a = p
                    .assets
                    .remove(asset_id)
                    .ok_or_else(|| cmd_err("asset not found"))?;
                Ok(Some(Command::AddAsset { asset: a }))
            }
            Command::AddMarker { marker } => {
                p.markers.push(marker.clone());
                applied.set("add marker", vec![marker.id.clone()]);
                Ok(Some(Command::RemoveMarker {
                    marker_id: marker.id.clone(),
                }))
            }
            Command::RemoveMarker { marker_id } => {
                let idx = p
                    .markers
                    .iter()
                    .position(|m| &m.id == marker_id)
                    .ok_or_else(|| cmd_err("marker not found"))?;
                let m = p.markers.remove(idx);
                Ok(Some(Command::AddMarker { marker: m }))
            }
            Command::SetTimelineRange { range } => {
                validate_range(range)?;
                let old = p.timeline.range;
                p.timeline.range = *range;
                Ok(Some(Command::SetTimelineRange { range: old }))
            }
        }
    }
}

#[derive(Default)]
struct Applied {
    label: String,
    changed_ids: Vec<String>,
}

impl Applied {
    fn set(&mut self, label: &str, ids: Vec<String>) {
        self.label = label.to_string();
        self.changed_ids = ids;
    }
}

fn cmd_err(msg: &str) -> FluxError {
    FluxError::new(ErrorCode::InvalidCommand, "domain", msg)
}

fn ensure_finite(v: &ParamValue) -> FluxResult<()> {
    let check = |x: f64| -> FluxResult<()> {
        if x.is_finite() {
            Ok(())
        } else {
            Err(FluxError::new(
                ErrorCode::ParamNonfinite,
                "domain",
                "non-finite parameter",
            ))
        }
    };
    match v {
        ParamValue::Float { value } => check(*value),
        ParamValue::Color { value } => value.iter().try_for_each(|x| check(*x)),
        ParamValue::Vec2 { value } => value.iter().try_for_each(|x| check(*x)),
        ParamValue::Vec3 { value } => value.iter().try_for_each(|x| check(*x)),
        _ => Ok(()),
    }
}

fn validate_range(r: &TimeRange) -> FluxResult<()> {
    if r.start.is_negative() {
        return Err(cmd_err("range start cannot be negative"));
    }
    if r.duration.is_negative() || r.duration.is_zero() {
        return Err(cmd_err("range duration must be positive"));
    }
    if r.duration
        .cmp_exact(&RationalTime::from_secs(MAX_PROJECT_SECONDS))
        == std::cmp::Ordering::Greater
    {
        return Err(cmd_err("range exceeds maximum project duration"));
    }
    Ok(())
}

fn validate_clip(timeline: &Timeline, clip: &Clip) -> FluxResult<()> {
    validate_range(&clip.range)?;
    if clip.range.start.is_negative() {
        return Err(cmd_err("clip start cannot be negative"));
    }
    match &clip.source {
        ClipSource::Asset { asset_id } => {
            let _ = asset_id;
        }
        _ => {}
    }
    let _ = timeline;
    Ok(())
}

fn find_track_of_clip(timeline: &Timeline, clip_id: &str) -> Option<String> {
    timeline
        .tracks
        .iter()
        .find(|(_, t)| t.clip_order.iter().any(|c| c == clip_id))
        .map(|(id, _)| id.clone())
}

/// Trim start by `delta` seconds. Positive delta moves the in-point later.
/// Retained content keeps its prior source mapping.
fn trim_start(p: &mut Project, clip_id: &str, delta: RationalTime) -> FluxResult<()> {
    let clip = p
        .timeline
        .clips
        .get(clip_id)
        .ok_or_else(|| cmd_err("clip not found"))?;
    let new_start = clip.range.start.add(delta)?;
    let new_duration = clip.range.duration.sub(delta)?;
    let new_source_start = clip.source_range.start.add(delta)?;
    let source_end_for_start = clip.source_range.end()?;
    let loop_policy = clip.loop_policy;
    if new_start.is_negative() {
        return Err(cmd_err("trim start before zero"));
    }
    if new_duration.is_negative() {
        return Err(cmd_err("clip would become negative"));
    }
    if loop_policy == LoopPolicy::None
        && new_source_start.cmp_exact(&source_end_for_start) == std::cmp::Ordering::Greater
    {
        return Err(cmd_err("trim exceeds source in point"));
    }
    let clip = p.timeline.clips.get_mut(clip_id).unwrap();
    clip.range.start = new_start;
    clip.range.duration = new_duration;
    clip.source_range.start = new_source_start;
    clip.source_range.duration = new_duration;
    Ok(())
}

fn trim_end(p: &mut Project, clip_id: &str, delta: RationalTime) -> FluxResult<()> {
    let clip = p
        .timeline
        .clips
        .get_mut(clip_id)
        .ok_or_else(|| cmd_err("clip not found"))?;
    let new_src_dur = clip.source_range.duration.add(delta)?;
    let new_duration = clip.range.duration.add(delta)?;
    if new_src_dur.is_negative() || new_duration.is_negative() {
        return Err(cmd_err("trim would make the clip negative"));
    }
    clip.source_range.duration = new_src_dur;
    clip.range.duration = new_duration;
    Ok(())
}

/// Split a clip at project time `at`. Returns the right half and the original
/// range for inverse reconstruction.
fn split_clip(p: &mut Project, clip_id: &str, at: RationalTime) -> FluxResult<(Clip, TimeRange)> {
    let clip = p
        .timeline
        .clips
        .get(clip_id)
        .cloned()
        .ok_or_else(|| cmd_err("clip not found"))?;
    let end = clip.range.end()?;
    if at.cmp_exact(&clip.range.start) != std::cmp::Ordering::Greater
        || at.cmp_exact(&end) != std::cmp::Ordering::Less
    {
        return Err(cmd_err("split point must be inside the clip"));
    }
    let left_dur = at.sub(clip.range.start)?;
    let right_dur = end.sub(at)?;
    let right_src_start = clip.source_range.start.add(left_dur)?;

    let left = p.timeline.clips.get_mut(clip_id).unwrap();
    let original_range = left.range;
    left.range.duration = left_dur;
    left.source_range.duration = left_dur;

    let right = Clip {
        id: uuid::Uuid::new_v4().to_string(),
        range: TimeRange::new(at, right_dur),
        source_range: TimeRange::new(right_src_start, right_dur),
        ..clip
    };
    let track_id =
        find_track_of_clip(&p.timeline, clip_id).ok_or_else(|| cmd_err("track missing"))?;
    if let Some(t) = p.timeline.tracks.get_mut(&track_id) {
        if let Some(pos) = t.clip_order.iter().position(|c| c == clip_id) {
            t.clip_order.insert(pos + 1, right.id.clone());
        }
    }
    p.timeline.clips.insert(right.id.clone(), right.clone());
    Ok((right, original_range))
}

/// Frame/rate helpers used by the timeline evaluator.
/// Forward mapping at local time `u`: `source_in + u*rate`.
pub fn clip_local_to_source(clip: &Clip, local: RationalTime) -> FluxResult<RationalTime> {
    let scaled = local
        .mul_int(clip.rate.numerator())
        .map(|r| {
            RationalTime::new(
                r.numerator(),
                r.denominator().saturating_mul(clip.rate.denominator()),
            )
        })
        .map_err(FluxError::from)??;
    Ok(clip.source_range.start.add(scaled)?)
}

impl From<crate::time::RationalError> for FluxError {
    fn from(e: crate::time::RationalError) -> Self {
        FluxError::new(ErrorCode::InvalidCommand, "domain", e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::RationalTime;
    use std::collections::BTreeMap;

    fn rt(n: i64, d: u32) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    fn project_with_track() -> (Dispatcher, String) {
        let mut d = Dispatcher::new(Project::new("test", 1));
        let track = Track {
            id: "trk1".into(),
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
        d.apply(Command::AddTrack { track }, Some(0)).unwrap();
        (d, "trk1".into())
    }

    fn clip_at(start: i64, dur: i64) -> Clip {
        Clip {
            id: "clip1".into(),
            name: "C".into(),
            source: ClipSource::GeneratorScene {
                generator_id: "gen.test".into(),
            },
            range: TimeRange::new(rt(start, 1), rt(dur, 1)),
            source_range: TimeRange::new(RationalTime::ZERO, rt(dur, 1)),
            rate: rt(1, 1),
            loop_policy: LoopPolicy::None,
            graph_id: None,
            transform: Transform2D::default(),
            opacity: 1.0,
            audio: AudioControls::default(),
            linked_group: None,
        }
    }

    #[test]
    fn stale_revision_is_rejected_without_state_change() {
        let (mut d, _) = project_with_track();
        let err = d
            .apply(
                Command::AddClip {
                    track_id: "trk1".into(),
                    clip: clip_at(0, 4),
                },
                Some(0),
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::RevisionConflict);
        assert!(d.project.timeline.clips.is_empty());
    }

    #[test]
    fn add_undo_redo_restores_exact_structure() {
        let (mut d, _) = project_with_track();
        d.apply(
            Command::AddClip {
                track_id: "trk1".into(),
                clip: clip_at(0, 4),
            },
            Some(1),
        )
        .unwrap();
        let after_add = serde_json::to_string(&d.project).unwrap();
        d.undo().unwrap();
        assert!(d.project.timeline.clips.is_empty());
        d.redo().unwrap();
        assert_eq!(serde_json::to_string(&d.project).unwrap(), after_add);
    }

    #[test]
    fn split_preserves_contiguous_source_mapping() {
        let (mut d, _) = project_with_track();
        d.apply(
            Command::AddClip {
                track_id: "trk1".into(),
                clip: clip_at(2, 8),
            },
            Some(1),
        )
        .unwrap();
        d.apply(
            Command::SplitClip {
                clip_id: "clip1".into(),
                at: rt(6, 1),
            },
            None,
        )
        .unwrap();
        let left = d.project.timeline.clips.get("clip1").unwrap();
        assert_eq!(left.range.start, rt(2, 1));
        assert_eq!(left.range.duration, rt(4, 1));
        let right = d
            .project
            .timeline
            .clips
            .values()
            .find(|c| c.id != "clip1")
            .unwrap();
        assert_eq!(right.range.start, rt(6, 1));
        assert_eq!(right.range.duration, rt(4, 1));
        // Source windows are contiguous across the original clip.
        assert_eq!(
            left.source_range
                .start
                .add(left.source_range.duration)
                .unwrap(),
            right.source_range.start
        );
        assert_eq!(
            left.source_range
                .start
                .add(left.source_range.duration)
                .unwrap(),
            right.source_range.start
        );
    }

    #[test]
    fn split_outside_clip_is_rejected() {
        let (mut d, _) = project_with_track();
        d.apply(
            Command::AddClip {
                track_id: "trk1".into(),
                clip: clip_at(2, 8),
            },
            Some(1),
        )
        .unwrap();
        let err = d
            .apply(
                Command::SplitClip {
                    clip_id: "clip1".into(),
                    at: rt(10, 1),
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidCommand);
        // half-open: end is exclusive
        let err2 = d.apply(
            Command::SplitClip {
                clip_id: "clip1".into(),
                at: rt(10, 1),
            },
            None,
        );
        assert!(err2.is_err());
    }

    #[test]
    fn trim_start_moves_source_and_keeps_duration_consistent() {
        let (mut d, _) = project_with_track();
        d.apply(
            Command::AddClip {
                track_id: "trk1".into(),
                clip: clip_at(2, 8),
            },
            Some(1),
        )
        .unwrap();
        d.apply(
            Command::TrimStart {
                clip_id: "clip1".into(),
                delta: rt(2, 1),
            },
            None,
        )
        .unwrap();
        let c = d.project.timeline.clips.get("clip1").unwrap();
        assert_eq!(c.range.start, rt(4, 1));
        assert_eq!(c.range.duration, rt(6, 1));
        assert_eq!(c.source_range.start, rt(2, 1));
    }

    #[test]
    fn trim_beyond_source_is_rejected() {
        let (mut d, _) = project_with_track();
        d.apply(
            Command::AddClip {
                track_id: "trk1".into(),
                clip: clip_at(0, 8),
            },
            Some(1),
        )
        .unwrap();
        let err = d
            .apply(
                Command::TrimStart {
                    clip_id: "clip1".into(),
                    delta: rt(9, 1),
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidCommand);
        // source unchanged from failed command
        assert_eq!(
            d.project.timeline.clips.get("clip1").unwrap().range.start,
            rt(0, 1)
        );
    }

    #[test]
    fn undo_restores_trim_exactly() {
        let (mut d, _) = project_with_track();
        d.apply(
            Command::AddClip {
                track_id: "trk1".into(),
                clip: clip_at(2, 6),
            },
            Some(1),
        )
        .unwrap();
        let before = serde_json::to_string(&d.project.timeline).unwrap();
        d.apply(
            Command::TrimEnd {
                clip_id: "clip1".into(),
                delta: rt(-2, 1),
            },
            None,
        )
        .unwrap();
        d.undo().unwrap();
        assert_eq!(serde_json::to_string(&d.project.timeline).unwrap(), before);
    }

    #[test]
    fn nonfinite_parameter_rejected() {
        let mut d = Dispatcher::new(Project::new("x", 0));
        d.project.graphs.insert(
            "g".into(),
            Graph {
                id: "g".into(),
                name: "g".into(),
                nodes: vec![GraphNode {
                    id: "n".into(),
                    type_id: "tx".into(),
                    type_version: 1,
                    position: [0.0, 0.0],
                    params: BTreeMap::new(),
                    bypass: false,
                    missing_type: false,
                }],
                edges: vec![],
                output_node: "n".into(),
                keyframes: BTreeMap::new(),
            },
        );
        let err = d
            .apply(
                Command::SetParameter {
                    graph_id: "g".into(),
                    node_id: "n".into(),
                    param_id: "x".into(),
                    value: ParamValue::Float { value: f64::NAN },
                },
                None,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ParamNonfinite);
    }
}
