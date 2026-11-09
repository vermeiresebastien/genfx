# Flux

A native Rust engine and GPU renderer for video editing, audio-reactive visuals,
procedural generation, and live performance — plus offline export to real media.

This repository is an implementation of the Flux product specification in `spec/`
(version 2.0). See `implementation/FINAL-REPORT.md` for what is verified, how to
run it, and what remains blocked, and `implementation/STATE.json` /
`implementation/requirements.json` for structured status.

## Layout

- `crates/domain` — exact rational time, project document, typed commands +
  deterministic undo/redo, parameter modulation.
- `crates/catalog` — every effect and generator as manifest + WGSL; 216 entries.
  The single source of truth for UI metadata, uniform layout, shader generation,
  docs and tests.
- `crates/engine` — graph validation/compile (types, cycles via feedback pairs,
  required inputs, limits), timeline→source mapping, modulation router, 96-byte
  frame ABI.
- `crates/render` — one authoritative wgpu renderer: adapter/device selection,
  per-manifest pipelines, graph execution with ping-pong targets, bounded preview
  and full-resolution readback.
- `crates/media` — FFmpeg process worker: probe, RGBA frame decode at exact PTS,
  audio decode, PNG/WAV/H.264/HEVC/ProRes/GIF encode.
- `crates/audio` — Hann FFT bands, RMS/peak, onset, tempo, chroma, mixer, PCM ring.
- `crates/app` — the `flux` CLI: list, doctor, demo-render, demo-media,
  contact-sheet, render, and the offline export orchestrator.
- `crates/xtask` — `cargo xtask` project commands and evidence.

## Quick start (Windows)

```
cargo test --workspace
cargo build -p flux-app --release
target/release/flux.exe doctor
target/release/flux.exe demo-render gen.plasma out      # -> out/Demo_gen_plasma.mp4
target/release/flux.exe demo-media clip.mp4 out         # import + export a video
target/release/flux.exe contact-sheet out               # 43-effect contact sheet
cargo xtask test-gpu --profile baseline                 # compile all catalog shaders on GPU
cargo xtask benchmark --profile ordinary                # 1080p timings
```

FFmpeg: set `FLUX_FFMPEG` or have `ffmpeg`/`ffprobe` on `PATH`.
