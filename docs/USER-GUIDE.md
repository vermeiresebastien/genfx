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
2. Drop it onto the **node-graph canvas** (center) and wire its pins; the
   `flux.output` node is the graph root. A **timeline** strip remains at the
   bottom and drives duration and transport, but composition happens on the
   canvas — the clip-per-track editor it replaced is gone (see D013).
3. Select a node to edit its **Inspector** parameters (right).
4. **Play** to scrub; **Export** renders to `out\`.
5. **Undo/Redo** step through edits; the revision badge shows the current revision.
6. **Monitor** (`open_monitor`) opens a second window with the live frame.

## Projects

Projects are JSON (`*.flux.json`) with an explicit `schemaVersion` (currently
`2`). Save writes a temp file and then swaps it in, but the swap is
copy → delete → rename, so there is a brief window with no project file at the
final path; treat power loss during a save as a real risk rather than assuming
an atomic rename. A `.bak` of the previous file is kept. Projects that reference
media store both the original path and a relative `media/…` path; run
`flux collect <project>` to copy assets next to the project. Note that relink
matches on filename and size only — the "fingerprint" it stores is a byte count,
so a zero-length asset matches anything.

- Autosave snapshots live in `<project dir>\.flux-autosave\` with a
  `recovery.json` manifest. `flux recover <dir>` restores the newest one.
- Known defect: the autosave pruner filters on extension `json`, so the `.bak`
  files it writes are never pruned — one per 30 seconds, forever. `RETENTION_DAYS`
  is declared and unused.
- A newer `schemaVersion` opens read-only and is never overwritten.

## Determinism

Time is exact rational (e.g. `30000/1001`). The same inputs always produce the same
frame — no wall-clock in evaluation. This is why offline export matches preview.

## Controls

- **OSC**: the receive path and the apply path are connected inside Flux Desktop since
  2026-11-15. `flux_app::control::OscBridge` binds `127.0.0.1:9000` (override with
  `FLUX_OSC_PORT`) when the app starts, coalesces parameter traffic to one document edit
  per 16 ms window, and applies it as `Command::SetParameter` on the active graph — so a
  fader on Live's controller is an undoable edit, not a parallel state. The webview never
  hears the packets; it is told an external edit landed and re-pulls the snapshot. Ask the
  app what it counted with the `osc_status` IPC command: `bound`, `bindError`, and
  per-outcome counters (`applied`, `unmapped`, `notApplied`, `bridge`, `droppedByReceiver`).
  Only `param` addresses change anything; `transport`, `trigger`, `preset` and `bridge`
  packets are counted and explained, because there is no Rust-owned playhead and no
  audio analyser to feed. `flux osc 9000` is still a 30-second print-only diagnostic and it
  holds the same port the app wants — run one or the other, not both. There is no OSC
  output path, so nothing is sent back on 9001. See `docs/FILE-FORMATS.md`.
- **MIDI**: CC/note/pitch-bend/pressure/clock are parsed into normalized controls.
  Parsing is tested; no virtual ports exist on Windows, so a real device is needed.
- **Ableton Link**: `flux link` joins a Link session (tempo, beat, phase, start/stop sync).
  `FLUX_LINK_BPM` sets the starting tempo, `FLUX_LINK_SECS` the observation time. Run it in
  two terminals (or on two machines on the same LAN) to see peer discovery. Requires a
  build with `--features link` and `FLUX_ABLETON_LINK_LIB` pointing at `abl_link.lib`.
- **Max for Live**: `.maxpat` patches + an OSC bridge in `apps/max-for-live/`. These
  are Max patches, not packaged Live devices — no `.amxd` exists yet, and the bridge
  addresses they send are currently discarded by the receiver.
- **NDI / Spout / Syphon / vision**: behind cargo features that are **off by
  default**, so the binary you just built does not contain them. Check with
  `flux integrations`. Even when enabled, the senders emit a synthetic 640×360
  gradient — there is no wgpu→NDI/Spout frame path yet.

## Troubleshooting

- **No GPU**: set `WGPU_BACKEND=vulkan` or `dx12`; run `flux doctor`.
- **No FFmpeg**: install FFmpeg or set `FLUX_FFMPEG` to the executable.
- **Blank preview**: ensure the project has a generator feeding `flux.output`.
- **Diagnostics bundle**: `flux diagnostics` prints a redacted JSON report (paths and
  device ids are redacted; no telemetry is sent).
