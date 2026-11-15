//! Live control bridge: map OSC/MIDI controls to engine commands, with a learned
//! binding table, value scaling, and rate safety. This is the wiring that makes
//! live controllers edit the authoritative document (not a parallel state).

use crate::commands::CommandHost;
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError};
use flux_domain::{Command, Dispatcher, FluxResult, ParamValue};
use flux_integrations::midi::MidiControl;
use flux_integrations::osc::{OscControl, OscReceiver};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One control-surface frame window: a 60 Hz fader stream becomes one document
/// edit per parameter per window.
const FLUSH_WINDOW: Duration = Duration::from_millis(16);
/// Depth of the channel between the socket thread and this loop. Beyond it the
/// receiver counts a drop rather than growing memory.
const CONTROL_QUEUE: usize = 256;

/// A learned binding from a control source to a node parameter.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ControlBinding {
    /// OSC address path → node/param.
    Osc {
        address: String,
        node_id: String,
        param_id: String,
    },
    /// MIDI CC (channel+cc) → node/param.
    MidiCc {
        channel: u8,
        cc: u8,
        node_id: String,
        param_id: String,
    },
    /// MIDI note on (channel+note) → transport-independent trigger.
    MidiNote {
        channel: u8,
        note: u8,
        node_id: String,
        param_id: String,
    },
}

/// The control map: bindings + a target graph id.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ControlMap {
    pub graph_id: String,
    pub bindings: Vec<ControlBinding>,
}

impl ControlMap {
    pub fn learn_osc(
        &mut self,
        address: impl Into<String>,
        node: impl Into<String>,
        param: impl Into<String>,
    ) {
        self.bindings.push(ControlBinding::Osc {
            address: address.into(),
            node_id: node.into(),
            param_id: param.into(),
        });
    }

    pub fn learn_cc(
        &mut self,
        channel: u8,
        cc: u8,
        node: impl Into<String>,
        param: impl Into<String>,
    ) {
        self.bindings.push(ControlBinding::MidiCc {
            channel,
            cc,
            node_id: node.into(),
            param_id: param.into(),
        });
    }
}

/// Result of applying one control event.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    Parameter {
        node: String,
        param: String,
        value: f32,
    },
    Unmapped,
    Transport,
}

/// Translate an OSC control into a parameter command using the binding table.
pub fn apply_osc(map: &ControlMap, ctrl: &OscControl) -> Option<(String, String, f32)> {
    match ctrl {
        OscControl::Param { node, param, value } => {
            if is_allowed(map, node, param) {
                Some((node.clone(), param.clone(), *value as f32))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether a live address may edit this node/param pair.
///
/// A learned binding table is a whitelist. An empty table accepts everything, so
/// a fader works before anyone has pressed Learn; a table with one entry restricts
/// the surface to exactly what was learned.
pub fn is_allowed(map: &ControlMap, node: &str, param: &str) -> bool {
    if map.bindings.is_empty() {
        return true;
    }
    map.bindings.iter().any(|b| {
        matches!(b, ControlBinding::Osc { node_id, param_id, .. } if node_id == node && param_id == param)
    })
}

/// Translate a MIDI control into a parameter command using the binding table.
pub fn apply_midi(map: &ControlMap, ctrl: &MidiControl) -> Option<(String, String, f32)> {
    match ctrl {
        MidiControl::Cc { channel, cc, value } => map.bindings.iter().find_map(|b| match b {
            ControlBinding::MidiCc {
                channel: c,
                cc: k,
                node_id,
                param_id,
            } if c == channel && k == cc => {
                Some((node_id.clone(), param_id.clone(), *value as f32))
            }
            _ => None,
        }),
        MidiControl::NoteOn {
            channel,
            note,
            velocity,
        } => map.bindings.iter().find_map(|b| match b {
            ControlBinding::MidiNote {
                channel: c,
                note: n,
                node_id,
                param_id,
            } if c == channel && n == note => {
                Some((node_id.clone(), param_id.clone(), *velocity as f32))
            }
            _ => None,
        }),
        _ => None,
    }
}

/// Apply a resolved (node,param,value) to the document as a SetParameter command.
/// Missing nodes are ignored (stale binding) rather than erroring the control path.
pub fn apply_to_dispatcher(
    dispatcher: &mut Dispatcher,
    graph_id: &str,
    node: &str,
    param: &str,
    value: f32,
) -> FluxResult<bool> {
    if dispatcher
        .project
        .graphs
        .get(graph_id)
        .and_then(|g| g.node(node))
        .is_none()
    {
        return Ok(false);
    }
    dispatcher.apply(
        Command::SetParameter {
            graph_id: graph_id.to_string(),
            node_id: node.to_string(),
            param_id: param.to_string(),
            value: ParamValue::Float {
                value: value as f64,
            },
        },
        None,
    )?;
    Ok(true)
}

/// Latest-value coalescing store so a burst of control updates yields one edit
/// per parameter per frame (avoids command spam from 60 Hz CC streams).
#[derive(Default)]
pub struct Coalescer {
    pending: HashMap<(String, String), f32>,
    pub applied_last: usize,
}

impl Coalescer {
    pub fn push(&mut self, node: impl Into<String>, param: impl Into<String>, value: f32) {
        self.pending.insert((node.into(), param.into()), value);
    }

    /// Drain pending updates into `dispatcher` for `graph_id`, returning count.
    pub fn flush(&mut self, dispatcher: &mut Dispatcher, graph_id: &str) -> usize {
        let drained: Vec<(String, String, f32)> =
            self.pending.drain().map(|((n, p), v)| (n, p, v)).collect();
        let mut n = 0;
        for (node, param, value) in drained {
            if apply_to_dispatcher(dispatcher, graph_id, &node, &param, value).unwrap_or(false) {
                n += 1;
            }
        }
        self.applied_last = n;
        n
    }
}

/// What the engine did with one received control.
#[derive(Debug, Clone, PartialEq)]
pub enum OscApplied {
    /// A parameter edit landed in the document.
    Parameter {
        node: String,
        param: String,
        value: f32,
    },
    /// Filtered by the binding table, or the graph/node is not in the document.
    Unmapped,
    /// The document rejected the command.
    Rejected(String),
    /// Decoded correctly, but nothing in the engine can act on it yet. The string
    /// is the reason, and `osc_status` reports it verbatim.
    NotApplied(&'static str),
}

/// Counters for one live-control listener. Every control that arrives lands in
/// exactly one of them, so "this fader does nothing" is answerable from numbers
/// instead of from what the operator can see on screen.
#[derive(Debug, Default)]
pub struct OscStats {
    received: AtomicU64,
    applied: AtomicU64,
    unmapped: AtomicU64,
    rejected: AtomicU64,
    not_applied: AtomicU64,
    bridge: AtomicU64,
    flushes: AtomicU64,
    last_reason: Mutex<String>,
    /// Shared with the socket thread: rate-limiter and full-queue drops.
    dropped: Arc<AtomicU64>,
}

impl OscStats {
    fn bumped(dropped: Arc<AtomicU64>) -> Self {
        Self {
            dropped,
            ..Default::default()
        }
    }

    fn note(&self, reason: &str) {
        if let Ok(mut g) = self.last_reason.lock() {
            *g = reason.to_string();
        }
    }

    fn record(&self, outcome: &OscApplied) {
        match outcome {
            OscApplied::Parameter { .. } => self.applied.fetch_add(1, Ordering::Relaxed),
            OscApplied::Unmapped => self.unmapped.fetch_add(1, Ordering::Relaxed),
            OscApplied::Rejected(msg) => {
                self.note(msg);
                self.rejected.fetch_add(1, Ordering::Relaxed)
            }
            OscApplied::NotApplied(why) => {
                self.note(why);
                self.not_applied.fetch_add(1, Ordering::Relaxed)
            }
        };
    }

    /// Everything the listener knows, for the status surface and for evidence.
    pub fn to_json(&self) -> serde_json::Value {
        let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
        serde_json::json!({
            "received": g(&self.received),
            "applied": g(&self.applied),
            "unmapped": g(&self.unmapped),
            "rejected": g(&self.rejected),
            "notApplied": g(&self.not_applied),
            "bridge": g(&self.bridge),
            "flushes": g(&self.flushes),
            "droppedByReceiver": g(&self.dropped),
            "lastReason": self.last_reason.lock().map(|s| s.clone()).unwrap_or_default(),
        })
    }
}

/// The receive half of live control: an OSC listener that edits the
/// authoritative document. It belongs to whatever process owns that document —
/// here the desktop app — and never to the webview.
pub struct OscBridge {
    port: u16,
    stats: Arc<OscStats>,
    stop: Arc<AtomicBool>,
    socket_thread: Option<std::thread::JoinHandle<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl OscBridge {
    /// Bind `port` and route controls into `host`.
    ///
    /// A bind failure is returned rather than logged and forgotten: the port has
    /// one owner and the `flux osc` command uses the same default, so "something
    /// else already holds it" is a fact the caller has to be able to report.
    ///
    /// `on_flush(params, revision)` runs on the listener thread, at most once per
    /// frame window and only when something actually landed, so a 1 kHz stream
    /// cannot turn into 1 kHz of IPC.
    pub fn start(
        port: u16,
        host: Arc<Mutex<CommandHost>>,
        on_flush: impl FnMut(&[String], u64) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (tx, rx) = bounded(CONTROL_QUEUE);
        let (receiver, socket_thread) = OscReceiver::start(port, tx)?;
        let stats = Arc::new(OscStats::bumped(receiver.dropped.clone()));
        let stop = Arc::new(AtomicBool::new(false));
        let (stats2, stop2) = (stats.clone(), stop.clone());
        // Moving `receiver` into the worker is what stops the socket thread: its
        // Drop flips the receiver's own stop flag when this loop exits.
        let worker = std::thread::spawn(move || {
            let _receiver = receiver;
            run_listener(rx, host, stats2, stop2, on_flush);
        });
        Ok(Self {
            port,
            stats,
            stop,
            socket_thread: Some(socket_thread),
            worker: Some(worker),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stats(&self) -> serde_json::Value {
        self.stats.to_json()
    }

    /// Stop both threads and report the final counters.
    pub fn stop(mut self) -> serde_json::Value {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        if let Some(s) = self.socket_thread.take() {
            let _ = s.join();
        }
        self.stats.to_json()
    }
}

impl Drop for OscBridge {
    fn drop(&mut self) {
        // Ask for shutdown but do not block on it: an app exiting through the
        // window close path must not wait on a socket thread.
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run_listener(
    rx: Receiver<OscControl>,
    host: Arc<Mutex<CommandHost>>,
    stats: Arc<OscStats>,
    stop: Arc<AtomicBool>,
    mut on_flush: impl FnMut(&[String], u64),
) {
    let mut pending: HashMap<(String, String), f32> = HashMap::new();
    let mut window = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let ctrl = match rx.recv_timeout(FLUSH_WINDOW) {
            Ok(ctrl) => ctrl,
            Err(RecvTimeoutError::Timeout) => {
                // No control arrived: fall through to the flush check below. A
                // burst followed by silence still has to land — waiting for the
                // next packet to trigger the flush would strand the last value
                // of every fader move until the operator touched another one.
                if !pending.is_empty() {
                    flush_pending(&mut pending, &host, &stats, &mut on_flush);
                    window = Instant::now();
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        stats.received.fetch_add(1, Ordering::Relaxed);
        match &ctrl {
            OscControl::Param { node, param, value } => {
                pending.insert((node.clone(), param.clone()), *value as f32);
            }
            // Everything else edits nothing, but it is counted and explained the
            // moment it arrives rather than queued: these are single-shot kinds,
            // and coalescing a trigger would lose edges.
            OscControl::Bridge(ev) => {
                stats.bridge.fetch_add(1, Ordering::Relaxed);
                let kind = match ev {
                    flux_integrations::bridge::BridgeEvent::Hello { .. } => "hello",
                    flux_integrations::bridge::BridgeEvent::Features { .. } => "features",
                    flux_integrations::bridge::BridgeEvent::Transport { .. } => "transport",
                    flux_integrations::bridge::BridgeEvent::Heartbeat { .. } => "heartbeat",
                };
                stats.note(&format!(
                    "bridge {kind} packet decoded; features not fed to the analyser"
                ));
                let outcome = host
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .apply_live_control(&ctrl);
                stats.record(&outcome);
            }
            other => {
                let outcome = host
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .apply_live_control(other);
                stats.record(&outcome);
            }
        }
        if window.elapsed() >= FLUSH_WINDOW && !pending.is_empty() {
            flush_pending(&mut pending, &host, &stats, &mut on_flush);
            window = Instant::now();
        }
    }
    // Do not lose the last edit because the app decided to exit.
    if !pending.is_empty() {
        flush_pending(&mut pending, &host, &stats, &mut on_flush);
    }
}

/// Drain the latest value per parameter into the document as one batch, so a
/// burst produces one undo entry per parameter per window.
fn flush_pending(
    pending: &mut HashMap<(String, String), f32>,
    host: &Arc<Mutex<CommandHost>>,
    stats: &Arc<OscStats>,
    on_flush: &mut impl FnMut(&[String], u64),
) {
    let batch: Vec<((String, String), f32)> = pending.drain().collect();
    if batch.is_empty() {
        return;
    }
    let mut changed = Vec::with_capacity(batch.len());
    // A poisoned lock means another thread panicked while holding the document.
    // Stopping the listener there would hide that behind "the fader does
    // nothing", so take the data and keep going.
    let mut h = host.lock().unwrap_or_else(|p| p.into_inner());
    for ((node, param), value) in batch {
        let outcome = h.apply_live_param(&node, &param, value);
        stats.record(&outcome);
        if let OscApplied::Parameter { param, .. } = outcome {
            changed.push(param);
        }
    }
    let revision = h.revision();
    drop(h);
    stats.flushes.fetch_add(1, Ordering::Relaxed);
    if !changed.is_empty() {
        on_flush(&changed, revision);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    #[test]
    fn osc_param_applies_when_bound_or_empty() {
        let ctrl = OscControl::Param {
            node: "n".into(),
            param: "p".into(),
            value: 0.5,
        };
        let empty = ControlMap::default();
        assert_eq!(
            apply_osc(&empty, &ctrl),
            Some(("n".into(), "p".into(), 0.5))
        );
        let mut map = ControlMap::default();
        map.learn_osc("/x", "n", "p");
        assert!(apply_osc(&map, &ctrl).is_some());
        let other = OscControl::Param {
            node: "z".into(),
            param: "q".into(),
            value: 0.1,
        };
        assert!(apply_osc(&map, &other).is_none());
    }

    #[test]
    fn midi_cc_and_note_binding() {
        let mut map = ControlMap::default();
        map.learn_cc(0, 7, "n1", "radius");
        assert_eq!(
            apply_midi(
                &map,
                &MidiControl::Cc {
                    channel: 0,
                    cc: 7,
                    value: 0.8
                }
            ),
            Some(("n1".into(), "radius".into(), 0.8))
        );
        assert!(apply_midi(
            &map,
            &MidiControl::Cc {
                channel: 0,
                cc: 8,
                value: 0.8
            }
        )
        .is_none());
    }

    #[test]
    fn applies_parameter_to_real_node() {
        let proj = samples::demo_generator_project("gen.plasma", &["fx.bloom"], 2);
        let mut d = Dispatcher::new(proj);
        let graph_id = d.project.graphs.keys().next().cloned().unwrap();
        // Seed a float param on a real node (params are optional overrides).
        let node_id = {
            let g = d.project.graphs.get_mut(&graph_id).unwrap();
            let n = g.nodes.first_mut().expect("a node");
            n.params
                .insert("intensity".into(), ParamValue::Float { value: 0.5 });
            n.id.clone()
        };
        assert!(apply_to_dispatcher(&mut d, &graph_id, &node_id, "intensity", 0.42).unwrap());
        let stored =
            d.project.graphs[&graph_id].node(&node_id).unwrap().params["intensity"].clone();
        match stored {
            ParamValue::Float { value } => assert!((value - 0.42).abs() < 1e-5),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn coalescer_merges_duplicates() {
        let proj = samples::demo_generator_project("gen.plasma", &[], 1);
        let mut d = Dispatcher::new(proj);
        let graph_id = d.project.graphs.keys().next().cloned().unwrap();
        let node_id = {
            let g = d.project.graphs.get_mut(&graph_id).unwrap();
            let n = g.nodes.first_mut().unwrap();
            n.params
                .insert("intensity".into(), ParamValue::Float { value: 0.0 });
            n.id.clone()
        };
        let mut c = Coalescer::default();
        for i in 0..10 {
            c.push(node_id.clone(), "intensity".to_string(), i as f32 / 10.0);
        }
        let applied = c.flush(&mut d, &graph_id);
        assert_eq!(applied, 1, "10 pushes coalesce into one edit");
    }

    #[test]
    fn stale_binding_ignored() {
        let proj = samples::demo_generator_project("gen.plasma", &[], 1);
        let mut d = Dispatcher::new(proj);
        let graph_id = d.project.graphs.keys().next().cloned().unwrap();
        assert!(!apply_to_dispatcher(&mut d, &graph_id, "nonexistent", "x", 0.5).unwrap());
    }

    // ---- the receive bridge, driven by real bytes on a real socket ------------

    fn live_host(param: &str) -> (Arc<Mutex<CommandHost>>, String, String) {
        let mut project = samples::demo_generator_project("gen.plasma", &[], 1);
        let graph_id = project
            .graphs
            .keys()
            .next()
            .expect("sample project has a graph")
            .clone();
        let node_id = {
            let g = project.graphs.get_mut(&graph_id).unwrap();
            let n = g.nodes.first_mut().expect("sample graph has a node");
            n.params
                .insert(param.into(), ParamValue::Float { value: 0.25 });
            n.id.clone()
        };
        let mut host = CommandHost::new(project);
        host.active_graph = graph_id.clone();
        (Arc::new(Mutex::new(host)), graph_id, node_id)
    }

    fn read_param(host: &Arc<Mutex<CommandHost>>, graph: &str, node: &str, param: &str) -> f64 {
        let h = host.lock().unwrap();
        let v = h.dispatcher.project.graphs[graph]
            .node(node)
            .unwrap()
            .params
            .get(param)
            .cloned()
            .unwrap();
        match v {
            ParamValue::Float { value } => value,
            other => panic!("expected a float override, got {other:?}"),
        }
    }

    fn send(port: u16, bytes: &[u8]) {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("client socket");
        sock.send_to(bytes, format!("127.0.0.1:{port}"))
            .expect("send_to");
    }

    /// Poll until `ready` or give up. The listener works on its own thread, so a
    /// receive test asserts on what eventually happened rather than on a sleep.
    fn wait_until(mut ready: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if ready() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn osc_bytes_reach_the_document() {
        let (host, graph, node) = live_host("intensity");
        let events = Arc::new(Mutex::new(Vec::<(Vec<String>, u64)>::new()));
        let sink = events.clone();
        let bridge = OscBridge::start(19011, host.clone(), move |params, revision| {
            sink.lock().unwrap().push((params.to_vec(), revision));
        })
        .expect("bind 19011");

        send(
            19011,
            &flux_integrations::osc::encode_param_feedback("p1", &node, "intensity", 0.8),
        );

        assert!(
            wait_until(|| read_param(&host, &graph, &node, "intensity") > 0.79),
            "a parameter fader in Live must change the authoritative document"
        );
        assert!(host.lock().unwrap().revision() > 0, "the edit is undoable");
        assert!(
            wait_until(|| !events.lock().unwrap().is_empty()),
            "the UI must be told an external edit happened"
        );
        let stats = bridge.stop();
        assert_eq!(stats["received"], 1, "one control arrived");
        assert_eq!(stats["applied"], 1, "and it landed");
        assert_eq!(stats["unmapped"], 0);
        assert_eq!(stats["droppedByReceiver"], 0);
    }

    #[test]
    fn a_burst_coalesces_to_one_edit_per_parameter() {
        let (host, graph, node) = live_host("intensity");
        let bridge = OscBridge::start(19012, host.clone(), |_, _| {}).expect("bind 19012");
        for i in 1..=10 {
            send(
                19012,
                &flux_integrations::osc::encode_param_feedback(
                    "p1",
                    &node,
                    "intensity",
                    i as f32 / 20.0,
                ),
            );
        }
        assert!(
            wait_until(|| read_param(&host, &graph, &node, "intensity") > 0.44),
            "the last value of the burst should win"
        );
        // Give the listener several flush windows, then look at the accounting.
        std::thread::sleep(Duration::from_millis(120));
        let stats = bridge.stats();
        assert_eq!(stats["received"], 10, "every packet was accepted");
        assert_eq!(
            stats["applied"], 1,
            "ten updates to one parameter become one document edit"
        );
        bridge.stop();
    }

    #[test]
    fn transport_and_bridge_packets_are_counted_not_dropped() {
        let (host, _graph, _node) = live_host("intensity");
        let bridge = OscBridge::start(19013, host.clone(), |_, _| {}).expect("bind 19013");
        let rev_before = host.lock().unwrap().revision();

        send(
            19013,
            &flux_integrations::osc::encode_message("/flux/v1/project/p1/transport/play", &[1.0]),
        );
        // /flux/v1/bridge/{instance}/features: seq, seconds, then the eight values.
        send(
            19013,
            &flux_integrations::osc::encode_message(
                "/flux/v1/bridge/live1/features",
                &[1.0, 0.5, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            ),
        );

        assert!(
            wait_until(|| {
                let s = bridge.stats();
                s["received"].as_u64() == Some(2)
            }),
            "both controls should reach the listener"
        );
        let stats = bridge.stats();
        assert_eq!(
            stats["notApplied"], 2,
            "both kinds were parsed and explained, not dropped"
        );
        assert_eq!(
            stats["bridge"], 1,
            "the bridge packet was decoded, not discarded"
        );
        assert_eq!(stats["applied"], 0);
        assert_eq!(
            host.lock().unwrap().revision(),
            rev_before,
            "neither kind may edit the document while no engine target exists"
        );
        let reason = stats["lastReason"].as_str().unwrap_or_default().to_string();
        assert!(
            reason.contains("bridge") || reason.contains("transport"),
            "the reported reason must name what is missing, got {reason:?}"
        );
        bridge.stop();
    }

    #[test]
    fn unknown_node_is_reported_unmapped() {
        let (host, graph, node) = live_host("intensity");
        let bridge = OscBridge::start(19014, host.clone(), |_, _| {}).expect("bind 19014");
        send(
            19014,
            &flux_integrations::osc::encode_param_feedback("p1", "no-such-node", "intensity", 0.6),
        );
        assert!(
            wait_until(|| bridge.stats()["unmapped"].as_u64() == Some(1)),
            "an address whose node is not in the document is counted, not swallowed"
        );
        assert_eq!(host.lock().unwrap().revision(), 0);
        assert_eq!(read_param(&host, &graph, &node, "intensity"), 0.25);
        bridge.stop();
    }

    #[test]
    fn a_taken_port_is_reported_not_swallowed() {
        let squat = std::net::UdpSocket::bind("127.0.0.1:19015").expect("squat on 19015");
        let (host, _graph, _node) = live_host("intensity");
        let err = match OscBridge::start(19015, host, |_, _| {}) {
            Ok(bridge) => {
                drop(bridge);
                panic!("the second listener must not get the port");
            }
            Err(e) => e,
        };
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::AddrInUse,
            "the caller must be able to tell a busy port from a broken socket: {err}"
        );
        drop(squat);
    }

    #[test]
    fn live_osc_edits_the_active_graph_with_no_lesson() {
        use crate::commands::{GuiCommand, Request};
        let (host, graph, node) = live_host("intensity");
        let response = host.lock().unwrap().handle(Request {
            protocol_version: 1,
            request_id: "r1".into(),
            project_id: None,
            expected_revision: 0,
            command: GuiCommand::LiveOsc {
                node: node.clone(),
                param: "intensity".into(),
                value: 0.6,
            },
        });
        assert!(
            response.error.is_none(),
            "unexpected error: {:?}",
            response.error.map(|e| e.message)
        );
        assert_eq!(
            response.result.expect("result")["changed"].as_bool(),
            Some(true),
            "before this rule an unlearned surface answered changed:false and did nothing"
        );
        assert_eq!(
            read_param(&host, &graph, &node, "intensity"),
            (0.6f32) as f64,
            "the document stores the f32 that came off the wire, widened"
        );
    }

    #[test]
    fn a_learned_table_restricts_what_the_listener_applies() {
        use crate::commands::{GuiCommand, Request};
        let (host, _graph, node) = live_host("intensity");
        let graph_id = host.lock().unwrap().active_graph.clone();
        let learned = host.lock().unwrap().handle(Request {
            protocol_version: 1,
            request_id: "r2".into(),
            project_id: None,
            expected_revision: 0,
            command: GuiCommand::LearnBinding {
                binding: ControlBinding::Osc {
                    address: "/a".into(),
                    node_id: node.clone(),
                    param_id: "intensity".into(),
                },
                graph_id,
            },
        });
        assert!(learned.error.is_none());

        assert_eq!(
            host.lock()
                .unwrap()
                .apply_live_param(&node, "unlearned", 0.7),
            OscApplied::Unmapped,
            "a learned table is a whitelist: what it did not learn must not move"
        );
        assert_eq!(
            host.lock()
                .unwrap()
                .apply_live_param(&node, "intensity", 0.7),
            OscApplied::Parameter {
                node: node.clone(),
                param: "intensity".into(),
                value: 0.7
            }
        );
    }
}
