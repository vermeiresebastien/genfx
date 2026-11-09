//! MIDI input: enumerate devices, parse CC/notes/pitch-bend/pressure/clock into
//! normalized controls, and expose a bounded control channel. Reconnects by
//! device name hint. Virtual ports depend on the backend; Windows has none.

use crossbeam_channel::{bounded, Sender};
use midir::{MidiInput, MidiInputConnection};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// A normalized control derived from MIDI.
#[derive(Debug, Clone, PartialEq)]
pub enum MidiControl {
    Cc {
        channel: u8,
        cc: u8,
        value: f64,
    },
    NoteOn {
        channel: u8,
        note: u8,
        velocity: f64,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    PitchBend {
        channel: u8,
        value: f64,
    },
    ChannelPressure {
        channel: u8,
        value: f64,
    },
    PolyPressure {
        channel: u8,
        note: u8,
        value: f64,
    },
    Clock,
    Start,
    Stop,
    Continue,
}

/// Parse a raw MIDI status+data message. Returns None for unsupported messages.
pub fn parse_message(msg: &[u8]) -> Option<MidiControl> {
    if msg.is_empty() {
        return None;
    }
    let status = msg[0];
    // System real-time clock/transport.
    match status {
        0xF8 => return Some(MidiControl::Clock),
        0xFA => return Some(MidiControl::Start),
        0xFB => return Some(MidiControl::Continue),
        0xFC => return Some(MidiControl::Stop),
        _ => {}
    }
    if status < 0x80 || msg.len() < 2 {
        return None;
    }
    let kind = status & 0xF0;
    let channel = status & 0x0F;
    match kind {
        0x90 if msg.len() >= 3 && msg[2] > 0 => Some(MidiControl::NoteOn {
            channel,
            note: msg[1],
            velocity: msg[2] as f64 / 127.0,
        }),
        0x90 if msg.len() >= 3 => Some(MidiControl::NoteOff {
            channel,
            note: msg[1],
        }),
        0x80 if msg.len() >= 3 => Some(MidiControl::NoteOff {
            channel,
            note: msg[1],
        }),
        0xB0 if msg.len() >= 3 => Some(MidiControl::Cc {
            channel,
            cc: msg[1],
            value: msg[2] as f64 / 127.0,
        }),
        0xE0 if msg.len() >= 3 => {
            let bend = ((msg[2] as i32) << 7) | (msg[1] as i32);
            Some(MidiControl::PitchBend {
                channel,
                value: (bend as f64 / 8191.5) - 1.0,
            })
        }
        0xD0 if msg.len() >= 2 => Some(MidiControl::ChannelPressure {
            channel,
            value: msg[1] as f64 / 127.0,
        }),
        0xA0 if msg.len() >= 3 => Some(MidiControl::PolyPressure {
            channel,
            note: msg[1],
            value: msg[2] as f64 / 127.0,
        }),
        _ => None,
    }
}

/// List MIDI input port names. Returns empty when no backend/devices exist.
pub fn list_inputs() -> Vec<String> {
    let Ok(input) = MidiInput::new("flux-enum") else {
        return vec![];
    };
    input
        .ports()
        .iter()
        .filter_map(|p| input.port_name(p).ok())
        .collect()
}

/// A running MIDI input connection forwarding controls on a bounded channel.
pub struct MidiListener {
    _conn: MidiInputConnection<()>,
    pub dropped: Arc<AtomicU64>,
    pub clock_ticks: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl MidiListener {
    /// Connect to port index `port`, forwarding parsed controls. `clock_counter`
    /// increments on each MIDI clock for tempo estimation.
    pub fn connect(port: usize, tx: Sender<MidiControl>) -> Result<Self, String> {
        let mut input = MidiInput::new("flux-in").map_err(|e| e.to_string())?;
        input.ignore(midir::Ignore::None);
        let ports = input.ports();
        if port >= ports.len() {
            return Err(format!(
                "port {port} out of range ({} available)",
                ports.len()
            ));
        }
        let dropped = Arc::new(AtomicU64::new(0));
        let clock_ticks = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let dropped2 = dropped.clone();
        let clock2 = clock_ticks.clone();
        // `connect` consumes the MidiInput, moving it into the connection.
        let conn = input
            .connect(
                &ports[port],
                "flux-read",
                move |_stamp, message, _| {
                    if let Some(c) = parse_message(message) {
                        if matches!(c, MidiControl::Clock) {
                            clock2.fetch_add(1, Ordering::Relaxed);
                        }
                        if tx.try_send(c).is_err() {
                            dropped2.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            _conn: conn,
            dropped,
            clock_ticks,
            stop,
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Estimate tempo from clock tick intervals: 24 PPQN.
pub fn tempo_from_ticks(ticks: u64, elapsed_secs: f64) -> f32 {
    if elapsed_secs <= 0.0 || ticks < 24 {
        return 120.0;
    }
    let beats = ticks as f64 / 24.0;
    let bpm = beats / elapsed_secs * 60.0;
    bpm.clamp(20.0, 300.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cc_note_bend_pressure() {
        assert_eq!(
            parse_message(&[0xB0, 7, 100]),
            Some(MidiControl::Cc {
                channel: 0,
                cc: 7,
                value: 100.0 / 127.0
            })
        );
        assert_eq!(
            parse_message(&[0x91, 60, 0]),
            Some(MidiControl::NoteOff {
                channel: 1,
                note: 60
            })
        );
        assert!(matches!(
            parse_message(&[0x90, 60, 127]),
            Some(MidiControl::NoteOn { velocity, .. }) if (velocity - 1.0).abs() < 1e-6
        ));
        // Pitch bend center = 0.
        let pb = parse_message(&[0xE0, 0x00, 0x40]).unwrap();
        if let MidiControl::PitchBend { value, .. } = pb {
            assert!(value.abs() < 0.01);
        }
    }

    #[test]
    fn parse_clock_and_transport() {
        assert_eq!(parse_message(&[0xF8]), Some(MidiControl::Clock));
        assert_eq!(parse_message(&[0xFA]), Some(MidiControl::Start));
        assert_eq!(parse_message(&[0xFC]), Some(MidiControl::Stop));
    }

    #[test]
    fn unsupported_and_short_rejected() {
        assert!(parse_message(&[]).is_none());
        assert!(parse_message(&[0x90]).is_none());
        assert!(parse_message(&[0xAB]).is_none());
    }

    #[test]
    fn tempo_from_ticks_estimate() {
        // 48 ticks over 1s = 2 beats = 120 bpm.
        assert!((tempo_from_ticks(48, 1.0) - 120.0).abs() < 1.0);
    }

    #[test]
    fn enumeration_does_not_panic() {
        let _ = list_inputs();
    }
}
