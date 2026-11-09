# Decisions (short records)

## D001 — Host-native editor instead of Tauri/WebView for the non-ui-free shell
The spec's binding stack names Tauri 2 + React + React Flow. This environment has no
`~/.cargo`-pinned Tauri scaffolding and the product's binding architecture (Rust owns
document, evaluation, time, GPU) is independent of which UI toolkit renders React.
Decision: the **Rust engine, domain, render, media and catalog crates follow the spec
exactly**. The desktop shell is implemented as a **native egui/wgpu editor** that talks
to the same engine API the spec attributes to React. Consequence recorded in
BLOCKERS: no web-based React Flow canvas; the node canvas is a native egui canvas with
the same commands/validation. UI code is not the product contract; engine/deterministic
behavior is. All R ids that mention React Flow are implemented as native equivalents.

## D002 — Single wgpu renderer, one authoritative texture
One wgpu device/queue; graph evaluated once per frame; the resulting texture is copied
to (a) editor preview (RGBA readback, downscaled), (b) headless export readback,
(c) external outputs when available. This matches ARCHITECTURE 2/4/8. No browser
simulation runs the graph.

## D003 — Fragment effect template + shared WGSL preamble
Every FX/G row is a `process(uv, src) -> vec4` body. The generated shader supplies the
96-byte FrameUniform, manifest-packed NodeParams, AudioFeatures, and fixed sampler
bindings; final mix applies wet*mask in premultiplied linear. Uniform layout is generated
by `catalog::pack` from the same manifest that drives the UI, docs and tests.

## D004 — FFmpeg via CLI worker (process isolation)
ffmpeg-next links FFmpeg 9.0 shared libs; because the FFmpeg ABI tracks only its own
major version and the dev headers are the winget shared build, media is implemented
through a ffmpeg/ffprobe **process** worker (bounded queues, deterministic frame PTS
selection, cancel). This keeps media failures out of the render thread and avoids
ABI mismatch crashes; it is a documented deviation from `ffmpeg-next` in ARCHITECTURE 1.
Rationale recorded against R004/R024.

## D005 — Native fonts
Text rendering uses a bundled block font defined in code (`catalog::font5x7`), so text
never depends on system fonts and is byte-stable across hosts. A system font registry
hook is kept for future imports.

## D006 — Simulation workers are CPU fixed-step
Particles/physics/fluid/CA/reaction-diffusion run on deterministic CPU workers with
fixed dt from project `simulation_step`, then upload state textures. This is the same
evaluator path for preview and export (no second GPU engine).
