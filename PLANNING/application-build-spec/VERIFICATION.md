# Verification and release evidence

The application is complete only to the extent proven by these tests. Unit tests, shader validation, real rendered frames, real encoded files, packaged application smoke tests, and hardware integration evidence answer different questions. Do not substitute one for another. This document defines tests for the implementation agent to create and run; no application tests have been performed by writing this plan.

## 1. Fixtures and numeric tolerances

Generate original fixtures locally with deterministic seeds and record hashes:

1. RGBA ramps/primaries/checkers/impulse/alpha circle at64×64 and320×180; linear and sRGB versions with known conversion.
2. 10-second constant-rate video with visible frame counter, moving marker and color bars; exactly600 frames at60/1.
3. Exact fractional-rate fixture at30000/1001 with known PTS; VFR fixture with irregular frame durations and B-frame codec when available.
4. 48kHz WAV: silence,1kHz sine -12dBFS,80Hz bass,8kHz treble, impulses at1s/3s/7s, stereo left/right markers. Avoid external songs.
5. Audio/video impulse-flash fixture at exact shared timestamps;20-minute drift fixture generated procedurally without enormous checked-in media.
6. Rotated portrait, non-square-pixel, odd-sized, alpha, limited/full-range, color-tagged and truncated/corrupt media samples.
7. SVG with donut hole, gradients, clip/mask, group IDs and text; animation fixture with known trim/path/morph frames.
8. Original small glTF scene with skin animation, morph target, alpha material and known coordinate markers; point cloud fixture.
9. Project fixtures for every graph scope, preset, supported schema version, missing node/asset and recorded take.
10. Invalid OSC/MIDI/project/font/SVG/manifest fixtures with bounded sizes and no external active payload execution.

Compare exact integers/events/IDs/timestamps with exact expected results. Rational conversions test rounding explicitly. GPU image comparisons convert into declared working space: baseline analytic pixel max absolute error<=0.005 for simple float pipelines and<=2/255 for RGBA8 quantization where applicable. Complex shader goldens default mean absolute error<=0.01 and99th-percentile absolute error<=0.05 in normalized SDR output, but also require the catalog-specific invariant. Document backend-specific tolerance changes with measured reason; no automatic baseline overwrite.

For simulations compare spawn/event counts exactly; compare numeric state by documented tolerance with same timestep/backend. Cross-vendor bitwise float equality is not a release requirement. Same-backend repeated export should reproduce event schedule and pass image tolerance. A second image being different is necessary for motion tests but insufficient to prove the intended algorithm.

Audio unit tests use known FFT/window gain: sine frequency peak within one bin, normalized amplitude within1dB, silence zero/no NaNs, out-of-band energy below documented window leakage. Mixer reference sample tests error<=1e-5 float where no codec is involved. Encoded audio allows codec delay metadata handling and documented lossy error. Live response target<=60ms p95 for the default local capture/analysis/render route; report measurement technique and audio/display latency separately. A software timestamp-only estimate is not glass-to-glass hardware latency.

## 2. Acceptance tests AT001–AT085

Each test creates JSON evidence with fixture hash, build commit/diff state, exact app/dependency version, OS/arch/GPU/backend, settings, command, exit, result, and artifact references. UI tests include screenshots; video tests include probe output and sampled frames; hardware tests include actual device/protocol route. Store under `implementation/evidence/<test-id>/`.

| ID | Scenario and required assertion |
|---|---|
| AT001 | Create/open/save project; Unicode name and spaced path; stable UUID/revision and exact rational settings survive round trip. |
| AT002 | Command transactions/undo/redo/stale revision; illegal operation changes no state; undo group restores exact structure. |
| AT003 | Schema migration/invalid/newer document; migration backup exists, corrupt document rejected, unknown extension metadata preserved. |
| AT004 | Headless/native GPU frame; analytic color/alpha oracle, row stride, first frame initialized, no double transfer encoding. |
| AT005 | Resize/minimize/fullscreen/close/reopen; no stale surface use, leaked targets or zero-size crash. |
| AT006 | Native output+editor preview+second consumer; frame IDs agree, simulation step count unchanged by consumers. |
| AT007 | Import/play/seek real file with B frames/rotation/VFR; displayed PTS follows selected source frame rule; stale seek discarded. |
| AT008 | Play audio+video flash/impulse, loop and frame-step; error<=one project frame+5ms after known device latency compensation. |
| AT009 | First PNG+WAV/available MP4 export; exact600 frames/480000 samples for10s60fps fixture; reimport duration/metadata validated. |
| AT010 | Graph type/cycle/missing-input validation; clear exact node/port error; temporal cycle exception only explicit pair. |
| AT011 | Branched/masked/composited graph; output pixel oracle and no scratch-resource read/write alias. |
| AT012 | Graph add/connect/group/copy/paste/undo; new IDs and preserved internal connections, no dangling references. |
| AT013 | Bad shader/edit; current document retains edit/error, last valid runtime frame graph remains usable, fix activates new version. |
| AT014 | Silence/bass/sine/treble analysis; bands/window normalization/onset time match numeric expectations. |
| AT015 | Audio callback/rings/device loss; no blocking/alloc/network in callback, overruns counted and silence on underflow. |
| AT016 | Keyframes/modulation fold/LFO/enum hysteresis/expression; CPU reference values at exact timestamps. |
| AT017 | File audio drives visuals in live and offline; features sampled by source timestamp, independent of export render speed. |
| AT018 | Trim/split/move/slip/rate/reverse; property tests preserve source mapping and half-open endpoints. |
| AT019 | Linked A/V ripple/insert/overwrite/locked track; atomic failure or correct synchronized edits; undo restores. |
| AT020 | Transition handles/overlap/clip-track-master-adjustment order; expected image/audio crossfade and masks. |
| AT021 | Waveforms/proxies/reverse GOP cache; cancel job, original source retained, same time selection, bounded memory. |
| AT022 | Mixer gain/pan/mute/solo/fades/time-stretch; pitch-preserving stretch maintains sine pitch while duration changes. |
| AT023 | FX001–FX028 all present, each manifest/algorithm/changed control/transparency/scale/export tested. |
| AT024 | FX100–FX123 every named variant implemented with distinguishing behavior; no identity alias masked as variant. |
| AT025 | Feedback/echo/delay/freeze; known impulse decay and history count; output rate does not change feedback step cadence. |
| AT026 | Seek/checkpoint/warm-up/invalidation; subrange output equals corresponding frames from full render within tolerance. |
| AT027 | Generator source clip add/trim/loop/save/load/mask/effect chain; no required source file for procedural node. |
| AT028 | All required G001–G028 rows with available IDs; fractal/periodic noise/L-system/automaton invariants and bounded loops. |
| AT029 | Field metadata/converters; known constant vector causes expected displacement, incompatible spaces rejected. |
| AT030 | SVG safe import/editor; donut hole/gradients/viewBox correct, active/unsupported content reported, no external fetch. |
| AT031 | Path draw/follow/morph/repeater; arc-length position and winding correct, checkpoint not needed for analytic motion. |
| AT032 | Lottie actual frame seek and color/control changes; fixed t agrees paused/live/export; unsupported features identified. |
| AT033 | Image motion G050–G068; transforms/warp/parallax/fragments/sprites with alpha and deterministic controls. |
| AT034 | Typography G070–G079; Unicode/shaped cluster animation, glyph holes, correct fallback, Mask/Geometry outputs. |
| AT035 | Video/image sequence/sprite retime and animation export; frame order/rate/loop boundaries correct. |
| AT036 | SDF analytic sphere/Boolean/normal/depth; correct center ray and finite no-hit; invalid tree rejected. |
| AT037 | Metaball/deformation/material animated through effects; opacity/Depth/Normal coherent; ray-step cap maintained. |
| AT038 | SDF mesh conversion/cancel/GLB export; finite bounds, normals, expected connected surface; job memory capped. |
| AT039 | Mesh primitive/scene/camera/light coordinates; expected marker positions, depth occlusion, transparent compositing. |
| AT040 | Imported glTF skin/morph/PBR and PLY point cloud; actual time-varying geometry/material, missing assets reported. |
| AT041 | G090–G107 terrain/instances/voxels/ocean/volumes; count/resource limits, no hidden infinite generation. |
| AT042 | Particle counts/IDs/birth/death at30/60/120Hz presentation; identical simulation schedule and reset. |
| AT043 | Image/text/path/mesh emitters, morph/trails/collision; known samples map correctly, no capacity out-of-bounds. |
| AT044 | Boids/slime/field events; bounded neighbor scan, event cap and state replay, no unbounded per-particle trail. |
| AT045 | Rigid body/joint/pendulum; free-fall reference tolerance, stable contacts, deterministic reset/checkpoint. |
| AT046 | Cloth/jelly constraints; pinned points remain fixed, finite volume/positions, no explosion under valid settings. |
| AT047 | Fluids/water/sand/reaction; projection reduces divergence, stable finite field, material rules and impulses correct. |
| AT048 | Simulation subrange seek/export/control replay equals full sequence; changes invalidate dependent checkpoints. |
| AT049 | OSC actual UDP encode/decode/all declared types; scheduled bundle order, native/normalized routes and learn. |
| AT050 | OSC malformed/rate limit/duplicate trigger/feedback loop; app responsive, no unauthorized file or shell action. |
| AT051 | MIDI real device or explicit unavailable; notes/CC/bend/pressure/MPE/clock/pickup correct and reconnects. |
| AT052 | Recorded controls/CSV/JSON/network replay; identical time-mapped data offline; unrecorded live source rejected for export. |
| AT053 | Event sequencer/rhythm/expressions/interaction record; deterministic quantization/random-on-beat, no eval escape. |
| AT054 | Actual webcam capture modes/orientation/timestamps; permission and disconnect/reconnect handled. |
| AT055 | Actual screen/display/window/region/cursor capture; crop/DPI/window resize correct and self-capture exclusion works. |
| AT056 | Actual microphone and system-loopback; no dummy stream, output-device change and permission recovery. |
| AT057 | Live source→take→offline export; captured timestamps align audio/video and survive relink. |
| AT058 | Actual Windows Spout send/receive with independent sender/consumer; resize/alpha/adapter behavior recorded. |
| AT059 | Actual macOS Syphon send/receive and loss/reconnect; ownership and orientation correct. |
| AT060 | Actual NDI official SDK/runtime send/receive loopback; frame/audio release counts, stride/timestamps, missing-runtime behavior. |
| AT061 | Max `.maxpat` structural checks plus real OSC bytes interoperate; multiple instances, stale release, sequence epochs. |
| AT062 | Real packaged `.amxd` in Live; audio/MIDI unchanged through pass-through, macros automate, Set reopen restores. |
| AT063 | Max/conditional Link transport; single master, seek/loop/start-stop behavior, tempo phase aligned, no control-as-audio confusion. |
| AT064 | Live workspace/macros/scene recall/crossfade/quantize/panic; compile-ahead, fixed state stepping, responsive editor. |
| AT065 | Fullscreen and multiple outputs at chosen rate/size; settings persist, consumer failure does not stop preview. |
| AT066 | Live recording with controls/audio; reimported content matches recorded response, overload policy and CFR gap handling measured. |
| AT067 | Export codec matrix and alpha; real available H.264/HEVC/ProRes/GIF/image-sequence/WAV encoders exercised, unsupported disabled accurately. |
| AT068 | Export frozen revision while editing; original snapshot used, concurrent live state undisturbed, no incomplete renamed output. |
| AT069 | Cancel/disk-full/encoder failure/restart/sequence resume; no lost originals, safe job-owned temp cleanup, errors actionable. |
| AT070 | Export fractional fps/VFR/odd size/color/alpha/range; DTS/PTS monotonic, frame/sample counts and output tags correct. |
| AT071 | Autosave and forced crash recovery on disposable project; last saved and recovered revisions both available and valid. |
| AT072 | Missing media/relink/collect project; ambiguity resolved visibly, originals preserved, relative portable project opens offline. |
| AT073 | FFI close/reopen/exit/GPU loss/shader error/disconnect injection; no crashes in supported recovery cases, no dangling workers. |
| AT074 | Diagnostics/redaction/settings/capability state; no secrets/media payloads in report, no default telemetry/network dependency. |
| AT075 | Conditional actual vision inference; known landmarks/mask/depth and stale/cancel states; exact model/runtime hashes. |
| AT076 | Conditional external sketch/runtime source; explicit time render repeats or live-only state recorded; worker crash isolated. |
| AT077 | Conditional model/source absent; preserved editable node, actionable dependency error, never fake successful output. |
| AT078 | Browser WebGPU harness vs native shader ABI/fixtures; correct uniform packing and documented image tolerance. |
| AT079 | Benchmarks below on real GPU with exact settings; measured frame times/queues/memory/latency, no assumed hardware results. |
| AT080 | UI keyboard/DPI/focus/drag undo/text input/accessibility; main journey can be completed without lost data or hidden focus. |
| AT081 | Soak30min then2h; stable cache budgets/queue depths and no drift/leaks/crash under repeated edits and scene switches. |
| AT082 | Windows installer clean standard-user/offline launch, DLL/resources found without dev PATH, uninstall preserves user projects. |
| AT083 | macOS arm64 packaged app clean launch, dylibs/rpaths/permissions and file dialogs work; exact signing status reported. |
| AT084 | macOS x64 packaged app native run or actual supported runner; architecture/build alone is not runtime verification. |
| AT085 | Examples/docs/requirements/artifact hashes/license records reconcile; every advertised workflow reproduces on available host. |

## 3. Primary user journeys

Run these in the packaged application, not solely a browser development server:

1. Edit: new1920×1080/60 project → import footage+audio → trim/split/ripple → crossfade → clip graph VHS→halftone→bloom → animate wet → save/reload → export MP4 or real available baseline → reimport. Confirm timeline/source mapping, sound and visual parity.
2. Bass visual: audio source → feature band → 3D blob deformation → Chromatic metal→Bloom→CRT → fullscreen second display → record30s → replay recorded take → deterministic export with same response.
3. SVG: safe SVG import → trim path/morph/repeat → text mask → particles→Pattern refraction→Slice shift → scrub backwards → export loop, inspect boundary.
4. Simulation: emit ink fluid and cloth/particles, record OSC/MIDI impulses, seek forward/backward, export a range after30s warm-up, compare against full exported sequence.
5. Live integration: webcam/screen/Spout/Syphon/NDI as available → mix with generator → external output → disconnect source/consumer → reconnect; editor continues.
6. Ableton: real `.amxd` on bass track → pass audio through → feature mapping/macros → transport forward → save/reopen Set → desktop responds without reset/hijack.
7. Recovery: edit disposable project, force terminate, restart/recover, rename/move source, relink, collect portable folder, open offline.

## 4. Performance profiles

Reference target class:16GB RAM, discrete GPU around RTX3060 class or Apple M1 Pro class, or measured equivalent. Minimum supported device is feature detection based; lower-tier hardware may require lower quality. Record exact adapter/driver and actual path. Do not convert these reference examples into an unsupported claim that all hardware reaches a rate.

Warm shaders/media caches before measurement, then capture120s steady state at1920×1080/60 and3840×2160/30 where feasible. Report UI preview copy separately from native output. Include GPU and CPU timings, p50/p95/p99, presentation misses, decode drops, audio underruns, queue max, peak RAM/VRAM estimates and exact source codec.

Profiles:

- Ordinary:1080p source→color adjustment→warp→halftone→VHS tracking→bloom→composite; bloom half-res, warp3 octaves. Goal render CPU+GPU critical-path p95<=16.67ms and presentation misses<1% after warm-up.
- Particles:20k particles, curl force,16-point trails, bloom half-res. Same target, capacity recorded.
- SDF:4 metaballs,96steps at0.5 render scale, AO off, bloom. Same target; full-res higher quality measured separately.
- Fluid:256² stable-fluid grid,20 pressure iterations, scene-to-texture plus color/bloom. Measure fixed-step throughput; no discarded export steps.
- Recording: ordinary profile plus1080p60 supported encoder+48k stereo+30Hz preview copy. Target queue stays bounded, audio underruns0 and sustained overload0. If not met, document actual sustainable quality/rate and repair avoidable stalls.
- 4K: six-effect graph at30fps; target p95<=33.33ms on capable recommended hardware. No pass from software renderer.

Performance goal failure is reported, diagnosed and repaired where possible. Do not lower an image/quality setting without recording it. Resource/memory leak test: compare warmed steady-state baseline and end of2h soak; unexplained growth>10% or64MiB (larger threshold) requires investigation. Stable cache filling within configured budget is not automatically a leak.

## 5. Test strategy and failure injection

Rust: unit/property tests for arithmetic, edits, schemas, modulation and schedule; integration tests for real decode/mux/OSC/files; GPU tests with native device. Frontend: Vitest/testing-library for accessible controls, state/revision handling and keyboard behavior; Playwright browser tests for UI behavior and WebGPU harness. Packaged native application automation uses supported host tooling/driver or recorded manual smoke procedure; a Playwright mock backend does not prove native integration.

Inject failures through test-only interfaces rather than destroying user resources: bounded-disk writer, decoder error, source lost, encoder exit, shader error, controlled device recreation, corrupt snapshot and command conflict. Test seams are disabled in release UI. Source capability mocks are labeled test fixtures and never appear as real production devices.

No flaky hardware test retries that hide repeated failures: one retry only for documented transient setup, preserving original failure. Tests with hardware prerequisites report unavailable. Unit/golden/integration failures return nonzero; skipped tests are counted distinctly. Every fixed bug needs a regression test where a meaningful automated assertion is possible.

## 6. CI and reproducible builds

Required CI config: Windows x64 native build/tests; macOS arm64 and x64 builds/tests on real compatible runners; generated schema/catalog drift; frontend lint/types/unit; Rust fmt/clippy/test; shader validation; native GPU suites on available appropriate runners; package/resource smoke; dependency notices. Avoid assuming hosted runner GPU hardware or camera/Max exists. Hardware integration jobs run on configured hosts and are reported separately.

Use `--locked`/frozen lockfile installs. Cache package/compile outputs with keys including OS/arch/toolchain/lockfile/native SDK version. Never cache credentials, user projects, or captured media. App version, source revision, spec version, shader/kernel versions and native build options appear in diagnostics/artifact manifest.

## 7. License and dependency evidence

Generate dependency inventory for actual shipped graph, including npm, Rust, native libraries, shaders, fonts and SDKs. FFmpeg license depends on configure flags and linked libraries; record configure output and source distribution location. Link/NDI require actual applicable terms/authorization. No obligation to list personal creators in product effect documentation; mandatory library notices remain in distribution records. Missing license data blocks shipping that component, not independent development of original effects.

## 8. Release classifications and final reconciliation

Produce four separate judgments:

- Source implementation coverage: required R/FX/G/T rows with real implementation paths.
- Host verification: tests actually run per OS/architecture/hardware route.
- Artifact readiness: actual executable/installer/device files and hashes, exact signing state.
- Full release readiness: all required features verified on promised platforms, external prerequisites resolved, primary journeys passing.

Do not collapse these to a single green badge. A Mac build command/configuration is not a verified Mac artifact; unsigned is not signed; `.maxpat` is not `.amxd`; `SDK_MISSING` is not working NDI; a generated matrix is not evidence.

Final audit script verifies every ID, all linked files/test IDs exist, every claimed pass has evidence and actual success, no test/golden skipped is labeled passed, no required row silently marked extension, and no optional missing SDK prevents portable tests from running. Compare catalog count with source manifests and generated UI. Open representative contact sheets and packaged app. Investigate potential identity shaders/TODO stubs by reading code and behavioral tests.

FINAL-REPORT includes launch/install steps, artifact paths/hashes, supported OS/architecture list, real test summary, failed/unavailable tests, detailed feature coverage, measured performance, known limitations/blockers, and exact resume steps. Report all unresolved required work. Do not claim the product is complete if required work is failed, unverified, or externally blocked.
