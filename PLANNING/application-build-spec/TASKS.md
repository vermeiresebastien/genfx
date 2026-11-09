# Ordered implementation work

Execute tasks sequentially unless an external prerequisite blocks only that task. Continue independent tasks in dependency order. A task is not verified because files exist. Record its specific gate and actual command result. Later changes rerun affected gates. All required catalog rows are implementation work, including advanced generators; stages are not scope exclusions.

## 1. Repository commands to implement

Create `.cargo/config.toml` alias `xtask = "run -p xtask --"`. These are project commands the agent must implement, not assumed preinstalled programs:

```text
cargo xtask doctor
cargo xtask fixtures
cargo xtask check --scope <domain|media|render|audio|ui|catalog|integration|all>
cargo xtask catalog --validate
cargo xtask catalog --contact-sheets
cargo xtask test-gpu --profile baseline
cargo xtask scenario <AT-id>
cargo xtask benchmark --profile <ordinary|particles|sdf|fluid|recording>
cargo xtask package --target <windows-x64|macos-arm64|macos-x64>
cargo xtask report
pnpm dev
pnpm build
pnpm typecheck
pnpm test
pnpm test:e2e
```

Each xtask returns nonzero on failure, emits concise console result and JSON evidence with tool versions/host/exit code. Unavailable hardware is `unavailable`/`blocked`, never pass. `check --all` must not call itself recursively. Implement supported flags and validate unknown flags. No placeholder command that just prints success.

## 2. Detailed tasks

### T00 — Inspect, resolve dependencies, and prove native integration

Preconditions: authorized repository and this specification.

1. Read applicable repository instructions and existing code. Record git status without overwriting changes. Inventory OS/architecture, Rust/Node/pnpm/build tools, GPU, FFmpeg executable/headers/libs, CMake/Meson/Ninja/Clang where needed, Max/Live, SDKs, signing availability, second display/camera/audio.
2. Create implementation records and feature requirement registry. Mark R and catalog/task/test IDs. Identify real blockers and continue independent paths.
3. Select compatible stable releases; record exact chosen versions, license IDs, checksums/commits, required features and minimum compiler. Pin Rust and pnpm and commit lockfiles. Select Tauri 2 native WindowBuilder feature path explicitly: if selected release gates native windows behind `unstable`, enable that documented feature, pin version, and isolate usage in platform presentation module. Do not create a competing winit main event loop.
4. Build a tiny local integration spike: Tauri editor plus native window, raw handles, wgpu clear/present/resize/close; decode one real FFmpeg fixture; create CPAL stream; render ThorVG SVG into RGBA. Test main-thread/lifetime behavior. Keep resulting reusable code, delete only task-owned disposable scaffolding.
5. Verify Windows FFmpeg development headers/import libraries/DLLs agree; macOS dylib target architecture/rpaths agree. A binary executable alone is insufficient for ffmpeg-next. If build requires package downloads use official/pinned build recipes; do not fetch random binary bundles.
6. Record chosen APIs in DECISIONS. When a selected dependency fails a supported feature, inspect its actual version documentation and fix compatibility before building all modules.

Gate: host-native window and decoded/vector frame displayed, clean close; toolchain and dependency report; blocked nonhost OS separately recorded. A dependency detector alone cannot satisfy this gate.

### T01 — Workspace, domain, and persistence foundation

1. Create Rust workspace and React/Tauri app following ARCHITECTURE layout. No cloud backend/login.
2. Implement rational time, frame rate, IDs, document schema, asset graph/timeline descriptors, typed errors and default settings.
3. Generate TS/JSON types from domain; add schema drift check. Implement strict document validation and size limits.
4. Implement command dispatcher/revisions/transactions/inverses, create/open/save/Save As, atomic writes/backups, dirty state and basic undo/redo.
5. Add deterministic test asset fixtures and project fixtures generated locally. Implement xtask doctor/check/fixtures and structured evidence.

Gate: AT001, AT002, AT003. Rust unit tests and TS typecheck pass. Save/reopen preserves IDs, exact rational values and unknown extension metadata.

### T02 — Native renderer and authoritative preview

1. Create render thread/device/texture pool/surface lifecycle. Support headless target and native output window.
2. Implement RGBA upload, fullscreen blit, color transform, premultiplied compositing, target clears, resize and device error state.
3. Add async reduced-preview readback with row padding, binary transport and bounded displayed URL lifetime.
4. Connect editor controls to native output; transport/stats subscriptions throttled. Keep texture handles out of JSON.
5. Add display/monitor selection, fullscreen Escape and secondary window close/reopen.

Gate: AT004, AT005, AT006. Same frame ID in preview/native output; two outputs do not double-step generator counter. GPU fixture readback matches color/alpha oracle.

### T03 — First real media playback and export

1. Probe video/audio and select streams. Decode frame with correct PTS/orientation/SAR/color. Build bounded decode/seek generations.
2. Put a video clip on one track. Play/pause/seek/frame-step/loop, clock and source mapping.
3. Decode/resample audio, basic device output and deterministic offline mix.
4. Export 5-second composition to PNG+WAV, encode MP4 if supported, reimport/probe it. Use frozen revision and exact frame/sample counting.
5. Add basic file bin, import errors, progress and cancellation.

Gate: AT007, AT008, AT009. Do not defer export until after all effects. Real test-media round trip and A/V flash/impulse alignment are mandatory.

### T04 — Graph model/compiler and editable node UI

1. Add typed ports and full DAG validation, stable IDs, group flattening, ScopeInput/Output, bypass.
2. Compile pass plans with resource/lifetime analysis, pipelines and last-valid-graph swap. Implement explicit temporal edge infrastructure.
3. React Flow canvas add/connect/remove/duplicate/group/copy/paste/search, selection/inspector and revision-safe commands.
4. Implement manifest registry, generated parameter controls/help and dynamic invalid-connection messages.
5. Add Color adjustment, source, blend, mask, output and Noise nodes. Save/reopen graph and undo transactions.

Gate: AT010–AT013. Wrong socket/cycle/missing input rejected; valid branched graph renders. No React-only execution engine.

### T05 — Audio analysis, modulation, and automation

1. Add device/source selection, preallocated rings and analysis worker.
2. Implement declared FFT normalization, bands, RMS/peak, envelopes, onset and feature timestamps. Add spectrum/waveform/spectrogram textures.
3. Add mapping fold, normalization/curve/smoothing, inspector contribution view, LFO, random hold, event and expression nodes.
4. Implement keyframe lanes/interpolation and modulation order. Changes persist; runtime overrides expire/release per policy.
5. Connect bass to displacement and onset to particle/pulse control, run live and offline with same input.

Gate: AT014–AT017. Silence/sine/impulse numeric tests, bounded callback audit, mapping math oracle, measured visual response.

### T06 — Conventional timeline and mixer

1. Add multi-track layout, clips/thumbnails/waveforms, zoom/scroll/snap, range/playhead, lock/mute/solo.
2. Implement trim, move, split, ripple/insert/overwrite, slip/roll, duplicate, reverse/rate/freeze/loop and linked A/V edits.
3. Add clip/track/master graphs and adjustment tracks with exact render order.
4. Add transitions with handle validation, audio fades/pan/gain, keyframe UI and markers.
5. Add safe reverse GOP cache/proxy route, waveform pyramid and thumbnails; generate/cancel/relink proxies.
6. Add pitch-preserving time-stretch using Signalsmith Stretch through the specified shim; compare with normal resampling and account for latency/sample counts. Document exact quality/rate limits.

Gate: AT018–AT022. Property tests for edits, no A/V desync on split/ripple, graph scopes and actual transition overlap verified.

### T07 — Exact requested effects and fills

1. Implement shader ABI/packing generators and shared sampling/noise/blur/edge helpers.
2. Implement FX001–FX019 one at a time with specified controls and distinguishing tests.
3. Implement FX020–FX028 as generator sources and attach to media/timeline/graph libraries.
4. Implement actual filter preset graphs, wet/dry/masks and meaningful neutral behaviors.
5. Generate contact sheet, help, parameter docs, tests and thumbnails from manifests.

Gate: AT023, each effect's own acceptance invariant, all 28 present in UI and actual export. No Gooey merge/Moiré omissions. No creator columns.

### T08 — Additional effect families and temporal state

1. Implement FX100–FX123 including all listed variants using shared kernels.
2. Add FeedbackRead/Write, canonical-frame history, freeze/echo/delay and reset/invalidation.
3. Add stateful runtime seek, checkpoints, warm-up and cache keys before adding simulations.
4. Build all named preset recipes and real graphs. Ensure technical style versus geometry-dependent modes report required data.

Gate: AT024–AT026. Every family variant covered; sequence/seek/export parity; no unbounded history.

### T09 — General generator source system and 2D/math library

1. Add generator assets/clips, virtual source library, seed/time/duration/loop controls and typed fields.
2. Implement analytic G001–G023/G025–G026 and stateful G024/G027–G028, using shared fill kernels.
3. Implement field→color/mask/displacement converters and field metadata validation.
4. Add animated thumbnails evaluated at fixed sample time, not continuously rendering all catalog cards.

Gate: AT027–AT029. Every required 2D row has meaningful controls, deterministic frames and exported output. Test automaton known states and fBm seams.

### T10 — Vector scene/SVG/Lottie

1. Implement safe SVG import report and internal editable VectorScene tree.
2. Add shape/path editor, selection, Bezier handles, fill/stroke/gradient controls, group transforms and undo.
3. Implement G030–G045, arc-length path/trim/repeat/morph and actual Lottie frame seek.
4. Add path-to-mask/mesh/particles interfaces and vector layer caching. ThorVG is behind tested ownership-safe C ABI.
5. Create original sample SVG and Lottie fixtures with holes, gradients, text and grouped motion.

Gate: AT030–AT032. Unsupported SVG content reported; logo morph and draw-on export match preview; no timer-driven divergence.

### T11 — Image animation and typography

1. Implement G050–G068 including manual depth parallax, mesh warp/pins, image fragments, sprites, time history and motion brush.
2. Build robust text shaping/font fallback/render paths, mask/vector/mesh outputs.
3. Implement G070–G079, kinetic text and data graphics; keep shaped cluster ordering.
4. Make animation templates editable and use generated/bundled licensed assets.

Gate: AT033–AT035. Imported image animates through normal effects/export; glyph holes/RTL and transparent warp edges; image-sequence ordering.

### T12 — SDF/3D blob system

1. Implement SdfField typed AST, validation/compiler/cache, bounded primitives/Boolean/domain/noise operators G080–G086.
2. Render raymarch color/depth/normal/mask via G087–G088 with hit/step budget and output-resolution scaling.
3. Add default breathing/metaball/audio blob templates and inspector controls.
4. Add marching cubes G089 as cancellable conversion and GLB geometry export.

Gate: AT036–AT038. Analytic sphere oracle, smooth merge, finite worst-case execution, correct silhouette/depth, animated blob through VHS/CRT export.

### T13 — Mesh scenes and imported model animation

1. Build small native Scene/Geometry/material/light/camera layer over wgpu; render same working color/alpha convention.
2. Implement G090–G107: primitives/parametric surfaces/deform/instances/terrain/voxels/clouds/architecture/ocean.
3. Implement GLB/glTF/PLY loaders, resource budgets, real animation clips, skins and morph targets. Unsupported extensions report clear error.
4. Add actual geometry wireframe/depth/normal/motion passes, camera animation and transparent scene compositing.

Gate: AT039–AT041. Imported animated model, correct scene axes, mesh+SDF depth, transparent background; resource and instance caps.

### T14 — GPU particles and field agents

1. Implement capacity-safe particle state, deterministic spawn IDs/ticks and alive/dead handling.
2. Implement G110–G120 emitters/forces/lifetime/over-life/collision/renderer/trails/morph/boids/slime/events.
3. Connect image/text/path/mesh emitters; stable point correspondence.
4. Checkpoint particle/field state and replay input event logs. Add all required particle templates.

Gate: AT042–AT044. Same spawn schedule across presentation rates; no overflow; image morph and MIDI burst; boid neighborhood bounded.

### T15 — Physics, cloth, fluids, and simulation seeking

1. Integrate Rapier world, fixed-step control and snapshot/restore G130–G134/G137.
2. Implement bounded PBD cloth/jelly G135–G136; pins/constraints/collision and stable dt.
3. Implement G140–G148 fluid/water/material/field solvers with explicit approximation modes.
4. Validate solver-specific invariants, resource bounds, replay and checkpoint invalidation.
5. Test switching scenes and exporting subranges of simulations with full warm-up.

Gate: AT045–AT048. Divergence reduction, finite outputs, cloth pin stability, replay parity, no silently reset export.

### T16 — OSC/MIDI/data/interaction control

1. Implement protocol addresses/bundles/types/timetags, validation/limits, mappings/learn, feedback/loop prevention.
2. Implement MIDI device enumeration, CC/notes/bend/pressure/MPE/clock and output feedback.
3. Implement G150–G162 plus event sequencer, chart source and file watcher.
4. Record timestamped controls/takes and replay them for export. Expose packet/source status and stale policy.

Gate: AT049–AT053. Real loopback packets and MIDI device where present, malformed/rate-limited robustness, duplicate triggers and timed bundle tests.

### T17 — Native capture and system audio

1. Implement real Windows camera/Graphics Capture/WASAPI loopback and macOS AVFoundation/ScreenCaptureKit adapters under platform cfg.
2. Add permission flow, real capability enumeration, device modes, timestamps, crop/region/cursor and source loss/reconnect.
3. Integrate captured frames/audio into same source contract and record them as takes.
4. Inspect actual capture color/orientation/scale, window resize and default audio device change.

Gate: AT054–AT057 on each real OS. A fixture is not webcam/screen evidence. Nonhost platform remains implemented_unverified until a real test host runs it.

### T18 — External video protocols

1. Implement Spout2 Windows and Syphon macOS native shims, enumeration/send/receive/lifetime/resize.
2. Implement CPU-copy baseline and label measured interop route; optimize only actual bottlenecks.
3. Implement authorized NDI SDK adapter and loopback helper when SDK available; otherwise complete loader/interface code that can be compiled only with official headers and record blocker.
4. Test losing sender/consumer, cross-resolution changes, alpha/orientation and GPU adapter mismatch.

Gate: AT058–AT060 with actual external sender/receiver. Detector-only paths cannot pass.

### T19 — Max for Live companions and transport

1. Build valid source `.maxpat` devices, pass-through signal/MIDI, meters, feature extraction and eight macros.
2. Bundle a tested OSC encoder/decoder helper. Prefer a local `node.script` helper using Node Buffer/dgram when built-in OSC objects are unavailable; verify Max-provided Node API/version. Keep all UDP outside DSP callbacks.
3. Implement desktop handshake/sequence/heartbeat/feature stale handling and transport epoch mapping.
4. Test OSC binary interoperability with rosc. Handle multiple bridge instances.
5. With Max/Live installed, save/freeze actual `.amxd`, automate macros, save/reopen Set and verify sound unaffected.
6. Add conditional native Link adapter when licensing permits and official integration tests are runnable. Otherwise record independent Max transport support accurately.

Gate: AT061–AT063. Source JSON structural tests are necessary but insufficient for `.amxd` device verification. No fake packaging.

### T20 — Live performance, scene routing, and recording

1. Implement Live/Hybrid workspace, preset search, scene slots, quantized transitions, macro bank and panic/blackout control.
2. Route one authoritative texture to preview/native monitors/external outputs; output resize/layout per sink.
3. Implement recording with audio/control take linking, bounded queues, CFR gap handling, recovery segments and disk/error stop.
4. Add source crossfades with resource preallocation and visible overload handling.

Gate: AT064–AT066. Editor stays usable, second output never double-steps, recording reimports with actual control response and A/V alignment.

### T21 — Export matrix and final media correctness

1. Complete codec capability UI, MP4/HEVC/ProRes/image-sequence/GIF/WAV paths and transparent output validation.
2. Persist export queue and frozen snapshots. Implement safe cancellation/failure/restart/image-sequence resume.
3. Validate output PTS/DTS/duration/audio sample count, color metadata, alpha and subrange warm-up.
4. Export sample projects with files, generators, particles, simulations, recorded controls and audio; reimport and compare.

Gate: AT067–AT070. Required available codecs actually encode; unavailable encoder is explicit and PNG+WAV still works.

### T22 — Recovery, relinking, and error hardening

1. Complete autosave retention/recovery, migration backups, portable collect-media and relink UI.
2. Inject disconnect/disk-full/corrupt-doc/shader/GPU-lost errors and repair failures.
3. Audit FFI ownership, target lifetimes, caches, URL cleanup and thread shutdown. No background workers survive application exit.
4. Add capability report and diagnostic bundle with redaction and no default telemetry.

Gate: AT071–AT074. Crash recovery is tested with forced termination of a disposable test project, not user's unsaved work.

### T23 — Conditional vision/creative runtime adapters

1. Implement G170–G177/G180–G185 for actually available runtimes/models with bounded worker contracts.
2. Maintain model/source inventory and checksums. No credentials/purchases inferred.
3. Test no-model/dependency state and actual inference/source integration separately.
4. Provide a deterministic sketch example and an explicitly live-only example plus capture-to-take workflow.

Gate: AT075–AT077 where dependencies exist; record per-row blocker otherwise. Do not let this replace required core fixes.

### T24 — WebGPU harness and backend parity

1. Run same WGSL manifests/uniform layouts in browser harness on explicit fixture frames.
2. Compare CPU/native/browser results using tolerances and keep backend-specific baselines where justified.
3. Verify unsupported device features show capability errors, not crashed pages.

Gate: AT078. Portable shaders demonstrably run; browser not available is a real verification gap.

### T25 — Performance, accessibility, and product polish

1. Measure representative profiles at stated host specs; identify CPU/GPU/readback/encode bottlenecks.
2. Fix allocations/queue growth/extra renders before adding new engines or infrastructure.
3. Validate panel resize/DPI/focus/keyboard/text entry/drag undo/errors, no unusable hidden controls.
4. Run 30-minute soak then final 2-hour live soak on available baseline. Record max/mean/p95/p99/memory/queue depths.

Gate: AT079–AT081 and benchmark thresholds. Do not label a shader-heavy graph 1080p60 if only editor copy ran30fps.

### T26 — Packaging, examples, docs, and license records

1. Create bundled original sample media, every required template and user guides. Generate effect/generator reference from manifests.
2. Package native libraries, manifests/shaders/fonts/presets and runtime DLL/dylib resources. Resolve macOS rpaths and Windows DLL lookup from app bundle.
3. Add Windows/macOS CI jobs, unit/type/catalog tests and actual native build targets. If CI cannot execute, preserve config and mark unverified.
4. Build unsigned development installers on available hosts; sign/notarize only if real configured identity exists.
5. Fresh-install test with dev dependencies hidden, offline, non-ASCII/spaced paths and standard user account.

Gate: AT082–AT085. Installer launches without dev server; actual platform artifacts/reports with hashes.

### T27 — Reconcile and deliver

1. Compare every R ID, FX ID/variant, required G row, T task and AT test against implementation/evidence.
2. Repair missing required behavior; run affected tests then full relevant suites after last change.
3. Run source scan for TODO/unimplemented/identity placeholders/fake capabilities. Review each hit, never blindly delete comments or hide required features.
4. Generate FINAL-REPORT, feature/capability matrix, reproducible build instructions, artifact hashes, performance report and blockers.
5. Launch packaged application and complete final primary journey. Stop only when work is verified or real external blockers are documented.

Gate: VERIFICATION final reconciliation. No successful claim based solely on counts or generated documentation.

## 3. State and evidence format

Each task record stores `id`, `spec_version`, `status`, `started_at`, `finished_at`, `implementation_paths`, `dependencies`, `commands`, `exit_codes`, `evidence_paths`, `tested_platform`, `known_failures`, `next_action`. Timings are actual wall-clock observations, not estimates. Report `blocked_external` with missing item and smallest command/workflow that would resume verification.

Checkpoint STATE before long builds, context compression, tool interruption or handoff. Run resumable work from last verified task plus current diff. Do not rerun every task simply because the model's context reset.
