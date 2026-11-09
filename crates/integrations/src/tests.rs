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
