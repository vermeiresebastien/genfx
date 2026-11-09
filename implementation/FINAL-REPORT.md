# Flux — Final Report

Specification version 2.0. Report updated after the completion batch (GUI, persistence,
export matrix, OSC/MIDI, platform capture, recording, diagnostics). Four judgments are
kept separate (VERIFICATION §8); they are not collapsed into one badge.

## 1. What works (verified on this host: Windows x64, RTX 2080 Ti)

The Flux engine, GPU renderer, effect/generator catalog, media, audio, persistence,
export, control integrations, platform capture enumeration, and the desktop GUI shell
are implemented and building. Verified suites (144 automated tests, `cargo test
--workspace`):

- **domain** (30): exact rational time, document, typed commands with revisions and
  deterministic undo/redo, modulation fold.
- **catalog** (8): 216 entries (28 requested FX + 24 families + 142 required + 14
  conditional G rows + utilities); unique ids, defaults in range, shader structure.
- **engine** (21): graph validation/compile (types, cycles via feedback pairs, required
  inputs, limits), timeline→source mapping, modulation router, 96-byte frame ABI.
- **media** (6): FFmpeg probe, RGBA frame decode at exact PTS, audio decode, PNG/WAV
  round-trips, encoder capability detection.
- **audio** (14): Hann FFT bands/RMS/onset/tempo/chroma, mixer pan law + fades, PCM ring.
- **render** (8): device, **all 216 shaders compile on the real GPU (0 failures)**,
  solid-color oracle, preview/full readback, row stride.
- **integration lib** (6): 123 analytic generators render, 24 effects over plasma differ,
  determinism, time variation, solid oracle.
- **producer contract** (6): every producer family (text, vector, SDF/mesh, particle,
  physics, field, control) renders visible non-transparent content, deterministic and
  animated. SDF metaballs verified visually as raymarched geometry.
- **golden export** (3): generator→effects→MP4, media→present, text→export.
- **persistence** (6): atomic save + backup, corrupt/newer-schema rejection, autosave +
  recovery manifest + pruning, collect-media, relink search.
- **export job/queue** (3): frozen snapshot independence, persisted queue state, codecs.
- **export matrix** (6): PNG+WAV always; H.264 MP4 probed; HEVC/ProRes/GIF/JPEG when the
  encoder exists; frozen-snapshot export unaffected by later edits.
- **integrations** (16): **real OSC UDP loopback** through `rosc`, all declared types,
  malformed/oversize rejection, rate limiting, feedback encode; MIDI message parsing
  (CC/note/bend/pressure/clock), tempo-from-ticks.
- **app commands** (6, incl. GUI): create/catalog/add-scene/set-param/undo, stale-revision
  conflict, protocol-version rejection, full **GUI acceptance flow** (edit→save→reload).
- **diagnostics** (3): redacted capability report, path/device redaction, no secrets.
- **recording** (2): frame-exact CFR recording for a bounded duration; drop counter.

Also produced this run:
- **Tauri 2 + web GUI** (`apps/desktop`): builds, links, and launches; Rust owns all
  state/validation/rendering; the webview submits typed requests (`CommandHost`) and
  shows preview frames. `cargo build -p flux-desktop` succeeds.
- **`flux-desktop.exe`** launches (process stays alive; WebView2 present).
- **Real device enumeration** (`flux capture`): 5 microphones, 6 render/loopback
  endpoints, 1 camera class via WASAPI COM + PnP registry.
- **Real offline export** (verified with ffprobe): generator demo 1280×720/240f/384000
  samples; media demo 640×360/180f; golden 320×180/180f — all valid H.264/AAC.
- **Effects contact sheet** of 43 effects (`implementation/evidence/effects_contact_sheet.png`).

Performance (`cargo xtask benchmark`, 1920×1080): most generators ~2.5 ms p50.

## 2. Launch and use

CLI (headless):
```
cargo build -p flux-app --release
target/release/flux.exe doctor | diagnostics | capture | integrations
target/release/flux.exe demo-render gen.plasma out
target/release/flux.exe demo-media clip.mp4 out
target/release/flux.exe render project.flux.json out --codec hevc
target/release/flux.exe record project.flux.json rec_out --seconds 5
target/release/flux.exe contact-sheet out
```
Desktop GUI:
```
cargo build -p flux-desktop
target/debug/flux-desktop.exe
```
FFmpeg: `FLUX_FFMPEG` env var, `PATH`, or the default winget install.

## 3. Source implementation coverage

- 216 catalog entries with manifests + WGSL; all compile into GPU pipelines.
- FX001–FX028, FX100–FX123, 2D/math generators: distinct WGSL kernels + metamorphic tests.
- Producer contract (DECISIONS D007) covers vector/text/SDF/mesh/particle/physics/field/
  control rows with real deterministic content and the `execute_with_producers` renderer
  hook. SDF is a CPU raymarcher (correct, slower than analytic WGSL at 4K).
- Persistence, export matrix + job queue, recording, OSC/MIDI, platform enumeration,
  diagnostics, and the Tauri React-shell GUI command surface are implemented.
- `implementation/requirements.json`: 345 records — 12 verified, 330 implemented_unverified,
  blocked/pending where externals are missing.

## 4. Host verification

Verified on Windows x64 (RTX 2080 Ti, wgpu Vulkan backend). macOS paths: not run
(unverified). Live camera/screen *capture* needs a device + OS permission; enumeration is
verified.

## 5. Artifact readiness

- Release CLI: `cargo xtask package` → `target/release/flux.exe`.
- Desktop GUI: `target/debug/flux-desktop.exe` (dev build; unsigned).
- No signed installer produced; no signing identity configured.

## 6. Explicit blockers / unverified

- **NDI, Spout2, Syphon, Ableton Link, Max for Live `.amxd`**: external SDKs/hosts/licenses
  absent → `blocked_external` (adapters/capability records present).
- **macOS builds and macOS-only paths**: no macOS host.
- **Code signing / notarization**: no identity.
- **Vision models (G170–G177)**: not present; conditional nodes stay editable.
- **Live camera/screen/loopback capture**: enumeration verified; actual frame/audio
  capture needs a device and permission (not exercised here).
- **GUI pixel-level verification**: the Tauri app builds and launches and its command
  surface passes the acceptance flow; end-to-end webview screenshot verification was not
  captured headlessly.
- **SDF/particle real-time at 4K**: CPU producer is correct but not realtime at 4K.

## 7. Honest overall status

A runnable development build of Flux exists on Windows x64: exact-time document/editing,
a native wgpu renderer running the whole 216-entry catalog, deterministic offline export
to real H.264/HEVC/ProRes/GIF/JPEG/WAV with correct counts, real media import/export,
OSC/MIDI control, persistence with autosave/recovery/relink/collect, diagnostics with
redaction, real device enumeration, and a Tauri 2 + web GUI that builds and launches with
a tested command surface. macOS, signed installers, external protocols, and live device
capture remain unverified or blocked; this is not a full cross-platform release.
