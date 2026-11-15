//! OSC control: receive, validate (bounded depth/size), translate canonical
//! addresses into control events, map to parameters, and rate-limit. Never
//! executes commands or file operations from network input.

use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_RECV_PORT: u16 = 9000;
pub const DEFAULT_FEEDBACK_PORT: u16 = 9001;
pub const MAX_BUNDLE_DEPTH: usize = 8;
pub const MAX_PAYLOAD: usize = 64 * 1024;
pub const MAX_RATE: u64 = 1000;

/// A normalized OSC control value extracted from a packet.
#[derive(Debug, Clone, PartialEq)]
pub enum OscControl {
    /// Normalized parameter 0..1.
    Param {
        node: String,
        param: String,
        value: f64,
    },
    /// Trigger edge.
    Trigger { node: String, id: String },
    /// Transport play/pause (1/0).
    TransportPlay(bool),
    /// Seek to a rational time (seconds string + samples-per-second denominator int).
    Seek { seconds: String, denominator: i64 },
    /// Preset recall by stable id.
    PresetRecall(String),
    /// A Max-for-Live bridge packet (`/flux/v1/bridge/{instance}/…`). Kept
    /// separate from the project controls: it carries features and transport for
    /// the whole Live set, not one parameter.
    Bridge(crate::bridge::BridgeEvent),
}

/// Decode one UDP datagram into a control.
///
/// Bridge packets are decoded from the raw bytes because they carry typed
/// arguments — sequence numbers, an instance string, a playing flag — that
/// [`decode_packet`] flattens to `f64`. `decode_bridge_packet` returns `None`
/// for anything that is not a bridge address, so the two schemes cannot collide.
pub fn decode_control(bytes: &[u8]) -> Option<OscControl> {
    if bytes.len() > MAX_PAYLOAD {
        return None;
    }
    if let Some(ev) = crate::bridge::decode_bridge_packet(bytes) {
        return Some(OscControl::Bridge(ev));
    }
    let (addr, args) = decode_packet(bytes)?;
    parse_address(&addr, &args, false)
}

/// Parse a canonical OSC address into a control, if it matches the v1 scheme.
pub fn parse_address(addr: &str, args: &[f64], trigger: bool) -> Option<OscControl> {
    // /flux/v1/project/{id}/node/{node}/param/{param}[/native]
    // /flux/v1/project/{id}/node/{node}/trigger/{id}
    // /flux/v1/project/{id}/transport/{play|seek}
    // /flux/v1/project/{id}/preset/recall
    let parts: Vec<&str> = addr.trim_start_matches('/').split('/').collect();
    if parts.len() < 5 || parts[0] != "flux" || parts[1] != "v1" || parts[2] != "project" {
        return None;
    }
    // parts: [flux, v1, project, {projectId}, {group}, ...]
    match parts[4] {
        "node" => {
            // [..., node, {node}, param|trigger, {name}]
            if parts.len() >= 8 && parts[6] == "param" {
                let node = parts[5].to_string();
                let param = parts[7].to_string();
                let value = args.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
                Some(OscControl::Param { node, param, value })
            } else if parts.len() >= 8 && parts[6] == "trigger" {
                Some(OscControl::Trigger {
                    node: parts[5].to_string(),
                    id: parts[7].to_string(),
                })
            } else {
                None
            }
        }
        "transport" => {
            if parts.len() >= 6 && parts[5] == "play" {
                Some(OscControl::TransportPlay(
                    args.first().copied().unwrap_or(0.0) >= 0.5,
                ))
            } else if parts.len() >= 6 && parts[5] == "seek" {
                Some(OscControl::Seek {
                    seconds: format!("{:.6}", args.first().copied().unwrap_or(0.0)),
                    denominator: 1_000_000,
                })
            } else {
                None
            }
        }
        "preset" if parts.get(5) == Some(&"recall") => {
            Some(OscControl::PresetRecall(addr.to_string()))
        }
        _ => {
            let _ = trigger;
            None
        }
    }
}

/// Decode raw UDP payload to (address, float args) using rosc. Returns None on
/// malformed input or exceeded limits.
pub fn decode_packet(bytes: &[u8]) -> Option<(String, Vec<f64>)> {
    if bytes.len() > MAX_PAYLOAD {
        return None;
    }
    let (_, packet) = rosc::decoder::decode_udp(bytes).ok()?;
    match packet {
        rosc::OscPacket::Message(m) => {
            let args = m
                .args
                .iter()
                .filter_map(|a| match a {
                    rosc::OscType::Float(f) => Some(*f as f64),
                    rosc::OscType::Double(d) => Some(*d),
                    rosc::OscType::Int(i) => Some(*i as f64),
                    rosc::OscType::Long(i) => Some(*i as f64),
                    rosc::OscType::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                    _ => None,
                })
                .collect();
            Some((m.addr, args))
        }
        rosc::OscPacket::Bundle(_) => None,
    }
}

/// Token bucket rate limiter for incoming packets.
pub struct RateLimiter {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last: Instant,
    dropped: AtomicU64,
}

impl RateLimiter {
    pub fn new(per_sec: u64) -> Self {
        Self {
            tokens: per_sec as f64,
            capacity: per_sec as f64,
            refill_per_sec: per_sec as f64,
            last: Instant::now(),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.refill_per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// A running OSC receiver that forwards parsed controls on a bounded channel.
pub struct OscReceiver {
    stop: Arc<AtomicBool>,
    pub dropped: Arc<AtomicU64>,
}

impl OscReceiver {
    /// Bind and spawn a receiver thread. The channel is bounded; when full, new
    /// continuous values coalesce (drop) and triggers are counted as dropped.
    pub fn start(
        port: u16,
        tx: Sender<OscControl>,
    ) -> std::io::Result<(Self, std::thread::JoinHandle<()>)> {
        let sock = UdpSocket::bind(("127.0.0.1", port))?;
        sock.set_read_timeout(Some(Duration::from_millis(50)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let stop2 = stop.clone();
        let dropped2 = dropped.clone();
        let handle = std::thread::spawn(move || {
            let mut limiter = RateLimiter::new(MAX_RATE);
            let mut buf = [0u8; MAX_PAYLOAD];
            while !stop2.load(Ordering::Relaxed) {
                match sock.recv_from(&mut buf) {
                    Ok((n, _)) => {
                        if !limiter.allow() {
                            dropped2.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        if let Some(ctrl) = decode_control(&buf[..n]) {
                            if tx.try_send(ctrl).is_err() {
                                dropped2.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => break,
                }
            }
        });
        Ok((Self { stop, dropped }, handle))
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for OscReceiver {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Encode one OSC message to UDP-ready bytes, with float arguments only.
///
/// The receive tests build packets with this rather than hand-rolling the wire
/// format, and `flux live` uses it for feedback: an encoder nobody sends through
/// cannot prove the decoder is right.
pub fn encode_message(addr: impl Into<String>, args: &[f32]) -> Vec<u8> {
    let msg = rosc::OscMessage {
        addr: addr.into(),
        args: args.iter().copied().map(rosc::OscType::Float).collect(),
    };
    rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap_or_default()
}

/// Encode a feedback message (change-only) for a parameter.
pub fn encode_param_feedback(
    project_id: &str,
    node: &str,
    param: &str,
    normalized: f32,
) -> Vec<u8> {
    let msg = rosc::OscMessage {
        addr: format!("/flux/v1/project/{project_id}/node/{node}/param/{param}"),
        args: vec![rosc::OscType::Float(normalized)],
    };
    rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap_or_default()
}

/// A learned mapping from an OSC address to a node/param.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OscMapping {
    pub address: String,
    pub node_id: String,
    pub param_id: String,
}

/// Apply controls to a map of (node,param)->value, coalescing latest values.
pub fn apply_controls(
    controls: &[OscControl],
    latest: &mut HashMap<(String, String), f64>,
) -> usize {
    let mut applied = 0;
    for c in controls {
        if let OscControl::Param { node, param, value } = c {
            latest.insert((node.clone(), param.clone()), *value);
            applied += 1;
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_parsing_param_normalized() {
        let c = parse_address("/flux/v1/project/p/node/n1/param/radius", &[0.7], false).unwrap();
        assert_eq!(
            c,
            OscControl::Param {
                node: "n1".into(),
                param: "radius".into(),
                value: 0.7
            }
        );
        // Clamp out-of-range.
        let c = parse_address("/flux/v1/project/p/node/n1/param/x", &[5.0], false).unwrap();
        if let OscControl::Param { value, .. } = c {
            assert_eq!(value, 1.0);
        }
    }

    #[test]
    fn trigger_and_transport() {
        assert!(matches!(
            parse_address("/flux/v1/project/p/node/n1/trigger/hit", &[1.0], true),
            Some(OscControl::Trigger { .. })
        ));
        assert_eq!(
            parse_address("/flux/v1/project/p/transport/play", &[1.0], false),
            Some(OscControl::TransportPlay(true))
        );
    }

    #[test]
    fn unknown_addresses_rejected() {
        assert!(parse_address("/foo/bar", &[], false).is_none());
        assert!(parse_address("/flux/v0/project", &[], false).is_none());
    }

    #[test]
    fn decode_roundtrip_via_rosc() {
        let msg = rosc::OscMessage {
            addr: "/flux/v1/project/p/node/n/param/x".into(),
            args: vec![rosc::OscType::Float(0.42)],
        };
        let bytes = rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap();
        let (addr, args) = decode_packet(&bytes).unwrap();
        assert_eq!(addr, "/flux/v1/project/p/node/n/param/x");
        assert!((args[0] - 0.42).abs() < 1e-6);
    }

    #[test]
    fn malformed_rejected_and_oversize_rejected() {
        assert!(decode_packet(b"garbage").is_none());
        assert!(decode_packet(&vec![0u8; MAX_PAYLOAD + 1]).is_none());
    }

    #[test]
    fn rate_limiter_drops_excess() {
        let mut rl = RateLimiter::new(10);
        let mut allowed = 0;
        for _ in 0..100 {
            if rl.allow() {
                allowed += 1;
            }
        }
        // Bucket starts full (10) and refills negligibly over this tight loop.
        assert!(allowed <= 12, "allowed {allowed}");
        assert!(rl.dropped() >= 88);
    }

    #[test]
    fn feedback_encodes() {
        let b = encode_param_feedback("p", "n", "x", 0.5);
        assert!(!b.is_empty());
        let (addr, args) = decode_packet(&b).unwrap();
        assert!(addr.ends_with("/x"));
        assert!((args[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn controls_coalesce_latest() {
        let mut latest = HashMap::new();
        let ctrls = vec![
            OscControl::Param {
                node: "a".into(),
                param: "x".into(),
                value: 0.1,
            },
            OscControl::Param {
                node: "a".into(),
                param: "x".into(),
                value: 0.9,
            },
        ];
        apply_controls(&ctrls, &mut latest);
        assert_eq!(latest[&("a".to_string(), "x".to_string())], 0.9);
    }
}
