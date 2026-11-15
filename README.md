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
- `crates/catalog` — every effect and generator as manifest + WGSL; **217
  entries** (`flux doctor` prints the authoritative count). The single source of
  truth for UI metadata, uniform layout, shader generation, docs and tests.
- `crates/engine` — graph validation/compile (types, cycles via feedback pairs,
  required inputs, limits), timeline→source mapping, modulation router, 96-byte
  frame ABI.
- `crates/render` — one authoritative wgpu renderer: adapter/device selection,
  per-manifest pipelines, graph execution with **one render target per node**
  (a true DAG, not ping-pong), bounded preview and full-resolution readback.
- `crates/media` — FFmpeg process worker: probe, RGBA frame decode at exact PTS,
  audio decode, PNG/WAV/H.264/HEVC/ProRes/GIF encode.
- `crates/audio` — Hann FFT bands, RMS/peak, onset, tempo, chroma, mixer, PCM ring.
- `crates/integrations` — OSC, MIDI, Ableton Link, NDI/Spout shims, vision. Only
  OSC + MIDI + the bridge decoder are compiled by default; `link`, `ndi`,
  `spout`, `syphon` and `vision` sit behind features that are off.
- `crates/platform` — Windows-only WASAPI endpoint and PnP camera enumeration.
- `crates/app` — the `flux` CLI (27 subcommands; `flux list` and `flux doctor`
  report the real set) and the headless preview/export orchestrator.
- `crates/xtask` — `cargo xtask` project commands and evidence.
- `crates/tests` — GPU integration suite. Requires a device.
- `apps/desktop` — the Flux Desktop GUI: Tauri 2 shell over a vanilla ES-module
  node-graph editor, bundled by esbuild with a content-hashed script URL.

## Quick start (Windows)

```
cargo test --workspace -- --test-threads=1
cargo build -p flux-app --release
target/release/flux.exe doctor
target/release/flux.exe demo-render gen.plasma out      # -> out/Demo_gen_plasma.mp4
target/release/flux.exe demo-media clip.mp4 out         # import + export a video
target/release/flux.exe contact-sheet out               # every fragment-shader effect + preset
cargo xtask test-gpu --profile baseline                 # compile all catalog shaders on GPU
cargo xtask benchmark --profile ordinary                # 1080p 8-node graph, 16.67 ms budget
cargo xtask benchmark --profile 4k                      # 4K same graph, 33.33 ms budget
cargo build -p flux-desktop                             # the GUI; run target/debug/flux-desktop.exe
```

`flux gui` / `flux editor` deliberately exit 2 — the desktop app is a separate
binary (`flux-desktop`), not a mode of the CLI.

FFmpeg: set `FLUX_FFMPEG` or have `ffmpeg`/`ffprobe` on `PATH`.

## Status docs are claims, not evidence

`implementation/` and `docs/` describe intent and history. They lag the code.
The status set was reconciled against the tree on 2026-11-14 (`e84fe67`, and
`implementation/DECISIONS.md` D014–D019), but reconciliation is a snapshot: the
diverges register in `docs/IMPLEMENTATION-ARCHITECTURE.md` §8 and the gates in
`docs/PERFORMANCE-READINESS.md` are where known drift is recorded. Re-run the
commands above rather than reusing the committed evidence in
`implementation/evidence/` — and note that a verification run rewrites those
tracked evidence files, so a dirty tree afterwards is expected.

One measured caveat, because it is the kind this section exists for: from 2026-11-10
until the commit that reformats the tree, HEAD did **not** pass its own first CI step
(`cargo fmt --all -- --check`, 56 diffs in 12 files). Everything downstream of it in
`ci.yml` therefore never ran in GitHub Actions, even though all of it passed locally.
It passes now (`exit 0`), and `cargo test --workspace -- --test-threads=1` is still
green behind the reformat — 218 passed, 0 failed across 40 result lines (29 test binaries
+ 11 doctests), re-measured 2026-11-15 on the D032 tree; it was 196 before the OSC receive
bridge (D022) added 8, then +1 D027, +2 D028, +7 D031 and +4 D032's freshness guards.
The harness that reported it was then
repaired: `infrastructure/verify.ps1` had one step chaining three `cargo test` runs behind
a single exit-code read, and recorded skipped SDK gates as passes. Both are fixed (D020),
and the full script measured `VERIFY: PASS` / exit 0 — 15 passed, 0 failed,
2 `blocked_external`. D027 found those two blockers were about an environment variable and
not about the machine: keyed instead on whether `abl_link.lib` and `spout_shim.dll` are
actually reachable, this host ran all 17 steps then — `VERIFY: PASS`, exit 0, **17 passed /
0 failed / 0 `blocked_external`**, the first recorded run with nothing blocked. That figure was
still shell-dependent until D029: the same commit measured **16 passed / 0 failed /
1 `blocked_external`** from a shell that had not exported `FLUX_ABLETON_LINK_LIB`, because the
Link gate looked for the library *inside the directory that variable names*. It now searches the
vendored candidates and exports the directory it finds, and the run at `7235afd` — clean tree,
variable removed from the environment — is `17 passed / 0 failed / 0 blocked_external` again. D032's
18th step, `requirements-freshness`, compares the generated ledger against what its generator
produces, and the run at `a91ebda` is `18 passed / 0 failed / 0 blocked_external` with
`gitDirty false`. The two SDK files are gitignored, so a fresh clone — and CI — still record 2
blocked steps. See `AGENTS.md` § "Traps inside this repo's own verification harness".

## Live control

Flux Desktop listens for OSC on `127.0.0.1:9000` (`FLUX_OSC_PORT` moves it) and turns
`/flux/v1/project/{id}/node/{node}/param/{param}` into an undoable `SetParameter` on the
active graph, coalesced to one edit per parameter per 16 ms window. The listener is in the
Rust process, not the webview. Only parameter addresses change anything: transport, trigger,
preset and Max-bridge packets are counted with a reason, because there is no Rust-owned
playhead and no analyser to feed them. The app never guesses — `osc_status` returns `bound`,
`bindError` and every counter, and `flux osc <port>` is a 30-second diagnostic that competes
with the app for the same port. Wire formats and the exact gaps: `docs/FILE-FORMATS.md`.

## Documentation

| Document | What it is |
|---|---|
| `AGENTS.md` | Working rules for anyone editing this repo — truthfulness, canonical commands, non-negotiable architecture, harness traps |
| `docs/USER-GUIDE.md` | How to drive Flux: GUI, node graph, CLI, controls, export |
| `docs/FILE-FORMATS.md` | Project document, export outputs, shader ABI, OSC and Max bridge wire formats |
| `docs/IMPLEMENTATION-ARCHITECTURE.md` | How the code actually works, incl. §3.6 add-a-node procedure and §8 divergence register |
| `docs/PERFORMANCE-READINESS.md` | Gates A–H: what must be proven on real hardware, and the shortest procedure for each |
| `docs/DISK-INVENTORY.md` | Measured inventory — committed vs regenerable bytes, and what could be reclaimed |
| `implementation/DECISIONS.md` | Decision record D001–D019; D014–D019 are the 2026-11-10→14 reconciliation |
| `implementation/STATE.json` | Machine-readable current state, re-measured at 2026-11-14 |
