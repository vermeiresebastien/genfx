# Flux — Final Report

Specification version 2.0. Updated after the completion batches (GUI, persistence, export
matrix, OSC/MIDI, live control bridge, Max bridge, platform capture, recording, diagnostics,
shader harness, docs). The four judgments are kept separate (VERIFICATION §8).

> ## Corrections — 2026-11-14
>
> Everything below this box was written on 2026-11-09 against an older tree and is kept
> as a historical record rather than silently rewritten. A full audit against HEAD
> `46cfd94` and re-measured at `e84fe67` found these lines no longer true. Read together with
> `implementation/DECISIONS.md` D014-D019, `implementation/STATE.json` and
> `docs/IMPLEMENTATION-ARCHITECTURE.md` §8.
>
> - **Catalog size is 217, not 216** (`flux list` and `flux doctor` both report 217).
>   Every "216" below, including "216 shaders compile on the real GPU", is one short.
> - **"§6b NDI — fully working (discovery, send, receive verified)" is wrong about the
>   frame.** The send/receive tests broadcast a synthetic 640×360 gradient built in
>   `crates/app/src/main.rs`; no wgpu frame has ever crossed that boundary, and feature
>   `ndi` is off by default. Discovery of 5 live sources is real. This also contradicts
>   the blockers paragraph at `FINAL-REPORT.md:81` (which lists NDI as
>   `blocked_external`) and `implementation/UPSTREAM.md:46-49` (which says no SDK
>   headers are shipped). Reconciliation: vendored headers and discovery are real;
>   live Flux output is not.
> - **"cargo test --workspace is green" is true but weak.** Measured 2026-11-14: 196
>   passed, 0 failed across 40 test binaries — and 14 of those passes are GPU tests that
>   return early when no device is created. `golden.rs` asserts file sizes, not pixels.
> - **"live control bridge -> engine commands" needs the nuance**: the apply command
>   exists (`GuiCommand::LiveOsc`, `crates/app/src/commands.rs:478`), the
>   transport→command wire does not, and no OSC output path exists at all.
> - **"deterministic undo/redo" holds only for accepted inverses**: `undo`/`redo`
>   re-enter `execute` without `apply`'s snapshot/rollback.
> - **Not covered below at all**: the node-graph editor that replaced the clip timeline
>   (D013), the slider/monitor-window work, the webview bundling, and the fact that
>   feedback loops currently render black.
> - **"CI workflow … VERIFY: PASS" did not hold at `e84fe67`; the blocking step is now
>   cleared.** Measured 2026-11-14 at HEAD `e84fe67`: `infrastructure/verify.ps1` exited 1
>   with `VERIFY: FAIL`, because `cargo fmt --all -- --check` — the first check step in
>   `ci.yml` — reported 56 diffs across 12 files, all of it style written on 2026-11-10 and
>   never rustfmt-ed. Every other step that ran passed (149 lib tests, 16 integration, 3
>   golden, catalog 217, doctor, 217 shader compiles with 0 failures, export_shaders,
>   maxpat, fixtures, both builds, NDI). In GitHub Actions the fmt failure aborted the job
>   before the suite even started, which is why the green local result and the red CI result
>   were both true. `cargo fmt --all` has since been applied, `--check` exits 0, and the
>   workspace suite behind the reformat still reports 196 passed / 0 failed. **The full
>   `verify.ps1` run has now been repeated, and it passes:** measured 2026-11-15 at HEAD
>   `75da874` it exited 0 with `VERIFY: PASS`, 15 steps passed, 0 failed, 2 blocked_external
>   (`link-integration`, `spout-enumeration`). The two harness defects D019 found in that
>   script — the chained step that masked exit codes, and skipped gates recorded as
>   `ok: true` — were repaired first, along with a third `$LASTEXITCODE` leak the repair
>   exposed. See D019 and D020.

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

Launched and screenshotted: library populates with all 216 entries by category, preview
renders real GPU plasma, inspector lists editable parameter controls, timeline shows one
clip per scene. `implementation/evidence/gui-release.png` is the release build running.

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

### 6b. NDI — fully working (discovery, send, receive verified)
NDI Tools v6.3.2 is installed and auto-detected. `apps/ndi-bridge/ndi_shim.cpp` is
compiled against the **official NDI SDK headers** (vendored by the `ndi-sdk` crate), so
every struct layout is correct — no ABI guessing. `crates/integrations/src/ndi_io.rs`
(feature `ndi`) runtime-loads the shim and exposes a live receiver and sender.

Verified on this host:
- **Discovery**: `flux ndi` found **5 live sources** including the NDI Test Pattern
  (`implementation/evidence/ndi-discovery-live.txt`).
- **Send → receive (cross-process)**: `flux ndi-send FluxE2E` broadcast a 640×360
  animated pattern; a second process `flux ndi-recv FluxE2E` connected by discovery and
  received a **real 640×360 frame** written to PNG
  (`implementation/evidence/ndi-received-frame.png`, 108,905 bytes) — pixel content
  matches the sent gradients (`ndi-io.txt`).
- 2 shim tests pass (load→init→create sender).

No remaining NDI blocker.

**Spout2**: prebuilt `SpoutLibrary.{dll,lib}` (x64, BSD-2) is present but exposes a **C++
pure-virtual** interface, so `apps/spout-bridge/spout_shim.cpp` wraps it in a flat `extern
"C"` ABI and links `SpoutLibrary.lib` → `spout_shim.dll`. `crates/integrations/src/spout.rs`
(feature `spout`) runtime-loads the shim; `flux spout` enumerates the OS-wide sender
registry and `flux spout-send` creates an offscreen WGL context. Verified: sender
enumeration and GL-context init (`evidence/spout-registry.txt`). **Limitation:** Spout2's
`CreateSender`/`SendImage` do **not** succeed through a bare offscreen WGL context (Spout
expects the app's real rendering context). The correct fix — sending from Flux's existing
wgpu device via a DX11 shared texture (SpoutDX) — is a larger interop task, so Spout live
send is reported as a limitation while enumeration works. **Syphon**: macOS-only.

### 6a. Ableton Link — supported and verified (licensing resolved)
Following the upstream audit (`implementation/UPSTREAM.md`, DECISIONS D010/D011), Ableton
Link was the only external integration buildable on this host without a proprietary SDK.
A thin raw-FFI adapter (`crates/integrations/src/link.rs`, feature `link`) wraps the
official `abl_link` C API, built from `https://github.com/Ableton/link @ 5ba907ae`.

**Licensing (DECISIONS D012):** the product owner has stated Flux is not sold/distributed
commercially, so Link's GPLv2-or-later terms are acceptable. The redistribution constraint
is lifted for this project; Link is a supported integration.

Verified on this host:
- 6 automated tests: enable + start/stop sync, clock advance, set/read playing,
  **two sessions discovering each other and converging on tempo**, **single-master tempo
  arbitration** (one proposer's tempo adopted by the peer), and **start/stop propagation**.
- **Cross-process**: two `flux.exe link` processes saw `peers=0 → peers=1` and shared a
  synchronized beat/phase clock (`implementation/evidence/link-a.txt`, `link-b.txt`).

Build: set `FLUX_ABLETON_LINK_LIB` (dir containing `abl_link.lib`) and
`cargo build --features link`. With the feature off, `protocol_status` reports
`unavailable` truthfully (SDK-less machines still build).

### 6c. Vision/models (G170–G177) — real inference verified (AT075 + AT077)
Model-backed conditional nodes have a real **model registry** (present/missing/incompatible;
missing is a non-fatal `SdkMissing` diagnostic — node stays editable, never fake output).
**Real inference works**: ONNX Runtime via the `ort` crate (statically linked, feature
`vision`) runs **Depth Anything v2** on `flux depth` → 518×518 depth in ~330 ms
(`implementation/evidence/vision-inference.txt`, `depth-demo/depth.png` — a real smooth
depth map). Weights are user-installed via `FLUX_MODEL_DEPTH`. AT075 and AT077 verified.

### 6d. Max for Live — source verified, `.amxd` packaging documented
Two `.maxpat` source devices + `bridge-osc.js` + `check-maxpat.cjs` (structural conformance
PASS) are provided and verified. The `.amxd` binary requires a Max/Max-for-Live host; the
full reproducible packaging procedure is documented in `apps/max-for-live/README.md`
(open → save as Max for Live device → metadata → load test). The `.amxd` bytes remain an
external step.

## 7. Honest overall status
A runnable Windows x64 development build: exact-time editing, native wgpu renderer running
the whole 216-entry catalog, deterministic export to real H.264/HEVC/ProRes/GIF/JPEG/WAV,
real media import/export, OSC/MIDI control wired into the document, Max bridge + source
patches, persistence with autosave/recovery/relink/collect, redacted diagnostics, real
device enumeration, and a Tauri 2 + web GUI that builds and launches with a tested command
surface. macOS, signed installers, external protocols' live I/O, and live device capture
remain unverified or blocked.
