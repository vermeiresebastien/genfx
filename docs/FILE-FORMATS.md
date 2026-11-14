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

**Not yet true:** nothing connects this transport to that command. `apply_controls`
and `control::apply_osc` have no caller outside their own unit tests, and Flux
Desktop opens no UDP listener, so `flux osc <port>` is a 30-second print-only demo.
The apply path itself exists (`GuiCommand::LiveOsc` → `control::apply_to_dispatcher`,
`crates/app/src/commands.rs:478`) and is a no-op until a `LearnBinding` has set
`control_map.graph_id`. There is no UDP **send** path at all — `encode_param_feedback`
is called only by its own test and the feedback port constant (9001) is unused.

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
