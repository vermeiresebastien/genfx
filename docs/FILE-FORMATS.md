# Flux — Project & File Formats

## Project document (`*.flux.json`)
```jsonc
{
  "schemaVersion": 2,
  "projectId": "uuid",
  "name": "My Project",
  "settings": {
    "size": { "width": 1920, "height": 1080 },
    "frameRate": { "num": 30000, "den": 1001 },   // exact rational
    "audio": { "sampleRate": 48000, "channels": 2 }
  },
  "assets": { "<id>": { "kind": "image|video|audio", "originalPath": "...", "relativePath": "media/..." } },
  "graphs": { "<graphId>": { "nodes": [...], "edges": [...] } },
  "timeline": { "range": {...}, "tracks": {...}, "clips": {...} }
}
```
Time is stored as exact rationals to avoid float drift. Commands carry revisions;
undo/redo is deterministic.

## Export outputs
- **Frames**: `frame_%06d.png` (RGBA8).
- **Audio**: `<name>.wav` (16-bit PCM stereo at project sample rate).
- **Video**: `.mp4` (H.264/HEVC), `.mov` (ProRes 4444), or `.gif`.
- **JPEG sequence**: `<frames>_jpeg/frame_%06d.jpg`.

Export uses a **frozen snapshot**: editing the live project after starting an export
does not change the output.

## Shader ABI
WGSL fragment shaders share a versioned preamble (`fmod/fmodv`, noise/fbm, path/
particle/temporal primitives). Uniforms are packed per `NodeParamsBuffer` with
separate float/uint regions. `cargo run -p xtask --bin export_shaders` writes the
ABI bundle for the WebGPU harness and Naga-validates every shader.

## OSC (control)
```
/flux/v1/project/{id}/node/{node}/param/{param}   float 0..1
/flux/v1/project/{id}/node/{node}/trigger/{id}
/flux/v1/project/{id}/transport/play              int 0|1
/flux/v1/project/{id}/transport/seek              string seconds, int denominator
```
Decoding and the control model are implemented and tested
(`crates/integrations/src/osc.rs`; `Seek { seconds: String, denominator: i64 }`).

**Connected in the desktop app since 2026-11-15, and still one-way.** Every datagram
arrives through one entry point, `osc::decode_control`
(`crates/integrations/src/osc.rs:47`), which the `OscReceiver` socket thread calls; the
listener in `flux_app::control::OscBridge` coalesces `Param` controls per 16 ms window and
applies them through `CommandHost::apply_live_param` → `control::apply_to_dispatcher`
(`crates/app/src/commands.rs:250`) → `Command::SetParameter`. A `LearnBinding` is no longer
a precondition: with an empty `control_map.graph_id` the edit targets `active_graph`, and
with a non-empty binding table the table acts as a whitelist (`control::is_allowed`).

Two things remain not true:

- Only `param` moves the document. `transport`, `trigger`, `preset` and `bridge` controls
  are counted and reported with the reason they changed nothing
  (`apply_live_control`, `crates/app/src/commands.rs:284`) — there is no Rust-owned playhead
  to receive transport, no `Command` variant for a node trigger, `PresetRecall` stores the
  address rather than the preset id, and the analyser is handed `FeatureSnapshot::default()`.
- `transport/seek` cannot round-trip as documented. `decode_packet` flattens arguments to
  `f64` and drops everything that is not numeric, so the seconds *string* the spec puts
  first is discarded and the first surviving number is the denominator. `parse_address`
  also hardcodes `denominator: 1_000_000` instead of reading it. Measured by
  `a_seek_packet_cannot_carry_its_seconds` (`crates/integrations/src/tests.rs`).

There is still no UDP **send** path — `encode_param_feedback` is called only by its own
test, and the feedback port constant (9001) is unused.

## Max bridge
```
/flux/v1/bridge/{instance}/hello      string protocol, int replyPort
/flux/v1/bridge/{instance}/features   int seq, double seconds, 8 floats
/flux/v1/bridge/{instance}/transport  int seq, double seconds, double beat, float tempo, int playing
/flux/v1/bridge/{instance}/heartbeat  int seq
```
**Not yet true:** the address parser requires `parts[2] == "project"`, so every
`/flux/v1/bridge/…` packet is discarded before decoding
(`crates/integrations/src/osc.rs:44`). `decode_bridge_packet` and `BridgeState`
are implemented but have no non-test callers, so the Max patches currently send
into a sink. OSC bundles are also dropped (`Bundle(_) => None`), not just
malformed packets.
