# Flux — Project & File Formats

## Project document (`*.flux.json`)
```jsonc
{
  "schemaVersion": 1,
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
/flux/v1/project/{id}/transport/seek              double seconds
```
Feedback is sent only on change to the configured feedback port.

## Max bridge
```
/flux/v1/bridge/{instance}/hello      string protocol, int replyPort
/flux/v1/bridge/{instance}/features   int seq, double seconds, 8 floats
/flux/v1/bridge/{instance}/transport  int seq, double seconds, double beat, float tempo, int playing
/flux/v1/bridge/{instance}/heartbeat  int seq
```
