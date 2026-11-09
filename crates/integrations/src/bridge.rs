//! Max for Live bridge protocol: decode bridge OSC packets into typed events,
//! track sequence (modular across 31-bit wrap), feature staleness, and transport
//! epochs. The bridge never sends raw PCM — only features/transport/control.

use std::time::{Duration, Instant};

/// A decoded message from a bridge instance.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    Hello {
        instance: String,
        protocol: String,
        reply_port: i32,
    },
    Features {
        instance: String,
        seq: i64,
        seconds: f64,
        /// RMS, peak, bass, mid, treble, onsetStrength, onsetFlag, confidence
        values: [f32; 8],
    },
    Transport {
        instance: String,
        seq: i64,
        seconds: f64,
        beat: f64,
        tempo: f32,
        playing: bool,
    },
    Heartbeat {
        instance: String,
        seq: i64,
    },
}

/// Decode a raw UDP OSC packet as a bridge message (if it matches the bridge
/// address scheme). Returns None for non-bridge or malformed packets.
pub fn decode_bridge_packet(bytes: &[u8]) -> Option<BridgeEvent> {
    let (_, packet) = rosc::decoder::decode_udp(bytes).ok()?;
    let rosc::OscPacket::Message(m) = packet else {
        return None;
    };
    let parts: Vec<&str> = m.addr.trim_start_matches('/').split('/').collect();
    // /flux/v1/bridge/{instance}/{kind}
    if parts.len() < 5 || parts[0] != "flux" || parts[1] != "v1" || parts[2] != "bridge" {
        return None;
    }
    let instance = parts[3].to_string();
    let float = |i: usize| match m.args.get(i) {
        Some(rosc::OscType::Float(f)) => Some(f64::from(*f)),
        Some(rosc::OscType::Double(d)) => Some(*d),
        Some(rosc::OscType::Int(n)) => Some(*n as f64),
        Some(rosc::OscType::Long(n)) => Some(*n as f64),
        _ => None,
    };
    let int = |i: usize| match m.args.get(i) {
        Some(rosc::OscType::Int(n)) => Some(*n as i64),
        Some(rosc::OscType::Long(n)) => Some(*n),
        Some(rosc::OscType::Float(f)) => Some(*f as i64),
        _ => None,
    };
    let string = |i: usize| match m.args.get(i) {
        Some(rosc::OscType::String(s)) => Some(s.clone()),
        _ => None,
    };
    match parts[4] {
        "hello" => Some(BridgeEvent::Hello {
            instance,
            protocol: string(0).unwrap_or_default(),
            reply_port: int(1).unwrap_or(0) as i32,
        }),
        "features" => {
            let mut values = [0.0f32; 8];
            for (i, v) in values.iter_mut().enumerate() {
                *v = float(i + 2).unwrap_or(0.0) as f32;
            }
            Some(BridgeEvent::Features {
                instance,
                seq: int(0)?,
                seconds: float(1)?,
                values,
            })
        }
        "transport" => Some(BridgeEvent::Transport {
            instance,
            seq: int(0)?,
            seconds: float(1)?,
            beat: float(2)?,
            tempo: float(3).unwrap_or(120.0) as f32,
            playing: int(4).unwrap_or(0) != 0,
        }),
        "heartbeat" => Some(BridgeEvent::Heartbeat {
            instance,
            seq: int(0)?,
        }),
        _ => None,
    }
}

/// Per-instance sequence tracking with 31-bit wrap detection and staleness.
pub struct BridgeState {
    pub instance: String,
    last_seq: Option<i64>,
    last_feature: Option<Instant>,
    pub stale_timeout: Duration,
    /// Transport epoch increments on a time discontinuity.
    pub epoch: u32,
    last_seconds: f64,
    pub dropped_out_of_order: u64,
}

impl BridgeState {
    pub fn new(instance: impl Into<String>, stale_timeout: Duration) -> Self {
        Self {
            instance: instance.into(),
            last_seq: None,
            last_feature: None,
            stale_timeout,
            epoch: 0,
            last_seconds: 0.0,
            dropped_out_of_order: 0,
        }
    }

    /// Returns true if `seq` is newer than the last seen (modular across 2^31).
    fn is_newer(last: Option<i64>, seq: i64) -> bool {
        let Some(last) = last else { return true };
        // Compare in unsigned modular space over 2^31.
        let mask: i64 = 0x7FFF_FFFF;
        let diff = (seq - last) & mask;
        // A forward gap under half the range is "newer".
        diff != 0 && diff < (mask / 2)
    }

    /// Apply an event, updating sequence/epoch/staleness. Returns the event if it
    /// is fresh (older/duplicate sequence numbers are rejected).
    pub fn on_event(&mut self, ev: &BridgeEvent, now: Instant) -> bool {
        let seq = match ev {
            BridgeEvent::Hello { .. } => {
                // A new hello resets sequence history for the session.
                self.last_seq = None;
                self.epoch = 0;
                return true;
            }
            BridgeEvent::Features { seq, seconds, .. } => {
                self.last_feature = Some(now);
                self.check_epoch(*seconds);
                *seq
            }
            BridgeEvent::Transport { seq, seconds, .. } => {
                self.check_epoch(*seconds);
                *seq
            }
            BridgeEvent::Heartbeat { seq, .. } => *seq,
        };
        if Self::is_newer(self.last_seq, seq) {
            self.last_seq = Some(seq);
            true
        } else {
            self.dropped_out_of_order += 1;
            false
        }
    }

    fn check_epoch(&mut self, seconds: f64) {
        // A large backward jump means transport discontinuity → new epoch.
        if seconds + 0.5 < self.last_seconds {
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.last_seconds = seconds;
    }

    /// Features are stale if no feature arrived within the timeout.
    pub fn features_stale(&self, now: Instant) -> bool {
        match self.last_feature {
            Some(t) => now.duration_since(t) > self.stale_timeout,
            None => true,
        }
    }

    /// Map bridge features into the engine's audio snapshot order.
    pub fn to_audio_bands(values: &[f32; 8]) -> ([f32; 4], [f32; 4]) {
        // Engine bands: [bass, mid, treble, low-mid]; extra: [rms, onset, peak, flux]
        (
            [values[2], values[3], values[4], 0.0],
            [values[0], values[6], values[1], values[5]],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(addr: &str, args: Vec<rosc::OscType>) -> Vec<u8> {
        let msg = rosc::OscMessage {
            addr: addr.into(),
            args,
        };
        rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap()
    }

    #[test]
    fn decode_hello_features_transport_heartbeat() {
        let hello = encode(
            "/flux/v1/bridge/inst-1/hello",
            vec![
                rosc::OscType::String("flux-bridge/1".into()),
                rosc::OscType::Int(9100),
            ],
        );
        assert!(matches!(
            decode_bridge_packet(&hello),
            Some(BridgeEvent::Hello {
                reply_port: 9100,
                ..
            })
        ));

        let features = encode(
            "/flux/v1/bridge/inst-1/features",
            vec![
                rosc::OscType::Int(7),
                rosc::OscType::Double(1.25),
                rosc::OscType::Float(0.1),
                rosc::OscType::Float(0.2),
                rosc::OscType::Float(0.3),
                rosc::OscType::Float(0.4),
                rosc::OscType::Float(0.5),
                rosc::OscType::Float(0.6),
                rosc::OscType::Float(1.0),
                rosc::OscType::Float(0.9),
            ],
        );
        match decode_bridge_packet(&features) {
            Some(BridgeEvent::Features {
                seq,
                seconds,
                values,
                ..
            }) => {
                assert_eq!(seq, 7);
                assert!((seconds - 1.25).abs() < 1e-9);
                assert!((values[2] - 0.3).abs() < 1e-6);
            }
            other => panic!("unexpected {other:?}"),
        }

        let hb = encode(
            "/flux/v1/bridge/inst-1/heartbeat",
            vec![rosc::OscType::Int(3)],
        );
        assert!(matches!(
            decode_bridge_packet(&hb),
            Some(BridgeEvent::Heartbeat { seq: 3, .. })
        ));

        let tr = encode(
            "/flux/v1/bridge/inst-1/transport",
            vec![
                rosc::OscType::Int(9),
                rosc::OscType::Double(2.0),
                rosc::OscType::Double(4.5),
                rosc::OscType::Float(128.0),
                rosc::OscType::Int(1),
            ],
        );
        match decode_bridge_packet(&tr) {
            Some(BridgeEvent::Transport {
                beat,
                tempo,
                playing,
                ..
            }) => {
                assert!((beat - 4.5).abs() < 1e-9);
                assert!((tempo - 128.0).abs() < 1e-6);
                assert!(playing);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn non_bridge_packets_rejected() {
        let p = encode("/something/else", vec![rosc::OscType::Int(1)]);
        assert!(decode_bridge_packet(&p).is_none());
        assert!(decode_bridge_packet(b"garbage").is_none());
    }

    #[test]
    fn sequence_wraps_and_rejects_old() {
        let mut st = BridgeState::new("i", Duration::from_millis(500));
        let now = Instant::now();
        let f = |seq: i64, secs: f64| BridgeEvent::Features {
            instance: "i".into(),
            seq,
            seconds: secs,
            values: [0.0; 8],
        };
        assert!(st.on_event(&f(100, 0.0), now));
        assert!(st.on_event(&f(101, 0.0), now));
        assert!(!st.on_event(&f(101, 0.0), now), "duplicate rejected");
        assert!(!st.on_event(&f(50, 0.0), now), "old rejected");
        assert_eq!(st.dropped_out_of_order, 2);
    }

    #[test]
    fn transport_discontinuity_bumps_epoch() {
        let mut st = BridgeState::new("i", Duration::from_millis(500));
        let now = Instant::now();
        let t = |secs: f64| BridgeEvent::Transport {
            instance: "i".into(),
            seq: 1,
            seconds: secs,
            beat: 0.0,
            tempo: 120.0,
            playing: true,
        };
        st.on_event(&t(10.0), now);
        assert_eq!(st.epoch, 0);
        st.on_event(&t(2.0), now); // backward jump
        assert_eq!(st.epoch, 1);
    }

    #[test]
    fn staleness_tracks_features() {
        let mut st = BridgeState::new("i", Duration::from_millis(10));
        let now = Instant::now();
        st.on_event(
            &BridgeEvent::Features {
                instance: "i".into(),
                seq: 1,
                seconds: 0.0,
                values: [0.0; 8],
            },
            now,
        );
        assert!(!st.features_stale(now));
        assert!(st.features_stale(now + Duration::from_millis(50)));
    }

    #[test]
    fn feature_mapping() {
        let (bands, extra) = BridgeState::to_audio_bands(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 1.0, 0.9]);
        assert_eq!(bands, [0.3, 0.4, 0.5, 0.0]);
        assert_eq!(extra[0], 0.1); // rms
        assert_eq!(extra[1], 1.0); // onsetFlag
    }
}
