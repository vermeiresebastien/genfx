//! Integration tests: real OSC UDP loopback through the receiver thread.

use crate::osc::{decode_packet, parse_address, OscControl, OscReceiver};
use std::net::UdpSocket;
use std::time::Duration;

#[test]
fn osc_udp_loopback_roundtrip() {
    let (tx, rx) = crossbeam_channel::bounded(64);
    let (recv, handle) = OscReceiver::start(19001, tx).expect("bind osc");
    // Send from a client socket.
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    let msg = rosc::OscMessage {
        addr: "/flux/v1/project/p/node/n1/param/radius".into(),
        args: vec![rosc::OscType::Float(0.75)],
    };
    let bytes = rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap();
    client.send_to(&bytes, "127.0.0.1:19001").unwrap();
    let ctrl = rx.recv_timeout(Duration::from_secs(2)).expect("control");
    match ctrl {
        OscControl::Param { node, param, value } => {
            assert_eq!(node, "n1");
            assert_eq!(param, "radius");
            assert!((value - 0.75).abs() < 1e-5);
        }
        other => panic!("unexpected {other:?}"),
    }
    recv.stop();
    let _ = handle.join();
}

#[test]
fn osc_malformed_datagram_is_ignored() {
    let (tx, rx) = crossbeam_channel::bounded(8);
    let (recv, handle) = OscReceiver::start(19002, tx).expect("bind osc");
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client
        .send_to(b"not osc at all", "127.0.0.1:19002")
        .unwrap();
    // No control should arrive; a valid one afterwards should.
    let msg = rosc::OscMessage {
        addr: "/flux/v1/project/p/transport/play".into(),
        args: vec![rosc::OscType::Int(1)],
    };
    let bytes = rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap();
    client.send_to(&bytes, "127.0.0.1:19002").unwrap();
    let ctrl = rx.recv_timeout(Duration::from_secs(2)).expect("control");
    assert_eq!(ctrl, OscControl::TransportPlay(true));
    recv.stop();
    let _ = handle.join();
}

#[test]
fn decoder_handles_native_double() {
    let msg = rosc::OscMessage {
        addr: "/x".into(),
        args: vec![rosc::OscType::Double(1.25)],
    };
    let bytes = rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap();
    let (_, args) = decode_packet(&bytes).unwrap();
    assert!((args[0] - 1.25).abs() < 1e-9);
    assert!(parse_address("/flux/v1/project/p/node/n/param/x/native", &[1.0], false).is_some());
}

/// The documented seek packet cannot survive this decoder.
///
/// `docs/FILE-FORMATS.md` specifies `/transport/seek` as *string* seconds followed by
/// *int* denominator. `decode_packet` flattens arguments to `f64` and keeps only the
/// numeric ones, so the seconds are dropped and the denominator is read as the seconds,
/// while `parse_address` hardcodes `denominator: 1_000_000`. This test pins the defect
/// rather than the intent: if someone fixes the wire format, it fails and says so.
#[test]
fn a_seek_packet_cannot_carry_its_seconds() {
    let msg = rosc::OscMessage {
        addr: "/flux/v1/project/p/transport/seek".into(),
        args: vec![
            rosc::OscType::String("2.5".into()),
            rosc::OscType::Int(1_000_000),
        ],
    };
    let bytes = rosc::encoder::encode(&rosc::OscPacket::Message(msg)).unwrap();
    let ctrl = crate::osc::decode_control(&bytes).expect("a seek control");
    match ctrl {
        OscControl::Seek {
            seconds,
            denominator,
        } => {
            assert_eq!(
                seconds, "1000000.000000",
                "the argument that arrives is the denominator, not the seconds"
            );
            assert_eq!(
                denominator, 1_000_000,
                "hardcoded, not read from the packet"
            );
        }
        other => panic!("expected a seek, got {other:?}"),
    }
}

/// `capability_report()` is the answer `flux integrations` gives. A `cfg!`-derived `verified`
/// makes that answer depend on how the binary was compiled instead of on whether anything was
/// measured, and no runtime test can see it: the row reads `false` in a default build and `true`
/// in a feature build, and each build only ever observes its own value passing. So the invariant
/// is checked against the source, the way `apps/max-for-live/check-maxpat.cjs` checks the patch.
#[test]
fn capability_report_never_derives_verified_from_a_build_feature() {
    let offender = include_str!("lib.rs")
        .lines()
        .map(str::trim)
        .find(|line| line.contains("verified:") && line.contains("cfg!"))
        .map(str::to_string);
    assert!(
        offender.is_none(),
        "capability_report() derives `verified` from a build feature: {offender:?}. A feature \
         flag is not evidence — state a measured value, or set it false and put what is missing \
         in that row's `limitation`."
    );
}

/// Every sidecar — shim DLL, helper EXE, FFmpeg — has to resolve from the
/// **installed** layout, not the working directory. A bare relative candidate
/// compiles, passes on a dev checkout, and finds nothing in a packaged app, so
/// the shape of each call site is gated here rather than left to the next
/// clean-machine run (which is AT082, and AT082 has never been run).
#[test]
fn sidecars_resolve_through_the_layout_not_the_working_directory() {
    /// Comment lines carry the history of the defect, so gate the code without them.
    fn code(src: &str) -> String {
        src.lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    let sites: [(&str, String, usize); 5] = [
        (
            "crates/integrations/src/spout.rs",
            code(include_str!("spout.rs")),
            2,
        ),
        (
            "crates/integrations/src/ndi_io.rs",
            code(include_str!("ndi_io.rs")),
            1,
        ),
        (
            "crates/integrations/src/ndi.rs",
            code(include_str!("ndi.rs")),
            1,
        ),
        (
            "crates/media/src/lib.rs",
            code(include_str!("../../media/src/lib.rs")),
            2,
        ),
        (
            "crates/app/src/main.rs",
            code(include_str!("../../app/src/main.rs")),
            1,
        ),
    ];

    for (file, body, required) in sites {
        let found = body.matches("layout::").count();
        assert!(
            found >= required,
            "{file} makes {found} layout:: calls, expected at least {required}. Sidecars must be \
             resolved through `flux_platform::layout` so an installed app finds them beside its \
             own executable."
        );
        for banned in ["Some(\"apps/", "Path::new(\"apps/"] {
            assert!(
                !body.contains(banned),
                "{file} resolves a sidecar with the CWD-only form `{banned}`. That is the \
                 pre-layout shape: it cannot work once `apps/` is not next to the launch directory."
            );
        }
        if file.starts_with("crates/integrations/") {
            assert!(
                !body.contains("set_var("),
                "{file} mutates the process environment to make a DLL resolve. Use \
                 `LOAD_WITH_ALTERED_SEARCH_PATH`, which the loader honours for the DLL's own \
                 imports; `set_var` races every other reader and does nothing for a search path \
                 already cached."
            );
        }
    }
}
