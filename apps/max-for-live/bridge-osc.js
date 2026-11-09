// Max for Live bridge OSC encoder (node.script helper).
// Runs inside Max's Node runtime; encodes OSC 1.0 datagrams and sends them to the
// desktop app. Kept dependency-free (Buffer + dgram only) because Max bundles a
// specific Node version. All UDP happens here, never in a DSP callback.

const dgram = require("dgram");

function pad4(buf) {
  const rem = buf.length % 4;
  return rem === 0 ? buf : Buffer.concat([buf, Buffer.alloc(4 - rem)]);
}

function oscString(s) {
  return pad4(Buffer.from(s + "\0", "ascii"));
}

// Encode an OSC message: address + ",types" + big-endian args.
// Supported arg types: f (float32), d (double), i (int32), s (string).
function encodeOsc(address, args) {
  let types = ",";
  const payload = [];
  for (const a of args) {
    const t = typeof a;
    if (t === "number") {
      if (Number.isInteger(a)) {
        types += "i";
        const b = Buffer.alloc(4);
        b.writeInt32BE(a, 0);
        payload.push(b);
      } else {
        types += "f";
        const b = Buffer.alloc(4);
        b.writeFloatBE(a, 0);
        payload.push(b);
      }
    } else if (t === "boolean") {
      types += "i";
      const b = Buffer.alloc(4);
      b.writeInt32BE(a ? 1 : 0, 0);
      payload.push(b);
    } else {
      types += "s";
      payload.push(oscString(String(a)));
    }
  }
  return Buffer.concat([oscString(address), oscString(types), ...payload]);
}

const HOST = "127.0.0.1";
let PORT = 9000;
let INSTANCE = "max-instance-1";
let seq = 0;
const sock = dgram.createSocket("udp4");

function send(address, args) {
  const bytes = encodeOsc(address, args);
  sock.send(bytes, 0, bytes.length, PORT, HOST);
}

function hello(replyPort) {
  send(`/flux/v1/bridge/${INSTANCE}/hello`, ["flux-bridge/1", replyPort]);
}

function features(rms, peak, bass, mid, treble, onsetStrength, onsetFlag, confidence, seconds) {
  seq = (seq + 1) & 0x7fffffff;
  send(`/flux/v1/bridge/${INSTANCE}/features`, [
    seq, seconds, rms, peak, bass, mid, treble, onsetStrength, onsetFlag, confidence,
  ]);
}

function transport(seconds, beat, tempo, playing) {
  seq = (seq + 1) & 0x7fffffff;
  send(`/flux/v1/bridge/${INSTANCE}/transport`, [seq, seconds, beat, tempo, playing ? 1 : 0]);
}

function heartbeat() {
  seq = (seq + 1) & 0x7fffffff;
  send(`/flux/v1/bridge/${INSTANCE}/heartbeat`, [seq]);
}

// Max inlets (declared in the .maxpat with node.script).
inlets = 1;
outlets = 0;

function msg_int(v) {
  heartbeat();
  if (v === 0) hello(9100);
}

module.exports = { encodeOsc, features, transport, heartbeat, hello };
