# Flux — Final Report

Specification version 2.0. Report generated in this run on the window x64 host.
This report distinguishes four judgments (VERIFICATION §8) and does not collapse
them into a single badge.

## 1. What works (verified on this host)

The Flux engine, renderer, effect/generator catalog, media pipeline, audio
analysis and offline export are implemented and verified end-to-end.

- **Exact rational time** (`crates/domain`: `RationalTime`, `FrameRate`), document
  model, typed commands with revisions and deterministic undo/redo, parameter
  modulation fold. 30 unit tests; save/reopen preserves IDs, rational values and
  unknown extension metadata.
- **Catalog**: 216 entries — the 28 requested effects/fills (FX001–FX028), the 24
  additional effect families (FX100–FX123), all 142 required generator rows, the 14
  conditional adapter rows (editable, `blocked_external`), and utility nodes. Each
  manifest drives UI metadata, uniform layout, shader generation, docs and tests.
- **GPU renderer** (`crates/render`, wgpu 27): one authoritative texture; all 216
  catalog shaders were **compiled into real wgpu render pipelines on the RTX 2080 Ti
  with 0 failures** (`cargo xtask test-gpu`). Generator→effect→output graphs execute
  on the GPU; results read back full-resolution and as a bounded preview.
- **Effects/generators render real output**: 123 analytic generators render without
  panic; a curated dense set (plasma, voronoi, mandelbrot, fractal noise, clouds,
  nebula, moiré, kaleidoscope, topographic, water caustic, …) produce non-uniform
  imagery; 24 effects over plasma produce visibly distinct results (see
  `implementation/evidence/effects_contact_sheet.png`). Same-time renders are
  byte-identical (determinism); animated sources differ across time.
- **Media** (`crates/media`, FFmpeg process worker): probe, RGBA frame decode at
  exact PTS, stereo audio decode, PNG/WAV encode, H.264/HEVC/ProRes/GIF encode.
  Round-trips tested. **Media import → decode → present → export verified**: a real
  `testsrc2` video frame (with timecode and frame counter) renders faithfully.
- **Audio** (`crates/audio`): Hann-window FFT bands, RMS/peak, spectral centroid/
  spread/rolloff/flatness, spectral-flux onset with rolling threshold, tempo
  estimate, chroma; deterministic offline mixer with -6 dB pan law and equal-power
  fades; preallocated PCM ring. Numeric tests (silence/sine/bass/treble/onset/chroma).
- **Offline export** (`crates/app`): renders the timeline frame-by-frame at exact
  project time through the real GPU graph, writes a PNG sequence + WAV, and encodes
  H.264/AAC MP4. Verified counts:
  - Generator demo: 1280×720, **240 frames**, **384,000 stereo samples** (4.0 s @ 60 fps).
  - Media demo: 640×360, **180 frames**, 3.0 s, faithful decoded content.
  - Both probed with ffprobe: `codec_name=h264`, matching width/height/duration.
- **CLI** (`flux`): `list`, `doctor`, `new-demo`, `demo-render`, `demo-media`,
  `contact-sheet`, `render`.
- **`cargo xtask`**: `doctor`, `fixtures`, `catalog`, `check`, `test-gpu`,
  `scenario`, `benchmark`, `report`, `package` — each writes JSON evidence under
  `implementation/evidence/`.

Measured performance (`cargo xtask benchmark`, 1920×1080 on RTX 2080 Ti / Vulkan):
most generators render + read back in **~2.5 ms p50** (well inside the 16.67 ms
60 fps budget); a few heavier ones (clouds/concentric) show p95 up to ~20 ms because
the benchmark includes a full preview readback per frame.

Test totals: **108 tests pass** (`cargo test --workspace`), comprising domain (30),
catalog (8), engine (21), media (6), audio (14), render (8, incl. full-catalog GPU
compile + solid color oracle), integration lib (6), producer contract (6), and golden
export (3).

## 2. Launch and use

Build and run on Windows:

```
cargo build -p flux-app --release
target/release/flux.exe doctor
target/release/flux.exe list
target/release/flux.exe demo-render gen.plasma out        # -> out/*.mp4 + *.wav + frames
target/release/flux.exe demo-media "C:\path\clip.mp4" out
target/release/flux.exe contact-sheet out                 # effects contact sheet
target/release/flux.exe render out\My.flux.json out2      # render a saved project
```

Toolchains: Rust 1.99 (MSVC), FFmpeg 9.0 shared build found via `FLUX_FFMPEG`,
`PATH`, or the default winget install. `cargo xtask package` builds the release
binary and records its path.

## 3. Source implementation coverage

- Required R/FX/G rows have real implementation paths recorded in
  `implementation/requirements.json` (345 records: 24 product, 28 effects, 24
  families, 156 generators, 28 tasks, 85 acceptance).
- FX001–FX028, FX100–FX123 and the required 2D/math generators have distinct WGSL
  kernels and pass analytic/metamorphic tests.
- **Generator producer contract** (`flux_app::producer`, DECISIONS D007): one
  interface for rows that do not compute pixels in WGSL. The renderer accepts
  per-node producer textures (`RuntimeGraph::execute_with_producers`), so the same
  producer drives preview and export. Implemented reference producers with visible,
  deterministic, animated output:
  - typography G070–G079 (bundled block font: glyph layout, scale, alignment, reveal);
  - vector G030–G045 (analytic shape rasterizer with animated petals/outline);
  - SDF/mesh G080–G107 (CPU sphere-traced signed-distance field: metaballs, gyroid,
    fractal bulb, terrain, voxel/planet — verified visually as raymarched geometry);
  - particle/physics/field G110–G148 (deterministic point field, damped-oscillator
    physics, analytic vector/height fields);
  - control G150–G162 (live-value bar visualization).
  Producer coverage is enforced by `crates/tests/tests/producer_tests.rs` (visible
  non-transparent content, parameter/​time sensitivity, determinism) and the golden
  export tests. SDF raymarching is CPU-bound and slower than the analytic WGSL
  generators at high resolution; acceptable for correctness/export, noted as a
  performance limitation.

## 4. Host verification

Verified on Windows x64 (RTX 2080 Ti, Vulkan). Full-catalog GPU compile, image
oracles, media round-trips, audio numeric tests and MP4 export were actually run.
macOS paths: not run (no host) → unverified.

## 5. Artifact readiness

- Release binary `target/release/flux.exe` built by `cargo xtask package`.
- Example exports under `out/` (generator) and `media_out/` (media), plus the
  effects contact sheet under `implementation/evidence/`.
- **Unsigned** development build. No installer `.msi`/`.dmg` was produced; no
  signing identity is configured.

## 6. Explicit blockers / unverified

- **Native GUI editor**: architecture decided (Tauri 2 + React over the Rust command
  surface, DECISIONS D001) but not yet built in this batch. The native egui attempt
  was abandoned due to the egui-0.37/wgpu-30 vs wgpu-27 conflict. The durable,
  tested part (typed commands, validation, project model) is complete; the React
  shell is the remaining GUI work.
- **NDI, Spout2, Syphon, Ableton Link, Max for Live `.amxd`, macOS builds, signing,
  vision models**: external prerequisites not present; `blocked_external`.
- **OS capture** (camera/screen/loopback): platform scaffolding only; no device
  verification in this run.
- **WebGPU harness**: not built.
- **Golden-image tolerance suite**: metamorphic/analytic tests used instead; no
  reference image baselines. Producer SDF/particle rows use CPU approximation, not
  the full GPU simulation engine.

## 7. Honest overall status

A verified development build of the Flux **core** exists on Windows x64: exact-time
document and editing model, a native wgpu renderer that compiles and runs the whole
216-entry effect/generator catalog, deterministic offline export to real H.264+AAC
MP4 with correct frame/sample counts, real media import/decode/export, and numeric
audio analysis. The desktop GUI, macOS artifacts, external protocols, and the
3D/simulation producer wiring remain incomplete or blocked. This is not a full
cross-platform release.
