# Architecture and implementation contracts

Requirements use MUST for mandatory behavior and MAY for an optional optimization. Numeric defaults are binding unless capability detection rejects them. Record changes to contracts in DECISIONS with the affected tests and schema migration.

## 1. Stack, targets, and repository

Use Tauri 2, React, TypeScript in strict mode, Vite, `@xyflow/react`, Rust, `wgpu`, and WGSL. Use `serde`, `serde_json`, `uuid`, `thiserror`, `tracing`, a bounded channel library, `cpal`, `rustfft`, `rosc`, and `midir`. Use `ffmpeg-next` with matching dynamically linked FFmpeg libraries for decode, resampling, encoding, and muxing. Use ThorVG behind a small C ABI for vector/Lottie rendering, Rapier for rigid-body physics, and WGSL compute for particles and field simulations. Resolve supported stable versions once in T00, pin exact versions/commits, and retain lockfiles. Version selection must pass the native/window/media/vector compatibility spike before broad implementation.

Choose Signalsmith Stretch through a bounded C ABI for pitch-preserving retiming, HarfRust plus matching read-fonts/skrifa for shaping/glyph outlines, and `gltf` from gltf-rs for glTF2 parsing. Shape/cache glyph positions in Rust and rasterize outlines through the existing vector renderer. Font registry resolves bundled/imported font assets first, then explicit system font selection; project records the actual selected font asset/fallback. Parser libraries do not themselves implement skinning, retiming, or rendering; those engine paths still require tests.

Use Node 24 LTS, pnpm with its exact version recorded in `packageManager`, and a pinned supported stable Rust toolchain chosen in T00. Treat the original model name and command-runner name as user-selected settings; do not invent runner APIs. Initial OS floor: Windows 11 x64; macOS 14 arm64/x64. Detect capture APIs at runtime. Document narrower API availability explicitly. Do not promise Windows on ARM, Linux, mobile, VST2, AAX, or older OS compatibility.

Repository layout:

```text
apps/desktop/                 React UI and Tauri shell
apps/max-for-live/            source patches, helper JS, packaging scripts
apps/shader-harness/          timestamp-driven browser shader conformance
crates/domain/                document, types, commands, rational time
crates/engine/                evaluator, graph compiler, timeline, control routing
crates/render/                wgpu resources, passes, shaders, presentation
crates/media/                 FFmpeg integration and media jobs
crates/audio/                 CPAL devices, mixer, features, resampling
crates/generators/            scene, SDF, vector, physics, simulations
crates/integrations/          OSC, MIDI, Link, external streams, adapters
crates/platform/              Windows/macOS native bindings
crates/xtask/                 doctor, fixtures, checks, packaging, reports
native/vector/                ThorVG C ABI shim and pinned build
native/video-io/              Spout/Syphon/NDI FFI shims
catalog/effects/              versioned manifests and graph presets
catalog/generators/           versioned manifests and templates
schemas/                      generated JSON schemas and TS types
fixtures/                     generated test assets and expected results
tests/                        integration, GPU, end-to-end, soak
docs/                         user guide, formats, troubleshooting
spec/                         this package
implementation/               state, requirements, evidence, blockers
```

Dependency direction: domain has no GPU/OS dependencies; render consumes domain descriptors; media/audio/generators/integrations expose interfaces to engine; engine coordinates them; desktop consumes engine. Avoid mutual dependencies between render and engine. Put shared handle types in domain or a small render API module. Keep fewer cohesive crates initially; split only at these ownership boundaries.

Generated code must be reproducible. CI regenerates and checks for drift. Rust integer values beyond JavaScript safe range are decimal strings on the wire. No JSON NaN/Infinity. Reject duplicate IDs and invalid enums before loading runtime objects.

## 2. Processes, threads, and presentation

The desktop process contains the UI, domain command dispatcher, render thread, media workers, audio streams, and control workers. Third-party executable creative-code sources and optional model inference use separate bounded workers/processes. Do not allow a worker failure to corrupt project state.

Ownership:

| Owner | Responsibility | Input/queue policy |
|---|---|---|
| UI/main thread | windows, dialogs, input, commands | short async operations; never decode/render synchronously |
| Domain dispatcher | authoritative document and revision | ordered commands, queue 256; reject overload with error |
| Render thread | device, queue submission, runtime graph, GPU handles | newest immutable render snapshot; swap only at frame boundary |
| Decode workers | per-source demux/decode, seek | bounded latest requests; job generation cancels stale seeks |
| Audio callback | copy/mix already prepared samples, timestamp | lock-free preallocated rings only |
| Audio analysis worker | feature windows, FFT, onset, caches | bounded PCM ring; report overruns |
| Control worker | OSC/MIDI parsing, scheduling, feedback | latest continuous value + bounded trigger queue |
| Export job | isolated runtime and timeline clock | backpressure; no missing frames |
| Persistence worker | snapshots, writes, recovery | one durable write job per project at a time |

One render evaluation advances each live graph instance once. Present the resulting texture to all outputs. Opening a second monitor, Spout receiver, or editor preview must not step physics twice. Offline export uses a separate runtime instance so it cannot reset the live simulation.

**Native presentation:** create a Tauri-managed native window without a webview for detached output; use its raw window/display handles with wgpu. The selected Tauri release may gate its WindowBuilder behind `unstable`; explicitly enable and isolate that API, pin the release, and prove it in T00. Follow documented async/setup creation to avoid the Windows synchronous-command deadlock. Do not add a competing winit main event loop. Create/destroy windows and capture handles on the required platform thread; retain the window for the full surface lifetime. On macOS create/attach the Metal layer through the supported wgpu/platform path on the UI thread. Surface acquisition handles lost/outdated by reconfigure, timeout by skipped presentation, out-of-memory by controlled shutdown/recovery. Zero-sized/minimized surfaces do not allocate or present. Render engine can run headlessly for export/tests.

**Embedded editor preview:** copy the authoritative final texture through a reusable asynchronous staging ring; downscale to 960×540 maximum by default, 30 fps. Encode a bounded JPEG/PNG frame on a worker and send binary data through a Tauri binary channel or pull via a scoped custom resource protocol. Never send base64 through reactive state. Keep at most one pending and one displayed frame, revoke old object URLs, attach frame ID/PTS, and drop stale preview copies. Use PNG when inspecting alpha; checkerboard is UI-only and excluded from export. Show actual preview rate independently from render rate. Upgrade to native embedded presentation only if measured copy cost requires it; detached output already provides full-quality native output.

This preview transport intentionally uses readback. It is never in the critical native output path. Recording, NDI CPU fallback, tests, and preview have separate readback budgets. Do not claim GPU zero-copy for any route that maps bytes or crosses an unrelated GPU device/process.

**WebGPU harness:** load portable WGSL, identical manifests/uniform packing, uploaded fixture textures, and explicit frame contexts. Render snapshots and compare within tolerance. It need not duplicate the desktop media/physics scheduler. Browser unavailable means this gate is unverified, while native rendering remains usable.

UI subscriptions: transport 30 Hz, meters 30 Hz, diagnostics 2 Hz. Graph edits emit commands; sliders update at most 60 Hz and coalesce. IPC frame counters and stats must not trigger full node-canvas rerenders.

## 3. Time, randomness, and evaluation

Define `RationalTime { numerator: i64, denominator: u32 }`, reduced, denominator > 0. Use i128 intermediates for arithmetic and checked conversion. Serialize numerator as decimal string. Timeline ranges are half-open `[start,end)`. `FrameRate { num:u32, den:u32 }` is exact. Defaults: 1920×1080, rate 60/1, square pixels, 48 kHz stereo, SDR Rec.709/sRGB working pipeline.

Frame n starts at `n * fps.den / fps.num` seconds. Export frame count for range duration D is `ceil(D * fps.num / fps.den)`; final audio has exactly `round(D * 48000)` samples. The last video frame may extend less than one frame beyond D; mux/container duration and tolerance must be recorded. Support exact 24000/1001, 30000/1001, 60000/1001, plus integer 24,25,30,48,50,60,90,100,120,144,240, and custom rational rates. Drop-frame timecode is a display convention, never skipped content frames.

`FrameContext` includes project time, clip-local time, frame index, simulation tick, transport epoch, delta, render size, quality profile, project seed, audio feature snapshot, beat state, and recorded control events. Wall-clock access is forbidden in deterministic kernels. Long-running shaders receive split high/low time or wrapped phases to avoid float precision drift. Absolute frame indices use split integers or bounded indices for GPU hashes.

Use a documented integer PRNG/hash with stable seeds. Derive node seed from project seed + node UUID + explicit seed parameter. Never seed from node iteration order, render completion time, or platform clock. Use sorted stable IDs for equal-priority control inputs and deterministic task ordering.

Clock modes:

- Edit: audio-device sample clock is playback master when monitoring sound. Monotonic transport clock when audio device is unavailable; clearly indicate silent playback.
- Live: monotonic clock, phase/tempo follower when selected; audio timestamps translated into that clock. Do not alter source audio merely to match visual fps.
- External transport: Max/Link/MIDI clock chosen explicitly. Only one transport master. Link start/stop is distinct from tempo/phase.
- Export: integer frame stepping and exact audio sample evaluation, no wall-clock pacing.

**Stateful nodes:** implement `reset(seed,epoch)`, `step(fixed_dt,inputs)`, `checkpoint()`, `restore(checkpoint)`, `render(time,interpolation)`, and `dispose()`. Default simulation dt = 1/120 s. Compute steps from rational target time, carry remainder, render interpolated state where supported. Live catch-up cap 8 ticks per presented frame; if exceeded advance an explicit discontinuity epoch, reset/reinitialize by policy, and log a count. Export never caps away simulation steps.

Checkpoints every 2 seconds while seeking/caching; default RAM cap 256 MiB, project simulation disk cache 2 GiB with LRU. Cache keys include graph version, kernel version, seed, all simulation-affecting parameter/control histories, audio feature cache hash, fixed dt, and source dependencies. Invalidate from earliest affected edit. A seek restores latest valid checkpoint <= target then simulates forward, with visible progress and cancellation. No secret reset to an arbitrary visual state when scrubbing.

Feedback is a special temporal resource: outputs read the previous canonical graph frame, not the previous monitor refresh. For export and cache replay, use the project's exact frame cadence. At non-frame seek positions evaluate the preceding canonical frame, then show it; do not invent half a feedback step. Define clear/warm-up duration; default history starts transparent at project start. Warm-up before export range must replay from a checkpoint or project start. Resolution changes reset size-dependent histories and create a discontinuity marker.

## 4. Domain, commands, graph, and automation

Project document schema version starts at 2, independent of app version. Include UUID, name, created/modified UTC metadata, settings, assets, timeline, graphs, mappings, scene templates, recorded takes, preset references, markers, source capabilities, and vendor extension metadata. Runtime GPU handles, open sockets, OS permission results, and volatile device IDs are excluded.

Asset: UUID, kind, original path, relative path, fingerprint, size/mtime, optional full SHA-256, media time base/duration, streams, rotation, sample aspect, color metadata, cache keys, proxy metadata, missing state. Asset paths resolve relative to project; runtime canonical path checks prevent bundled references escaping their directory. Never identify assets only by basename. Live source descriptors reference stable user selections and reconnection hints, not open handles.

All edits use typed commands with `command_id`, `expected_revision`, `payload`. Examples: CreateProject, ImportAssets, AddClip, MoveClips, TrimClip, SplitClip, RippleDelete, SetParameter, AddKeyframe, AddNode, ConnectPorts, SetMapping, SaveProject. Domain validates atomically, increments revision, emits changed IDs and undo inverse. Reject stale conflicting structural commands with `REVISION_CONFLICT` and current revision; allow explicitly coalesced continuous control through a separate runtime override channel. Drag edits are transient preview followed by one commit. Undo grouping covers paste, multi-trim, preset insertion, and gesture sliders. Undo does not replay network sends or reopen devices.

Port types: Texture2D, Mask, Depth, Normal, MotionVectors, Scalar, Int, Bool, Color, Vec2, Vec3, Transform, AudioFeatures, AudioSpectrum, Event, BeatState, Geometry, Scene, ParticleBuffer, SdfField, VectorField, and PathSet. Frames and images share Texture2D plus metadata. Avoid separate incompatible Image/Video socket families. Convert color/scalar/mask explicitly. Enumerated selectors are parameters, not arbitrary integer sockets.

Texture metadata: dimensions, format, color encoding, alpha mode, sample orientation, PTS, epoch. Field metadata: coordinate space, bounds, units, grid dimensions, sampling mode. Geometry: bounds, topology, positions/normals/UVs/indices/material groups; Scene handles reference immutable scene descriptions and a render-instance ID.

Normal graph edges form a DAG. FeedbackRead/FeedbackWrite pairs break temporal cycles; compile their read as an external prior-frame resource and write after consumers finish. Unmarked cycles are invalid. Required inputs must connect or have documented defaults. Groups expose typed ports; flatten with namespace and depth/node limits. Graph limits default 4096 nodes, 16384 edges, 16 nesting levels, 16 texture inputs per compiled kernel subject to device limits. Cap imported manifests and generated shader sizes.

Compile pipeline: validate document → expand groups → resolve types → reject cycles → dependency/liveness analysis → prune unreachable nodes → produce pass DAG → allocate/reuse targets → compile pipelines → atomically activate. Leave last valid graph active on a bad edit. Display invalid nodes and errors without discarding editable document. No synchronous pipeline compilation in presentation.

Every manifest declares stable type ID/version, input/output ports, parameter descriptors, defaults/ranges/units, shader/kernel reference, resource estimates, temporal policy, alpha/color policy, quality controls, help, and serialization migration. See EFFECTS and GENERATORS. Missing type loads as a preserved missing-node record and visible error. Never substitute an unrelated effect silently.

Parameter standard: scalar/integer/boolean/enum/color/vector/path/asset-reference; native range + soft range, linear/log scaling, native unit, default, automation support, modulation support, smoothing ms, and whether changing it resets state or recompiles resources. Recompile-only parameters include simulation dimensions and maximum particle capacity; changes are staged and committed at frame boundary.

Parameter evaluation:

1. Evaluate base value or timeline keyframes. A keyframe lane replaces the base; it is not an additional copy of the base.
2. Sort enabled mappings by explicit `order`, then mapping ID.
3. Normalize raw source with declared input range. Apply inversion, dead zone, clamp, curve, attack/release smoothing using timestamp dt.
4. Fold add, multiply, replace, min, max, or bipolar-add in order. Add uses native-unit depth; multiply uses `value *= lerp(1,source,amount)` with documented amount; replace uses lerp(current,mapped,amount).
5. Apply final native bounds, integer rounding, boolean threshold with hysteresis, enum quantization. Reject nonfinite values and keep last valid value with error count.

No user-authored strings evaluated by JavaScript `eval` or shell. Expression node uses a limited math parser (arithmetic, trig, abs/min/max/clamp/mix, variables time/beat/audio/custom inputs), bounded AST depth and operation count. Restrict divisions and invalid roots; expose diagnostics.

Keyframes: rational time, typed value, step/linear/cubic Bezier. Rotation supports shortest-path and explicit unwrapped mode; quaternion scene rotation uses slerp. Step-only bool/enum. Color interpolation in linear RGB default, perceptual interpolation optional with explicit selection. LFO supports sine, triangle, saw, square, seeded random, sample-and-hold; frequency Hz or cycles/beat; deterministic phase anchor. Every numeric animatable control supports drag, typed entry, reset, keyframe, modulation, OSC/MIDI learn, and visible final value.

Automation lanes declare time domain: clip-local by default for clip graphs/transforms, project time for track/master, source time only when explicitly selected. Move preserves clip-local animation. Trim-start by delta moves clip source in and rebases local keys by -delta, inserting the evaluated boundary value so retained content keeps its prior motion. Split rebases right-side keys by -split-local-time and inserts matching boundary values; split cubic curves with de Casteljau subdivision to preserve shape. Do not simply delete keys outside retained range and change interpolation. Generator source phase defaults source-local, so slip changes phase, move does not, and timeline trim preserves retained source phase. Record these edit invariants in domain tests.

## 5. Timeline and editor behavior

Timeline contains ordered video, audio, and adjustment tracks, each with lock/mute/solo/visibility, opacity/blend, and optional graph. Clips reference an asset or generator scene, timeline start/duration, source in/duration, rational playback rate, looping, transform/crop/opacity, audio gain/pan/fades, clip graph, automation, linked-clip group, and markers. Use finite explicit durations for generators default 8 seconds; live source clips default 30 seconds in Edit mode and unlimited runtime in Live scenes.

Render order: source → clip-local generators/animation → clip graph → transform/crop/matte → track composition → track graph → adjustment tracks affecting tracks below → master graph → display/export transform. Graph scope receives a ScopeInput node. Explicit ExternalSource nodes may be used; referencing an ancestor scope's unfinished output is invalid. Prevent track/matte cycles. Layers render bottom to top in stored order.

Source mapping at local time u: forward `source_in + u*rate`; reverse selects valid source frames before the half-open source end, never at the exclusive endpoint. Video selection: largest PTS <= target within source interval, clamp at endpoints. VFR uses indexed presentation timestamps. Never compute video frame number from nominal fps alone. Audio resampling/time-stretch follows the identical rational clip mapping.

Editing rules:

- Trim clamps to source boundaries unless looping; generator duration can extend. Minimum video duration one project frame; audio minimum one sample.
- Split only inside a clip. Preserve graph/settings, correct source mapping and automation continuity, and linked A/V alignment. New IDs, shared asset.
- Ripple delete closes the removed interval on selected ripple-enabled tracks; locked tracks stay fixed and conflicting linked edits fail atomically.
- Slip moves source window without changing timeline range. Rate change dialog chooses preserve duration (change source span) or preserve source span (change timeline duration); default preserve source span.
- Insert shifts later clips on targeted unlocked tracks. Overwrite replaces covered intervals and splits survivors. No silent overlap merging.
- Crossfade is a transition with two inputs and explicit overlap. Add creates overlap by trimming handles only when available; otherwise explain missing handles. Audio fades use equal-power crossfade, video default linear-light dissolve.
- Reverse audio runs cached chunks in reverse; pitch preservation is a separate option. Time-stretch default off; when on use a proven reviewed stretcher adapter with recorded license, not a misleading resample toggle.
- Freeze uses selected frame PTS and requested finite duration. Loop crossfades are optional explicit controls; loop endpoints must not duplicate the first frame.
- Snapping defaults on: nearest clip edge/playhead/marker within 8 logical pixels; Alt bypass. Internally snap to exact target rational time, not pixel-rounding.
- Range selection, multi-select, duplicate, copy/paste, undo/redo, markers, slip, roll trim, ripple trim, zoom-to-fit, scroll/zoom, and audio fades are required.

Audio mixer: 48 kHz float32 stereo engine; retain multichannel source metadata, expose channel selection/downmix; -6 dB center pan law and unity stereo balance center; peak meters and optional limiter with documented delay. Clip gain, track gain, master gain, mute/solo, fades. No clipping silently hidden: use float headroom and final optional limiter; encoder clips only per explicit export policy. Mixer has deterministic offline path. Scrub audio default off, user enables short prebuffered grains.

Pitch-preserving retime uses Signalsmith Stretch on prepared audio workers. Account for input/output latency, prefill and drain, and produce exact requested output sample count. Default quality uses its supported default preset, initial reliable retime range0.5×..2× with wider rates explicitly lower quality. Reverse first resolves bounded reversed samples then applies stretch. Test held sine pitch, transient timing and long-run output sample count; do not run stretching allocations in CPAL callback.

UI layout: 56 px top bar; left bin/library default 260 px; right inspector 320 px; center preview/graph tabs; bottom timeline 300 px; resize panels with persisted bounds. Minimum usable editor 1280×800 logical pixels; smaller screens allow collapsible panels. Dark neutral theme, readable 13–14 px labels, color-coded ports plus type labels, visible focus, control contrast at least WCAG AA for text, keyboard-accessible controls. Transport always visible, output can be active with editor hidden.

Media bin supports drag/drop, folders, search, tags, thumbnails, waveform peaks, missing badge, relink, proxies. Node library has search, categories, favorites, preview thumbnail, required input description, and cost class. Graph has search-add at pointer, connection validation, auto-insert on edge, grouping/macros, box selection, duplicate, alignment, frame-selection, zoom, bypass, delete/undo. Visible unconnected requirements must be actionable. Shader editor is Advanced; ordinary users choose controls and presets.

Shortcuts: Space play/pause; J/K/L reverse/stop/forward with speed ladder; arrows frame-step while paused; Shift-arrows 10 frames; Ctrl/Cmd+S save; Shift+S Save As; Z undo; Shift+Z redo; C/V/X copy/paste/cut with Ctrl/Cmd; Delete removes selected objects; S split in focused timeline; Home/End range start/end; I/O range in/out; F focus selected; Escape stops capture gesture/closes dialog/exits fullscreen. Text fields consume their editing keys. Do not use a global shortcut that deletes clips while typing.

## 6. Media and live sources

Use per-source FFmpeg demux/decode workers with bounded packet/frame queues and explicit seek epochs. Probe stream metadata once; preserve stream index, PTS/time base, rotation, pixel aspect, range/matrix/primaries/transfer, channel layout, and VFR data. Seek to keyframe <= target then decode forward; discard old epoch packets/results. Support EOF/draining correctly, including delayed B frames. Missing PTS fallback is recorded and deterministic.

Baseline decode converts to RGBA8 or planar YUV then uploads; implement a correct CPU conversion before hardware interop. Hardware decode through VideoToolbox on macOS and D3D11VA on Windows when supported; hardware decode may still download/upload. Report actual path. Texture interop is a measured optimization with explicit adapter/device/format constraints. Do not block feature correctness on it.

Decoder pool: default max 4 active file decoders, 256 MiB decoded frame cache, LRU by asset/PTS/scale/color conversion. Live queues depth 3, drop oldest. File decode prefetch 8 frames maximum subject to byte budget. Export sources use separate worker ownership to avoid live seek interference. Reverse playback caches GOP windows; long/reverse high-resolution sources offer proxies, never decode whole files into RAM.

Thumbnails: original rotation respected, time samples at defined positions, background job cancellation. Waveform cache: min/max and RMS pyramid per channel, 256/1024/4096/16384 samples per bucket; avoid decoding audio in UI. Proxy: 960 or 1280 longest dimension, CFR project-compatible rate, preserve timestamp mapping via sidecar index, known color conversion; source is always authoritative for export unless user explicitly exports proxy quality. Cache key includes file fingerprint and tool version.

Source interface: enumerate/query capabilities/open/poll frame/close/reconnect. Frame timestamp and receive timestamp separate. Device state: closed → opening → active → lost → retrying → active/failed. Retry 0.5,1,2,4,8 seconds then 8-second intervals while requested; stop on deliberate close. Permission denial requires user OS action, not endless reopen prompts. Freeze-last or transparent replacement after loss selectable; default transparent after 500 ms with visible status.

Windows camera: Media Foundation enumeration and source reader, supported modes from native media types. Screen: Windows Graphics Capture for displays/windows, region = capture plus GPU crop; DXGI desktop duplication fallback for displays where valid. Loopback: WASAPI loopback, default-output selection and device changes. Capture cursor option. Handle protected content as unavailable/black with source diagnostic; never claim bypass.

macOS camera: AVFoundation, explicit authorization. Screen/system audio: ScreenCaptureKit with native content picker/permissions, display/window selection and crop; query availability on supported OS. Use separate microphone stream if selected. If system-audio API unavailable, enumerate installed virtual input devices and document that route without installing drivers. Handle display scaling, top-left vs bottom-left coordinates, and excluded self-capture to avoid recursive editor capture.

Audio files: WAV/AIFF/FLAC/MP3/AAC/OGG when installed decode supports them; images PNG/JPEG/WebP/TIFF/BMP and GIF/APNG where supported. Image sequences require explicit numeric ordering/rate and missing-frame policy. Codec lists are runtime capabilities. Bad media must not crash import or hide previously imported assets.

## 7. GPU, color, shader ABI, and composition

Use one adapter/device per runtime GPU context. Required baseline: sampled/storage textures where needed, RGBA16F renderable/filterable capability checked, RGBA8 fallback with diagnostics; no mandatory subgroup/raytracing/vendor extension. Query actual limits; avoid requesting maximum device limits indiscriminately. Prefer high-performance adapter, expose manual selection/restart. Windows D3D12 primary with Vulkan fallback; macOS Metal. Persist selected adapter hint safely.

Working image: linear Rec.709 RGB, premultiplied alpha, top-left image origin, UV [0,1] with y downward. Default working targets RGBA16F. Source sRGB/Rec.709 transfer decoded explicitly, YUV matrix/range converted from metadata, input alpha premultiplied once. Display encodes through sRGB surface or explicit OETF, never both. Export output transfer/matrix/range explicitly tagged. HDR/P3 source imports tone-map/gamut-map to SDR with diagnostic; HDR mastering is an extension. User-facing color picker values are display-sRGB converted to linear internally.

Normal source-over: `out.rgb=src.rgb+dst.rgb*(1-src.a)`, `out.a=src.a+dst.a*(1-src.a)`. For artistic blend modes unpremultiply with epsilon, apply defined blend, composite with source/destination alpha, premultiply. Transparent RGB must be zero. Edge effects cannot introduce black fringes. Masks are linear R8/R16F 0..1. Depth uses positive camera-space meters, zero/invalid recorded separately. Normals declare view/world space. Motion vectors use output pixels per second; conversions explicit.

Shader ABI version 1:

- Group 0 binding 0: per-frame uniform, aligned to WGSL rules; matrices column-major.
- Group 0 binding 1: node parameter uniform; generated layouts use vec4/uvec4 blocks, documented offsets, padded to required alignment.
- Group 0 binding 2: read-only audio/features storage when supported; fixed defaults otherwise.
- Group 1: declared sampled input textures and samplers; manifest-derived layout, not unbound arbitrary arrays.
- Group 2: storage buffers/textures for compute/state when present; explicit read/write usage and pass barriers.
- Fullscreen triangle fragment entry `fs_main`; compute entry `cs_main`. All reads clamp/mirror/repeat according to manifest; no undefined out-of-bounds access.

Generate CPU packing and JS harness packing from the same schema. Test byte offsets and Naga validation. Include width/height/inverse size, time phases, frame index, seed, wet/dry, simulation dt, and quality. Masks/aux inputs get bound defaults (white mask, transparent color, neutral field), not invalid bindings. Every target is cleared or fully overwritten before reuse.

Texture pool keys by size, format, usage, sample count, and lifetime. Never alias a texture read and written by the same pass; ping-pong for blur/fluid/feedback. Resource planning counts both histories and scratch. Asynchronous pipeline creation errors attach to node ID, leave last valid pipeline. GPU loss recreates resources from immutable document and latest checkpoint or resets with discontinuity. Disable repeatedly failing custom nodes with an actionable error.

Every effect supports bypass and wet/dry. Default wet/dry blend uses premultiplied linear image interpolation; stylistic operations may explicitly select perceptual processing internally and must convert back. Pixel radii are expressed in reference output pixels; preview scale adjusts sampling so appearance matches export. No effect speed tied to display refresh.

## 8. Offline export, recording, and render parity

Export operates on a frozen project revision, selected range, recorded take, source fingerprints, seed, dimensions/rate, color/alpha settings, codec choice, and simulation warm-up policy. Save snapshot beside job metadata. Editing can continue, but it does not mutate a queued export. GPU export can share device with live runtime at lower priority; pause export when live frame budget is threatened. Offline kernels/time/audio are identical to live evaluator.

Recheck source fingerprints before and during job opening; changed/missing media fails with an explicit source-change report or uses a previously collected immutable copy. A document snapshot alone does not freeze external files. Simulation grid/capacity/fixed dt belong to the snapshot independently of output resolution; higher export resolution does not silently change fluid/particle physics. Changing a simulation-quality setting deliberately creates a new simulation/cache identity and requires warm-up again.

Sequence: validate capabilities/free space → resolve all sources → warm state from valid checkpoint/project start → evaluate audio/features/controls → render every output frame → async GPU readback staging (3 buffers, 256-byte row padding handled) → convert → encode → mux → flush video/audio encoders → finalize temporary output → probe/check → atomic rename. Handle encoder delay, AAC priming, packet DTS monotonicity, PTS/time-base conversion, and resampler draining. Keep alpha for PNG/ProRes 4444; reject incompatible alpha selections for H.264.

Required export choices: PNG RGBA sequence + WAV baseline; H.264/AAC MP4 when a real available encoder exists; HEVC MP4, ProRes MOV, JPEG sequence, GIF, and standalone WAV through detected encoders. Query encoder capabilities. H.264 default uses platform encoder when available, reviewed OpenH264 fallback or user-provided compatible build otherwise. Do not assume an LGPL FFmpeg binary contains libx264. Always provide PNG+WAV on supported hosts. Presets include web 1080p, vertical 1080×1920, square 1080, 4K UHD, master image sequence. MP4 even dimensions/pixel formats enforced or explicitly padded with user-visible choice.

Export queue persists job state; concurrency default one. Cancellation stops producers, drains or closes safely, removes only job-owned temporary files after validation, preserves completed output. Restart a failed encoded-video job from beginning; image sequences resume only if manifest hashes/time/seed match and state is restored/warmed correctly. Do not append arbitrary MP4 chunks as a fake resume.

Offline use of live input: require a recorded media take or user-selected frozen sample/explicit omit choice. An export must not silently poll current webcam/OSC state. Record live controls as timestamped take data. Live input recordings and control takes are linked by common capture epoch.

Live recording: render output to bounded encoder queue (8 frames max plus 3 staging); timestamp by master clock; capture chosen audio bus. When overloaded, CFR mode repeats last frame for missing output intervals and records drop counters, or VFR mode uses real PTS if container supports it. Default CFR. Stop with explicit `RECORDING_OVERRUN` if sustained overload >2 seconds; do not accumulate minutes of delay. No silent change from recording into offline export. Finalize recoverable recording on normal stop/error where possible. Segment long recordings every 5 minutes when recovery mode enabled.

## 9. Audio analysis and musical controls

CPAL handles selected mic/line devices. OS loopback is a distinct adapter. File audio comes from mixer/source selection. Max features are a distinct remote feature source, never raw PCM over OSC. Default analyze post-track/pre-master bus; selectable pre-effects/post-effects/individual channel/sidechain. Avoid feedback between monitored microphone and speakers by default input monitoring off.

Audio callback may copy/mix preallocated buffers, read atomics, and write rings. Preallocate maximum negotiated block sizes; handle variable block size. Underrun outputs zero with counter. Overflow drops stale analysis samples with epoch discontinuity, while recording can fail explicitly. No mutex/heap/network/FFT/logging on callback. Reconfigure device outside callback. Preserve input timestamps and sample positions, map to project clock with drift estimation.

Default analysis: 48 kHz mono sum with safe gain; Hann window N=2048; hop=512; configurable 1024/2048/4096/8192. Bass 20–160 Hz, low-mid 160–500, mid 500–2500, high-mid 2500–6000, treble 6000–16000; cap bands at Nyquist. Band energy uses window-gain-normalized power spectrum and documented sqrt power averaging. Features include RMS/peak, 5 bands, centroid Hz, spread Hz, rolloff Hz, flux, flatness, spectral magnitude texture, waveform, spectrogram, onset, tempo estimate/confidence, pitch/confidence, chroma. Silence features defined (0, confidence 0). Gate -60 dBFS default; attack 5 ms/release 120 ms. Normalize explicitly fixed calibration/adaptive tracked range, with same state replay for offline.

Onsets: positive spectral flux, rolling threshold + sensitivity, refractory default 80 ms, timestamp at window center plus known processing delay. Beat/tempo confidence is an estimate; do not display it as sample-accurate transport. Internal tempo default 120 BPM, 4/4. MIDI note/channel/pitch/pressure features are source streams; preserve MPE note identity and channel lifetimes, configurable lower/upper zone. Note-off clears gates.

Feature delay compensation range -500..+500 ms. Positive delay buffers visual response; negative delay requires prerecorded lookahead or is limited by causal live input. Never promise live prediction from a negative number. Default Max feature stale timeout 500 ms: release to zero using configured envelope and mark disconnected after 2 seconds.

Offline features compute from exact chosen audio bus and cache with source mapping/mixer settings/algorithm version. Same windows, center timestamps, calibration state, and latency policy as live. Export at a given time reads features by timestamp, not by render speed.

## 10. OSC, MIDI, and data protocols

OSC receiver default 127.0.0.1:9000; feedback target 127.0.0.1:9001 configured explicitly. No broadcast by default. OSC 1.0 messages/bundles: int32/int64, float32/64, strings, booleans, blob, color; recursively validate max depth 8 and payload 64 KiB. Timetags translated from NTP wall reference into monotonic schedule; immediate=1; horizon 5 seconds; late policy continuous apply-latest, triggers apply once within 250 ms else drop with count. Preserve order within equal timetag bundles.

Canonical addresses:

```text
/flux/v1/project/{projectId}/node/{nodeId}/param/{paramId}   float normalized 0..1
/flux/v1/project/{projectId}/node/{nodeId}/trigger/{id}     int 1, event edge
/flux/v1/project/{projectId}/transport/play                int 0|1
/flux/v1/project/{projectId}/transport/seek                string rational num, int den
/flux/v1/project/{projectId}/preset/recall                 string preset UUID
/flux/v1/bridge/{instanceId}/hello                        string protocol, int replyPort
/flux/v1/bridge/{instanceId}/features                     int seq, double seconds, 8 floats
/flux/v1/bridge/{instanceId}/transport                    int seq, double seconds, double beat, float tempo, int playing
/flux/v1/bridge/{instanceId}/heartbeat                    int seq
```

Feature order: RMS, peak, bass, mid, treble, onsetStrength, onsetFlag, confidence; use separate explicit messages for additional features. Bridge relative seconds anchor at handshake; transport discontinuity starts new epoch. Reply with session/instance identity. Raw native-value parameter route ends `/native`; color/vector arguments use native documented values. UI Copy OSC Address always emits real IDs. Human aliases map to IDs and remain optional.

OSC learn captures incoming address/type and binds via mapping. Wildcards exist only in configured mappings, not arbitrary project mutation. Rate defaults 1000 packets/s, 10000 total argument values/s, trigger queue 1024; token buckets and visible drops. Continuous parameter values coalesce by address; triggers do not. Feedback default max 30 Hz/change-only, tagged origin/session where possible, deadband 1e-4 to avoid loops. Never infer feedback target from arbitrary packet source. Control cannot execute commands, open arbitrary file paths, or load remote scripts; preset recall uses known project/catalog IDs.

MIDI 1: CC, notes, note velocity, pitch bend (14-bit), channel/poly pressure, program change preset mapping, clock 24 PPQN/start/stop/continue, learn, pickup/hysteresis, optional output feedback. Enumerate real devices and reconnect by stable ID/name hints; virtual ports only where actual backend supports them. Preserve raw arrival time. MIDI mapping into same modulation router. Keep Windows virtual-port limitations visible.

JSON/CSV data input: explicit local assets, schema/column selection, time column or row interval, deterministic interpolation. WebSocket/HTTP input: explicit source URL, timeout, byte/rate limits, user-enabled network source; async worker. Record values/events into a take for export. No arbitrary URLs loaded from untrusted projects without review in UI. File watcher debounces and cancels jobs; imported CSV/JSON cannot become shell commands.

## 11. Spout, Syphon, NDI, and external sources

Use a `VideoSource`/`VideoSink` interface: capability descriptor, enumerate, open config, frame lease/readiness, poll status, change size, reconnect, close. Formats RGBA/BGRA plus declared YUV options; alpha, orientation, and time metadata explicit. Output sends after final project transform; allow separate output tone/size conversion by user choice. Consumer disconnection cannot stall main render.

Spout: native Spout2 C/C++ shim on Windows, enumerate sender, receive/send texture and metadata, source name uniqueness, handle resize and adapter mismatch. Baseline bounded CPU BGRA transfer is acceptable and labeled. D3D11/12 sharing requires explicit same-adapter synchronization and actual tested interop; wgpu resources are not automatically shareable D3D11 textures.

Syphon: Objective-C shim on macOS using the selected official framework API, Metal route where available, IOSurface/OpenGL bridge only with correct lifetime and synchronization. Baseline bounded pixel-copy route if supported, report real path. Close clients/server on teardown, respect native callback thread ownership. Jitter source/consumer requires actual installed external; detector alone is not display support.

NDI: compile adapter against authorized official SDK headers, load compatible runtime, enumerate/find sources asynchronously, receive/release native frames with exact ownership, send video/audio, maintain stride/frame-rate metadata, map timestamps to project clock, and implement disconnect recovery. Keep runtime loader and FFI behind optional build feature when SDK unavailable. Provide loopback test tool. Missing SDK/runtime/license is `blocked_external`; do not invent header symbols, download untrusted binaries, or silently ship the runtime.

External engines (Three.js/p5/Godot/Blender/TouchDesigner) can render via existing screen/Spout/Syphon/NDI or a dedicated worker producing timestamped RGBA frames. This boundary avoids incompatible GPU texture types. A browser creative-code adapter must use explicit virtual time and renderFrame(t) for deterministic export; arbitrary web animations that rely on timers are live-capture-only until recorded.

## 12. Max for Live, Link, and optional plugin bridge

Ship two source devices: Flux Audio Bridge and Flux MIDI Bridge. Target Live 12 + bundled Max 9 as initial test target; record exact tested version. Include valid `.maxpat` sources, dependencies, README, tests, and source-package manifest. Package `.amxd` using actual Max for Live save/freeze workflow when available. Never rename `.maxpat` to `.amxd`; missing Max/Live host is packaging/runtime-verification blocker.

Audio bridge signal: `plugin~` → unity audio pass-through → `plugout~`. Branch analysis using built-in MSP filters/envelopes or bundled tested JS/native helper; compute RMS/peak and bass/mid/treble, onset gate. Snapshot meters at 60 Hz and defer UDP to low-priority queue. No audio stream over OSC. Use available OSC encoding object/helper verified in actual Max version; `udpsend` alone does not establish binary OSC conformance. Round-trip packet tests with desktop rosc decoder are mandatory.

Bridge UI: host/ports, enabled/connect, status/heartbeat, instance UUID, feature gain/gate/attack/release, meters, eight visual macros, preset previous/next/recall, latency offset, transport forward toggle, record-controls indicator. Parameter names stable for Live automation. Use Live API observers for playing/tempo/song time, bridge relative monotonic timestamps and sequence; forward true transport discontinuities. Multiple device instances coexist through UUIDs and configurable reply ports, one selected remote source per mapping. Save/reload a Live Set restores device state without hijacking standalone playback.

MIDI bridge passes MIDI through unchanged and exposes normalized controls/notes. MPE forwarding respects per-note channels. Optional Jitter viewer uses actual installed Spout/Syphon/NDI receiver and shows finished desktop output. It cannot fabricate a visual if external unavailable.

Standalone Link: implement SyncProvider interface and conditional official-library adapter after licensing clearance; GPL/proprietary options require explicit project licensing decision. Max transport forwarding works independently and is not advertised as native Link compatibility. Link synchronizes beat/tempo/phase and optional start/stop; no arbitrary effect values or PCM. Follow official test plan when enabled. Bridge and Link must not both compete as transport master.

VST3/CLAP extension: thin analyzer/control plugin sharing protocol/features with Max. Keep renderer in standalone process; audio callback never runs GPU UI. Build only if extension requested/enabled and framework/license approved. No promise that every DAW hosts video windows or receives visual output as a standard VST video port.

## 13. Persistence, recovery, and content

Project format `.flux.json`, UTF-8 versioned JSON. Preset `.fluxpreset.json` with stable IDs, required node versions, macros, seed policy, and asset references. Recorded take `.fluxtake` directory with JSON manifest, control-event chunks and captured media references. Portable project = folder with project plus `media/`, content index, presets, takes; ZIP export is optional convenience with zip-slip/bomb/size validation.

Persist atomic snapshots to same-volume temp, flush, then atomic replace preserving backup. Autosave every 30 seconds when dirty and after committed major operations; retain 20 versions, 7-day cleanup of app-owned autosaves only. Project manual save never becomes a partial autosave. Recovery manifest records original path, revision, timestamp, migration version; startup offers recover/open saved/discard recoverable copy. Corrupt snapshot falls back to previous valid one, reports exact revision. Migration creates backup, is tested/idempotent per step, preserves unknown extension fields. Newer unsupported schema opens read-only with error; never rewrite destructively.

Relink: choose path/search directory, match fingerprint/size/duration, show candidates, require user to resolve ambiguity, bulk relative-root remap with preview. Cache invalidation follows source changes. Collect Media copies to project-owned folder, checks disk space and collisions, never moves/deletes originals.

Settings in app-data JSON: window bounds, recent projects, preferred devices, preview scale, cache budgets, UI theme. Secrets excluded from project/preset; use OS credential store only if optional network source requires credentials. No analytics by default, no forced login/cloud/backend.

## 14. Performance, failures, security, and operation

Initial budgets: decoded frames 256 MiB; GPU targets/simulation resources 512 MiB or user-adjusted within queried limits; preview staging three buffers; all live queues bounded. Memory estimator before graph activation; reject OOM-risk config with suggested reductions. 4K RGBA16F is roughly 63 MiB per full-size texture before scratch/history; never allocate a dozen such targets without planning. Default particle 20,000/max 1,000,000 with capacity checks; simulation grid 256²/default, configurable 64..1024; SDF half-resolution/default 96 steps. No hidden unlimited loops.

Adaptive live quality can reduce preview copy rate, simulation render scale, ray steps, particle draw count, and expensive blur scale in that order. It cannot silently change project resolution, sim fixed dt, saved parameters, audio clock, or export quality. Show an active quality badge and reset option. Live output pacing deadline-based; latest frame presentation, no accumulated latency. GPU timestamp queries optional; CPU timings separately labeled.

Errors contain stable code, subsystem, project/asset/node/source IDs, severity, human message, recovery action, technical detail, and timestamp. Codes include GRAPH_TYPE_MISMATCH, GRAPH_CYCLE, GRAPH_REQUIRED_INPUT, PARAM_NONFINITE, SHADER_COMPILE, GPU_DEVICE_LOST, GPU_LIMIT, MEDIA_MISSING, MEDIA_DECODE, AUDIO_DEVICE_LOST, AUDIO_OVERRUN, OSC_MALFORMED, OSC_RATE_LIMIT, SOURCE_PERMISSION, SOURCE_LOST, SDK_MISSING, EXPORT_ENCODER, EXPORT_SOURCE_UNRECORDED, DISK_FULL, PROJECT_CORRUPT, PROJECT_VERSION, REVISION_CONFLICT. Dedupe repeated warnings; keep counter.

Logs rotate at 10 MiB × 5. Diagnostics export redacts user paths/device names/network secrets by default. Crash reports opt-in, no media upload. GPU hangs or decoder crashes require explicit runtime/process isolation escalation when observed; no claims that a Rust panic boundary catches arbitrary driver/native crashes.

Tauri capability scope restricts frontend to required commands/file operations. Expose asset IDs via custom protocol rather than unrestricted filesystem URLs. Reject SVG scripts, `foreignObject`, external entities, external network references and unsupported active content; parse supported vector subset. Validate fonts/paths/project lengths. Native third-party plugin loading stays disabled unless explicitly enabled; plugins are trusted executable code. FFI uses owned opaque handles, matching free calls, version checks, catch/no-unwind boundary, and thread-affinity documentation.

Package default app plus required media/vector libraries and notices. Avoid admin installation where possible. Windows MSI/NSIS and macOS `.app`/DMG with exact tested architecture; signing/notarization only with real credentials. Installer must launch without dev server, repository, package manager, internet, or developer PATH. Include license texts for shipped code, but do not add creator columns to effect/generator UI or catalog documentation.
