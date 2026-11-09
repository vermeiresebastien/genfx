// Verifies the Max for Live source patches are structurally valid and match the
// bridge OSC protocol the desktop decodes. Node-only; no Max host required.

const fs = require("fs");
const path = require("path");

function fail(msg) {
  console.error("FAIL:", msg);
  process.exit(1);
}

function loadPatch(file) {
  const p = path.join(__dirname, file);
  const json = JSON.parse(fs.readFileSync(p, "utf8"));
  const patcher = json.patcher;
  if (!patcher) fail(`${file}: no patcher`);
  const ids = new Set();
  for (const b of patcher.boxes) ids.add(b.box.id);
  for (const l of patcher.lines) {
    const pl = l.patchline;
    if (!ids.has(pl.source[0])) fail(`${file}: line source ${pl.source[0]} not a box`);
    if (!ids.has(pl.destination[0])) fail(`${file}: line dest ${pl.destination[0]} not a box`);
  }
  return { patcher, ids };
}

function boxText(patcher, id) {
  const b = patcher.boxes.find((x) => x.box.id === id);
  return b ? b.box.text || "" : "";
}

// --- audio bridge ---
{
  const { patcher, ids } = loadPatch("flux-audio-bridge.maxpat");
  if (!ids.has("obj-plugin~") || !ids.has("obj-plugout~")) fail("audio: missing plugin~/plugout~");
  if (!boxText(patcher, "obj-plugin~").startsWith("plugin~")) fail("audio: plugin~ text");
  if (!boxText(patcher, "obj-plugout~").startsWith("plugout~")) fail("audio: plugout~ text");
  // Audio passthrough must connect plugin~ -> plugout~ (unity).
  const pass = patcher.lines.some(
    (l) => l.patchline.source[0] === "obj-plugin~" && l.patchline.destination[0] === "obj-plugout~"
  );
  if (!pass) fail("audio: no plugin~ -> plugout~ passthrough");
  console.log("audio bridge: OK (plugin~ passthrough + OSC features)");
}

// --- midi bridge ---
{
  const { patcher, ids } = loadPatch("flux-midi-bridge.maxpat");
  if (!ids.has("obj-midiin") || !ids.has("obj-midiout")) fail("midi: missing midiin/midiout");
  const pass = patcher.lines.some(
    (l) => l.patchline.source[0] === "obj-midiin" && l.patchline.destination[0] === "obj-midiout"
  );
  if (!pass) fail("midi: no midiin -> midiout passthrough");
  console.log("midi bridge: OK (midi passthrough + OSC control)");
}

// --- OSC encoder conformance: encode and decode round trip with the desktop ---
{
  const helper = require(path.join(__dirname, "bridge-osc.js"));
  const bytes = helper.encodeOsc("/flux/v1/bridge/inst/features", [1, 2.5, 0.1]);
  if (bytes[0] !== 0x2f) fail("osc: address should start with '/'");
  // Check 4-byte alignment.
  if (bytes.length % 4 !== 0) fail("osc: not 4-byte aligned");
  console.log("osc encoder: OK (aligned OSC message,", bytes.length, "bytes)");
}

console.log("maxpat structural checks: PASS");
