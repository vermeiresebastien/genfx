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
>   `ndi` is off by default. Discovery really did run — the "5 live sources" was that
>   afternoon's LAN and reads 0 today (§6b). This also contradicts the blockers paragraph
>   at `FINAL-REPORT.md:151-152` (NDI listed as `blocked_external`) and `UPSTREAM.md:84`
>   ("**No SDK headers are shipped**"). Both are true of this tree: the headers the shim
>   compiles against sit in this machine's cargo registry, not in the repository.
> - **"cargo test --workspace is green" is true but weak.** Measured 2026-11-15 (D033): 220 passed,
>   0 failed across 40 result lines (29 test binaries + 11 doctests) — and 14 of those passes
>   are GPU tests that early-return with no device. `golden.rs` asserts file sizes, not pixels.
> - **"live control bridge -> engine commands" was the optimistic line, and it is now
>   the accurate one — half of it.** Since D022 the Flux Desktop process binds the OSC
>   port itself and applies `param` addresses to the document (measured by UDP loopback,
>   not by watching a screen). Still not true: transport/trigger/preset/bridge controls
>   change nothing, and there is no OSC output path at all.
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
>   workspace suite behind the reformat reported 196 passed / 0 failed at that commit; it is
>   **220 passed / 0 failed / 0 ignored across 40 result lines** (`90d9e59` was already 204 —
>   196 +8 D022's — then +1 D027, +2 D028, +7 D031, +4 D032, +2 D033). **The full `verify.ps1`
>   run has now been repeated, and it passes:** measured at HEAD `75da874` it exited 0 with
>   `VERIFY: PASS`, 15 steps passed, 0 failed, 2 blocked_external (`link-integration`,
>   `spout-enumeration`), re-measured identically at `90d9e59`. D027 re-keyed both gates on
>   machine state — the spout shim resolves itself, and Link needed its directory exported until
>   D029 taught that gate to search — at `cf95a2f`, dirty tree, the run is **17 passed / 0 failed /
>   0 blocked_external**, the first with nothing blocked. D032 added `requirements-freshness`:
>   18-0-0 at `a91ebda` (D032) and again at `7adb5e6` (D033), both `gitDirty false`. The two D019 defects, plus a `$LASTEXITCODE` leak, were repaired at D020.
> - **§1 is a dated inventory: its numbers are the 2026-11-09 capture, and several have moved.**
>   The catalog is **217 entries**, not 216 — re-measured at `90d9e59` today: `flux doctor` prints
>   `catalog entries: 217`, `xtask test-gpu --profile baseline` reported "compiled 217 shaders, 0
>   failures" (exit 0) and `xtask bin export_shaders` "wrote 217 shaders … (0 naga failures)" (exit
>   0). The per-suite figures below count test *groups* as they were then; the eight lib targets
>   contain **157** tests today (app 34, integrations 27, domain 30, engine 22, catalog 16,
>   audio 14, render 8, media 6). The 216 in the "Launched and screenshotted" paragraph stays,
>   because that sentence reports what was on screen when `gui-release.png` was captured on
>   2026-11-09 — the catalog has since grown, and nobody has re-opened that window since.
>   §3's command block, by contrast, has been rewritten to match CI parity rather than the old
>   copy — see D025.

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
- **app control** (12): OSC/MIDI→engine command mapping, learn, coalescing, stale-binding,
  plus 7 that send real UDP bytes through `control::OscBridge` and assert the document,
  the revision, the coalescing and the counters moved (D022).
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
  learned bindings and 16 ms coalescing. The desktop process starts an OSC listener on
  `FLUX_OSC_PORT` or 9000 and reports it through `osc_status`; the webview is only told that
  an external edit landed. Loopback-tested — nobody has watched a Live fader move a Flux pixel.
- **Live source/sink interfaces** (`crates/integrations/src/live.rs`): NDI/Spout/Syphon
  contracts, frame validation, truthful protocol status.
- **Real device enumeration** (`flux capture`): 5 mics, 6 loopback endpoints, 1 camera class.
- **Real offline export** (ffprobe-verified): 1280×720/240f H.264; 640×360; golden 320×180.
- **Docs** (`docs/`): user guide and file/protocol formats.

## 2. Launch

CLI: `flux doctor|caps|capture|integrations|diagnostics|demo-render|render --codec …|
record|recover|collect|contact-sheet`. GUI: `cargo build -p flux-desktop` → `flux-desktop.exe`.

## 3. Verification commands
These are the commands `infrastructure/verify.ps1` and `.github/workflows/ci.yml` actually run —
the `--test-threads=1` and `--profile` arguments are not decoration, and `fmt` is the first step,
so a red `fmt` aborts everything below it in CI.
```
cargo fmt --all -- --check                   # gate 1: style
cargo test --workspace -- --test-threads=1    # all suites (serialised: GPU device creation races)
cargo run -q -p xtask -- test-gpu --profile baseline   # 217 shaders on GPU, 0 failures
cargo run -q -p xtask --bin export_shaders    # WGSL export + Naga validate (217, 0 failures)
node apps/max-for-live/check-maxpat.cjs       # Max patch structure
infrastructure\verify.ps1                     # full gate, no -Quick; writes verify.json
```

## 4. Host verification
Windows x64 (RTX 2080 Ti, wgpu Vulkan). macOS: not run (unverified). Live capture needs a
device + permission (enumeration verified). Browser WebGPU conformance: page provided;
headless GPU browser not available here.

## 5. Artifact readiness
Release CLI `target/release/flux.exe`; dev GUI `target/debug/flux-desktop.exe`. No signed
installer (no identity configured).

## 6. Explicit blockers (not faked)
Third-party NDI/Spout2 interop, a second machine for NDI over a real network, Max `.amxd` host:
interfaces + capability records present, `blocked_external`. Ableton Link's license terms were a
blocker only until D012 lifted them, and D027 retired `blocked_external` on the Link and Spout
gates — they are now keyed on machine state and ran. The NDI and Spout2 runtimes *are* installed
on this host (`Processing.NDI.Lib.x64.dll`, `apps/ndi-bridge/ndi_shim.dll`,
`apps/spout-bridge/SpoutLibrary.dll` + `spout_shim.dll`), so "NDI SDK blocked" would be
false — what is blocked is the evidence that a non-Flux peer can consume a Flux frame, and
there is no Flux-frame producer to send (§6b). Syphon is macOS-only and this is a Windows
host, so that one really is an absent SDK. macOS builds unverified. Live camera/screen capture
and GUI webview pixel verification need a device/session. SDF/particle CPU producers not
realtime at 4K.

### 6b. NDI — transport verified, product path absent (re-measured 2026-11-15)
NDI Tools v6.3.2 is installed and auto-detected. `apps/ndi-bridge/ndi_shim.cpp` is
compiled against the **official NDI SDK headers**, so every struct layout is correct — no ABI
guessing. The older wording here ("vendored by the `ndi-sdk` crate") was vague about where those
headers live, and it matters: `apps/ndi-bridge/build_shim.bat:7` points at
`C:\Users\Sebastien\.cargo\registry\src\…\ndi-sdk-0.2.0\vendored\include` — a copy of the
`ndi-sdk` crate in **this machine's cargo registry**, declared by no `Cargo.toml` and absent from
`Cargo.lock`. On another host that script prints `MISSING_HEADERS` and exits 1, so a fresh clone
cannot rebuild the shim at all; and `ndi_shim.dll` itself is gitignored (`.gitignore:53`), so the
binary that does the sending is a 2026-11-09 artifact of this machine, like the Link library and
the Spout shim. `crates/integrations/src/ndi_io.rs` (feature `ndi`) runtime-loads that shim and
exposes a live receiver and sender.

Verified on this host, all of it in `implementation/evidence/ndi-2026-11-15.txt`:
- **Runtime + discovery**: `flux ndi` initialises the runtime and enumerates. It found
  **0 sources** on 2026-11-15. The "5 live sources" recorded here on 2026-11-09
  (`implementation/evidence/ndi-discovery-live.txt`) was whatever the LAN was
  broadcasting that afternoon, so it is an environment reading, not a capability.
- **Send → receive, same process**: `flux ndi-loopback` advertises a 640×360 sender,
  resolves its advertised name (`DESKTOP-MLIF3MV (Flux Loopback 4044)`) through
  discovery, connects a receiver, and gets back 921,600 bytes = 640\*360\*4 with
  non-zero content. Exit 0.
- **Send → receive, cross-process**: `flux ndi-send FluxE2E12 16` in one process,
  `flux ndi-recv FluxE2E12` in a second received a real 640×360 frame written to PNG;
  the red and green ramps in the received pixels match the gradient the sender
  generates.
- **Failure now reports failure**: `ndi-recv` on a name that does not match, and
  `ndi-recv` while nothing is broadcasting, both exit 1 (`SdkMissing`). Before this
  session they printed the failure and exited 0 — which is why "verified" could sit
  next to a command that had never worked (`ndi-loopback` was one of them).

What is **not** verified: there is no wgpu→NDI producer, so a Flux render never crosses
this path; no third-party consumer (Resolume, NDI Video Monitor, TouchDesigner) has
received from Flux; nothing past same-machine loopback is proven. The repeatable harness
gate is now named `ndi-runtime-loads`, which asserts only that the DLL loads and a
discovery query does not crash — see D023. Its `ndi::` filter selects exactly the 2 tests
in `crates/integrations/src/ndi.rs` (`runtime_loads_and_initializes`,
`discovery_returns_state_without_crash`); `ndi_io.rs` has 1 test
(`shim_loads_and_initializes`), `ndi::` does not match it, and both ndi.rs tests return
early when the runtime is absent — so before the gate was added, the harness proved less
about NDI than its name suggested, and on a machine without NDI it proved nothing at all
while recording a pass.

**Spout2**: prebuilt `SpoutLibrary.{dll,lib}` (x64, BSD-2) is present but exposes a **C++
pure-virtual** interface, so `apps/spout-bridge/spout_shim.cpp` wraps it in a flat `extern
"C"` ABI and links `SpoutLibrary.lib` → `spout_shim.dll`. `crates/integrations/src/spout.rs`
(feature `spout`) runtime-loads the shim; `flux spout` enumerates the OS-wide sender
registry and `flux spout-send` creates an offscreen WGL context. Sender enumeration was re-run
2026-11-15 (`implementation/evidence/spout-2026-11-15.txt`): it returns **0 senders** and exits 0,
so the call path works and nothing on this host is currently broadcasting; the GL-context half is
still the 2026-11-09 capture in `spout-registry.txt` and has not been repeated. D027 retired the
`blocked_external` on this gate by keying it on the shim's presence rather than on
`FLUX_SPOUT_SHIM` being set — the test behind it returns early when the runtime is absent and
cargo still records a pass, so the harness now resolves an absolute shim path and exports it —
and repaired the CLI's exit code: asked from a directory where no candidate resolves, `flux spout`
now exits 1 (`SdkMissing`) instead of printing the failure and exiting 0. `capability_report()`
answers `verified: false` for spout in **every** build — it used to answer
`cfg!(feature = "spout")`, which made a claim about evidence depend on how the binary was
compiled. **Limitation:** Spout2's
`CreateSender`/`SendImage` do **not** succeed through a bare offscreen WGL context (Spout
expects the app's real rendering context). The correct fix — sending from Flux's existing
wgpu device via a DX11 shared texture (SpoutDX) — is a larger interop task, so Spout live
send is reported as a limitation, and the enumeration half is a 2026-11-09 measurement that
nothing on this host has repeated. **Syphon**: macOS-only.

### 6a. Ableton Link — supported; 7 tests ran here 2026-11-15, 52 clean after D030
Following the upstream audit (`implementation/UPSTREAM.md`, DECISIONS D010/D011), Ableton
Link was the only external integration buildable on this host without a proprietary SDK.
A thin raw-FFI adapter (`crates/integrations/src/link.rs`, feature `link`) wraps the
official `abl_link` C API, built from `https://github.com/Ableton/link @ 5ba907ae`.

**Licensing (DECISIONS D012):** the product owner has stated Flux is not sold/distributed
commercially, so Link's GPLv2-or-later terms are acceptable. The redistribution constraint
is lifted for this project; Link is a supported integration.

Assertions that exist in the tree — all 7 ran on this host on 2026-11-15, each of them holding
both sessions in one process; see the caveat below:
- 7 automated tests: enable + start/stop sync, clock advance, set/read playing, D030's poll guard,
  **two sessions discovering each other and converging on tempo**, **single-master tempo
  arbitration** (one proposer's tempo adopted by the peer), and **start/stop propagation**.
- **Cross-process**: two `flux.exe link` processes saw `peers=0 → peers=1` and shared a
  synchronized beat/phase clock (`implementation/evidence/link-a.txt`, `link-b.txt`). Those
  transcripts print `playing=false` on every line, so they show discovery and a shared clock —
  not start/stop propagation — and they are a 2026-11-09 capture that has not been repeated.

Build: set `FLUX_ABLETON_LINK_LIB` (dir containing `abl_link.lib`) and
`cargo build --features link`. With the feature off, `protocol_status` reports
`unavailable` truthfully (SDK-less machines still build).

**What the list above does not cover (re-measured 2026-11-15).** All 7 tests exist only inside a
`--features link` build, and `link-integration` was recorded `blocked_external` in every run this
repository has ever recorded — not because `abl_link.lib` is missing (it is built and sitting in
the gitignored `_upstream/`, dated 2026-11-09) but because the gate keyed on
`FLUX_ABLETON_LINK_LIB` being set. D027 keys it on the variable *and* the file, and with it
exported the group ran here for the first time: 13 invocations, **11 clean and 2 failures of
`set_playing_reflected_in_state`** (`crates/integrations/src/link.rs:203` as it then stood: the
assertion that a `set_playing(false)` shows in the state captured on the very next line). D030
measured why, and bounded the read-back instead of weakening it — the SDK re-writes its client
state from its own IO thread with no timestamp preference (`evidence/link-flake-2026-11-15.txt`).
A gate that went red twice in thirteen runs is still no hard CI step, so CI — which sets none of
these variables — records it blocked, and the acceptance test's remaining clauses (Max /
conditional Link transport, seek/loop, "no control-as-audio confusion") are still asserted by
nothing, which is why AT063 reads `implemented_unverified`: corrected in the generator arm
(`crates/xtask/src/bin/requirements.rs`), as a hand-edit is undone by the next regeneration (D025).

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
