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
2. **`verify.ps1` could record a passing step for failing work** — repaired in D020, and
   true only of the tree this entry measured. `Step` reads `$LASTEXITCODE`
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
4. **The benchmark gate was weaker than its output looked** — true of the tree this entry
   measured, no longer true of HEAD (`crates/xtask/src/main.rs:299-382`
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
("Each evidence file names the commit SHA", `docs/PERFORMANCE-READINESS.md:621`) for
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

The committed `verify.json` names a later tree: after D021 the harness was re-run bare at
`6d2130d`, and the only fields that moved were `generatedAt` and `gitHash` — same
`VERIFY: PASS`, exit 0, and 15/0/2 counts. That diff is itself the point of recording the
hash: the file says which tree it speaks for, so a later reader can tell a re-run from a
stale verdict. Two more re-runs have since overwritten it, at `3214caa` (lane 9) and `90d9e59`
(lanes 10–11), both with the same 15/0/2 verdict — so the field is the only thing that
distinguishes four different measurements of the same PASS, and the table above is read as
D020's run, not as today's file.

### Not changed here

- **`ndi-discovery` still overpromises its name.** It runs `ndi::`, which selects the 2
  tests that prove the DLL loads and discovery does not crash. Renaming the step is a
  docs-and-labelling job, not a harness-correctness one, so it is left open. Left open by
  this commit, closed by D023 — and the labelling job turned out to have a real defect
  under it: those 2 tests assert nothing at all when the runtime is absent.
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

`BENCH_PROFILES` (`crates/xtask/src/main.rs:302-352`) maps a profile name to resolution, budget
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
  a measurement. The `BLOCKERS.md:58-59` CPU-producer ceiling is untouched by this change.
- **`benchmark.json` holds one profile at a time.** Each run overwrites the file, so the 4K row
  above was read from a copy taken before the next run replaced it. Unchanged, because making the
  evidence filename profile-specific is a separate call about what `implementation/evidence/`
  should contain.
- **The sweep got shallower on purpose.** 12 samples became 10 per generator with no percentile
  claim attached. The old rows were 12 samples labelled p50/p95; the label was the problem, not
  the count.


## D022 — 2026-11-15 the OSC receive path moved inside the desktop process

`docs/PERFORMANCE-READINESS.md` A1 carried the product's central claim — a fader on Live moves
a pixel in Flux — as `NOT WIRED`: the desktop opened no socket, and the one command that would
have applied a control answered `changed: false` unless a `LearnBinding` had first set
`control_map.graph_id`. Gate A was therefore a wiring job with a hardware step after it, and the
wiring is what this entry records. A2 (bridge packets silently discarded) closed on the way.

### What was built

One decode entry point. `osc::decode_control` (`crates/integrations/src/osc.rs:47`) tries the
bridge grammar before the project grammar and returns `OscControl::Bridge(..)`; `OscReceiver`'s
socket thread calls it, so the discard in A2 is gone for both the CLI and the app.
`osc::encode_message` (`:237`) exists so a test sends real bytes instead of hand-rolling the
wire format — an encoder nobody transmits through cannot prove a decoder.

The bridge itself, `control::OscBridge` (`crates/app/src/control.rs:304`), owns an `OscReceiver`
plus a worker thread. Parameters coalesce latest-wins per `(node, param)` on a 16 ms window, so
a 1 kHz stream becomes at most ~60 document edits per second per parameter and one undo entry
per flush; every other kind is applied the moment it arrives, because coalescing a trigger loses
edges. `OscStats` counts every control that arrives into exactly one bucket, and a bind failure
is returned rather than logged — the port has one owner and "something else holds 9000" has to
be reportable.

On the authority side, `CommandHost::apply_live_param` and `apply_live_control`
(`crates/app/src/commands.rs:250`, `:284`) target `live_target_graph()`, which falls back to
`active_graph` when nothing was learned, and `control::is_allowed` treats an empty binding table
as an open surface and a populated one as a whitelist.

The shell starts it in `setup` (`apps/desktop/src-tauri/src/lib.rs:474`) on `FLUX_OSC_PORT` or
9000, publishes it through `osc_status` (`:320`), and emits `live-control` once per flush. The
webview's whole participation is one listener that re-pulls the snapshot, at most one pull in
flight.

### The decision, and the rule it had to satisfy

"Rust is the only authority" (`AGENTS.md`) rules out the obvious shape. A listener in the
webview, or a `LiveOsc` dispatch triggered by JS on receipt, both put control authority on the
wrong side of the IPC boundary and make the app a relay for its own document. What was chosen is
(c): the listener lives in the library, is owned by the process that owns the document, and
applies through the same `apply_to_dispatcher` path a click uses. The preview already re-reads
the graph from Rust every frame, so this needed no new render path — only a snapshot re-pull so
the inspector cannot disagree with the preview about a value that just changed.

### What the tests found, measured

The first version flushed only on the branch that had just received a control. A single fader
move is a packet followed by silence, so it sat in `pending` forever: five of the seven receive
tests timed out at their 5-second deadline, one deadlocked on a nested `host.lock()` inside the
test itself, and only the port-contention test passed. Fixed by flushing on the timeout branch
(`crates/app/src/control.rs:389`). The whole set now runs in 0.37 s.

Two further failures were bugs in the assertions, and they are recorded because they describe
behaviour rather than mistakes: the document stores the f32 that came off the wire widened, so
`0.6f32 as f64 != 0.6`; and a bridge packet increments `notApplied` as well as `bridge`, because
it genuinely is a control that changed nothing.

Separately measured: `decode_packet` flattens arguments to `f64` and drops non-numerics, so the
`/transport/seek` packet that `docs/FILE-FORMATS.md` specifies as string-seconds-then-int-denominator
loses its seconds, reads the denominator as the seconds, and hardcodes `denominator: 1_000_000`.
That would be a seek to the wrong place with total confidence, so it is pinned by
`a_seek_packet_cannot_carry_its_seconds` (`crates/integrations/src/tests.rs`) instead of left
latent.

### Deliberately not claimed

- **bytes → document is not document → pixel.** No frame was rendered by this path in this
  session and no Max device has sent a packet. A1 moves from `NOT WIRED` to `UNVERIFIED`, which
  is the honest distance: the code half is closed, the observation half is still a host task.
- **Transport, seek, trigger, preset and bridge features are counted with a stated reason, not
  implemented.** Implementing them here would have meant either a JS-owned playhead writing into
  the document — the authority rule again — or a silent counter. `apply_live_control` names the
  missing thing per kind and `osc_status.stats.lastReason` carries it to the operator.
- **Two real gaps stayed gaps.** There is no `Command` variant for a node trigger, and
  `parse_address` stores the whole address as a `PresetRecall` id while dropping the argument
  that names the preset. Both are recorded rather than worked around.
- **`Command::SetParameter` inserts any parameter id without catalog validation**, so a mistyped
  address produces an inert override plus an undo entry instead of an error. The learned table is
  the only whitelist. Changing that contract belongs to the domain crate, not to this wiring.
- **`flux osc <port>` was not repurposed.** It is still a 30-second print-only diagnostic, and it
  binds the same default port the desktop app now wants; that collision is a trap, so it is in
  `AGENTS.md`.

### Measured at this tree

`cargo fmt --all -- --check` exit 0. `cargo test --workspace -- --test-threads=1` = **204 passed
/ 0 failed / 0 ignored across 40 binaries, exit 0** (196 before this change; the +8 are the
twelve tests in `app/src/control.rs` — seven new UDP loopback tests — plus the integrations seek
test above, and the pre-existing 14 GPU tests still count a skipped device as a pass).
`cargo build -p flux-desktop` exit 0, with `app.bundle.js` regenerated by `build.rs` and carrying
the listener — the page loads the bundle, not `main.js`, so a `main.js`-only change would have
shipped nothing.

Full CI-parity harness `powershell -NoProfile -ExecutionPolicy Bypass -File
infrastructure/verify.ps1`: **VERIFY: PASS, exit 0, 17 steps: passed=15 failed=0
blocked_external=2**, evidence written to `implementation/evidence/verify.json`. The two
`blocked_external` rows were `link-integration` and `spout-enumeration` (their env vars were unset
in that shell) and they exit 0 regardless — check the third number, not the verdict. D027 later
found both artifacts on this machine and re-keyed those gates, so this is no longer the run shape.

That PASS was not immediate, and the first FAIL is the reason to re-run the harness rather than the
suite. `fmt` and `unit-tests` were red because of two defects added after the earlier green
measurements: `rosc::OscType::Str` does not exist (the variant is `String`), which failed the
compile of the new integrations test and therefore both cargo-test steps, and the new Rust code was
unformatted. Because `fmt` is the first step, the tree at that moment could not have been merged at
all, exactly as D019 describes.


## D023 — 2026-11-15 what the NDI step proved, and the command that had never worked

The third lane was a sentence about labelling: *ndi-discovery still names more than its 2
tests prove, deliberately left open by D020*. Closing it meant measuring what the step
actually asserts, and under the label were two live defects.

### Measured

All five rows below ran against `target/debug/flux.exe`, 10,262,016 bytes, mtime
2026-11-15T15:12:19Z, sha256 `db1d915f2e0c9f325f9fba9cc864b35c9aedd3b3172f7fc6cb407f60d6b9f9cf`,
built with `cargo build -p flux-app --features ndi` — **not** the default-feature build, which
compiles `ndi` out entirely.

| command | output | exit |
|---|---|---|
| `flux ndi` | `0 NDI source(s) found:` | 0 |
| `flux ndi-loopback` | `resolved advertised source: DESKTOP-MLIF3MV (Flux Loopback 4044)`, `NDI loopback OK: sent 640x360 RGBA, received 640x360 (921600 bytes, nonzero=true)` | 0 |
| `flux ndi-send FluxE2E12 16` (process A) | `NDI sender 'FluxE2E12' broadcasting 640x360 for 16s` | 0 |
| `flux ndi-recv FluxE2E12 tmp_ndi` (process B, 6 s later) | `connecting to: DESKTOP-MLIF3MV (FluxE2E12)`, `received 640x360 NDI frame -> tmp_ndi\ndi_frame.png` | 0 |
| `flux ndi-recv BogusNameHere` while A broadcast | `No source matched …` + `SdkMissing` | **1** |
| `flux ndi-recv FluxE2E12` after A stopped | `0 NDI sources discovered; nothing to connect to` | **1** |

The received PNG was 108,601 bytes, sha256 `dcccc4401e21c13b8bbc75c45b3fb79cb7ee0e8cece3bb5e6199542db19e9818`.
Sampled pixels: (0,0) R162 G0 B128 A255, (0,359) R161 G254 B127 A255, (320,180) R33 G127 B127 A255,
836 distinct colours over a 17-px stride. G ramps top to bottom and B sits at 128 — the gradient
`cmd_ndi_send` generates, arriving intact. The R values are not reproducible: the sender adds
`t/8` to R, so this hash dates the run instead of identifying it. Full transcript:
`implementation/evidence/ndi-2026-11-15.txt`.

### Defect 1 — the gate could pass by being absent

`crates/integrations/src/ndi.rs` contains exactly two tests, and both open with

```rust
let Some(rt) = NdiRuntime::load() else { return; };
```

so `Step 'ndi-discovery' { cargo test -p flux-integrations --features ndi ndi:: }` recorded
`ok: true` on a machine with no NDI runtime while executing **zero assertions**. That is the
repo's own "a missing prerequisite is `blocked_external`, never a pass" rule broken in a second
place — D020 fixed how steps are *judged* and never looked at what a step *selects*.

verify.ps1 gained `Test-NdiRuntime`, which walks the same candidate list `ndi.rs` walks
(`FLUX_NDI_LIB`, NDI 6 Tools, Resolume Arena, then the bare name — `Test-Path` cannot answer a
PATH-relative lookup, so PATH is walked explicitly) and branches:

- runtime present → `Step 'ndi-runtime-loads'` with the same filter, under a name that says what
  it proves;
- runtime absent → `Step 'ndi-feature-compiles'` (`--no-run`), then `Blocked 'ndi-runtime-loads'`
  with the four candidates named in the reason.

The compile-only step is not decoration: `crates/integrations` sets `default = []`, so on a
machine without NDI nothing else in CI would ever type-check the `ndi` feature again. Checked
before running anything: the PowerShell parser reports no errors, and `Test-NdiRuntime` returns
`True` on this host (`C:\Program Files\NDI\NDI 6 Tools\Runtime\Processing.NDI.Lib.x64.dll`,
29,863,120 bytes).

### Defect 2 — `flux ndi-loopback` had never once worked

It called `shim.receiver(&name)` with the **sender's own** name. `ndi_shim_recv_create` forwards
that straight to `NDIlib_recv_create_v3` as `p_ndi_name`, and NDI advertises sources as
`{machine} ({sender})`, so the subscription could never match; the receive loop expired after
40 × 150 ms, the command printed `NDI loopback: no frame received within timeout`, and returned
`Ok(())`. Every run of that command since it was written failed and reported success.

Fixed by resolving the advertised name through discovery first and subscribing to *that* — the
sequence `cmd_ndi_recv` already had right, which is what located the bug. The unused `NdiRuntime`
import inside `cmd_ndi_loopback` was the fingerprint: a same-process receiver needs a runtime
handle, and this function had imported one and never used it. Then the exit statuses: every
unfulfilled purpose in `ndi-loopback`, `ndi-send`, `ndi-recv` and `ndi` became
`ErrorCode::SdkMissing`, which exits 1. `flux ndi` printing an empty list stays exit 0 on
purpose — finding nothing is an answer; failing to connect is not. The two `1`s in the table
above are the point of that change: the same two invocations returned `0` before it.

### Documentation corrected against the measurement

Five documents asserted NDI states these runs do not support, all of them phrased in the past
tense as if they were permanent properties of the product:

- `implementation/BLOCKERS.md:4` said **"Fully working — discovery, send, and receive verified"**
  and "discovery found **5 live sources**". Today the same command finds 0; that count was the
  LAN's state on 2026-11-09 (Premiere, the NVIDIA sender, Test Patterns were open). Rewritten to
  separate what reproduces from what was environmental, and to name the two absent halves: no
  wgpu→NDI producer, no third-party consumer.
- `implementation/FINAL-REPORT.md` §6b carried the same headline, and §6 listed the NDI SDK as
  `blocked_external` when it is installed. Both rewritten. The line deleted from §6b — "2 shim
  tests pass (load→init→create sender)" — was wrong on its own terms: `ndi_io.rs` has **1** test.
- `apps/ndi-bridge/README.md:31` claimed cross-process verification with no date and no scope;
  it now states the fresh result, the advertised-name matching rule (the trap behind defect 2) and
  what is not proven.
- `docs/PERFORMANCE-READINESS.md` C4 was already honest — labelled UNVERIFIED, owner EXTERNAL — and
  only needed its 5-source reading dated and pointed at the new transcript.
- `crates/integrations/src/lib.rs` set `verified: cfg!(feature = "ndi")` with the limitation
  "discovery + live send/receive verified (cross-process, **real frame**)", which is what a reader
  takes as *a Flux frame* — the frame was real NDI video and synthetic content. Now a measured
  constant with its basis and date in a comment. `live::protocol_status()` said "NDI SDK linked"
  for a field documented as "availability … on this host"; it now says it answers the build half
  and points at `flux ndi` for the runtime half.
- `AGENTS.md` traps section: the overpromise bullet records the repair and generalises the lesson —
  a CLI that prints a failure and exits 0 is the same defect as a skipped gate counting as a pass.

### Not changed here

- `spout` and `link` still derive `verified` from `cfg!` (then `crates/integrations/src/lib.rs:86`,
  `:114`), the same pattern this entry calls a defect for ndi. Left open deliberately: replacing
  those booleans means asserting a fresh verdict about paths this lane did not re-measure, and the
  spout row's own limitation already says the send path fails on this host. Recorded as still open
  in `implementation/STATE.json` `ci_defects_found_2026_11_14`, and closed by D027 two lanes later.
- `apps/ndi-bridge/ndi_shim.dll` was not rebuilt — 24,064 bytes, mtime 2026-11-09 — so the shim
  half of these runs is evidence about a six-day-old binary that happens to still be on disk.
- The `ndi::` filter still excludes `ndi_io::tests::shim_loads_and_initializes`; the shim path is
  exercised by the CLI commands, not by the harness.
- `live::discover()` is still a stub, and there is still no wgpu→NDI producer.


## D024 — 2026-11-15 provenance moved from prose into the writer, and the soak exists

The second lane was "finish the F-series evidence — the 120 s soak with peak RAM/VRAM, plus
F1's unfinished half (binary mtime, and the other 25 evidence files naming their commit SHA)".
Its stated risk profile was "measurement only, can't break anything". The measurement half ran
against existing code; the F1 half needed a writer change, because the remaining evidence files
were not *missing* a SHA someone had forgotten to add — most of them have no writer at all that
adds one.

### F1: provenance is now injected, not remembered

`implementation/FINAL-REPORT.md:146` and `docs/PERFORMANCE-READINESS.md` F1 both rested on the
same observation: the release CLI every document credited was built on Nov 9, before the
preview and export paths it was credited with were written. F1's pass criterion — "each evidence
file names the commit SHA and the binary's mtime" — had been discharged for prose (D019/D020
recorded hashes in decision text) and not for data.

Doing it per file would have meant editing eight independent `serde_json::json!` payloads in
`crates/xtask/src/main.rs`. Instead one function does it: `write_evidence()` now merges a
`provenance` object into every JSON before writing — `gitHash`, `gitHashFull`, `gitDirty`,
`generatedAtUtc`, and the running binary's own `path`/`mtimeUnix`/`mtimeUtc`, plus host os/arch —
so all eight xtask writers inherited it from one edit (`crates/domain/src/document.rs` gained
`utc_from_unix_pub` for the calendar, reusing the existing days-from-civil arithmetic rather than
adding a chrono dependency).

`gitDirty` is load-bearing, not decoration. A gate that runs *before* its own tree is committed
names its **parent** commit, which is exactly what happened here: the `verify.json` that run wrote
carried `gitHash: 3214caa` while HEAD was already `90d9e59`. Without the dirty flag that reads as
tampering; with it, it reads as a re-run of the harness against the tree it started from. (The file
has been rewritten since and now names `90d9e59` + `gitDirty: true` — the same shape one commit
later, because this session keeps verifying before it keeps committing.) That sentence asked the
flag to carry more than it could: it measured the whole working tree, so a gate that ran after
another gate had rewritten its evidence called a clean tree dirty. D026 re-scoped it, and says what
the field meant before that.

`cargo run -p xtask -- evidence` then audits the directory and writes `manifest.json`: every file
with bytes, mtime, the SHA it names, a `producer` and an `attribution` of `stamped_current` /
`stamped_other_commit` / `no_provenance` / `unattributed`, plus sha256 and mtime for the four
binaries, the rustc version, and `recheckCommand` so the next session can re-ask the question in
one line. Exit code is nonzero only for a file **xtask owns** that lacks provenance. That scoping
was found by being wrong first: the initial rule ("any JSON without provenance is a defect")
flagged `diagnostics.json`, which has no writer in the tree at all — `flux diagnostics` only
prints — so the tool was demanding that a hand-saved transcript name a commit it never came from.
The 19 hand-captured `*.txt`/`*.png` files then on disk (20 now, and 29 files in the directory
rather than 28, because D023's `ndi-2026-11-15.txt` was saved by hand the same way) are therefore
reported `unattributed`, which is a
statement about the tool's reach, not a pass.

F1's original wording said "with the git SHA stamped **in the filename**". Superseded: a filename
can be renamed while the bytes inside stay silent, so the SHA lives in the payload and the
manifest cross-checks it. F1's *criterion* is met for every file that has a writer; F2 and F4 stay
open because they need a swapchain and a device, not a stamp.

### The 120 s soak: `xtask soak`

Windows APIs were already vendored for `flux-platform`, so `K32GetProcessMemoryInfo` came from
the `windows` crate at the version the workspace already resolves (0.62 — no new lockfile entry),
and GPU memory from `nvidia-smi --query-gpu=memory.used`. The loop runs the same 8-node merging
chain as `xtask benchmark`, on a wall-clock deadline, discarding 15 warm-up frames, and it keeps
the memory probes **outside** the per-frame timing windows so a 500 ms Working-Set sample cannot
appear as frame-time noise.

Two runs of 120 s at `ordinary` (1920×1080, budget 16.67 ms):

| run | timed frames | render median / p95 / p99 / max | frames > budget | WS first → last → peak | device GPU peak |
|---|---|---|---|---|---|
| 1 | 24,591 | 1.51 / 3.75 / 4.90 / **17.44** ms | **1** | 207.83 → 219.28 → 221 MiB | 1,248 MiB |
| 2 | 23,673 | 1.63 / 3.51 / 4.92 / 8.21 ms | 0 | 207.23 → 219.48 → 221.46 MiB | 1,247 MiB |

Growth 11 MiB (5.51 %) and 12 MiB (5.91 %) against `spec/VERIFICATION.md:145`'s 10 % / 64 MiB.
Three things this says that the run-2 file alone would not:

1. **Run 1's hitch did not reproduce**, and D021's "~6× run-to-run tail" is now confirmed at
   24,000-frame scale rather than 120-sample scale. Recorded anyway because run 2 *overwrote*
   run 1's evidence file — a single-name evidence path is a single-slot memory, and the honest
   fix is to keep both sets in prose, which is what this entry and F5 do.
2. **120 s cannot settle the leak question.** Both runs ramp ~10.6 MiB in the first 30 s and then
   add ≈1.1 MiB/min for the remaining 90 s; both end within 0.2 MiB of each other. Two runs of
   identical duration look like that whether the curve is plateauing near 220 MiB or creeping — the
   experiment is too short to tell, and if it is a creep, 64 MiB is spent near the 58-minute mark.
   AT081 asks for 2 h. That is now one command, not a plan.
3. **The two numbers people will quote are not per-process.** `peakDeviceUsedMiB` is device-wide
   from `nvidia-smi`, which does not attribute a Vulkan renderer's allocations to Flux; per-process
   VRAM is written as `{"measured": false, "reason": "no Vulkan-memory telemetry in this build;
   NVAPI not linked"}`. `presentationMisses` and `audio` are the same shape — the soak opens no
   device and creates no swapchain, so `dropped()` is never populated and there is no miss counter
   to read. All three are stated inside the evidence file so a reader cannot meet them as results.

The GPU probe had a live bug in its first version: the `nvidia-smi` check was nested inside the
500 ms Working-Set block that reset the same timer, so `elapsed()` could never reach 10 s and the
first smoke run reported `peak: None` for a GPU that was measurably busy. Fixed with independent
`last_rss` / `last_gpu` clocks; `gpu_probes` now counts *attempts*, so a missing `nvidia-smi` is
not spawned 24,000 times.

### Measured at this tree

`cargo fmt --all -- --check` exit 0 after this change. The four binaries were rebuilt
(release CLI + release GUI, debug CLI + debug GUI); sizes, mtimes and sha256 are in
`implementation/evidence/manifest.json` — the F1 conclusion "the release CLI predates the export
path" is no longer true of the tree on disk, and is now checkable rather than asserted.
`soak-ordinary.json` carries the run-2 numbers above with its own provenance block.

### Not changed here

- The four unbuilt spec profiles (`particles`, `sdf`, `fluid`, `recording`) are absent from
  `BENCH_PROFILES`, so `xtask soak --profile particles` exits 2. Adding a table row would be
  naming a measurement that has not happened.
- No 2 h run, no device-bound half of AT081: `dropped()`, scene switches, repeated edits and the
  external panel are still `blocked_external` in the sense that they need a live session.
- Evidence files still lack per-run history: writing `soak-ordinary.json` over the previous run is
  how run 1 was lost. A timestamped filename or an append log would fix it; left open because the
  manifest already records *when* each file was written, and inventing a second naming scheme for
  one command is not obviously better than saying so in the decision.
- `xtask evidence` reports, it does not repair. The hand-captured transcripts — 19 in that entry's
  own run, 20 now, because `ndi-2026-11-15.txt` was saved the same way — stay `unattributed` until
  someone re-runs the thing that produced them. Note that a transcript *naming* its commit in prose
  (this one does: gitHash, binary size, mtime and sha256 in its header) is still `unattributed` to
  the tool, because `attribution` asks whether the payload carries machine-readable provenance.

## D025 — 2026-11-15 the ledger's `verified` rows were fixed where they are decided: the generator

Lane 3's leftover was stated as "ndi-discovery still names more than its 2 tests prove", and D023
fixed that inside the harness. Following the same thread one level up found the identical defect
shape in the status ledger: `implementation/requirements.json` is **generated** (`generatedBy:
"cargo xtask report (requirements)"`, writer `crates/xtask/src/bin/requirements.rs:174`), and its
AT060 row asserted `verified` because a `match` arm three hundred lines earlier says so.

### Why editing the JSON would have been the wrong repair

Two acceptance records claimed more than their evidence, in the same way the NDI step did:

| record | at HEAD | after | the gap |
|---|---|---|---|
| AT060 — "Actual NDI official SDK/runtime send/receive loopback; frame/audio release counts, stride/timestamps, missing-runtime behavior" | `verified` | `implemented_unverified` + blocker | one of four clauses is measured (loopback, re-measured today); release counts, stride/timestamps and the missing-runtime path on a host without NDI are not, and there is still no wgpu→NDI producer |
| AT063 — "Max/conditional Link transport; single master, seek/loop/start-stop behavior, tempo phase aligned, no control-as-audio confusion" | `verified` | `implemented_unverified` + blocker | `crates/integrations/src/link.rs:249` does assert single-master arbitration, but only inside a `--features link` build, and the AC's Max-transport and seek/loop clauses are asserted by nothing — which is exactly what `BLOCKERS.md:26-35` and `:54-55` had already said. The ledger was the outlier, not the blockers file. |

A hand-edit would have survived until the next `cargo run -p xtask --bin requirements`, i.e. until
the next gate run. The generator now produces the claim its own evidence supports, and the
regenerated file diff is **exactly these two records** — every other line byte-identical, which is
also the only available proof that the generator is deterministic on this tree.

### The `blocker` field got a second job

`record()` already took `blocker: Option<&str>`, but the acceptance loop always passed `None`, so an
AC could never say *why* it was not verified. `at_blocker(id)` (`crates/xtask/src/bin/requirements.rs:472`)
is now wired into that loop and carries the missing clause list for both rows. `blocker_for()`
(product requirements) is untouched.

### The citation blast radius, measured rather than eyeballed

Adding one evidence line to AT060 shifted every citation into `requirements.json` below line 3743 by
+1 — ten of them in `docs/PERFORMANCE-READINESS.md`. Two more (`:3655` for AT050, `:3729` for AT059)
were already pointing one line above their record when they were written. Rewriting `BLOCKERS.md`'s
Link entry shifted sixteen more citation instances across `PERFORMANCE-READINESS.md`,
`DECISIONS.md` and `STATE.json`, and the Rust edits from D022/D023 had already moved four `live.rs`
ranges.

So a throwaway checker resolved every `path:line` citation across the status docs against the current
tree and printed the target text — 81 of them at the final check, every one landing on the record or
key it names (the checker prints `UNRESOLVED`, `OUT OF RANGE` and `BLANK TARGET`, and printed none).
That is the mechanical part; it does not prove the *sentence* around a citation is still true, which
is what the next section is for. Line-number citations are cheap to write and expensive to maintain
in a repo whose status docs are rewritten every session; switching to heading anchors would be the
fix, and is deliberately not done here so that it stays a decision rather than a side effect.

The same cost decided the *shape* of the `STATE.json` refresh: `verify_ps1_full` was re-measured by
rewriting single-line string values — `at_head`, `passing_steps`, `current_task`, two
`commands_to_resume` entries — and renaming one key (`re_run_lane9` → `re_run_lanes`) instead of
adding keys. `STATE.json` is cited by line from three documents, so a fresh line anywhere in the top
half would have shifted the dozen citations below it for the second time in one session.

The same sweep was then run against the *code* and *spec* citations in those documents, plus
`docs/IMPLEMENTATION-ARCHITECTURE.md` and the user guide — 161 of them at the final check — and it
found wrong ones that predate this session entirely. The leak trip wire was cited three times against
`spec/VERIFICATION.md` at line 144, and line 144 is **blank**; the 10 % / 64 MiB sentence is `:145`.
The "Record exact adapter/driver and actual path" requirement was cited as `:134` and `:136` where it
lives on `:132`. And `IMPLEMENTATION-ARCHITECTURE.md` §3.6 pointed the "a generator must have no
`source` input" rule at `crates/tests/tests/sweep_catalog.rs`, lines 41–49 — a range that opens on a
blank line and cuts the predicate in half; the detection and the `needs_source` branch are `:45-62`.
All of them resolve now, and the checker refuses any citation whose target line is empty, which is
exactly how the `:144` family would have been caught the day it was written.

Neither checker is committed — they were scratch, and a second unenforced tool would just create a
new way to look verified. Re-deriving them is two regexes over the six status docs:
`(?:BLOCKERS|STATE|requirements|FINAL-REPORT|DECISIONS|PERFORMANCE-READINESS)\.(?:md|json):(\d+)(?:-(\d+))?`
for the internal ones and
`((?:crates|apps|spec|infrastructure)/[\w./-]+\.(?:rs|md|json|ps1|cjs|toml)):(\d+)(?:-(\d+))?`
for the code ones, each resolved against the current file, with failure prints for an unresolved
path, an out-of-range line and a blank target. What makes the check worth re-running is not the
totals: it is that a record edit in `requirements.json`, `STATE.json` or `BLOCKERS.md` silently
invalidates every citation below it, and nothing in CI notices.

### Documents that were wrong for reasons the ledger change exposed

- `docs/PERFORMANCE-READINESS.md` **C1** argued that BLOCKERS.md:4 "says NDI Fully working". D023 had
  already retired that sentence, so the row was attacking a document that no longer said it. Rewritten
  against current lines (`main.rs:703`/`:558`/`:439`, `live.rs:37-41`, `STATE.json:85-86`,
  `requirements.json:3745`) and it now says what changed since: the docs, not the code.
- **A6 "Doc conflict"** used to be a conflict *between* files (BLOCKERS vs requirements vs STATE). The
  ledger is no longer in the disagreement, so the remaining conflict is inside one object:
  `STATE.json:77` "supported + verified when built with feature link" against `STATE.json:82`, which
  already names itself as contradicting :77.
- **§11 drift rows** for Max bridge, Ableton Link, GUI verified list and `.amxd` re-cited. The GUI
  row's complaint — "that list predates the node-graph editor" — is no longer true of
  `STATE.json:93-100`, and the row now says so and points at the residue instead: the only screenshot
  is `gui-release.png` at 2026-11-09T09:33:05Z, 398,725 bytes, `unattributed`.
- `FINAL-REPORT.md` **§6a** headline "Ableton Link — supported and verified" → "supported; verified
  inside a feature build", with a paragraph stating that `link-integration` was `blocked_external` in
  the current run and the cross-process artifact is from 2026-11-09.
- `BLOCKERS.md` **Link entry** stopped flattening "single-master is not covered": it *is* covered by a
  test; a plain build just does not compile it. That distinction is the whole difference between an
  unverified claim and an unbuildable one.
- `STATE.json`: `gui_status.release_binary` still said "stale: built 2026-11-10, predates HEAD", which
  the D024 rebuild overtook — it now defers to `evidence/manifest.json` instead of restating a
  timestamp; `ableton_link.not_covered` rewritten to name which voice disagrees.
- `STATE.json` **`verify_ps1_full`**: `at_head` named `75da874` and `passing_steps` credited that run
  with 149 lib tests. Re-measured at `90d9e59`: 157, and each integration step's count re-taken rather
  than inherited (producer 6, export_matrix 6, dag_parallel 4, golden 3, `ndi::` 2 of 30).
- `FINAL-REPORT.md` **§1 and §3** disagreed with the tree in opposite directions. §1 announced a
  "216-entry catalog" — `flux doctor` says 217, and AGENTS.md has a whole line about every doc that
  says 216 being stale — so §1 is now explicitly labelled the 2026-11-09 inventory it is, with the
  current counts next to it. §3 claimed to be the verification commands and listed
  `cargo test --workspace` with no `--test-threads=1`, `test-gpu` with no profile, and no `fmt` step
  at all, which is not the CI-parity set: a reader copying that block runs a racy suite and never
  sees the gate that aborts the real job. It now lists what `verify.ps1` runs.

### Measured at this tree

The ledger rows are prose about tests, so the repair is only honest if the tests were run at the
tree that carries the repair. All of the following is from this session, at HEAD `90d9e59`:

- `cargo fmt --all -- --check` — exit 0 (`requirements.rs` is the last Rust file touched).
- `cargo test --workspace -- --test-threads=1` — **204 passed / 0 failed / 0 ignored across 40 test
  binaries**, exit 0. `docs/PERFORMANCE-READINESS.md` F1 claims that measurement in words, so it had
  to be re-taken rather than inherited.
- `infrastructure/verify.ps1` (no `-Quick`) — 17 steps, **passed=15 failed=0 blocked_external=2**,
  `VERIFY: PASS`, exit 0; `evidence/verify.json` carries `gitHash: 90d9e59`, `gitDirty: true`. The two
  blocked steps are `link-integration` (`FLUX_ABLETON_LINK_LIB` unset) and `spout-enumeration`
  (`FLUX_SPOUT_SHIM` unset) — the feature build that AT063's six tests live in is precisely the one
  this host did not run at that capture, which is what the new `blocker` string says instead of
  "verified". D027 has since put a `FLUX_ABLETON_LINK_LIB` export in front of that gate and run it.
- The per-step counts were re-counted at this head rather than copied from the `75da874` capture:
  unit-tests **157** (app 34, integrations 27, domain 30, engine 22, catalog 16, audio 14, render 8,
  media 6 — the +8 over 149 is D022's), producer 6, export_matrix 6, dag_parallel 4, golden 3, and
  `cargo test -p flux-integrations --features ndi ndi::` **2 passed / 28 filtered out**. Every one of
  those numbers is an exit-0 run, and `ndi::` selecting 2 of 30 is the whole reason D023 renamed the
  step.
- `cargo run -q -p xtask -- evidence` — exit 0: "29 files, 9 name their commit, 20 unattributed, 0
  missing provenance". 29 and 20, not D024's 28 and 19: `ndi-2026-11-15.txt` is a hand-saved
  transcript from D023's run, so it joins the `unattributed` group rather than the stamped one.
- The three read-only gates, re-run for the catalog count because `FINAL-REPORT.md` §1 and §3 still
  printed **216**: `xtask doctor` → `catalog entries: 217`, `xtask test-gpu --profile baseline` →
  "test-gpu: compiled 217 shaders, 0 failures", `xtask bin export_shaders` → "wrote 217 shaders …
  (0 naga failures)". All exit 0. §1's number is now framed as the 2026-11-09 inventory it is, and
  §3's block was rewritten to the commands CI actually runs (`--test-threads=1`, `--profile
  baseline`, and `fmt` as the step whose red aborts the rest).

None of that moves a row toward *stronger* evidence. The tree is as CI-green as it was at D024; what
changed is that two of its ledger rows now claim less, and say which clause is missing.

### Not changed here

- **AT049/AT050 (OSC) stay `verified` on the generator's word.** The AC wants "all declared types" and
  "scheduled bundle order"; the tree has 7 UDP loopback tests and I did not measure whether they cover
  those two clauses, so there is no basis to move them in either direction. Recorded so the two
  changes above are not mistaken for a completed sweep of the ledger.
- `implementation_paths` and `test_ids` are still empty for every acceptance record, so no claim can be
  linked to the test that proves it — already a `STATE.json` known gap, still true.
- `product_paths()` and `blocker_for()` hardcode paths and blockers for the R-rows: the same
  "generator asserts the claim" shape, not swept.
- `crates/integrations/src/lib.rs:86` and `:114` (before D027 moved them) set `verified:
  cfg!(feature = "spout")` / `cfg!(feature = "link")`, against that function's own documented rule
  that presence is not verification. Left alone on purpose: those two lines are what `flux
  integrations` prints, so fixing them is a decision about the command's contract, not a
  documentation repair. D027 made that decision.
- The spec's acceptance text stays one human sentence per AC with one status field, so a four-clause
  AC can only ever be as verified as its weakest clause. Nothing in the schema expresses partials.

## D026 — 2026-11-15 `gitDirty` was measuring the harness, not the source

Committing D023–D025 made the tree clean for the first time in this sequence, which is the only
condition under which a stamp can be read as anything but a confession. So
`infrastructure/verify.ps1` ran again at `e110381` with an empty `git status` in front of it.

### What the refresh said

| evidence file | gitHash | gitDirty |
|---|---|---|
| `catalog.json` | `e110381` | **false** |
| `doctor.json`, `test-gpu.json`, `fixtures.json`, `manifest.json`, `verify.json` | `e110381` | true |
| `benchmark.json`, `package.json`, `soak-ordinary.json` | `90d9e59` | true |

Six writers, one commit, one tree, two answers. Nothing was uncommitted but the evidence the run
itself had just rewritten, and `catalog` — the first xtask writer in step order — ran before any of
that existed. So the field D024 called load-bearing answered *did an earlier gate write a file
before me*, not *was the source I measured at the commit I name*. Anyone who read it as the second
thing would have to conclude that every stamp after the first gate in this repo was taken against
unseen local edits, which is not true of a single one of them.

The same pass caught a smaller mis-report. `manifest.json`'s own row said
`90d9e59 / stamped_other_commit` inside a file whose `provenance` names `e110381`: `cmd_evidence`
scans the directory and then writes into it, so the row describes the manifest that was on disk
before the scan — a file that no longer exists. This one cannot be fixed by computing harder,
because a writer cannot stat or size its own output before it produces it. The disposition is to
stop implying otherwise, so the payload now carries a note: the self-row is the previous run's, the
`provenance` block is this run's.

### The change

`crates/xtask/src/main.rs` measures dirt as
`git status --porcelain -- ":(exclude,top)implementation/evidence"` and writes `gitDirtyScope`
beside it, because an unqualified boolean with a hidden scope is how this entry came to exist.
`infrastructure/verify.ps1` uses the same pathspec for `$dirty` and the same key in its payload.
`cmd_evidence`'s `notes` gained the two sentences above.

Probed before touching code, on the real tree: appending a newline to a tracked evidence file made
unscoped `git status --porcelain` print 1 line and the excluded form print 0; restoring the file
made both print 0. That is the whole mechanism, and the reason the fix is a pathspec rather than a
second field.

### Measured at this tree

`cargo fmt --all -- --check` exit 0, no output. `cargo test --workspace -- --test-threads=1` exit 0
— 204 passed / 0 failed / 0 ignored across 40 binaries. verify.ps1 parsed with `parse_errors=0`.
The run that produced the table was `VERIFY: PASS`, exit 0, `steps: passed=15 failed=0
blocked_external=2` — same verdict as D025's, same two blocked steps (`link-integration`,
`spout-enumeration`).

The refresh at the commit carrying this change confirms it end to end. `catalog`, `doctor`,
`test-gpu`, `fixtures` and `verify` all wrote `gitDirty: false` with `gitDirtyScope` beside them,
while `git status --porcelain` listed five modified files under `implementation/evidence/` — one
commit, six writers counting the re-run `manifest`, and no disagreement where the old rule produced
two answers. The table above is what the defect looked like from the inside. The manifest's own row
still names the previous commit (`e110381` inside a file stamped `97d2ae2`), which is the limitation
the new note describes rather than one the tool can compute away.

### Not changed here

- Old stamps keep the old meaning. The single `gitDirty: false` committed before this change
  (`catalog.json` at `e110381`) reads true under either rule, so no earlier claim flips.
- `benchmark.json`, `package.json` and `soak-ordinary.json` stay `stamped_other_commit` at
  `90d9e59`: this commit re-ran neither, and an evidence file is a measurement, not a date stamp to
  refresh for appearances.
- Untracked files still count as dirt. That is the case the field exists to catch; only this tool's
  own output is excluded.
- The citation re-derivation that went in with `e110381` belongs to D025's method, not this lane:
  D025's own counts (81 internal, 161 code/spec) reproduced exactly, while `STATE.json` had been
  crediting the sweep with 71. Re-measured here at 242 citations across the six status docs plus
  `docs/IMPLEMENTATION-ARCHITECTURE.md`, 0 unresolved, 0 out of range, 0 blank targets;
  `docs/USER-GUIDE.md` contributes none to either bucket.


## D027 — 2026-11-15 the capability report was asked what its fields mean, and two "external" gates were not external

D025 closed with a line it deliberately did not touch: `crates/integrations/src/lib.rs` set
`verified: cfg!(feature = "spout")` and `cfg!(feature = "link")`, and the entry said so because
"those two lines are what `flux integrations` prints, so fixing them is a decision about the
command's contract, not a documentation repair". This lane made that decision. Reading the two
rows turned up more than the booleans: two gates this repository has recorded as
`blocked_external` since they were written can run here today, and one of them had never been
run in the repo's recorded history.

### The contract

The function's own doc comment read: *"Never derives `implemented`/`verified` from mere
presence."* The code derives `implemented` from `cfg!` in three rows, and that part is correct —
a feature compiled out of the running process is not a capability that process has. So the rule
as written could not be obeyed, which is how it survived: it named the right fear and attached it
to the wrong field. What `flux integrations` now documents, per field:

| field | whose property | may track the build? |
|---|---|---|
| `available`, `implemented` | this binary | yes — that is what they mean |
| `verified` | the evidence | no — a constant per row, identical in every build |
| `limitation` | the artifact and date the claim rests on | n/a |

"A prerequisite that is merely absent is `false`, never a pass" is stated in the comment, and is
the sentence the two rows had been violating.

### What the rows said, and say

- **spout** (`crates/integrations/src/lib.rs:91-113`): `verified: cfg!(feature = "spout")` with
  the limitation "sender enumeration verified; offscreen-GL SendImage fails on this host" — a
  string that contradicts itself before its semicolon, and returns `true` in a feature build.
  Now `verified: false` in every build, with the re-measurement named in the limitation.
- **link** (`:125-154`): `verified: cfg!(feature = "link")` with "built+verified locally; GPLv2+
  redistribution constraint". Now `false` in every build, and the limitation names what the 6
  assertions do cover, what they do not, and that one of them flakes.
- **ndi** keeps `verified: true` (measured earlier today, `evidence/ndi-2026-11-15.txt`), but its
  *off-build* limitation was the bare string "built without feature `ndi`" — which left a
  `verified: true` row with no basis in the default build, breaking the new comment's own rule.
  It now names the date and the artifact and adds "so this process can do no NDI at all".
- `syphon` and `max` were already `false`; `osc`, `midi`, `bridge` derive nothing from a feature.

### Guard: a source-text test, because a runtime test cannot see this

`crates/integrations/src/tests.rs:106-119`
(`capability_report_never_derives_verified_from_a_build_feature`) reads `include_str!("lib.rs")`
and fails any line containing both `verified:` and `cfg!`. A behavioural test cannot express the
invariant: each build observes only its own value, so the old code passed its own test suite in
both configurations. Method borrowed from `apps/max-for-live/check-maxpat.cjs` — a structural
check on text that no run touches.

Measured both ways: against the pre-lane file it FAILS, naming
`"verified: cfg!(feature = \"spout\"),"` ; against this lane's file it passes.

Cross-build probe of the shipped command, same source, two builds:

| build | `spout` row printed by `flux integrations` |
|---|---|
| `cargo build -p flux-app` (default) | `available=false implemented=false verified=false` |
| `cargo build -p flux-app --features spout` | `available=true implemented=true verified=false` |

`available` and `implemented` flip, `verified` does not — which is the contract, exercised. Under
the code this lane replaced, the second row read `verified=true`.

### Two gates recorded as blocked_external that this host can run

- `_upstream/link/build/Release/abl_link.lib` — 19,502,302 bytes, mtime 2026-11-09T12:50, i.e.
  the Ableton Link build D012's lane produced and left on disk. `verify.ps1` keyed
  `link-integration` on `if ($env:FLUX_ABLETON_LINK_LIB)`, so the step has been
  `blocked_external` in every run because one variable was unset in one shell, not because a
  prerequisite was missing. `Test-LinkLib` (`infrastructure/verify.ps1:77`) now also checks that
  the directory holds the file -- but the variable still named the directory, which D029 measured.
- `apps/spout-bridge/spout_shim.dll` — 13,312 bytes, 2026-11-09, present on this host and
  gitignored (`.gitignore:38` `apps/spout-bridge/*.dll`), so it is in no checkout. Same shape:
  the gate keyed on `FLUX_SPOUT_SHIM` while the adapter itself (`crates/integrations/src/spout.rs:44-55`)
  also tries `spout_shim.dll` and `apps/spout-bridge/spout_shim.dll` relative to the cwd, and
  `flux spout` from the repo root has been resolving it all along.

### The vacuity trap that wiring those gates would have created

`spout::tests::shim_loads_when_present` prints "spout_shim.dll not present; skipping" and
returns — and cargo records it as `1 passed`, exit 0. Measured with the env var unset:

```
FLUX_SPOUT_SHIM unset : spout_shim.dll not present; skipping → ok. 1 passed, exit 0
shim path exported    : (no skip line)                       → ok. 1 passed, exit 0
```

The two look identical in `verify.json`. They are not the same step, and a probe that only asked
"is the file present" would have made it worse — because cargo runs a test binary with its cwd
set to the *package* directory (`crates/integrations`), where the adapter's relative candidate
does not resolve, so a repo-root probe reports "present" while the test skips. So
`Resolve-SpoutShim` (`infrastructure/verify.ps1:99`, was `:90`) resolves an absolute path and exports it
before the step runs: the probe and the process now answer the same question. This is D023's
`ndi-discovery` defect arriving through the repair door, caught by running the command before
trusting the gate.

`flux spout` itself was the other half of the same class: when no candidate resolved it printed
"spout_shim.dll not found" and returned `Ok` — exit 0. It now returns
`ErrorCode::SdkMissing` (`crates/app/src/main.rs:821-827`), measured exit 1 from
`target/debug`, while an empty registry stays exit 0 because finding nothing is an answer.

`verify.json` gained `external = { ndiRuntime, abletonLinkLib, spoutShim }`
(`infrastructure/verify.ps1:200-202`, was `:187-189`) so a run states which prerequisites it could reach. The
step list alone cannot: a blocked step and an absent SDK look identical, and a step that ran
because a vendored library happened to be on the machine is not a step that ran on CI.

### What running the Link gate for the first time found

13 invocations of `cargo test -p flux-integrations --features link link::` today: **11 clean, 2
failures**, both `set_playing_reflected_in_state` at `crates/integrations/src/link.rs:203`
(`assertion failed: !s.state().playing`, immediately after `set_playing(false)`). So the first
execution of these assertions in this repository's history found a defect in them. Transcript:
`implementation/evidence/link-2026-11-15.txt` — including the fact that `_upstream/` is
gitignored, so a fresh clone reproduces neither gate.

Not fixed here, deliberately: weakening an assertion to make a gate green is the move this whole
sequence exists to prevent, and the candidate mechanism (Link aligns a *start* to a quantum
boundary while `set_playing` commits a stop at `clock_micros()` now, `link.rs:131-143`) is
derived from reading, not measured. It goes to `STATE.json` as open work with its rate attached.

### Files changed

| file | change |
|---|---|
| `crates/integrations/src/lib.rs` | `capability_report()` doc comment rewritten to the table above; `spout` and `link` `verified` → constants; three limitations rewritten against the measurements; ndi's off-build string given its basis |
| `crates/integrations/src/tests.rs` | +1 source-text guard test |
| `crates/app/src/main.rs` | `cmd_spout`: missing shim → `SdkMissing` instead of a printed message and exit 0 |
| `infrastructure/verify.ps1` | `Test-LinkLib`, `Resolve-SpoutShim`, both gates re-keyed on the machine, `external` block, and a comment on what `spout-enumeration` actually asserts. Parse-checked: `parse_errors=0` |
| `implementation/evidence/` | `spout-2026-11-15.txt`, `link-2026-11-15.txt` added (hand transcripts; both name `gitHash cf95a2f` in a `# provenance (F1)` header, and both still read `unattributed` in the manifest because `cmd_evidence` only extracts provenance from `.json` — see `crates/xtask/src/main.rs:183-193`) |
| `implementation/BLOCKERS.md` | `:54-55` carried the pre-D025 flattened Link sentence while `docs/PERFORMANCE-READINESS.md:242` cited it as repeating the split — the citation was false until the text was made true. Line-count preserved (10 citations land below it) |
| `implementation/STATE.json`, `FINAL-REPORT.md`, `docs/PERFORMANCE-READINESS.md`, `AGENTS.md`, `implementation/DECISIONS.md` | every place that quoted the old booleans or the old strings; D023's and D025's "Not changed here" bullets now read as closed |

### Measured at this tree

`cargo fmt --all -- --check` exit 0 (one diff produced by this lane's own string edit, applied).
`cargo test --workspace -- --test-threads=1` exit 0 — **205 passed / 0 failed / 0 ignored**
across 40 result lines (29 test binaries + 11 doctests): D026's 204 plus the guard test.

`infrastructure/verify.ps1`, run with `FLUX_ABLETON_LINK_LIB` pointing at the vendored lib:

| | |
|---|---|
| verdict | `VERIFY: PASS`, process exit 0 |
| steps | **passed=17 failed=0 blocked_external=0** — the first run in this repository's recorded history with no blocked step (`evidence/verify.json`, `gitHash cf95a2f`, `gitDirty true`) |
| `external` | `{ndiRuntime: true, abletonLinkLib: true, spoutShim: true}` |
| `link-integration` | ran, exit 0, 6 passed — the gate's first execution; it is the 14th invocation in this lane's 13-plus-harness tally and the 14th was clean |
| `spout-enumeration` | ran with `spout shim resolved: D:\Projects\visual\genfx\apps\spout-bridge\spout_shim.dll` printed before the step, exit 0, no skip line |
| `ndi-runtime-loads` | unchanged, exit 0 |

`0 blocked_external` is a statement about this host with two vendored artifacts present, not
about CI: `ci.yml` sets neither variable *and* neither artifact is in a checkout — `_upstream/`
is `.gitignore:4` and `apps/spout-bridge/*.dll` is `.gitignore:38` — so a CI run keeps
`link-integration` and `spout-enumeration` blocked. That is reasoned from `.gitignore`, not
measured: no CI run has been observed since these gates changed. The asymmetry is what the
`external` block exists to make visible.

### Not changed here

- Seven branches still print "X is not built into this binary" and return `Ok` in a feature-off
  build (`crates/app/src/main.rs:531`, `:592`, `:689`, `:746`, `:810`, `:843`, `:880`). Same
  family as the `cmd_spout` repair, left because whether "this binary cannot" is a failure or a
  usage answer is one decision for all seven, and `flux ndi`'s exit codes are what D023 measured.
- `live::protocol_status()` keeps `available: cfg!(all(windows, feature = "spout"))` and reason
  strings like "Spout2 SDK linked" (`crates/integrations/src/live.rs:70-95`). Under the contract
  above that is correct: the field describes the binary. It is also the field whose output
  `capability_report()` copies for `link` and `syphon` `available`.
- The Link flake: no test edited, no adapter changed, no re-run-until-green.
- `crates/xtask/src/bin/requirements.rs` untouched *by this lane*. No arm there claimed spout or link
  `verified`, so the ledger's statuses needed no change; what is wrong there is `evidence_paths`
  rotation — Max bridge evidence on `AT057 | AT058 | AT059`, which `spec/VERIFICATION.md:88-90`
  assigns to a live take, Spout and Syphon, and Spout evidence on AT061/AT062, which `:92-93`
  assigns to Max. D028 repaired that the same day, and found AT057 in the same arm — this bullet
  named four records and the defect had five.
- `spout-registry.txt`, `spout-send-dx.txt`, `link-a.txt`, `link-b.txt` stay as they are:
  measurements of 2026-11-09, now superseded rather than edited.
- `--features ndi` was not re-run this lane; the ndi row's `true` still rests on this morning's
  transcript, and `--features link` cannot be produced by a fresh clone at all.


## D028 — 2026-11-15 the generated ledger was citing evidence for somebody else's acceptance test

D027's "Not changed here" closed by pointing forward: `requirements.rs` was untouched, and what
was wrong there was `evidence_paths` rotation — "Max bridge evidence on AT058/AT059 … and Spout
evidence on AT061/AT062". That note named four records. The defect had five, and the fifth is not
a transport at all.

### What the file said

`implementation/requirements.json` is generated, so these strings are code — `at_status()` in
`crates/xtask/src/bin/requirements.rs`. Two `match` arms each covered several acceptance tests:

| record | what `spec/VERIFICATION.md` actually asks for | what its `evidence_paths` cited |
|---|---|---|
| AT057 (`:88`) | live source → take → offline export, timestamps surviving relink | Max bridge tests, `check-maxpat.cjs` |
| AT058 (`:89`) | actual Windows **Spout** send/receive with an independent consumer | Max bridge tests, `check-maxpat.cjs` |
| AT059 (`:90`) | actual macOS **Syphon** send/receive | Max bridge tests, `check-maxpat.cjs` |
| AT061 (`:92`) | **Max `.maxpat`** structure plus real OSC bytes interoperating | `apps/spout-bridge`, `spout-registry.txt` |
| AT062 (`:93`) | a real packaged **`.amxd`** loaded in Live | `apps/spout-bridge`, `spout-registry.txt` |

So the ledger told a reader checking whether Max OSC bytes interoperate that a Spout sender
registry was the evidence, and told a reader checking Syphon that a Max patch structure was the
evidence. AT060 (NDI) sat between them with correct evidence, which is what kept the shape from
being obvious: the arms were grouped by *adjacent numbering*, not by shared subject. Grouping is
fine where the ACs are siblings — AT067–AT070 are four clauses of the export matrix and stay one
arm, as do AT071/AT072 and AT049/AT050 — and silent everywhere else.

### What the records say now

- **AT057** — capture enumeration (`capture.txt`), the persistence relink tests and the export
  matrix, with a blocker saying plainly that no live source has ever been taken into an export and
  the timestamps the AC aligns are the fixture's.
- **AT058** — `apps/spout-bridge` + `crates/integrations/src/spout.rs`, `spout-send-dx.txt`
  (83 frames through a real D3D11 sender) and `spout-2026-11-15.txt` (bounded enumeration answers
  **0 senders**). Blocker: no independent consumer has confirmed a Flux frame, Flux's enumerator
  does not see its own DX sender, and `capability_report()` returns `verified=false` in every build.
- **AT059** — `blocked_external`, not `implemented_unverified`. There is no Syphon adapter:
  `crates/integrations/Cargo.toml:29` is `syphon = []`, a flag with no code behind it (grep of the
  whole tree finds the string only in that flag, the capability row and the protocol-status row).
  `implemented_unverified` claims something was built.
- **AT061** — the 6 bridge tests, `check-maxpat.cjs` and the 7 `control.rs` UDP loopback tests,
  with a blocker naming the actual gap: those bytes are Flux's own encoder meeting Flux's own
  listener, and `multiple instances` is asserted in-process by the epoch test.
- **AT062** — `blocked_external`: no `.amxd` exists, so every clause is unexercised.
- **AT063** — status unchanged, blocker falsified. It said "no `FLUX_ABLETON_LINK_LIB` build on
  this host today", which D027 made untrue that afternoon; it now says the tests hold both
  sessions in one process, that the step is not run-independent (2 of 13 failed at
  `crates/integrations/src/link.rs:203`), and `link-2026-11-15.txt` is cited as evidence.

`spec/TASKS.md:302` asks a `blocked_external` record to carry the missing item *and* the smallest
command that would resume verification, so both new blockers name a resume step.

### Guard

`mod evidence_routing` (`crates/xtask/src/bin/requirements.rs:528-571`) — 2 tests. One asserts,
per record, that its evidence contains its own subject and none of the neighbouring protocols';
the other pins AT059/AT062 to `blocked_external`. A string test is the right shape here because a
grouped arm cannot fail: nothing observes *which* record a string was written for. The assertions
are not vacuous — the committed copy of the generator (`git show HEAD:...`, saved to
`target/req.old.rs`) still contains both grouped arms, and each `must_not` token is present in the
arm it forbids: `check-maxpat` inside `"AT057" | "AT058" | "AT059"`, `spout` inside
`"AT061" | "AT062"`. That is a check against the old source text, not a run of the test on it.

### Citation churn

The block grew by 3 lines (`git diff --numstat` = 23/20 on a generated file), which moved every
ledger citation below it. Re-resolved by a script that finds the enclosing `"id"` for each cited
line in both revisions rather than by eye: 22 citations, **13 pointing at the wrong record** after
regeneration, 0 now. Two of them were continuation-style — the pairs `3816, 3826` and `4017, 4027`
in the old numbering — where only the first matches a `file:NNN` regex, so a shift that moves one
number and not the other leaves the second pointing into the previous record. Both moved too (→3829, →4030).
Four citation sites into the generator itself — `requirements.rs:374`, `:396`, `:441`, `:445`,
`:453` across `DECISIONS.md:825`, `docs/PERFORMANCE-READINESS.md:245`, `:426` and `STATE.json:134`
— moved to `:393`/`:424`/`:472`/`:494`/`:512`.

### Measured at this tree

- `cargo fmt --all -- --check` exit 0.
- `cargo test --workspace -- --test-threads=1` exit 0 — **207 passed / 0 failed / 0 ignored**
  across the same 40 result lines (29 test binaries + 11 doctests): D027's 205 plus the 2 routing
  tests, which landed in the xtask bin target that used to report `running 0 tests`.
- `cargo run -q -p xtask --bin requirements` exit 0, own status, un-piped. The regeneration diff is
  the AT057–AT063 block and nothing else.
- `infrastructure/verify.ps1` was **not** re-run for this change. The 17/0/0 figure quoted elsewhere
  is D027's run at `cf95a2f`; this lane touched only the generator and the docs, and no step in that
  harness runs `xtask --bin requirements` — which is the gap noted below.

### Not changed here

- **Nothing regenerates or freshness-checks the ledger.** `ci.yml` runs `catalog`, `doctor`,
  `test-gpu`, `export_shaders` and `fixtures` (`:35`–`:45`); `verify.ps1` has zero matches for
  `requirements`. So a hand-edit of `requirements.json` survives CI silently and the statuses that
  D025 and D028 fixed are only as current as the last manual run. Patched as documentation, not as
  a step: adding it to one harness and not the other is the divergence AGENTS.md forbids.
- `docs/PERFORMANCE-READINESS.md:22-23` insists a Max GUI is *not* external ("he has Live 12")
  while `BLOCKERS.md:36-40` makes `.amxd` depend on Max's Save-As dialog. AT062 has to sit on one
  side of that, and the ladder (`spec/START-HERE.md:93`) has no status for "blocked on a human
  clicking Save". `blocked_external` is the closest legal label, not the right one — flagged, not
  resolved.
- AT054–AT056 keep citing `capture.txt` for three different ACs, which is one transcript doing the
  work of three clauses; that is thin, but it is at least the same subject.
- No `spec/VERIFICATION.md` row was re-run here. The spec text was read to find out what each
  record is *about*; every status string still rests on evidence captured in earlier lanes.
- D025's citation paragraph (`:832`) names two numbers — "`:3655` for AT050, `:3729` for AT059" —
  that resolve to neither record in the committed file or this one: 3655 sits inside AT052 and 3729
  inside AT058's blocker, while AT050's `id` is at `:3620`. No live document cites either number, so
  it is a stale note in a historical record rather than a broken pointer; the record is left as
  written. Whether those were pre-fix or post-fix values is not recoverable from this tree.

## D029 — 2026-11-15 one of the two "keyed on the machine" gates was still keyed on a pointer

This lane exists because a sentence in D027's own summary turned out to be true of one gate and
not the other, and the way to find that out was to run the harness from a shell that had not been
prepared for it.

### The measurement that produced it

`infrastructure/verify.ps1` (full, no `-Quick`) at `7ecc1f1`, working tree clean, with
`FLUX_ABLETON_LINK_LIB` unset in the invoking shell: `VERIFY: PASS`, exit 0, **passed=16
failed=0 blocked_external=1**, `external = {ndiRuntime: true, abletonLinkLib: false, spoutShim:
true}`. The blocked step was `link-integration`, on a host that holds
`_upstream/link/build/Release/abl_link.lib`. The same script minutes earlier, from a shell where
that directory had been exported, recorded **17 / 0 / 0** with `gitHash 7ecc1f1` and
`gitDirty false`.

Same machine, same commit, same tree: one variable decides whether a gate runs. That is the defect
D027 says it closed — AGENTS.md's bullet ("both gates were *decided by an environment variable*
rather than by the machine … They now test for the artifacts") is exact about Spout and wrong about
Link. `Test-LinkLib` tested for `abl_link.lib` **inside the directory the variable names**, so the
variable remained load-bearing: D027 could only upgrade "set but pointing nowhere" from a gate run
to a blocker, which is real but is not the same claim as testing the machine.

### What changed, and why testing was never enough

`crates/integrations/build.rs:16` panics unless `FLUX_ABLETON_LINK_LIB` names a directory, so a
gate that merely *knows* the library is present cannot hand it to cargo. The fix is what
`Resolve-SpoutShim` already does for the shim: return a path and export it. `Resolve-LinkLib`
(`infrastructure/verify.ps1:81`) tries the variable's directory, then
`_upstream/link/build/Release`, `_upstream/link/build/Debug` and `link/build/Release` — the
candidate-list shape `Test-NdiRuntime` already uses — and the gate assigns the answer to
`$env:FLUX_ABLETON_LINK_LIB` before the step, printing `abl_link.lib resolved: <dir>` so the run
says which candidate it used. `external.abletonLinkLib` now reports the resolved directory rather
than re-running the search. `Test-LinkLib` is deleted: nothing calls a boolean that a path
variable already answers.

A fresh clone is unaffected: none of those candidates exist there, so `link-integration` and
`spout-enumeration` stay `blocked_external` and CI still exits 0 with 2 blocked steps. The change
moves the decision from the operator's shell to the filesystem, which is the only place a
prerequisite can be true.

### The guard is behavioural, because a .ps1 has no test surface

`verify.ps1` has no unit tests — the parse check is the whole static surface (`parse_errors=0`),
so the guard had to be a run: full harness, at the commit that carries the change, from a shell with
`FLUX_ABLETON_LINK_LIB` explicitly removed from its environment. That ran at `7235afd`:
`VERIFY: PASS`, exit 0, **passed=17 failed=0 blocked_external=0**, `external` all three true,
`gitHash 7235afd` with `gitDirty false`, and the printed line `abl_link.lib resolved:
D:\Projects\visual\genfx\_upstream\link\build\Release` — the candidate list, not the operator,
decided that step. `evidence/verify.json` now carries it. The pointer-keyed before-state survives
only in scratch logs (`target/verify-7ecc1f1.log` and siblings), which is why the 16-0-1 figure is
quoted in this record rather than stored beside it.

What that run also changed is the flake tally. `link::` has now executed 16 times today: the 13
invocations D027 counted (11 clean, 2 red), plus the harness run at `cf95a2f`, plus the two at
`7ecc1f1` and `7235afd` — and `set_playing_reflected_in_state` passed in all three harness
executions. Three consecutive clean runs is not a mechanism and not a fix; the assertion at
`crates/integrations/src/link.rs:203` is still untouched, still not wired into CI, and still the
only known red in the suite. It stays open with its rate attached: 2 failures in 16, both at the
same line, none of them in a run where the gate had to find its own library.

### Citation churn

13 lines inserted at `:74` moved everything below it in the same file: `Resolve-SpoutShim`
`:90`→`:99`, the `external` block `:187-189`→`:200-202`, and `Test-LinkLib` `:77` disappears.
Re-resolved in `implementation/DECISIONS.md:1103`, `:1125`, `:1136` and
`docs/PERFORMANCE-READINESS.md:1073`; `:32`, `:35` and `:47-61` are above the insertion and were
checked, not assumed.

### Not changed here

- The `passed=17 failed=0 blocked_external=0` figure quoted across README, AGENTS.md, STATE.json
  and FINAL-REPORT.md now reads without a caveat, because the bare-shell run above is what produces
  it. The shell-dependence stays in those sentences as history with its number attached — 16-0-1 at
  `7ecc1f1` — rather than being deleted, because a reader who runs an older checkout meets it.
- `gitDirty false` at `7ecc1f1` closes the half of D026 that stopped at the mechanism: the
  excluded pathspec is now measured at a commit that contains it, on a tree that was clean before
  the run, with the source change committed after. No claim in that record needed editing.
- The seven feature-off CLI branches that print and return `Ok` are untouched, and the
  requirements-ledger freshness gate D028 documented is still a gap in both harnesses.
