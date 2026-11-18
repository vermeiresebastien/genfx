# Flux — implementation architecture (code-verified)

Written against HEAD of this working tree by reading the Rust/JS sources, not the spec.
Every structural claim carries a repo-relative path and line numbers. Claims I could not
confirm by reading code are labelled **[inferred]**. Where `spec/`, `implementation/` or
`README.md` disagree with the code, the code is reported and the disagreement is tagged
**DIVERGENCE**. Nothing here restates the README or sells anything.

Crate map (`Cargo.toml:3-16`): 12 workspace members — `crates/{domain,engine,render,media,
audio,catalog,xtask,app,integrations,platform,tests}` + `apps/desktop/src-tauri`.
`apps/{max-for-live,ndi-bridge,spout-bridge,shader-harness}` are **not** members (data +
external build steps only). Edition 2021, `rust-version = 1.85` (`Cargo.toml:17-21`),
wgpu 27 (`Cargo.toml:30`).

---

## 1. What Flux is, and the invariants an editor must not break

Flux is a document-driven, GPU-executed motion-graph editor with an exact-time engine, a
Rust-built node catalog that generates both the UI and the shaders, and FFmpeg as the only
encoder. Six invariants are load-bearing.

### 1.1 Exact rational time is the only timeline authority

`RationalTime { numerator: i64, denominator: u32 }` — `crates/domain/src/time.rs:10-13`.
Denominator 0 is rejected (`time.rs:30-35`); values are always reduced.

- Frames→time: `from_frames(frame, rate) = frame * rate.den / rate.num` (`time.rs:45-50`),
  so 59.94 (`FrameRate{num:60000,den:1001}`, `time.rs:183-200`) stays exact. Pinned by
  test: frame 60 @ 60000/1001 = 1001/1000 s (`time.rs:325-330`).
- Comparison is an i128 cross-multiply, never float (`time.rs:131`).
- Frame/sample conversion is explicit about direction: `div_floor` (`time.rs:146`),
  `div_round` (`time.rs:155`), `div_ceil` (`time.rs:163`). Tests: 10.3 s @ 60 fps ceil = 618
  (`time.rs:333-340`); 10.5 s @ 48 k round = 504 000 samples (`time.rs:343-348`).
- `as_f64` is documented "For rendering only; never the timeline authority"
  (`time.rs:170-173`). Any new code path that stores seconds into the document breaks this.
- Wire format: the numerator is serialised as a **decimal string** so no JSON consumer can
  round it (`time.rs:217-222, 246-254`); test pins exactly
  `{"numerator":"-5","denominator":2}` (`time.rs:357-363`).
- Overflow returns `Err`, it does not wrap (`time.rs:351-354`).

### 1.2 Typed commands + deterministic undo, gated on `expected_revision`

`Command` has 22 public variants plus 3 `#[serde(skip)]` inverse-only variants
(`AddNodesBatch`, `RemoveAsset`, `RemoveMarker`) — `crates/domain/src/commands.rs:19-120`.

`Dispatcher { project, revision, undo, redo, max_undo }`, `max_undo = 200`
(`commands.rs:142-159`). `apply(cmd, expected_revision: Option<u64>)`:

1. stale revision → `ErrorCode::RevisionConflict` with recovery "reload document"
   (`commands.rs:175-184`);
2. the project is **cloned** before execution and the clone is restored if the command
   errors — snapshot-restore atomicity (`commands.rs:185-194`);
3. on success: revision++, transaction pushed, **oldest** entry trimmed above 200, redo
   stack cleared, `modified_utc` stamped (`commands.rs:196-212`).

Determinism of undo is structural, not a separate code path: `undo()`/`redo()` synthesise
the inverse command and re-enter the *same* `execute()` (`commands.rs:215-256`). Anything
that makes an editor action mutate `Project` directly destroys this guarantee.

Guards inside the dispatcher:
- non-finite numeric parameters are rejected as `ParamNonfinite`
  (`commands.rs:642-661`);
- ranges must be non-negative start, positive duration, ≤ `MAX_PROJECT_SECONDS`
  (`commands.rs:663-677`, `document.rs:13` = 604 800 s = 7 days);
- `Connect` records the sentinel inverse `Disconnect { edge_index: usize::MAX }` meaning
  "remove the last edge" (`commands.rs:461-464, 474-477`);
- clip→source mapping is `source_in + local * rate` with no float round-trip
  (`commands.rs:793-804`).

**Defect (verified):** `validate_clip` is a stub — `let _ = asset_id; let _ = timeline;`
(`commands.rs:679-692`). Nothing checks that a clip's asset exists at edit time; the
failure surfaces later at decode as `MediaMissing`.

### 1.3 The 96-byte frame ABI

`FrameUniform` is `repr(C)` with six 16-byte blocks and `FRAME_UNIFORM_SIZE: usize = 96`
(`crates/engine/src/frame.rs:12-23`), serialised by a hand-written little-endian
`to_bytes()` (`frame.rs:26-52`) so the engine needs no bytemuck (`engine/src/pod.rs`, 8
lines of `Pod`/`Zeroable` shims; the real mirrors live in `render/src/pipeline.rs:12-29`).
The layout is pinned by a test that asserts `dims[1]` sits at offset 4 and `counters[0]` at
offset 48 (`frame.rs:227-229`). The WGSL mirror is emitted by the generator
(`catalog/src/shader_gen.rs:64-71`) and `SHADER_ABI_VERSION = 1` is stamped into the file
header (`pack.rs:12`, `shader_gen.rs:60`). **Changing field order, or the struct text in
`shader_gen.rs`, without bumping `SHADER_ABI_VERSION` silently shifts every shader's view
of time.**

Time is split to survive f32: `time_split()` puts `high = floor(secs/1024)*1024` and
`low = secs - high` into `clock.x/clock.y` (`frame.rs:136-141`). **[verified code, doc
wrong]** the doc comment above it does not describe that 1024 quantum; trust the code.
Shaders recover seconds with `time_seconds() = clock.x + clock.y`
(`shader_gen.rs:120-154`).

### 1.4 Catalog as single source of truth

`crates/catalog/src/lib.rs:1-4` states it; the mechanism is: one Rust-built `Manifest`
per node type → parameter UI (`apps/desktop/ui/view/inspector.js:300-306` switches on
`decl.kind`), uniform packing (`catalog/src/pack.rs`), WGSL (`catalog/src/shader_gen.rs`),
docs/tests (`crates/catalog/src/tests.rs`), and the requirement coverage report
(`crates/xtask/src/bin/requirements.rs:22-32`, output `implementation/requirements.json`, which both harnesses re-generate in memory and byte-compare with `--check` rather than trusting the committed file).
There is no `.wgsl` file on disk for catalog nodes and no `build.rs` in `flux-catalog`;
shader text is assembled at runtime.

### 1.5 Graph limits

`MAX_NODES 4096`, `MAX_EDGES 16384`, `MAX_NESTING 16` — `crates/engine/src/graph.rs:17-19`.
Only the first two are enforced (as `ErrorCode::GpuLimit`, `graph.rs:384-395`).
**[verified] `MAX_NESTING` has no enforcement site anywhere in the tree** — grep finds the
constant and nothing else. Nested/subgraph composition is therefore not actually bounded
or implemented at the engine level.

### 1.6 Colour model

Exactly one working colour space is representable: `WorkingColor::LinearRec709`,
serde-renamed `"linear_rec709"` (`crates/domain/src/document.rs:43-47`) — there is no
alternative variant to migrate to. Shaders declare `color_policy:
"linear_premultiplied"` (`catalog/src/builders.rs:228-244`), the helper `unpremul()` is in
the shared preamble (`shader_gen.rs:113-117` → `catalog/src/wgsl_common.rs:155,191`), and
the offscreen target format is `Rgba8Unorm` with **`blend: None`** — compositing is done in
shader code, never by the GPU blend state (`render/src/pipeline.rs:338-342`). Clear colour
is transparent black (`render/src/graph_exec.rs:445-465`).

### 1.7 Audio analysis constants

`crates/audio/src/lib.rs`: `ANALYSIS_VERSION = 1`, `SAMPLE_RATE = 48_000`,
`DEFAULT_WINDOW = 2048`, `DEFAULT_HOP = 512`, five band edges 20–160, 160–500, 500–2500,
2500–6000, 6000–16000 Hz (`:14-27`). Config defaults: gate −60 dBFS, onset sensitivity 1.5,
refractory 80 ms (`:30-49`). FFT amplitude scale `2/n/window_rms(coeffs)` (`:229`), periodic
Hann `0.5*(1-cos(TAU*i/n))` (`:152-159`), bands = sqrt(mean power)/0.5 clamped (`:234-251`),
spectral rolloff at 85 % energy (`:272`), onset = **median** of a 43-entry flux history ×
`max(sensitivity,1.0)` with a refractory gate (`:296-320`), tempo by autocorrelation limited
to 60–200 BPM (`:322-326, 394`), centroid normalised by 20 000 Hz (`:104`).

The vector layout handed to the GPU is the contract: `to_snapshot_audio()` sets
`bands = [bass, mid, treble, low_mid]`, `extra = [rms, onset, peak, flux]`,
`params = [tempo_bpm, beat_phase, centroid_hz, tempo_confidence]` (`audio/src/lib.rs:113-127`),
and the shader accessors read `bands.x/.y/.z` as bass/mid/treble
(`catalog/src/shader_gen.rs:120-154`). The audio uniform is 48 bytes
(`render/src/graph_exec.rs:98-103`).

Related document constants: `SCHEMA_VERSION: u32 = 2` (`domain/src/document.rs:11`), default
project 1920×1080 (`document.rs:22-25, 60-75`), 60 fps, PAR 1/1, 48 kHz/2 ch, seed `"0"`,
`simulation_step = 1/120`, default timeline range 8 s (`document.rs:443`), 21-variant
`PortType` (`document.rs:126-149`), `ParamValue` tagged enum incl. `Color([f64;4])`
(`document.rs:162-173`). All document collections are `BTreeMap` so serialised key order is
deterministic (`document.rs:3-5, 567-578`) — required for byte-stable project files and for
the `git`-diffability the workflow depends on. Save is atomic (`.tmp` → fsync → rename,
keeping `.bak`) and refuses to rewrite a newer schema (`app/src/persistence.rs:22-53,
63-86`).

---

## 2. Data flow: document → engine compile → render execute → media encode

```
Project (domain/src/document.rs)
  ├─ Command/Dispatcher (domain/src/commands.rs)      edits + undo, revision++
  ├─ params.rs resolution: base→keyframes→mappings→fold→clamp (domain/src/params.rs:3-8)
  ▼
flux_engine::timeline::render_items(project, project_time)   (engine/src/timeline.rs:186)
  │  visible_clips: solo override, audio tracks skipped,
  │  opacity = clip.opacity * track.opacity                  (timeline.rs:98-120)
  │  map_source_time: local*rate exact + Loop wrap via div_floor (timeline.rs:60-95)
  ▼
flux_engine::graph::compile(&Graph, &Registry, graph_version) -> CompiledGraph
  │  (engine/src/graph.rs:382) Kahn topo sort, deterministic sorted ready queue,
  │  edges into flux.feedback_read skipped (graph.rs:399-436)
  │  passes: [CompiledPass { index, node_id, type_id, inputs:
  │           BTreeMap<port, InputBinding::{NodeOutput|ScopeSource|Default}>, bypass }]
  ▼
flux_render::graph_exec::RuntimeGraph::activate(...)  (render/src/graph_exec.rs:54-123)
  │  pipelines precompiled; one render target per non-terminal node + output_target
  ▼
RuntimeGraph::execute_with_producers(gpu, compiler, cat, ctx, scope_view, resolved, wet)
  │  (graph_exec.rs:291-465) uniform packing + bind groups + draw(0..3) per pass
  ▼
flux_render::preview::read_full(...) -> RGBA8 (render/src/preview.rs:236-299)
  ▼
flux_media::StreamingEncoder (rawvideo over stdin) + finish() audio mux
  │  (media/src/lib.rs:474-541, 548-600)
  ▼
MP4/MOV/GIF/JPEG-seq + WAV  (app/src/export.rs:104-198)
```

Concrete entry points, in order, with the real names:

- **GUI edit**: `apps/desktop/ui/ipc.js:30` → Tauri `dispatch` →
  `apps/desktop/src-tauri/src/lib.rs` → `flux_app::commands::CommandHost::handle(Request)`
  (`crates/app/src/commands.rs:233`) → `Dispatcher::apply` → `Response { request_id,
  revision, result, error }` (`commands.rs:157-174`).
- **Per-frame CPU state**: `FrameContext::for_project(&project, t, counters)`
  (`engine/src/frame.rs:92-109`); the exporter steps frames with
  `t = start + frame_time(i, fps)` (`app/src/export.rs:148-160`) and the total with
  `export_frame_count = div_ceil` (`engine/src/frame.rs:174-178`).
- **Parameter values**: `flux_engine::router::Router::resolve_node` produces
  `ResolvedParams = BTreeMap<String, f64>` (`engine/src/router.rs:11, 41`); the exporter
  instead resolves defaults → stored `ParamValue` → floats itself
  (`app/src/export.rs:405-449`) because the GUI path needs the per-node override cache.
- **CPU producers**: nodes whose content is computed on the CPU return RGBA8 from
  `flux_app::producer::produce_node` (`app/src/producer.rs:784`), get uploaded as
  `aux/producer` textures and are bound in preference to edges
  (`app/src/export.rs:518-595`, `graph_exec.rs:361-375`).
- **Encode**: one `StreamingEncoder` per video codec run writes raw RGBA8 to FFmpeg's
  stdin (`media/src/lib.rs:474-486`), with a strict `w*h*4` frame-size check
  (`media/src/lib.rs:528-541`), then `finish(Some(&wav))` does the audio mux as a second
  pass with `-c:v copy -c:a aac -b:a 192k -shortest` through a temp `.mux.mp4` and rename
  (`media/src/lib.rs:548-600`).

FFmpeg is deliberately out-of-process (`media/src/lib.rs:1-7`, DECISIONS D004); resolution
order is `FLUX_FFMPEG` → `PATH` → the winget `Gyan.FFmpeg` directory
(`media/src/lib.rs:17-41`). Decoded frames carry `DecodedFrame.pts: RationalTime`
(`media/src/lib.rs:74-79`) so seek precision survives the round trip.

---

## 3. The catalog: how one node exists end to end

### 3.1 Aggregation and arithmetic

`flux_catalog::build_catalog()` lives in `crates/catalog/src/utility.rs:240-429` and
enumerates `(key: &'static str, Manifest, body: &'static str)` tuples from nine modules:

| group | module | rows |
|---|---|---|
| fx001–fx028 requested effects | `effects_a` (6) `effects_b` (13) `effects_c` (9) | 28 |
| fx100–fx123 families | `families.rs` | 24 |
| g001–g028 2D/math generators | `generators_a.rs` | 28 |
| g030–g045 + g070–g079 vector/type | `generators_b.rs` | 26 |
| g050–g068 image animators | `generators_c.rs` | 19 |
| g080–g185 scene/particle/physics/sim/control/conditional | `generators_d.rs` | 83 |
| `flux.*` utility nodes | `utility.rs:7-181` | 9 |
| | **total `Catalog::entries`** | **217** |

DIVERGENCE (partly closed 2026-11-15): `README.md:15` now says **217**; the stale "216" survives
only in `FINAL-REPORT.md` :71, :77, :81, :100, :102, :278 (its own :15-16 flags them). The code
produces **217**. `catalog_row_count()` (`catalog/src/lib.rs:28-34`) filters out rows tagged `R007`
— all 9 utility rows are (`utility.rs:20,35,55,93,126,145,159,168,175`) — so it returns **208**,
which matches `implementation/requirements.json` `counts`: 28 effects + 24 families +
156 generators = 208. The "216" is a stale count that survived one added row.

`Catalog::get` is a linear scan over `entries` (`manifest.rs:208-210`) — see §4.5 for why
that matters.

### 3.2 The `Manifest`

`crates/catalog/src/manifest.rs`: `Manifest` (`:143-177`) = id, version, requirement_ids,
name, category, `implementation`, inputs, outputs, parameters, time_model, color_policy,
quality_class, fixtures, help, generator_id, loop_status, tags, accent.
`PortDecl { id, port_type, required }`, `ParamDecl` (`:81-111`) = id, label, kind, default,
min/max, soft_min/soft_max, scale, unit, automatable, modulatable, smoothing_ms,
state_impact, options, hidden. `Implementation { kind, path, entry }` (`:115-120`),
`TimeModel::{Analytic,FixedStep,CanonicalFrame,External}` (`:128-133`),
`QualityClass::{L,M,H}`. `Manifest::defaults()` returns parameters **in manifest order**,
which is explicitly documented as the order "used by uniform packing and by node creation"
(`manifest.rs:184-191`) — manifest order is layout, not cosmetics.

`builders::effect()` (`catalog/src/builders.rs:196-245`) is the constructor every row uses:
it injects `wet` via `standard_params()` (`builders.rs:191-193`) if absent, then **forces
`wet` to index 0 "for stable uniform slots"** (`builders.rs:206-221`), and pins
`kind:"fragment"`, `path:"{shader}.wgsl"`, `entry:"fs_main"`,
`color_policy:"linear_premultiplied"`, `time_model: Analytic` (`builders.rs:228-244`).
Param helpers encode capability defaults: `p_enum` sets `modulatable:false`, `p_text` sets
`automatable:false`. Rows that need non-standard ports overwrite them after the call
(e.g. `utility.rs:22,37-42,95-107,161-162`; `generators_b.rs:10-24` replaces ports and
re-prepends its own `wet` named "Opacity").

### 3.3 Uniform packing

`crates/catalog/src/pack.rs`: `SlotKind` per param; `ParamLayout::byte_size() =
vec4_count*16` (`pack.rs:51-53`). `params_wgsl()` (`pack.rs:73-199`) skips `Text` params
entirely, maps Float→1×f32, Color→vec4, Vec2→vec2, Vec3→vec3, Int/Bool/Enum→1×u32, and
packs into **two independent vec4 arrays** — floats first, then uints — with `f_count` and
`u_count` floored to 1 (`pack.rs:126-138`) — WGSL forbids zero-length arrays. It emits
`struct NodeParams { f: array<vec4<f32>,N>, u: array<vec4<u32>,M> }` plus one accessor
`fn p_<id>() -> ...` per parameter; Int/Enum are surfaced as WGSL `i32` (`pack.rs:194-199`).
So a node's slot numbers depend on: manifest order (hence the `wet`→index-0 rule), which
params are `Text`, and the float/int mix. Reordering parameters in a manifest is an ABI
change.

### 3.4 The shared WGSL template

`crates/catalog/src/shader_gen.rs` holds **one** template. `generate_shader(entry)` emits,
in order: the "Do not edit by hand / Shader ABI version 1" header (`:60`), the generated
`params_wgsl` output (`:62`), `struct FrameUniform` (`:64-71`) and `struct AudioFeatures`
(`:73-77`), `@group(0)`: binding 0 frame / 1 audio / 2 node params (`:79-81`),
`@group(1)`: `src_tex`, `mask_tex`, `src_sampler`, `aux_tex` (`:87-105`), extras from
binding 4 (`:106-111`), the `wgsl_common` preamble fragments HASH/NOISE/COMMON/BLEND/SAMPLE
(`:113-117` → `catalog/src/wgsl_common.rs:6,24,127,155,191`), the helpers
`time_seconds()`, `seedf() = (counters.z % 100003)*0.0001`, `audio_bass/mid/treble()`,
`px_to_uv(px)` (`:120-154`), the bufferless fullscreen-triangle `vs_main` (`:161-174`), and
`fs_main` whose last line is `final_mix(src, process(uv, src), p_wet(), m)` (`:179-188`).

**A node's entire per-type shader content is the `fn process(uv, src)` body.** `wet`,
masking, colourspace mixing and the vertex stage are shared and not writable by rows.
Two code smells worth knowing before you touch it: the "effect vs generator" branches at
`:87-105` emit **identical** text (a dead if/else), and `quick_wgsl_check` (`:197-213`) is
only a brace/entry heuristic — the real validator is Naga in the renderer (§4.3).

### 3.5 Presets are multi-node recipes, not parameter dumps

`crates/catalog/src/presets.rs`. A `Preset` = `{ id, name, description, requirement_refs,
nodes: [PresetNode{type_id, index, params, position}], edges: [PresetEdge{from,from_port,
to,to_port}], output, macros }` (`:9-37`); edges reference **indices**, and inserting a
preset materialises real editable nodes with fresh ids.

`chain(types, id)` (`:52-158`) is the single generator of recipes. It is not a literal
transcription: `flux.source` in an authored recipe is **substituted** by a generator chosen
from the preset id — `gen.mesh_gradient_fill` for glass/chrome/thermal/metal/bloom,
`gen.texture_field` for the luminance-derived looks, otherwise `gen.plasma` (`:56-79`) — and
if the first node needs a `source` input a seed generator is prepended (`:88-103`). Each
adjacent pair is connected only when both ports exist **and the port types are equal**, via
`pick_output` (prefers `color`) and `pick_input` (prefers required `source` → `source` →
`a` → first) (`:286-307`); a `flux.output` sink node is always appended (`:131-146`).
`shared_catalog()` memoises `build_catalog()` in a `OnceLock` (`:281-284`) — note that
`chain()` calls it per recipe, so preset construction still rebuilds once, but
`Router::resolve_node` does not memoise (§4.5).

`all_presets()` (`:309-504`) returns **274** presets: 41 hand-authored, 25
`preset.brik_NN_*` workspace recipes (`:459-488`), 124 `preset.brik.gallery_NNNN` and 84
`preset.brik.community_NNNN`, the last two groups generated by keyword-matching name lists
(`BRIK_GALLERY_NAMES` `:229-253`, `BRIK_COMMUNITY_NAMES` `:255-279`) through
`brik_library_recipe` (`:169-227`) — i.e. those two hundred-odd "presets" are heuristic
bucket assignments over the same 8 template chains, not individually authored graphs.
Tests assert the counts and that every referenced `type_id` exists in the catalog
(`:511-563`).

### 3.6 How to ADD a node (numbered procedure)

1. **Pick the requirement id.** Row ids are `FX0xx`/`G0xx`/`R0xx` and drive
   `implementation/requirements.json` via `crates/xtask/src/bin/requirements.rs:26-32`.
   Utility-only nodes use `R007` (which excludes them from `catalog_row_count()`).
2. **Choose the module** for the family (`effects_a|b|c`, `families`,
   `generators_a|b|c`, `generators_d`). Rows are plain `pub fn gNNN_name() ->
   (&'static str, Manifest, &'static str)` tuples; there is no macro requirement except the
   local conveniences (`vgen!` in `generators_b.rs:10-24`, `scene_row`/`cpu_row`/`cond` in
   `generators_d.rs`).
3. **Build the manifest with `builders::effect(...)`** (`builders.rs:196-205`), passing id
   (`fx.*`/`gen.*`/`flux.*`), display name, **category string** (this exact string is the
   library grouping — `Catalog::by_category`, `manifest.rs:212-218`), the shader-name stem
   (`"{stem}.wgsl"` is recorded as `source_path`, no file is read), `QualityClass`, params
   from the `p_float`/`p_int`/`p_enum`/`p_color`/`p_text`/`p_vec*` helpers, help text,
   requirement id.
4. **Set ports explicitly** if the default `source + mask → color` shape is wrong: assign
   `m.inputs` / `m.outputs` after the call, using `source_port()`, `mask_port()`,
   `color_output()` or raw `PortDecl`s with `manifest::PortType`. A second image input must
   be named **`b`** — the executor binds `b` to `aux_tex` and `mask` to `mask_tex`
   (`render/src/graph_exec.rs:361-389`). Generator rows must have **no** `source` input or
   they will be treated as needing an upstream image by the preset builder and the sweep
   harness (`presets.rs:95-104`, `crates/tests/tests/sweep_catalog.rs:45-62`).
5. **Override `time_model`** if the node is not `Analytic` (`FixedStep` for the
   simulation rows, `CanonicalFrame` for the feedback pair — `utility.rs:170,177`), and
   `loop_status`/`generator_id` for generators (`generators_b.rs:21-22`).
6. **Write the body** as a `const &str` WGSL snippet containing exactly
   `fn process(uv: vec2<f32>, src: vec4<f32>) -> vec4<f32>`. Use only the generated
   `p_<paramId>()` accessors, `src_tex`/`mask_tex`/`aux_tex`/`src_sampler`, the extras from
   binding 4, and helpers from `wgsl_common.rs`. Never redeclare `FrameUniform`,
   `NodeParams`, `vs_main`, `fs_main` or any helper — the template already did.
7. **Register it** by adding the call to the appropriate array in
   `utility.rs::build_catalog()` (or to the `*_rows()` chain list at `utility.rs:407-417`).
   Nothing is discovered by reflection; an unregistered row does not exist.
8. **If content is CPU-computed**, add the type id to
   `crates/app/src/producer.rs::producer_for` (`:43-131`; 96 ids across 7 CPU producer
   families today) and use the passthrough body (`generators_d.rs::cpu_node`), otherwise
   the node renders whatever texture happens to be bound. Keep the producer contract:
   exactly `w*h*4` bytes, premultiplied, top-left origin, no wall clock
   (`producer.rs:11-19`).
9. **Do not touch the renderer for a new catalog node.** Pipelines, layouts and targets are
   derived from the manifest at `RuntimeGraph::activate`/`ensure_pipeline`
   (`render/src/graph_exec.rs:54-123`, `render/src/pipeline.rs:145+`).
10. **Run the lints** (they are ordinary `#[test]`s): `catalog_ids_are_unique`,
    `every_manifest_defaults_in_range`, `every_shader_generates_valid_structure`,
    `uniform_layout_matches_expected_size` (asserts `byte_size() % 16 == 0`, ≤ 4096, and
    slot count == non-`Text` param count) — `crates/catalog/src/tests.rs:21-231`.
    If the node should appear in a preset, add the chain in `presets.rs` (§3.5); the
    catalog-membership test is `preset_types_exist_in_catalog` (`presets.rs:806-820`).
11. **Then the GPU gates**: `cargo run -p xtask -- catalog` (duplicate ids + empty
    implementation paths, `xtask/src/main.rs:460-497`) and `cargo xtask test-gpu`
    (`xtask/src/main.rs:539-585`), plus `crates/render/src/tests.rs:17-33` which compiles
    the whole catalog on the real device.

---

## 4. The renderer

### 4.1 Device and formats

`flux_render::gpu::Gpu::new(BackendPref::Auto)` (`render/src/gpu.rs:10-20, 66`) stores
device/queue/adapter info (`GpuInfo::to_json`, `gpu.rs:32-56`) — this is what `flux doctor`
and `cargo xtask doctor` report. Readback honours the 256-byte row alignment:
`padded_bytes_per_row(width, 4)` (`gpu.rs:211-215`, tests at `:223-225`), applied again in
`preview.rs`.

### 4.2 One render target per node — a real DAG

`RuntimeGraph::activate(gpu, compiler, catalog, graph, w, h, version)`
(`render/src/graph_exec.rs:54-123`) does five things: precompile every pipeline
(`:54-65`); guarantee an identity `flux.source` pipeline whenever anything is bypassed so
the scope texture still reaches the graph (`:66-78`); allocate **`node_targets:
HashMap<String,(Texture,TextureView)>` — one texture per non-output node — plus a dedicated
`output_target`** (`:24-28, 82-91, 327-330`); allocate the 96-byte frame buffer and 48-byte
audio buffer (`:92-103`); allocate one UBO per node with `size = bytes.len().max(256)`
(`:118-123`).

`execute_with_producers` then walks `compiled.passes` in topological order and, per pass,
resolves the `source` binding by priority **producer texture > scope source
(`flux.source`/`flux.mask_input`) > `"source"`/`"a"` edge > dummy texture** (`:361-375`),
`mask` from a `"mask"` edge or the shared `white_mask` (`:377-382`), `aux` from a `"b"` edge
or a dummy (`:384-389`), renders with `LoadOp::Clear(TRANSPARENT)` and a
`draw(0..3)` bufferless triangle (`:445-465`), and if the output node never wrote, copies
the last node's texture as a fallback (`:475-499`).

DIVERGENCE (one of two, and it moved): `render/src/graph_exec.rs:2` module doc still says
"ping-ponged targets" — `README.md:22`, which this paragraph also used to cite, now reads "one
render target per node". The code is a true DAG; `implementation/DECISIONS.md:116` records the migration.

### 4.3 Pipeline creation and the Naga guard

`EffectCompiler` (`render/src/pipeline.rs:135-149`) owns the shared Clamp/Linear sampler, a
255-valued R8Unorm `white_mask`, a `black_tex` and a `dummy_tex`. `NodeParamsBuffer::
defaults(&manifest)` (`:41-95`) writes each param's default into its slot in the
floats-then-uints layout and pads to 16 (`:97-110`); enum defaults serialise as 0 with the
comment "index is resolved by the caller" (`:84-86`) — i.e. an enum's manifest default is a
string and the numeric index is resolved on the GUI/export side, not in the buffer.

`validate_wgsl` runs **Naga parse + validator before** `create_shader_module`, because
`create_shader_module` on a malformed module "aborts the process" rather than returning an
error (`:227-229, 396-416`). Any change to `shader_gen.rs` therefore fails as a
`ShaderCompile` error, not a crash — keep that ordering.
The colour target is `Rgba8Unorm`, `blend: None` (`:338-342`); `render_target()` uses
`RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC | COPY_DST` (`:493-513`).

Dead state to be aware of (verified, harmless today): the stored
`EffectPipeline.bind_group_layout` is created with `entries: &[]` (`:369-374`) and never
used for a real bind group; the present/process heuristic string-matches the body
(`:180-188`) and then does `let source = if Present { generated } else { generated }` —
a literal no-op (`:221-226`).

### 4.4 Preview path and the real 960×540 story

`crates/render/src/preview.rs`: `PREVIEW_MAX_W = 960`, `PREVIEW_MAX_H = 540` (`:7-8`),
`preview_size()` fits without upscaling (`:11-18`), `read_preview()` blits/linear-samples
into the downscaled RGBA8 texture, pads the row stride and blocks on
`map_async(..., PollType::Wait{timeout: None})` (`:23-232`); `read_full()` is the same at
native size, documented "Used by the exporter" (`:236-299`).

**Verified: `read_preview` has exactly three callers, none in the shipped product** —
`crates/render/src/tests.rs:164`, `crates/xtask/src/main.rs:774` and `:1134` (grep over the whole tree). The GUI and the exporter both go
through `PreviewRenderer::render_single_frame` → `read_full`
(`app/src/export.rs:319-610, 604`). The Rust-side 960×540 cap therefore does **not** bound
the editor today. What bounds it is JavaScript: `apps/desktop/ui/main.js:185-191`
computes `thumbSize() = min(960, max(160, round(176 * min(2, dpr))))` with `h = w*9/16`,
and `monitorSize()` returns a hardcoded `{w:960,h:540}` under the comment "The GPU readback
is capped at 960x540" (`main.js:193-196`).
DIVERGENCE: `spec/ARCHITECTURE.md:66` describes the intended native readback cap (960×540
max, 30 fps, binary channel, never base64 through reactive state). Partially realised: the
binary channel exists (`render_preview_raw`) but base64 PNG data URLs are still returned by
`render_preview`/`render_graph_preview` with a 4 MiB ceiling
(`apps/desktop/src-tauri/src/lib.rs:95-100, 396-401`), and there is no staging ring or reusable
pooled texture — a fresh bind group per pass per frame (§4.6).

### 4.5 The compile-time cost that is easy to miss

`Router::resolve_node` calls `flux_catalog::build_catalog()` **on every invocation**
(`engine/src/router.rs:41`) — a 217-manifest rebuild (with all WGSL text allocation) per
node per frame, followed by a linear `Catalog::get` scan (`manifest.rs:208-210`). The
export path avoids `Router` for exactly this reason and does its own resolution
(`app/src/export.rs:405-449`); the GUI's `CommandHost` holds a `route: Router`
(`app/src/commands.rs:187-194`) and uses it for `ResolveParams`.
**[inferred]** this is the reason preview throughput looks better through the export path
than through `resolve_params`; it is not measured here.

### 4.6 Per-frame allocations that should not be

`execute_with_producers` calls `ctx.uniform(wet)` **six times per frame**
(`graph_exec.rs:291-298`) and creates fresh `BindGroupLayout`s and `BindGroup`s for every
pass on every frame (`:391-443`), while the layouts it computed at activation time
(`:126-223`) are never read afterwards. Combined with the `max(256)` UBO sizing
(`:118-123`) this is the obvious first place to look for preview jank before blaming the
shaders. Also verified dead: `produce_generator_frame` is a registered Tauri command that
throws its result away — it runs `produce_node` and returns only the string `"producer"` or
`"analytic"` (`src-tauri/src/lib.rs:286-306`); and `engine/src/graph.rs:538` is a literal
self-assignment (`nodes[0].id = nodes[0].id.clone();`) inside `simple_chain`, the helper the
router uses to build probe graphs (`router.rs:287`). That same helper mints **random** uuids
for every node id and for the graph id (`graph.rs:496, 505, 514, 540`), so anything built
through it is not byte-reproducible.

---

## 5. Control surfaces

### 5.1 `flux` CLI (`crates/app/src/main.rs`)

Default with no arguments is `"gui"` (`main.rs:30`), and `"gui" | "editor"` both print
"The GUI is not included in this build." and `exit(2)` (`main.rs:133-136`). 26 match arms
covering 27 subcommand names (`main.rs:31-133`):

`list` `doctor` `new-demo` `demo-render` `demo-media` `contact-sheet`
`render` (`--codec`, default h264, `main.rs:58-74`) `recover` `collect` `caps` `capture`
`diagnostics` `models` `depth` `integrations` `link` `ndi` `ndi-recv` `ndi-loopback`
`ndi-send` `spout` `spout-send` `spout-send-dx` `osc` (port arg defaults to
`DEFAULT_RECV_PORT`) `record` (`--seconds` default 5.0, dir `rec_out`, `main.rs:116-132`)
`gui`/`editor`.

DIVERGENCE: the `unknown command` help line (`main.rs:140`) lists 18 names and the module
doc (`main.rs:2-9`) lists 7. Both are stale; `main.rs:31-133` is the truth.
**Recording is not a real-time path (verified, and it contradicts its own doc).**
`cmd_record` calls `export::export_project` — the same deterministic offline exporter as
`flux render` — and only *afterwards* compares `start.elapsed()` against
`seconds * 1.05`; if it exceeded that budget it synthesises a `dropped_frames` count from
the wall-clock deficit and sets `overrun: true` (`app/src/record.rs:59-97`). The pacing
inputs are explicitly unused (`let _ = interval; let _ = target_frames;` at `:88-89`) and
`RECORDING_OVERRUN` exists only in the module doc comment (`:1-6`) — there is no such
variant in `crates/domain/src/error.rs:6-34`. A `DropCounter` using atomics does exist (`:100-120`) but is the
"for a real-time render loop" scaffolding, not what `run_job` uses here. Consequence:
`flux record` cannot tell you whether a setup sustains real time, only whether the offline
export was more than 5 % slower than the clip length.

### 5.2 Desktop GUI

Shell: Tauri 2 + vanilla ES modules, bundled by esbuild (`apps/desktop/ui/bundle.mjs`:
`format:"iife"`, `target:"es2020"`, `entryPoints:["main.js"]`) which then rewrites the
`<script src>` query of **every document it is told to serve** — one row per document in
`VERSIONED`, so `index.html` gets the bundle's hash and `monitor.html` gets `monitor.js`'s
own hash (`monitor.js` is not bundled: it has no imports, and a second entry would move the
artifact the Tauri window points at) — with a 12-hex content hash specifically to defeat the
WebView2 cache (`bundle.mjs:1-5, 57-123, 156-215`). Sources: `main.js` (887 lines), `commands.js`,
`ipc.js`, `store.js`, `monitor.js` and `view/{canvas,node,cable,inspector,library,overlay,
timeline}.js`; `app.bundle.js` is the committed build output, and it is now *gated* against
those sources: `check-bundle.mjs` re-bundles `main.js` in memory through the same exported
`BUNDLE_OPTIONS` (`absWorkingDir` pinned, so the artifact does not depend on the launcher's
directory) and compares bytes — a committed bundle that its own sources do not produce is a
red gate, not a green build (`bundle.mjs:156-215, 217-234`, I15 item 2).

Exactly **10** IPC commands are registered (`apps/desktop/src-tauri/src/lib.rs:430-441`):
`dispatch`, `open_monitor`, `toggle_monitor_fullscreen`, `render_preview`,
`render_graph_preview`, `render_preview_raw`, `build_info`, `export_project_cmd`,
`capabilities`, `produce_generator_frame`. UI call sites: `ipc.js:30,35,40,51,68,72`,
`commands.js:114,119`, `monitor.js:15,39,61`.

`flux-desktop` depends on domain/engine/catalog/app/media/platform/integrations but **not**
on flux-render or flux-audio (`apps/desktop/src-tauri/Cargo.toml`) — GPU work is reached
only through `flux_app::export::PreviewRenderer`, so there is exactly one renderer
implementation for GUI and CLI.

Shell behaviour worth knowing before editing:
- `AppState = { host: Mutex<CommandHost>, preview: Mutex<Option<PreviewRenderer>>,
  exporting: Arc<AtomicBool> }` (`src-tauri/src/lib.rs:16-24`).
- Preview times arrive as **f64 seconds** and are converted with
  `RationalTime::new(t*1e6, 1_000_000)` (`:78, 338, 386`) — the only place seconds enter the
  system as an input, and it is quantised to microseconds rather than to frames.
- `render_preview_raw` returns `tauri::ipc::Response::new(rgba)` because JSON-array bytes
  "was the real cause of choppy previews" (`:308-349`).
- `export_project_cmd` is single-flight, **drops the preview renderer during export so only
  one wgpu device exists** ("two concurrent wgpu devices can crash some drivers"),
  resolves relative output dirs, throttles `export-frame` events to 90 ms at ≤480 wide, and
  wraps the work in `catch_unwind` (`:154-275`).
- `run()` takes a leaked `CreateMutexW("Local\\dev.flux.desktop.single")` single-instance
  lock, closes the monitor window before the main window, and on `RunEvent::Exit` shells out
  to `taskkill /PID <self> /T /F` (`:404-468, 475-505`).
- `open_monitor` spawns `monitor.html` at 960×540 (`:32-47`).

The IPC envelope is `Request { protocolVersion, requestId, projectId, expectedRevision,
command }` (`app/src/commands.rs:14-21`), `protocolVersion: 1` and
`expectedRevision ?? 0` filled in by `ipc.js`. `GuiCommand` has ~28 variants
(`app/src/commands.rs:29-133`) wrapping domain commands. The module doc declares this file
"the anti-divergence boundary — the GUI cannot execute anything that does not go through
here" (`app/src/commands.rs:3-4`), and that holds: no `Dispatcher::apply` call exists in
`apps/desktop`.

### 5.3 OSC / MIDI / Link addressing

OSC (`crates/integrations/src/osc.rs`): `DEFAULT_RECV_PORT 9000`,
`DEFAULT_FEEDBACK_PORT 9001`, `MAX_BUNDLE_DEPTH 8`, `MAX_PAYLOAD 65536`, `MAX_RATE 1000`
(`:12-16`). Address scheme (`:38-87`):

```
/flux/v1/project/{id}/node/{node}/param/{param}      float, clamped to 0..1
/flux/v1/project/{id}/node/{node}/trigger/...
/flux/v1/project/{id}/transport/play|pause|seek      seek as "{:.6}" s, denom 1_000_000
/flux/v1/project/{id}/preset/recall
```
`/flux/v1/bridge/{instance}/{hello|features|transport|heartbeat}` is a separate namespace
(`integrations/src/bridge.rs:44, 72`) carrying the 8 feature floats in the order
RMS, peak, bass, mid, treble, onsetStrength, onsetFlag, confidence — "The bridge never sends
raw PCM" (`bridge.rs:3`).

Receiver is loopback-only: `UdpSocket::bind(("127.0.0.1", port))`, 50 ms read timeout,
bounded channel with a drop counter (`osc.rs:156-211`); feedback encoding
(`osc.rs:214-225`); application is latest-wins coalesced (`osc.rs:236-248`).
Verified dead constant: `decode_packet` returns `None` for bundles (`osc.rs:112`), so
`MAX_BUNDLE_DEPTH` is never consulted anywhere in the tree.

Mapping into the document is `crates/app/src/control.rs`: `ControlBinding` =
`Osc{address,node_id,param_id}` | `MidiCc{channel,cc,..}` | `MidiNote{channel,note,..}`,
`ControlMap{graph_id, bindings}` (`:25-52`). `apply_osc` passes an unbound address through
when the map is **empty** (`:98`, through `is_allowed` at `:116`) — "no bindings learned yet"
means "everything is live", which is a deliberate but easy-to-trip-over rule; a table with one
entry restricts the surface to exactly what was learned. `apply_midi` requires an exact
binding (`:126`). `apply_to_dispatcher` calls `dispatcher.apply(cmd, None)`
(`:160`): **live control edits deliberately bypass `expected_revision`**, and each one
is its own undo transaction.

Two things flush, and they are not the same thing. The `Coalescer` (`:193`) is the batch path.
`OscBridge` (`:302`) is the one the desktop process has run since 2026-11-15 (D022): an
`OscReceiver` socket thread, a bounded channel, and `run_listener` (`:374`) holding a
latest-wins map it drains on a 16 ms window — including when the window merely *expires*,
which the first version did not do and which meant a single fader move queued and never
landed. Both limit one parameter to one edit per flush, which is what keeps a 60 Hz stream
from filling the 200-deep undo ring in seconds.

MIDI (`crates/integrations/src/midi.rs`) uses `midir`; note messages are normalised
(`value = byte/127.0`, bend `(raw/8191.5)-1` — `:66-93`), real-time clock/transport are
separate variants (`:53-59`), and virtual ports are explicitly not available on Windows
(`midi.rs:3`); `list_inputs()` returns empty when there is no backend (`:104-112`).

Ableton Link (`crates/integrations/src/link.rs`) is a thin FFI wrapper over the vendored
C library (`LinkState { tempo, ... }` `:50-56`, `set_tempo` `:117`);
`crates/integrations/build.rs` is a **no-op unless `CARGO_FEATURE_LINK` is set** and
otherwise panics with cmake instructions unless `FLUX_ABLETON_LINK_LIB` points at a build,
linking `static abl_link` plus winmm/Avrt/ws2_32/iphlpapi/bcrypt/userenv
(`crates/integrations/build.rs:1-36`). Link is GPLv2+, which `capability_report()` records
as a redistribution constraint.

Feature gating, verified in `crates/integrations/Cargo.toml`: `default = []`, optional
`link`/`ndi`/`spout`/`syphon`, `vision = ["dep:ort"]` with `ort = "=2.0.0-rc.10"`
(`download-binaries`, `copy-dylibs`). Only `vision` is module-gated in `lib.rs`; `link.rs`,
`ndi.rs`, `ndi_io.rs`, `spout.rs` self-gate with `#![cfg(feature = ...)]`. **The shipping
build therefore has OSC + MIDI + bridge only.** `capability_report()`
(`crates/integrations/src/lib.rs`) answers per integration with
`{available, implemented, verified, limitation}` and is deliberately honest about negatives.

### 5.4 NDI / Spout shims

Two different NDI mechanisms exist; do not conflate them.
- `integrations/src/ndi.rs` (259 lines) runtime-loads the official
  `Processing.NDI.Lib.x64.dll` with six discovery entry points and two hand-declared
  structs, explicitly keeping live frames out of scope.
- `integrations/src/ndi_io.rs` (270 lines) runtime-loads **`ndi_shim.dll`**, built from
  `apps/ndi-bridge/ndi_shim.cpp` against the real SDK headers, exposing a flat `extern "C"`
  ABI (`ndi_shim_recv_create/recv_frame/send_create/send_frame`, see
  `apps/ndi-bridge/README.md`); this is the path that actually moves RGBA frames, and the
  README records a verified cross-process `ndi-send` → `ndi-recv` frame. The rationale is
  "correct by construction — Rust never guesses the ABI".
`apps/ndi-bridge` is not a cargo member and is built by `build_shim.bat` (dumpbin + lib /def
+ cl); the NDI runtime DLL is committed next to the shim.

Spout (`integrations/src/spout.rs`, 358 lines) runtime-loads `spout_shim.dll` from
`FLUX_SPOUT_SHIM`, the cwd, or `apps/spout-bridge/`, and prepends the shim directory to
`PATH` so `SpoutLibrary.dll` resolves; the shim wraps the SpoutLibrary C++ vtable.
Documented limitations (`apps/spout-bridge/README.md`): sender enumeration works but
`GetSenderCount` can block for tens of seconds, and `SendImage` needs a bound GL context and
returns 0 rather than faking success — matching `capability_report`'s "offscreen-GL
SendImage fails on this host".

`integrations/src/live.rs` is the transport-agnostic contract (`LiveSourceInfo`,
`LiveSource`, `:15-30`): pull-latest RGBA8, non-blocking consumer, never GPU/file work on an
audio thread; a missing SDK reports `unavailable` with a reason.

### 5.5 Max for Live (`apps/max-for-live`)

Two source patches, `flux-audio-bridge.maxpat` and `flux-midi-bridge.maxpat`, plus
`bridge-osc.js` (the `node.script` the patches already instantiate) and `check-maxpat.cjs`
(a conformance test over the patch JSON). Audio path is `plugin~ → snapshot~ → node.script
→ udpsend` at 60 Hz into a low-priority UDP queue; features only, never PCM
(`apps/max-for-live/README.md`). The `.amxd` packaging step is genuinely manual — Max's save
dialog is the only writer of that format — and `capability_report` says so
("Max for Live host not installed"). Note the README's verification path still points at
`D:\Projects\skeer-audio\genfx\apps\max-for-live\*.amxd`, i.e. an older checkout location.

### 5.6 Vision

`crates/integrations/src/vision.rs` (277 lines, `#![cfg(feature="vision")]`): Depth Anything
v2 via ONNX/`ort`; input `pixel_values` f32[1,3,H,W] ImageNet-normalised, output
`predicted_depth` [1,H,W]; default size 518, overridable with `FLUX_DEPTH_SIZE`; model
directory from `FLUX_MODEL_DEPTH` else `FLUX_MODELS_DIR/depth` with `model.onnx` inside
(`vision.rs:8-9, 53-61, 118-121`; ImageNet normalisation into NCHW at `:150`). The
catalog side is `crates/catalog/src/models.rs`: conditional rows G170–G185 stay **editable
when the model is absent** and report `ModelState::{Available,Missing,Incompatible}` with an
actionable message (`models.rs:1-25`); only seven requirement ids are truly model-backed
(G170–G174, G176, G177 — `models.rs:43-52`), and `FLUX_MODEL_<ENV_KEY>` is the per-model
override that makes present/missing/incompatible testable without shipping weights. This
registry is the source for `flux models` / `flux depth` and
`app/src/diagnostics.rs`, which is redacted-by-default and never carries media or secrets
(`diagnostics.rs:1-5`).

### 5.7 Platform

`crates/platform/src/lib.rs:1-7` states the scope precisely, and the code backs it: on
Windows it enumerates WASAPI **audio** endpoints for microphones (`eCapture`) and loopback
(`eRender`) via `EnumAudioEndpoints` (`:54, 79`), **and** enumerates cameras by reading the
PnP imaging device class registry key
`{e5323777-f976-4f5b-9b55-b94699c46e44}` (`:112-161`); `enumerate()` sets
`camera_available = !cameras.is_empty()` (`:163-183`). Live frame capture is not
implemented: the Windows limitation string says "WASAPI endpoints + PnP camera class
enumerated for real; live frame capture needs a device and OS permission" (`:179-181`) and
non-Windows returns `camera_available: false` with "capture adapters are implemented for
Windows x64 only" (`:40-45`). `flux caps` / `flux capture` surface this.

---

## 6. Testing

**Where tests live.** Two layers.
1. `#[cfg(test)] mod tests` inside each crate: `crates/domain` (time/document/commands/
   params), `crates/engine` (notably the graph tests at `engine/src/graph.rs:560-698`),
   `crates/audio/src/tests.rs` (217 lines), `crates/media/src/tests.rs` (166),
   `crates/catalog/src/tests.rs` (231 — the catalog linter, §3.6 step 10) plus
   `catalog/src/presets.rs:801-878`, `crates/integrations/src/tests.rs` (63 — **real UDP
   loopback** on hardcoded ports 19001/19002, `tests.rs:9,33`), `crates/app/src/control.rs`
   (`:224-240`).
2. `crates/tests` (member `flux-tests`, `crates/tests/Cargo.toml`): 13 GPU/export
   integration binaries in `crates/tests/tests/` — `dag_parallel` (264), `node_editor` (648),
   `sweep_catalog` (228), `producer_tests` (179), `export_formats` (147), `golden` (137),
   `export_matrix` (126), `gui_backend` (110), `preview_topmost` (94), `preset_sweep` (89),
   `preview_stress` (57), `preview_render` (42), `preview_perf` (42) — sharing helpers in
   `crates/tests/src/lib.rs`.

**How GPU tests are obtained.** There is no mock device anywhere. `crates/tests/src/lib.rs`
hands out a real adapter with `Gpu::new(BackendPref::Auto).expect("GPU required")`
(`lib.rs:14-16`) and renders at 32²–128² (`:72, 206, 242, 301`), asserting metamorphic
properties instead of golden images: non-degenerate output
(`dense_generators_produce_structure`, `:218-253`), effects change the frame they are added
to (`:265-315`), byte-identical pixels at a fixed time
(`render_is_deterministic_at_fixed_time`, `:317-325`), different pixels at different times
(`:327-335`), and a `gen.solid` colour oracle (`:337-349`). The passthrough filter
`is_passthrough()` (`:257-262`) exists so presentation-only rows are not counted as
analytic generators.

Two different no-GPU behaviours coexist, and this is the sharpest gotcha in the suite:
- `crates/tests/**` **skips**: every GPU test matches `Err(_) => { eprintln!("no GPU;
  skipping"); return; }` (e.g. `dag_parallel.rs:72-78`, `preview_render.rs:12-16`,
  `sweep_catalog.rs:167`), so the suite goes green on a headless box while having tested
  nothing.
- `crates/render/src/tests.rs:13-15` **panics** instead: `.expect("GPU available")`, and
  its headline test compiles all 217 catalog shaders on the device (`:17-33`). So
  `cargo test -p flux-render` fails without a GPU while `cargo test -p flux-tests` does not.
- `cargo xtask test-gpu` neither skips nor panics — it requires a device and reports
  `UNAVAILABLE` in `implementation/evidence/` (`crates/xtask/src/main.rs:539-585`).
- `crates/media/src/tests.rs` and `crates/tests/tests/golden.rs` require **FFmpeg**
  (`find_ffmpeg().expect("ffmpeg available")`, `media/src/tests.rs:16`; but skip in
  `export_formats.rs:98,113`).

**What is serialised and what is not.** The documented reason is in
`crates/tests/tests/dag_parallel.rs:10-17`: "GPU device creation is not safe concurrently
from multiple test threads (it can deadlock)" — a process-wide `OnceLock<Mutex<()>>`
`gpu_lock()` held by each test. It exists **only** in `dag_parallel.rs` and
`node_editor.rs:15-19`, and because each file in `tests/` is its own test binary the lock
never crosses files. `crates/tests/src/lib.rs` (the shared harness) has no lock at all, so
its tests race each other on the same binary. The remaining GPU files carry no lock and are
serialised **by convention in their doc comments**:
`cargo test -p flux-tests --test sweep_catalog -- --nocapture --test-threads=1`
(`sweep_catalog.rs:4-5`) and the same for `preset_sweep` (`preset_sweep.rs:4`).
`.cargo/config.toml` contains only the `xtask` alias (3 lines) — there is **no**
`RUST_TEST_THREADS`/`--test-threads` default anywhere, so a plain `cargo test` runs
everything multi-threaded. **[inferred]** This is why `crates/render/src/tests.rs` can hang
rather than fail on some hosts.

Not serialised but stateful: `golden.rs` writes into `fixtures/generated/` and deletes
subdirectories before each run (`:10-14, 43-45, 88, 116`), and `gui_backend.rs` uses
`std::env::temp_dir().join(format!("flux-gui-export-{}", process::id()))` (`:87`) — so
`golden.rs` tests collide with each other if run in parallel, while `gui_backend` is only
safe across processes.

**Evidence harness.** `crates/xtask` is the only thing that writes machine evidence.
`cargo xtask <doctor|fixtures|catalog|check|test-gpu|scenario|benchmark|report|package>`
(`crates/xtask/src/main.rs:18-28, 30-49`) emits JSON into `implementation/evidence/`:
`doctor` records rustc/GPU/FFmpeg/encoders/`catalogEntries` **plus a hardcoded `blockers`
list** (`:348-392`), `fixtures` generates `fixtures/generated/{test_10s_60fps.mp4,
test_2997.mp4, sine_1k_48k.wav}` via lavfi (`:393-459`), `catalog` lints duplicate ids and
empty `implementation.path` (`:460-497`), `check` runs `cargo test -p` per package with a
`--scope` filter (`:498-538`), `benchmark` takes a named profile from a `BENCH_PROFILES` table
(`:614-650`) — `ordinary` (1920×1080, 16.67 ms budget) or `4k` (3840×2160, 33.33 ms) — and times
an 8-node merging generator→six-effect→output graph over 15 warm-up plus 120 sampled frames,
reporting render and preview-copy separately with nearest-rank median/p95/p99, then sweeps 30
(10 at 4K) single-generator graphs at 10 samples each with median/max only (`:806-962`); an
unknown profile exits 2 rather than benching the default, and it exits 1 when the chain tail
breaches the budget. `package` builds `flux-app --no-default-features` expecting
`target/release/flux.exe` (`:1397-1524`). Two more
bins exist: `requirements` (regenerates `implementation/requirements.json` from
`spec/SPEC-INDEX.json` + the live catalog; `--check` compares instead of writing, which is the form
both harnesses run) and `export_shaders` (dumps `apps/shader-harness/shaders.json` and
Naga-validates every shader with the same validator the native renderer uses).

---

## 7. Where things actually are

| Concern | File | Anchor |
|---|---|---|
| Exact rational time, frame/sample conversion, wire format | `crates/domain/src/time.rs` | `10-13, 45-50, 131-163, 217-254` |
| Document model, schema 2, limits, defaults, port types | `crates/domain/src/document.rs` | `11, 13, 22-25, 43-47, 60-75, 126-173` |
| Commands, undo/redo, revision, all guards | `crates/domain/src/commands.rs` | `19-120, 142-256, 642-692, 793-804` |
| Parameter evaluation order, LFO/mappings/smoothing | `crates/domain/src/params.rs` | `3-8, 16-50, 78-223` |
| `ErrorCode` vocabulary (26 variants) | `crates/domain/src/error.rs` | `6-34` |
| Graph validation + Kahn compile | `crates/engine/src/graph.rs` | `17-19, 22-49, 102-123, 382-436` |
| Timeline → per-frame render items, source mapping | `crates/engine/src/timeline.rs` | `43-120, 186-230` |
| 96-byte frame ABI, frame stepping | `crates/engine/src/frame.rs` | `12-23, 92-178` |
| Runtime parameter resolution (uncached catalog rebuild) | `crates/engine/src/router.rs` | `11, 41, 76, 113-171` |
| Catalog aggregation + utility nodes | `crates/catalog/src/utility.rs` | `7-181, 240-429` |
| Row builders, `wet`-to-slot-0 rule | `crates/catalog/src/builders.rs` | `191-245` |
| Manifest/ParamDecl/Implementation schema | `crates/catalog/src/manifest.rs` | `81-133, 143-191` |
| WGSL slot packing (two vec4 arrays) | `crates/catalog/src/pack.rs` | `12, 51-53, 73-199` |
| The one shared shader template | `crates/catalog/src/shader_gen.rs` | `60-188, 197-213` |
| Shared WGSL preamble fragments | `crates/catalog/src/wgsl_common.rs` | `6, 24, 127, 155, 191` |
| Preset graph recipes (274 of them) | `crates/catalog/src/presets.rs` | `52-158, 281-307, 309-504` |
| Conditional/model-backed node status | `crates/catalog/src/models.rs` | `1-58, 75` |
| DAG execution, per-node targets, binding priority | `crates/render/src/graph_exec.rs` | `24-28, 54-123, 291-465, 475-499` |
| Pipelines, Naga validation, node param buffers | `crates/render/src/pipeline.rs` | `41-110, 135-149, 180-229, 338-374, 396-416` |
| Preview readback + 960×540 constants | `crates/render/src/preview.rs` | `7-18, 23-232, 236-299` |
| Device creation, row padding | `crates/render/src/gpu.rs` | `10-20, 66, 211-215` |
| Export orchestration, frame loop, runtime cache | `crates/app/src/export.rs` | `48-198, 230-317, 319-610` |
| GUI command boundary | `crates/app/src/commands.rs` | `3-4, 14-174, 187-233, 732` |
| OSC/MIDI learn bindings + coalescing | `crates/app/src/control.rs` | `11-40, 86-193` |
| CPU producers (96 type ids, 7 families) | `crates/app/src/producer.rs` | `11-19, 43-131, 784` |
| Atomic save, autosave retention, schema refusal | `crates/app/src/persistence.rs` | `19-53, 63-86` |
| CLI subcommands | `crates/app/src/main.rs` | `30-141` |
| FFmpeg worker, streaming encode, mux | `crates/media/src/lib.rs` | `17-41, 474-541, 548-600, 623-631` |
| Audio analysis constants and snapshot layout | `crates/audio/src/lib.rs` | `14-49, 104-127, 229-326` |
| OSC addressing / UDP receiver | `crates/integrations/src/osc.rs` | `12-16, 38-112, 156-248` |
| MIDI parsing (midir) | `crates/integrations/src/midi.rs` | `46-93, 104-112` |
| Ableton Link FFI + build gating | `crates/integrations/src/link.rs`, `crates/integrations/build.rs` | `30-119`, `1-36` |
| NDI discovery vs NDI live I/O | `crates/integrations/src/ndi.rs` / `ndi_io.rs` | shim ABI in `apps/ndi-bridge/ndi_shim.cpp` |
| Spout shim loading | `crates/integrations/src/spout.rs` | `apps/spout-bridge/spout_shim.cpp` |
| Max-for-Live bridge protocol | `crates/integrations/src/bridge.rs` | `3, 44, 72, 181-203` |
| Depth Anything v2 ONNX | `crates/integrations/src/vision.rs` | feature `vision` |
| WASAPI endpoints + PnP camera enumeration | `crates/platform/src/lib.rs` | `1-7, 40-45, 54-79, 112-183` |
| GPU integration harness (metamorphic, skipping) | `crates/tests/src/lib.rs` | `14-16, 63-111, 218-349` |
| GPU test serialisation lock (only two files) | `crates/tests/tests/dag_parallel.rs`, `node_editor.rs` | `10-17`, `15-19` |
| Full-catalog GPU shader compile (panics without GPU) | `crates/render/src/tests.rs` | `13-33` |
| Evidence JSON writer | `crates/xtask/src/main.rs` | `18-100, 129-347` |
| Requirements coverage generator + the CI command-carrier gate (`--test-threads=1`, I15 item 0) | `crates/xtask/src/bin/requirements.rs` | `17-32, 821-978` |
| Tauri IPC surface (10 commands) | `apps/desktop/src-tauri/src/lib.rs` | `16-24, 32-47, 78, 95-98, 154-275, 308-349, 430-441` |
| UI preview sizing (the effective cap) | `apps/desktop/ui/main.js` | `185-196, 198-215` |
| UI bundling, cache-busting and bundle-freshness comparison | `apps/desktop/ui/bundle.mjs` | `1-5, 8-48, 57-82, 156-234` |

---

## 8. Corrections to the prior survey, and the divergence register

Claims from the earlier survey that are **wrong as stated** (code is right):

1. **"216 catalog entries" → 217.** `build_catalog()` pushes 28+24+28+26+19+83+9 = 217
   entries (`utility.rs:252-427`); `catalog_row_count()` is 208 because the 9 utility rows
   carry `R007` (`catalog/src/lib.rs:28-34`).
2. **"Cycles are legal only through feedback reads" → only half true.** Kahn is real and
   does skip edges into `flux.feedback_read` (`graph.rs:399-436`), but
   `is_feedback_boundary()` hard-returns `false` (`graph.rs:358-361`),
   `is_feedback_write/read` compare **type-id string literals** rather than the registry
   (`graph.rs:349-356`), `cycle_has_feedback` demands **both** a read and a write inside the
   cycle (`graph.rs:363-379`), and `CompiledGraph.temporal_pairs` is always `vec![]`
   (`graph.rs:487`). Since `flux.feedback_read` has no input ports at all
   (`utility.rs:176`), a genuine read→…→write→read loop trips the fatal `GraphTypeMismatch`
   edge check before the cycle logic helps. The one test around this only asserts the
   *absence* of `GraphCycle` (`graph.rs:648-698`). The feedback story is designed, not
   implemented.
3. **"`NodeParams` packs 16-byte slots in manifest order" → incomplete.** Two independent
   vec4 arrays (floats, then uints), `Text` params omitted, counts floored to 1, and `wet`
   force-moved to index 0 (`pack.rs:73-199`, `builders.rs:206-221`).
4. **"Integrations = Link FFI, NDI/Spout shims, ONNX vision, MIDI, OSC" → imprecise.** All
   of `link/ndi/spout/syphon/vision` are **off by default**
   (`crates/integrations/Cargo.toml`), so the shipping binary is OSC + MIDI + bridge;
   NDI additionally has a second, discovery-only DLL loader (`ndi.rs` vs `ndi_io.rs`).
5. **"gui/editor exits 2, ~25 subcommands" → 27 names / 26 arms**, and no arguments
   defaults to `gui` (`main.rs:30`) which also exits 2 (`main.rs:133-136`).
6. **"Platform = WASAPI/camera device enumeration" → refine, not correct.** WASAPI audio
   endpoints and PnP camera-class enumeration are both real
   (`platform/src/lib.rs:54-79, 112-161`); camera *capture* is not implemented.
7. **The 960×540 preview cap is not enforced by Rust in production.** `read_preview` has no
   production caller (`preview.rs:23-232`; callers are `render/src/tests.rs:164`,
   `xtask/src/main.rs:774` and `:1134`); the effective limit is `main.js:185-196`.

Divergences between code and the markdown, ordered by how much they will mislead:

| Doc claim | Reality in code |
|---|---|
| `README.md:22` "graph execution with ping-pong targets" — **corrected 2026-11-15**, it now reads "one render target per node"; `graph_exec.rs:2` module doc still says "ping-ponged" | True DAG, one texture per node (`graph_exec.rs:24-28, 327-330`); `DECISIONS.md:116` already records the migration |
| `README.md:15` "216 entries" — **corrected**, it prints **217**; the stale six are now only in `implementation/FINAL-REPORT.md` (`:71, :77, :81, :100, :102, :278`, flagged by its own corrections box) | 217 |
| `spec/ARCHITECTURE.md:66` native 960×540 staging ring, binary channel, never base64 | No ring, no reuse, per-frame bind groups (`graph_exec.rs:391-443`); base64 PNG still returned (`src-tauri/src/lib.rs:95-100, 396-401`) |
| `spec/ARCHITECTURE.md:199` texture pool keyed by size/format/usage, ping-pong for blur/fluid/feedback | No pool; targets are allocated per activation and keyed only on graph shape (`graph_exec.rs:82-91`) |
| `implementation/COMPLETION_PLAN.md:95` "ping-pong execution" | Same as row 1 |
| `main.rs:2-9` (7 commands) and `main.rs:140` (18 commands) | 27 subcommand names (`main.rs:31-133`) |
| `apps/max-for-live/README.md` verification path `D:\Projects\skeer-audio\genfx\...` | **Fixed 2026-11-14** — patch `bootpath` and README now use `D:/Projects/visual/genfx`; `check-maxpat.cjs` PASSes |
| `implementation/STATE.json` (Nov 9) "CI workflow + verify.ps1 (VERIFY: PASS)" | **`VERIFY: FAIL` at `e84fe67`, measured 2026-11-14.** `cargo fmt --all -- --check` (`ci.yml:53`, `verify.ps1:32`) exited 1 with 56 diffs in 12 files; every other executed step passed. That fmt step has since been cleared by `cargo fmt --all`, so the failure is no longer reproducible — the two harness defects are: `verify.ps1:35` records only the last of three `;`-chained `cargo test` exit codes, and skipped Link/Spout gates are stored as `ok: true`. See DECISIONS D019 and AGENTS.md "Traps inside this repo's own verification harness" |
| `graph.rs:17-19` implying all three limits bind | `MAX_NESTING` is never enforced |
| `commands.rs:679-692` naming a `validate_clip` guard | Stub; asset existence is not checked on edit |
| `Track::blend_mode` (`document.rs:413-414`) + 9-variant `BlendMode` (`document.rs:381-391`) | Read nowhere in the engine or renderer; the real blend vocabulary is the 13-mode `flux.blend` enum (`utility.rs:70-89`) |
| `engine/src/lib.rs:17` re-exporting `router::pack_params` | Second, divergent packing implementation with **no production caller**; it mis-packs `Color` as `[v,0,0,1]` (`router.rs:156`). `pack.rs`/`pipeline.rs` is the live path |
| `osc.rs:15` `MAX_BUNDLE_DEPTH` | Unused; bundles are dropped at `osc.rs:112` |
| `pipeline.rs:369-374` stored bind group layout | Created with `entries: &[]` and never used; the real layouts are built per pass in `graph_exec.rs:391-443` |

Things I could not verify from code and deliberately did not assert: performance
characteristics (§4.5 is labelled inferred), any behaviour of the feature-gated
Link/NDI/Spout/Syphon/vision paths on this host (they are not compiled by default), and
whether the `sweep_catalog` classification currently reports any node as blank — that
output lives in `implementation/evidence/` and is a build artefact, not source.
