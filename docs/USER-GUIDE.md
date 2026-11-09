# Flux — User Guide

Flux is a GPU video-effects and generative-video app. It ships two front ends over
the same Rust engine:

- **Flux Desktop** (`flux-desktop`) — the GUI editor: library, preview, inspector,
  timeline, transport, export.
- **`flux` CLI** — headless rendering, diagnostics, and control.

Both are driven by the same authoritative engine; the GPU and CPU effects are
identical in preview and export (only resolution/quality differ).

## Quick start (CLI)

```powershell
cargo build -p flux-app --release
target\release\flux.exe doctor                 # GPU, FFmpeg, catalog
target\release\flux.exe demo-render gen.plasma out
target\release\flux.exe contact-sheet out      # all effects in one PNG
```

Outputs go to `out\`: a frame sequence (`frame_000001.png`…), a `.wav`, and an
`.mp4`. Use `--codec hevc|prores|gif|jpeg|none` on `render`.

## Quick start (GUI)

```powershell
cargo build -p flux-desktop
target\debug\flux-desktop.exe
```

1. Pick an effect or generator from the left **Library**.
2. The starter scene appears in the **Preview** (center) and on the **Timeline** (bottom).
3. Select a node to edit its **Inspector** parameters (right).
4. **Play** to scrub; **Export** renders the timeline to `out\`.
5. **Undo/Redo** step through edits; the revision badge shows the current revision.

## Projects

Projects are JSON (`*.flux.json`) with an explicit `schemaVersion`. Save is atomic
(temp file + fsync + rename) and keeps a `.bak` of the previous file. Projects that
reference media store both the original path and a relative `media/…` path; run
`flux collect <project>` to copy assets next to the project.

- Autosave snapshots live in `<project dir>\.flux-autosave\` with a
  `recovery.json` manifest. `flux recover <dir>` restores the newest one.
- A newer `schemaVersion` opens read-only and is never overwritten.

## Determinism

Time is exact rational (e.g. `30000/1001`). The same inputs always produce the same
frame — no wall-clock in evaluation. This is why offline export matches preview.

## Controls

- **OSC**: `flux osc 9000` listens for `/flux/v1/project/{id}/node/{node}/param/{param}`.
- **MIDI**: CC/note/pitch-bend/pressure/clock are parsed into normalized controls.
- **Ableton Link**: `flux link` joins a Link session (tempo, beat, phase, start/stop sync).
  `FLUX_LINK_BPM` sets the starting tempo, `FLUX_LINK_SECS` the observation time. Run it in
  two terminals (or on two machines on the same LAN) to see peer discovery. Requires a
  build with `--features link` and `FLUX_ABLETON_LINK_LIB` pointing at `abl_link.lib`.
- **Max for Live**: source devices + OSC bridge in `apps/max-for-live/`.

## Troubleshooting

- **No GPU**: set `WGPU_BACKEND=vulkan` or `dx12`; run `flux doctor`.
- **No FFmpeg**: install FFmpeg or set `FLUX_FFMPEG` to the executable.
- **Blank preview**: ensure the project has a generator feeding `flux.output`.
- **Diagnostics bundle**: `flux diagnostics` prints a redacted JSON report (paths and
  device ids are redacted; no telemetry is sent).
