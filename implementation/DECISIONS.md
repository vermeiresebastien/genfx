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

**Defect found 2026-11-14, not yet fixed:** `src-tauri/build.rs:20-28` reports a failed
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
   (`sweep_catalog.rs:158`), which also reclassifies `flux.feedback_read` Blank→Ok at
   `:193`.
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

## D019 — 2026-11-14 re-measurement: the tree does not pass its own CI

After D018 the docs were reconciled and the artifacts untracked (`5fbfc1e`, `e84fe67`).
The completion batch then re-ran every canonical command bare, at HEAD `e84fe67`, on the
same host. Measured, not reasoned:

| Gate | Result |
|---|---|
| `cargo test --workspace -- --test-threads=1` | 196 passed / 0 failed / 0 ignored, 40 binaries, exit 0 |
| `cargo run -q -p xtask -- catalog` | 217 entries, duplicates=0, exit 0 |
| `cargo run -q -p xtask -- doctor` | PASS, RTX 2080 Ti / Vulkan / DiscreteGpu, exit 0 |
| `cargo run -q -p xtask -- test-gpu --profile baseline` | 217 shaders compiled on the device, 0 failures, exit 0 |
| `cargo run -q -p xtask --bin export_shaders` | 217 WGSL, 0 Naga failures, exit 0 |
| `cargo run -q -p xtask -- fixtures` | 3 fixtures rebuilt, exit 0 |
| `cargo run -q -p xtask -- benchmark --profile ordinary` | 30 generators @ 1920x1080, median p95 4.98 ms, exit 0 |
| `node apps/max-for-live/check-maxpat.cjs` | structural PASS, exit 0 |
| `cargo build -p flux-desktop` | exit 0 |
| `cargo fmt --all -- --check` | **FAIL — exit 1, 56 diffs across 12 files** |
| `infrastructure/verify.ps1` (full) | **VERIFY: FAIL — exit 1**, fmt the only failing step of 15 |

Three conclusions follow, and they change how the earlier evidence should be read.

1. **The repository has been failing its own CI since 2026-11-10.** `Format check` is the
   first check step in `ci.yml` (`:29`). rustfmt objects to the compact style written by
   `fe9e35e`, `ac73572` and `8139007` — multi-item arrays like `BRIK_GALLERY_NAMES` left
   several strings per line, and long `assert!` calls left on one line. In Actions a failed
   step aborts the job, so the workspace suite and every xtask gate would never have run in
   CI. Locally they all pass; only the formatting is wrong. A single `cargo fmt --all` fixes
   it, but that is a formatting-only commit touching the 25-preset tables, so it is the
   user's call and was not made here.
   **Discharged in the commit that adds this note.** `cargo fmt --all` was run, and
   `cargo fmt --all -- --check` now exits 0. The reformat moved lines in 12 files, so every
   `file:line` citation into those files was re-audited against the new tree, not carried
   over. Proof that it changed no behaviour: `cargo test --workspace -- --test-threads=1`
   afterwards is `196 passed, 0 failed` across 40 binaries — the same totals as the
   pre-format run recorded in the table above. The two files where rustfmt did more than
   reflow whitespace and add trailing commas (`crates/catalog/src/presets.rs`,
   `crates/tests/tests/sweep_catalog.rs`) gained redundant `{ }` around multi-line closure
   bodies, which is a token change, and the green suite is what makes it safe to call
   inert.
2. **`verify.ps1` can record a passing step for failing work.** `Step` reads `$LASTEXITCODE`
   once, after the whole scriptblock. Step `'integration-tests'` (`:35`) chains three
   `cargo test` invocations with `;`, so its verdict is `dag_parallel`'s alone and a
   `producer_tests` or `export_matrix` regression is invisible in `verify.json`. Proved
   directly rather than argued: a block containing a failing `cargo fmt --all -- --check`
   followed by a passing `xtask doctor` records `ok=True exit=0`, while the same `cargo fmt`
   run alone exits 1. The same file also writes `{ ok = true; skipped = true }` for the
   Link and Spout gates when their env vars are unset — the exact "missing prerequisite is
   `blocked_external`, never `passed`" rule from AGENTS.md, violated inside the harness that
   is supposed to enforce it.
   **Both defects discharged in D020.** The chained step is now three steps; the skipped
   gates are `ok = null` + `status = 'blocked_external'`; repairing them exposed a second
   leak (`$LASTEXITCODE` carrying into a step that runs no command), also fixed.
3. **Some step names describe more than their tests prove.** `ndi-discovery` runs
   `cargo test -p flux-integrations --features ndi ndi::`, which selects 2 tests
   (`runtime_loads_and_initializes`, `discovery_returns_state_without_crash`); `ndi.rs`
   contains no others. The "5 live sources discovered + live send/receive cross-process"
   line in the old STATE.json came from a manual run that nothing reproduces. Meanwhile
   `test-gpu --profile baseline` genuinely does the work it claims — 217 shader compiles on
   the device, 0 failures — which is the strongest single piece of evidence in the tree, and
   the reason D018 item 3's "the green suite is weak" caveat needs the exception recorded
   here.
4. **The benchmark gate is weaker than its output looks** (`crates/xtask/src/main.rs:299-382`
   as it stood at `e84fe67`; that code no longer exists — the line numbers below are the
   historical ones, not a pointer into the current file).
   `--profile` is parsed and written into the JSON but never read again: resolution is
   hardcoded 1920×1080 (`:331`, `:355`) and the sample set is the first 30 fragment
   generators (`:324`), so no profile changes the work. Every timed iteration wraps a full
   1920×1080 `read_preview` (`:371`), so the numbers are render **plus** a GPU→CPU
   copy and are not a frame-budget measurement. And with 12 samples, `times[len*95/100]`
   is index 11 — the single worst sample — so the field labelled `p95ms` is really `max`.
   (The copy size stated here was first written as "~8 MB" and is wrong: `read_preview`
   downsamples to the 960×540 preview cap, ~2 MB. See D020.)
   **All three counts discharged in D021** — and with them the premise of the paragraph below,
   which is now superseded: the `gen.concentric` "outlier in twelve" was a preview readback
   artefact, not a node, and the multi-node DAG this table said nothing in the tree times is now
   the thing the gate measures.

Performance, for the record (superseded by D021, which re-measured this on a real graph): on the
ordinary label the benchmark put the median worst-sample
at 4.98 ms across 30 single generators at 1920×1080, with one row above a 16.6 ms budget
(`gen.concentric`, worst 21.49 ms, median 2.75 ms — one outlier in twelve, not a slow node).
Each row is one generator on one frame. Nothing in the tree times a multi-node DAG at 1080p60:
`preview_perf.rs` renders `chain_graph("gen.plasma", &[])` — a generator with zero effects —
at 320×180, 352×198, 640×360 and 960×540 only. That missing number, not this one, decides
whether Flux is performable. See `docs/PERFORMANCE-READINESS.md` gate F3.

## D020 — 2026-11-15 the verification harness itself was repaired

D019 conclusion 2 found that `infrastructure/verify.ps1` could record a passing step for
failing work, and left the defect in place. This is the record of removing it, and of the
second masking bug that only surfaced while proving the first.

### What was wrong, and how each item was established

| # | Defect | How it was proved | Now |
|---|---|---|---|
| 1 | `Step 'integration-tests'` chained three `cargo test` runs with `;`; `Step` read `$LASTEXITCODE` once after the whole block, so the verdict was `dag_parallel`'s alone. | Falsification, not argument: a block of `failing cargo fmt --all -- --check; passing xtask doctor` recorded `ok=True exit=0` while that same `cargo fmt` run alone exits 1. | Split into `integration-producer-tests`, `integration-export-matrix`, `integration-dag-parallel` — one command per block. |
| 2 | **`$LASTEXITCODE` leaks across steps.** `Step` read the variable without clearing it first, so a block that launched no external process at all was judged on whatever the *previous* step left behind. | Reproduced in a standalone `Step`-semantics harness, using `powershell -NoProfile -Command "exit N"` as the fake commands so real exit codes were exercised: step A failing made step B, which ran nothing, record `ok=true`. | `Step` sets `$global:LASTEXITCODE = 0` on entry and coerces `$null` to 0, so a step can only be judged on the command it actually ran. |
| 3 | Skipped gates wrote `{ ok = true; skipped = true }` for Link and Spout when their env vars were unset — AGENTS.md's own "missing prerequisite is `blocked_external`, never `passed`" violated inside the tool meant to enforce it. | Read the committed `verify.json`: `link-integration.ok === true`. | `Blocked` writes `ok = null`, `status = 'blocked_external'`, `skipped = true`, `reason`. In this run both show `ok=null`, and `counts.blocked_external = 2` — they are neither passes nor failures. |
| 4 | `Set-Content -Encoding UTF8` under Windows PowerShell prepends a BOM, so the committed machine-readable evidence failed strict `JSON.parse`. | Measured on HEAD's file: byte 0 is `0xFEFF`. | `WriteAllText` with `UTF8Encoding($false)`; the regenerated file parses with no BOM. |

### What the evidence carries now

`verify.json` gained `gitHash` (short SHA), `rustc`, `quick`, and `counts`
(`passed` / `failed` / `blocked_external`), plus a per-step `exit` code. The hash is the
point: without it a verdict cannot be attributed to a tree, which is exactly how an
evidence file and a HEAD that fails its own CI sit next to each other for four days without
anyway to tell which tree said what. This partly discharges gate F1's pass condition
("Each evidence file names the commit SHA", `docs/PERFORMANCE-READINESS.md:620`) for
`verify.json` alone — F1 also asks for the binary's mtime, and for the other 25 files.
The summary line now prints the three counts before `VERIFY: PASS|FAIL`.

Two PowerShell traps were hit on the way, and both produced a *silent* wrong answer rather
than an error, so they are worth recording here: `$name:` inside a double-quoted string is
a drive-qualified variable reference, not `$name` followed by a colon (fixed to `${name}`);
and a syntax error inside a scriptblock is only raised when it is invoked, so the edits
were checked with a throwaway `[scriptblock]::Create(Get-Content -Raw)` parse gate before
spending a full suite run on them. That gate was scratch, not a committed tool.

### Measured result at `75da874` (the fmt-clearing commit)

| | |
|---|---|
| `infrastructure/verify.ps1` (full, bare) | `VERIFY: PASS`, process exit 0 |
| steps | 17 total: **passed=15, failed=0, blocked_external=2** (`link-integration`, `spout-enumeration`) |
| `verify.json` | parses strictly, no BOM, `gitHash = 75da874`, `rustc 1.99.0 (b940084d7 2026-09-28)` |

This is the first recorded run in this repository where the harness that defines CI
parity exits 0 end to end. It is a statement about the tree at `75da874` on this host
(RTX 2080 Ti / Vulkan / driver 560.94), and nothing else.

### Not changed here

- **`ndi-discovery` still overpromises its name.** It runs `ndi::`, which selects the 2
  tests that prove the DLL loads and discovery does not crash. Renaming the step is a
  docs-and-labelling job, not a harness-correctness one, so it is left open.
- **`xtask benchmark --profile X` ignored X** (D019 conclusion 4) — left open by this commit and
  closed by the next one, D021. Nothing in the harness repair changed what the benchmark measured.
- CI does not consume `verify.json`. `.github/workflows/ci.yml` runs its own steps and
  only uploads `implementation/evidence/**`; so this repair makes the *local* gate
  truthful, and the Actions job is unaffected except that the artifact it uploads now has
  the new shape.
- D019's "~8 MB GPU→CPU copy" in conclusion 4 is **wrong**, and the correct number is
  ~2 MB: `read_preview` downsamples through `preview_size()` (`crates/render/src/preview.rs:11`),
  capped by `PREVIEW_MAX_W = 960` / `PREVIEW_MAX_H = 540` (`:7-8`), so a 1920×1080 render
  reads back 960×540×4 = 2,073,600 B. The defect it was describing — the benchmark times a
  readback it should not — stands; only its magnitude was overstated. The wrong number is
  corrected here rather than left to the benchmark rework, because a reader of
  `PERFORMANCE-READINESS.md` or `STATE.json` should not have to find D020 first to learn
  that the figure they are quoting is wrong.


## D021 — 2026-11-15 the benchmark now measures a graph, at the resolution it claims

D019 conclusion 4 said `xtask benchmark` "lies about what it measured" on three counts:
`--profile` was read and thrown away, the timed loop included a full preview readback, and the
`p95ms` field was the worst of twelve samples wearing a percentile's name. `docs/PERFORMANCE-READINESS.md`
F3 called the harness "the bigger problem" and asked for the multi-node graph that nothing in the
tree timed. All four are addressed here, and the first real numbers are recorded.

### What the tool does now

`BENCH_PROFILES` (`crates/xtask/src/main.rs:302-353`) maps a profile name to resolution, budget
and graph. `ordinary` is 1920×1080 against 16.67 ms (`spec/VERIFICATION.md:138`); `4k` is
3840×2160 against 33.33 ms (`:143`). An unknown profile exits 2 with the list of known names
instead of silently benching the default, and a profile that names a node absent from the catalog
exits 2 for the same reason — a graph that quietly drops a node would report fewer passes than it
claims.

The timed graph is the merge the old harness never built: `gen.plasma` feeds
`fx.color_adjustment → fx.warp → fx.halftone → fx.vhs → fx.bloom → fx.composite → flux.output`,
and also feeds `fx.composite` port `b` directly, so the measured path is a DAG with a join rather
than a straight line. 8 nodes, and the evidence records `compiled_passes: 8` so a silent
degradation would be visible rather than inferable.

15 warm-up frames are discarded and 120 are timed. `execute()` only submits, so the render window
now ends in an explicit `PollType::Wait` — without it the "render" number was CPU submission and
the GPU was still busy. The preview readback is timed in its own window, and a third series sums
the two, so the old single conflated figure is replaced by three numbers with different meanings
rather than being quietly redefined. Percentiles are nearest-rank; `p95` is emitted only at ≥20
samples and `p99` only at ≥100, which the chain run satisfies and the per-generator sweep does
not, so the 30 sweep rows report `median`/`max` and nothing else. Adapter identity now rides in
the evidence as `gpu.info.to_json()`.

### Measured result on this host (RTX 2080 Ti, Vulkan, driver 560.94)

| profile | resolution | render median | render p95 | render p99 | render max | copy median | budget |
|---|---|---|---|---|---|---|---|
| `ordinary` run 1 | 1920×1080 | 1.53 ms | 2.32 ms | 3.37 ms | 3.40 ms | 2.52 ms | 16.67 ms |
| `ordinary` run 2 | 1920×1080 | 2.09 ms | 3.71 ms | 7.49 ms | **19.57 ms** | 4.28 ms | 16.67 ms |
| `ordinary` run 3 | 1920×1080 | 1.59 ms | 2.59 ms | 2.78 ms | 2.96 ms | 2.51 ms | 16.67 ms |
| `4k` | 3840×2160 | 3.98 ms | 5.09 ms | 6.12 ms | 8.37 ms | 3.32 ms | 33.33 ms |

Three things fall out of this, and they change the reading of the old evidence.

1. **The budget is met, at both resolutions.** The chain's render p95 is 2.3–3.7 ms against
   16.67 ms at 1080p and 5.09 ms against 33.33 ms at 4K. `benchmark.json` exits 1 when the chain
   tail breaches the budget, so this is a gate and not a printout.
2. **The historic over-budget rows were a copy artefact, not a node.** With render and readback in
   separate windows, every one of the 30 swept generators is under 16.67 ms on render alone
   (worst 10.04 ms, `gen.fractal_noise`), while the same rows' `render+copy` max reaches
   30.58 ms (`gen.clouds`). The `gen.concentric` "p95 20.91 ms" that F3 carried for six days
   belonged to the readback. It was never a slow node.
3. **The readback barely depends on resolution** — 2.5 ms at 1080p, 3.3 ms at 4K — because
   `preview_size()` clamps it to 960×540 either way. That is the mechanism behind D020's `~8 MB`
   correction, and the reason the old figure could not scale with the profile it claimed.

### Deliberately not claimed

- **Run-to-run variance is not understood.** Three identical invocations produced render maxima of
  3.40, 19.57 and 2.96 ms. The tool now exposes that instead of hiding it, but nothing here
  explains it, and a single invocation is therefore not a frame-budget verdict. F3 keeps this open.
- **No presentation data.** Headless graph execution has no swapchain, so
  `presentation_misses` is recorded as `{measured: false}` with the reason. F3's numbers are
  critical-path render, not frames on glass; that is gate F2 and it is still unmeasured.
- **Four of the six spec profiles do not exist.** `particles`, `sdf`, `fluid` and `recording`
  (`spec/VERIFICATION.md:139-142`) are not in the table, so naming them is a refusal rather than
  a measurement. The `BLOCKERS.md:46-47` CPU-producer ceiling is untouched by this change.
- **`benchmark.json` holds one profile at a time.** Each run overwrites the file, so the 4K row
  above was read from a copy taken before the next run replaced it. Unchanged, because making the
  evidence filename profile-specific is a separate call about what `implementation/evidence/`
  should contain.
- **The sweep got shallower on purpose.** 12 samples became 10 per generator with no percentile
  claim attached. The old rows were 12 samples labelled p50/p95; the label was the problem, not
  the count.
