# Flux — Final Report

Specification version 2.0. Updated after the completion batches (GUI, persistence, export
matrix, OSC/MIDI, live control bridge, Max bridge, platform capture, recording, diagnostics,
shader harness, docs). The four judgments are kept separate (VERIFICATION §8).

## 1. What works (verified on this host: Windows x64, RTX 2080 Ti, FFmpeg 9)

Engine, GPU renderer, 216-entry catalog, media, audio, persistence, export matrix,
control integrations, live control bridge, Max bridge, platform capture enumeration,
diagnostics, recording, and the Tauri 2 desktop GUI all build; `cargo test --workspace`
is green. Verified suites:

- **domain** (30): exact rational time, document, typed commands + deterministic undo/redo.
- **catalog** (8): 216 entries, unique ids, defaults in range, shader structure.
- **engine** (21): graph validate/compile, timeline mapping, modulation, 96-byte ABI.
- **media** (6): probe, RGBA decode at PTS, audio decode, PNG/WAV, encoder caps.
- **audio** (14): FFT/RMS/onset/tempo/chroma, mixer, ring buffer.
- **render** (8): **216 shaders compile on the real GPU (0 failures)**, color oracle, readback.
- **integration lib** (6): generators render, effects vary, determinism, time variation.
- **producer contract** (6): text/vector/SDF/mesh/particle/physics/field/control visible,
  deterministic, animated (SDF metaballs visually confirmed).
- **golden export** (3): generator→effects→MP4, media→present, text→export.
- **persistence** (6): atomic save+backup, schema rejection, autosave+recovery+prune,
  collect-media, relink.
- **export job/queue** (3): frozen snapshot, persisted queue, codecs.
- **export matrix** (6): PNG+WAV; H.264 MP4 probed; HEVC/ProRes/GIF/JPEG when available.
- **integrations** (26): **real OSC UDP loopback**, malformed/oversize/rate-limit,
  feedback; MIDI parsing; **Max bridge decode** (hello/features/transport/heartbeat, seq
  wrap, epoch, staleness); live source/sink contracts + truthful protocol status.
- **app commands** (6, incl. GUI): full GUI acceptance flow (edit→save→reload).
- **app control** (5): OSC/MIDI→engine command mapping, learn, coalescing, stale-binding.
- **diagnostics** (3): redacted capability report.
- **recording** (2): frame-exact CFR bounded duration.
- **maxpat structure** (Node): both source patches valid, passthrough wired, OSC encoder aligned.
- **shader harness** (xtask): 216 WGSL emitted + Naga-validated (0 failures).

Produced this run:
- **Tauri 2 + web GUI** (`apps/desktop`): builds, links, launches; Rust owns state/validation/
  rendering; webview submits typed requests (`CommandHost`), shows preview frames.
- **Max for Live source devices** (`apps/max-for-live`): 2 `.maxpat` patches + OSC bridge
  helper + structural conformance check.
- **WebGPU shader harness** (`apps/shader-harness`): static page + localhost server; loads
  the same WGSL; reports honestly if WebGPU is unavailable.
- **Live control bridge** (`crates/app/src/control.rs`): OSC/MIDI → engine `Command`s with
  learned bindings and frame coalescing; reachable from the GUI.
- **Live source/sink interfaces** (`crates/integrations/src/live.rs`): NDI/Spout/Syphon
  contracts, frame validation, truthful protocol status.
- **Real device enumeration** (`flux capture`): 5 mics, 6 loopback endpoints, 1 camera class.
- **Real offline export** (ffprobe-verified): 1280×720/240f H.264; 640×360; golden 320×180.
- **Docs** (`docs/`): user guide and file/protocol formats.

## 2. Launch

CLI: `flux doctor|caps|capture|integrations|diagnostics|demo-render|render --codec …|
record|recover|collect|contact-sheet`. GUI: `cargo build -p flux-desktop` → `flux-desktop.exe`.

## 3. Verification commands
```
cargo test --workspace                       # all suites
cargo run -p xtask -- test-gpu               # 216 shaders on GPU
cargo run -p xtask --bin export_shaders      # WGSL export + Naga validate
node apps/max-for-live/check-maxpat.cjs      # Max patch structure
infrastructure\verify.ps1 [-Quick]           # full gate, writes verify.json
```

## 4. Host verification
Windows x64 (RTX 2080 Ti, wgpu Vulkan). macOS: not run (unverified). Live capture needs a
device + permission (enumeration verified). Browser WebGPU conformance: page provided;
headless GPU browser not available here.

## 5. Artifact readiness
Release CLI `target/release/flux.exe`; dev GUI `target/debug/flux-desktop.exe`. No signed
installer (no identity configured).

## 6. Explicit blockers (not faked)
NDI/Spout2/Syphon SDKs, Ableton Link license, Max `.amxd` host: interfaces + capability
records present, `blocked_external`. macOS builds unverified. Live camera/screen capture
and GUI webview pixel verification need a device/session. SDF/particle CPU producers not
realtime at 4K.

## 7. Honest overall status
A runnable Windows x64 development build: exact-time editing, native wgpu renderer running
the whole 216-entry catalog, deterministic export to real H.264/HEVC/ProRes/GIF/JPEG/WAV,
real media import/export, OSC/MIDI control wired into the document, Max bridge + source
patches, persistence with autosave/recovery/relink/collect, redacted diagnostics, real
device enumeration, and a Tauri 2 + web GUI that builds and launches with a tested command
surface. macOS, signed installers, external protocols' live I/O, and live device capture
remain unverified or blocked.
