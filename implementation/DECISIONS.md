# Decisions (short records)

## D001 — GUI architecture: Tauri 2 + React over the Rust command surface
Revised decision (supersedes the initial native-egui attempt). The native egui editor
was abandoned because egui 0.37 pins `wgpu 30` while the verified renderer is pinned to
`wgpu 27`; mixing both aborts on a windows-crate conflict in wgpu-hal D3D12, and
upgrading the renderer to wgpu 30 is a large rework that would invalidate all GPU
evidence. **Decision: adopt the spec's Tauri 2 + React shell**, keeping Rust as the
single authority for project state, validation, rendering, media, export and file I/O.
The durable, testable part is the typed command/event surface (engine `Dispatcher`,
`Command`, `Project`), which is already implemented and covered by tests; the GUI is a
thin consumer of it. GUI implementation is staged after the producer contract and CI
are stable (Workstream C). Rationale: avoids a divergent second rendering engine and
the wgpu version conflict, and matches the product contract.

## D002 — Single wgpu renderer, one authoritative texture
One wgpu device/queue; graph evaluated once per frame; the resulting texture is copied
to (a) editor preview (RGBA readback, downscaled), (b) headless export readback,
(c) external outputs when available. This matches ARCHITECTURE 2/4/8. No browser
simulation runs the graph.

## D003 — Fragment effect template + shared WGSL preamble
Every FX/G row is a `process(uv, src) -> vec4` body. The generated shader supplies the
96-byte FrameUniform, manifest-packed NodeParams, AudioFeatures, and fixed sampler
bindings; final mix applies wet*mask in premultiplied linear. Uniform layout is generated
by `catalog::pack` from the same manifest that drives the UI, docs and tests.

## D004 — FFmpeg via CLI worker (process isolation)
ffmpeg-next links FFmpeg 9.0 shared libs; because the FFmpeg ABI tracks only its own
major version and the dev headers are the winget shared build, media is implemented
through a ffmpeg/ffprobe **process** worker (bounded queues, deterministic frame PTS
selection, cancel). This keeps media failures out of the render thread and avoids
ABI mismatch crashes; it is a documented deviation from `ffmpeg-next` in ARCHITECTURE 1.
Rationale recorded against R004/R024.

## D005 — Native fonts
Text rendering uses a bundled block font defined in code (`catalog::font5x7`), so text
never depends on system fonts and is byte-stable across hosts. A system font registry
hook is kept for future imports.

## D006 — Simulation workers are CPU fixed-step
Particles/physics/fluid/CA/reaction-diffusion run on deterministic CPU workers with
fixed dt from project `simulation_step`, then upload state textures. This is the same
evaluator path for preview and export (no second GPU engine).

## D007 — Generator producer contract
Generator rows that do not compute pixels directly in WGSL are *producers*: they compute
RGBA on the CPU (or a worker) and upload a texture that the presentation pass shows.
One interface (`flux_app::producer::Producer`) defines inputs (manifest id, resolved
native params, width×height px, local time seconds, seed), output (tightly packed RGBA8,
premultiplied, top-left origin, exactly w*h*4 bytes), determinism (identical inputs →
identical bytes, no wall clock), and ownership (caller owns the buffer; bounded to the
target size). The renderer accepts per-node `producer` textures via
`RuntimeGraph::execute_with_producers`, so the same producer implementation drives
preview and export with only resolution/quality differing. Reference producers:
typography (G070–G079, bundled block font), vector (G030–G045, analytic rasterizer),
SDF/mesh (G080–G107, CPU raymarch), particle/physics/field (G110–G148), control
(G150–G162).

## D008 — GUI command surface, persistence, export matrix, recording
The GUI is a thin consumer of `flux_app::commands::CommandHost`, which validates and
executes every request against the engine `Dispatcher` (revision-checked, typed
`GuiCommand`/`Request`/`Response`). The Tauri shell exposes `dispatch`, `render_preview`,
`export_project_cmd`, `capabilities`, `produce_generator_frame`; the webview never runs
its own engine. Persistence is `flux_app::persistence` (atomic save+backup, autosave with
retention/pruning, recovery manifest, relink search, collect-media). Export is a matrix
(PNG/JPEG sequence, WAV, H.264/HEVC MP4, ProRes MOV, GIF) with a frozen-snapshot job +
persisted queue (`job.rs`). Recording (`record.rs`) renders the deterministic exporter at
wall-clock budget and reports overrun honestly.

## D009 — Real where buildable, blocked where external
OSC and MIDI are implemented from crates (`rosc`, `midir`) and verified with real UDP
loopback and message parsing. Windows device enumeration uses the `windows` crate (WASAPI
COM endpoints + PnP camera class) and is verified on-host. NDI, Spout, Syphon, Ableton
Link, and Max for Live require SDKs/hosts/licenses that are absent; their adapters are
capability records marked `blocked_external`, never faked as working.

## D010 — Upstream audit (completion plan §2A)
Shallow clones audited on 2026-11-09 (recorded in `implementation/UPSTREAM.md`):

| Repo | URL | Commit | License | Decision |
|------|-----|--------|---------|----------|
| Ableton link | https://github.com/Ableton/link | `5ba907ae` | **GPLv2-or-later** | Build `abl_link` C-API from source, wrap behind `link` feature. GPLv2 is a **redistribution constraint**: shipping Flux with Link requires Flux to be GPL-compatible or Link to be a separately-loaded plugin. Local build+verify is fine. |
| Spout2 | https://github.com/leadedge/Spout2 | `6b68a5b4` | **BSD-2-Clause** | Permissive; wrap Spout2 shared-texture API behind `spout` feature on Windows. Needs Spout2 build + a DX11 device; not available in this headless run. |
| obs-ndi | https://github.com/Palakis/obs-ndi | `fa41b38f` | GPLv2 | Reference **only** for NDI lifecycle/timestamps; still needs official NDI SDK/runtime (separate license + redistribution terms). Not vendored. |
| Syphon | https://github.com/Syphon/Syphon-Framework | `f4761677` | BSD-2-Clause | macOS-only; wrap behind `syphon` feature; cannot build/verify on Windows. |

Only **Ableton Link** is buildable and verifiable on this host without a proprietary
SDK, so it is the first adapter implemented end-to-end (DECISIONS D011).

## D011 — Ableton Link adapter (thin FFI)
`flux-integrations::link` wraps the `abl_link.h` C API with raw `extern "C"` FFI and no
extra crate. Build path: clone `Ableton/link`, `cmake -G "Visual Studio 18 2026" -A x64`,
`cmake --build --target abl_link`, then link `abl_link.lib`. The Rust side links the lib
via a `build.rs` that reads `FLUX_ABLETON_LINK_LIB` (dir) and is only compiled with
`--features link`. When the feature is off or the runtime is absent, `protocol_status`
reports `unavailable` truthfully. Transport stays separate from the internal/MIDI
transport and can be disabled without affecting offline rendering.

## D012 — Ableton Link licensing resolved (product owner decision)
The product owner has stated Flux will not be sold or distributed commercially. Ableton
Link is GPLv2-or-later; under this decision the GPL terms are acceptable for the project's
use, so **the redistribution constraint recorded in D010 is lifted** for this project.
Link is therefore treated as a **supported integration**, not an experimental/optional one:
it is built and verified on this host (two-process peer discovery + tempo/beat/phase), and
its capability is reported as `available=true, implemented=true, verified=true` when the
`link` feature is compiled in. The feature stays behind the flag so SDK-less machines still
build cleanly, but enabling it is the intended configuration for a Flux build that offers
live tempo sync. `flux link` is documented in the user guide.

## D013 — Node-graph compositor (replaces the clip timeline); true DAG executor
The editor is now a node graph (TouchDesigner / Max style): generators and effects are
nodes with typed input/output ports, wired arbitrarily (series AND parallel). The frontend
draws an SVG node canvas; Rust stays authoritative (typed commands AddNode / Connect /
Disconnect / RemoveNode / MoveNode). The GPU executor was rewritten from a linear
two-target ping-pong into a **true DAG executor**: each non-terminal node renders into its
own texture and each input binds to the source node's texture by edge. A second image
binding (`aux_tex`, group 1 / binding 3) lets two-input nodes (`fx.composite`, `flux.blend`)
merge parallel branches (both were previously single-input stubs). `render_preview` renders
the active graph via `PreviewRenderer::render_graph`.

Two correctness bugs found during this work and fixed: (1) **non-float params** (colour /
enum / bool / vec) were never packed into uniforms from node params — only floats applied,
which is why "clicking a generator/effect did nothing"; now all kinds pack correctly.
(2) the runtime cache was keyed by graph **id**, so edits could render a stale/blank frame;
it is now keyed by a **content hash** of the graph.





## D014 — Desktop shell is vanilla ES modules + esbuild, not React (corrects D001)
D001 recorded "Tauri 2 + React". What actually shipped is Tauri 2 over hand-written ES
modules in `apps/desktop/ui/` (no framework, no virtual DOM), bundled by
`ui/bundle.mjs` with esbuild and served from disk (`bundle.active: false`). Rust remains
the only authority; the shell is a thin `dispatch` client. **Decision: keep the vanilla
shell** — reactivating D001 would mean rebuilding the canvas, inspector and monitor for a
framework that buys nothing here. Update D001 if React is ever actually adopted.

## D015 — Bundled webview, content-hashed cache-bust, single instance (`fe9e35e`)
The UI bundle is embedded at build time; `bundle.mjs` sha256s the bundle and rewrites the
`?v=` on the script tag so WebView2 cannot serve a stale inspector. A raw
`CreateMutexW("Local\dev.flux.desktop.single")` enforces one instance, the monitor window
is closed on the main window's `CloseRequested`, all webviews are closed on
`ExitRequested`, and `taskkill /PID /T /F` reaps the WebView2 tree on `Exit`.

**Defect found 2026-11-14, not yet fixed:** `src-tauri/build.rs:18-20` reports a failed
bundle step as `cargo:warning=ui bundle not rebuilt (... os error 267); using committed
bundle` and continues. The last build hit exactly that, so the cache-buster this decision
exists to provide silently did not run, and `bundle.mjs:37` only rewrites a tag that
already carries `?v=`. The bundle step must fail the build, and `bundle.mjs` /
`package.json` must be added to `rerun-if-changed`.

## D016 — Sliders, monitor window, catalog/producer refinements (`ac73572`)
Inspector range inputs commit through `setParam` (discrete) or `setParamLive`
(rAF-coalesced, used by canvas drags) into the same typed `dispatch` path; preview is
debounced 40 ms behind a single-flight guard. A second WebView2 window shows the live
frame. **Known cost:** `export.rs::graph_content_key` hashes node params, so every slider
tick re-enters `RuntimeGraph::activate` (recompile + one render-target allocation per
node) and the 12-entry `roots_cache` clears itself mid-drag.

## D017 — Max patch repoint (`46cfd94`) pointed at a tree that does not exist
The commit "Repoint the Max patch at the new tree" set `bootpath` to
`D:/Projects/skeer-audio/genfx/apps/max-for-live` and `apps/max-for-live/README.md:55`
to the same absent root — a third wrong location after `D:\CURSOR LOCAL PROJECTS\...`.
The repo is `D:\Projects\visual\genfx`. Both corrected 2026-11-14. Still true: these are
`.maxpat` patches, no `.amxd` has ever been produced, and the bridge addresses they send
are discarded by the receiver (see D018).

## D018 — 2026-11-14 audit: what the status docs were asserting that the code cannot do
A full read of the tree against HEAD `46cfd94` (host: RTX 2080 Ti, Vulkan, FFmpeg 9.0)
found the following. None of it is fixed yet; it is recorded so the evidence files stop
being read as proof.

1. **Feedback loops render black, silently.** `is_feedback_boundary` returns `false`
   unconditionally (`engine/src/graph.rs:358`) and `temporal_pairs` is always `vec![]`
   (`:487`), so no legal loop can be built; an unresolved `NodeOutput` falls through to
   `compiler.dummy_tex` (`render/src/graph_exec.rs:368`). D013's "true DAG executor"
   claim holds for acyclic graphs only.
2. **The capability report overstates three integrations.** `integrations/src/lib.rs`
   marks `osc`, `bridge` and NDI `implemented: true, verified: true`. Precisely:
   OSC receive (UDP 9000) and OSC→engine application (`GuiCommand::LiveOsc` →
   `control::apply_to_dispatcher`, `app/src/commands.rs:478`) **both exist and are not
   connected** — no code turns a received `OscControl` into a `LiveOsc` dispatch,
   `apply_controls`/`apply_osc` have no non-test callers, and the desktop process opens
   no UDP listener at all, so `flux osc` remains a print-only demo. There is no OSC
   output path (`encode_param_feedback` is test-only, 9001 unused). `LiveOsc` is also a
   no-op until a `LearnBinding` sets `control_map.graph_id`. Separately,
   `/flux/v1/bridge/…` is rejected by the address gate (`osc.rs:44`), and the NDI/Spout
   senders emit a synthetic 640×360 gradient rather than a rendered frame — behind
   features that are off by default.
   *Correction to the first pass of this audit, which claimed "control application is
   not wired": the application half is implemented; the gap is the transport→command
   wire. That makes it a wiring job, not a missing subsystem.*
3. **14 GPU tests pass vacuously** by returning on `Err` ("no GPU; skipping"), and
   `golden.rs` asserts file sizes, not pixels (`bytes.len() > 1000`). "216 catalog
   shaders verified" is a compile check plus a 1 % non-black threshold
   (`sweep_catalog.rs:152`), which also reclassifies `flux.feedback_read` Blank→Ok at
   `:184`.
4. **VRAM is unbounded and per-frame work is real.** One full-res target per live node
   with no pool or budget (4096 nodes = 32 GiB), bind groups and layouts rebuilt per pass
   per frame, while the eager `bind_groups` / `input_bind_groups` / `node_uniforms`
   caches are dead fields. `MAX_NESTING` is unreferenced and the "16 texture inputs"
   limit does not exist in code.
5. **Two packers disagree.** `engine/src/router.rs::pack_params` allocates a slot for
   `Text` params that `catalog/src/pack.rs` skips, and resolves Color/Vec to 0 because
   `ResolvedParams` is `BTreeMap<String,f64>`. The live export path uses `pack.rs`; the
   dead one would shift every later int/bool/enum by one.
6. **Media and undo defects.** `StreamingEncoder` has no `Drop`, so a failed export
   orphans ffmpeg writing an unindexed MP4; stderr is piped and never drained
   (`media/lib.rs:287`, `:511`) which can deadlock past 64 KB; `codec_from_name` and
   `parse_codec` disagree on `mov` and `sequence`; `undo`/`redo` re-enter `execute`
   without the snapshot/rollback that `apply` has, so a rejected inverse leaves the
   project half-mutated; autosave `.bak` files are never pruned.
7. **Catalog count is 217, not 216** (`flux list`, `flux doctor` both agree).
