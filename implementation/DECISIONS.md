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
"cargo xtask report (requirements)"`, writer `crates/xtask/src/bin/requirements.rs:584`, was `:174` until D032 separated generating from writing), and its
AT060 row asserted `verified` because a `match` arm three hundred lines earlier says so.

### Why editing the JSON would have been the wrong repair

Two acceptance records claimed more than their evidence, in the same way the NDI step did:

| record | at HEAD | after | the gap |
|---|---|---|---|
| AT060 — "Actual NDI official SDK/runtime send/receive loopback; frame/audio release counts, stride/timestamps, missing-runtime behavior" | `verified` | `implemented_unverified` + blocker | one of four clauses is measured (loopback, re-measured today); release counts, stride/timestamps and the missing-runtime path on a host without NDI are not, and there is still no wgpu→NDI producer |
| AT063 — "Max/conditional Link transport; single master, seek/loop/start-stop behavior, tempo phase aligned, no control-as-audio confusion" | `verified` | `implemented_unverified` + blocker | `crates/integrations/src/link.rs:288` (then `:249`) does assert single-master arbitration, but only inside a `--features link` build, and the AC's Max-transport and seek/loop clauses are asserted by nothing — which is exactly what `BLOCKERS.md:26-35` and `:54-55` had already said. The ledger was the outlier, not the blockers file. |

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
  against current lines (`main.rs:714`/`:571`/`:453`, `live.rs:37-41`, `STATE.json:85-86`,
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
`ErrorCode::SdkMissing` (`crates/app/src/main.rs:857-863`, was `:821-827` until D036), measured exit 1 from
`target/debug`, while an empty registry stays exit 0 because finding nothing is an answer.

`verify.json` gained `external = { ndiRuntime, abletonLinkLib, spoutShim }`
(`infrastructure/verify.ps1:206-208`, was `:200-202` until D032 inserted a step above it) so a run states which prerequisites it could reach. The
step list alone cannot: a blocked step and an absent SDK look identical, and a step that ran
because a vendored library happened to be on the machine is not a step that ran on CI.

### What running the Link gate for the first time found

13 invocations of `cargo test -p flux-integrations --features link link::` today: **11 clean, 2
failures**, both `set_playing_reflected_in_state` at `cf95a2f`'s `crates/integrations/src/link.rs:203`
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
| `implementation/evidence/` | `spout-2026-11-15.txt`, `link-2026-11-15.txt` added (hand transcripts; both name `gitHash cf95a2f` in a `# provenance (F1)` header, and both read `unattributed` in the manifest then, because `cmd_evidence` only extracted provenance from `.json` (D031 gave a typed header its own tier) — see `crates/xtask/src/main.rs:183-193`, `crates/xtask/src/main.rs:244-254` since D031) |
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
  [done: D036 made it that one decision and closed the family, drawing the line at the apparatus rather
  than the outcome. The count here was a copy-forward miscount that every later lane repeated: the
  compiled-out branches are nine, and with eight feature-on arms plus `cmd_caps` the class is seventeen
  sites. Those seven `main.rs` numbers are the pre-D036 lines; D036's table maps each to where it is
  now.]
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
  `crates/integrations/src/link.rs:203` as it then was), and `link-2026-11-15.txt` is cited as evidence.

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
  harness then ran `xtask --bin requirements` — which is the gap noted below, closed by D032.

### Not changed here

- **Nothing regenerated or freshness-checked the ledger** — the gap D032 closed, in both harnesses at
  once rather than one and not the other. `ci.yml` then ran `catalog`, `doctor`, `test-gpu`,
  `export_shaders` and `fixtures` (`:35`–`:41`, `:50` since D032 inserted a step above that last
  `export_shaders` and `fixtures` (`:35`–`:41`, `:50`; D032 inserted a step between the last two),
  and `verify.ps1` had zero matches for `requirements`. So a hand-edit of `requirements.json`
  survived CI silently, and the statuses D025 and D028 fixed were only as current as the last manual
  run. D028 patched prose, not a step — one harness alone is the divergence AGENTS.md forbids.
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
`spout-enumeration` stay `blocked_external`. D039 corrects the CI half of this sentence: CI has no
blocked steps — its SDK legs are jobs that clone their own sources, measured failing on a stale
toolchain path, not blocked. The change moves the decision to the filesystem, where it can be true.

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

What that run also changed is the flake tally. `link::` had by then executed 16 times today: the 13
invocations D027 counted (11 clean, 2 red), plus the harness run at `cf95a2f`, plus the two at
`7ecc1f1` and `7235afd` — and `set_playing_reflected_in_state` passed in all three harness
executions. Three consecutive clean runs is not a mechanism and not a fix, so D029 left the
assertion at `crates/integrations/src/link.rs:203` untouched, with its rate attached: 2 failures in
16, both at the same line, none of them in a run where the gate had to find its own library. That is
the state D030 inherited, and the record below is what D030 did with it.

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
- The seven feature-off CLI branches that print and return `Ok` are untouched, and the ledger had no
  freshness gate. [D032 closed that: `verify.ps1:128` plus a matching `ci.yml` step, both harnesses.]
  [D036 closed the other half, and corrected the count: nine compiled-out branches, seventeen sites in
  the class. Both sentences above — this one and D027's — repeated D027's miscount for four lanes.]

## D030 — 2026-11-15 the Link flake had a mechanism, and the test was asserting a property Link does not deliver

**Decision:** `set_playing_reflected_in_state` stops asserting that a commit is visible in the
capture taken immediately after it, and instead waits, within a 250 ms bound, for the state to
converge. The assertion is not deleted, not marked `#[ignore]`, and not given a longer deadline
than the measurement justifies. The mechanism is written down as what was read out of the vendored
SDK, with the parts that remain inferred named as such. Transcript:
`implementation/evidence/link-flake-2026-11-15.txt`.

### Why this was still open when D029 closed

D027 executed the Link gate for the first time and found 2 red in 13 runs; D029 re-counted it at 2
red in 16 and refused to touch it, correctly, because "three consecutive clean runs is not a
mechanism and not a fix". What was missing was not courage but data: without knowing *why* the read
was stale, any change to the assertion would have been a guess dressed as a repair. Four cheap
experiments supplied it, all against `f49bb20`:

| # | what ran | what came back |
|---|---|---|
| 1 | 20 group runs of `link::` | 18 clean, 2 red (runs 12 and 19) — reproduces D027's rate |
| 2 | 20 runs of only `set_playing_reflected_in_state` | 19 clean, 1 red — so the group, the mutex and the previous test's sessions are all exonerated |
| 3 | 24 iterations: commit stop, read 20 ms later | 24 of 24 already stopped, `visible_after_ms=Some(0)`, 0 stuck after 2.6 s |
| 4 | 40 iterations in the test's own back-to-back shape | 1 red after the stop, recovering on the first re-poll (`recovery_ms=Some(0)`), **and 1 red in the other direction** (`playing_after_start=false`); `peers=0` throughout |

Measured, therefore: the window needs the read to be back-to-back with the commit; it is ~2.5-10 %
depending on which sample is counted; when it hits, the next read is already correct; the committed
value is never lost; and it goes wrong in both directions. `Ableton Live 12 Suite` was running on
the host and bound nothing on UDP 20806, so no external peer is in any of those numbers.

### What the SDK does, and why D027's guess was the wrong guess

`abl_link_commit_app_session_state` lands in `Controller::setClientState`
(`_upstream/link/include/ableton/link/Controller.hpp:207-226`), which updates the shared client
state synchronously under a mutex and then hands *the same* `IncomingClientState` to the IO thread.
On that thread `handleClientState` reaches :443-459, and :457 writes
`currentClientState.startStopState = *clientState.startStopState` with no timestamp preference —
unlike the synchronous path at :219-223 and `handleRtClientState` at :472-491, both of which first
call `selectPreferredStartStopState` (:85-92), which keeps the incoming value only if its timestamp
is strictly greater. Two commits issued inside one IO-thread cycle therefore let the older
announcement land after the newer write and regress it for as long as it takes the newer handler to
run. Read from the code, and it explains the symmetry of experiment 4, which D027's
quantum-boundary hypothesis (`link-2026-11-15.txt:46-48`) cannot: this adapter commits start and
stop through one identical call, so a start-only alignment rule could not produce a start-direction
miss.

Reasoned, not measured: that the regression is specifically this interleaving. No timestamp was
logged from inside the SDK and upstream was not modified. Stated as the best-supported explanation,
not as a demonstrated one.

### The change, and what it still fails on

`crates/integrations/src/link.rs:200-234` adds `PLAY_VISIBLE_MS = 250` and `wait_playing(s, want,
ms)` returning `Some(elapsed)` or `None`; both directions of the test use it, and
`start_stop_propagates_to_peer`'s read of its own initiator (:336-339) no longer asserts instantly
either. A seventh test, `wait_playing_reports_a_timeout_when_nothing_changes` (:235-243), commits
nothing and requires `None` — without it, a helper that always returned `Some` would leave every
other claim in this record untested. The bound is load-bearing by measurement rather than by
argument: commenting the commit call out of `set_playing` (:140) turns the gate red with
`start never visible within 250 ms` and exit 101, which is the negative control a repair whose
whole content is "wait instead of assert" owes the reader.

After it: 30 group runs clean, 20 isolated runs clean, 52 consecutive clean invocations in total
against 18/20 and 19/20 before. `cargo test --workspace -- --test-threads=1` is still 207 passed /
0 failed / exit 0 across 40 result lines, because `link.rs` is `#![cfg(feature = "link")]` whole and
none of its assertions ever entered that count.

### What this does not buy

- The flake was never the transport, so nothing about Link got more reliable. What changed is that
  the suite no longer asserts immediacy, which the SDK does not guarantee. A UI that reads
  `state()` on the line after its own start/stop click can still see the previous value for a few
  milliseconds. Nothing in Flux does that today — the CLI prints on a timer, the desktop re-pulls a
  snapshot — so it is recorded as a property of the adapter rather than papered over as a green
  suite.
- 52 clean runs against a ~5 % base rate is roughly 2.6 expected pre-fix failures, so the clean
  tally is consistent with the fix and is not proof of it. The proof used here is the control.
- The step is still not a CI step: `_upstream/` is gitignored, so a fresh clone cannot build the
  feature and records `blocked_external` (D027's rule, unchanged by this).
- AT063 is still `implemented_unverified` and its blocker still says so: single-process peers, no
  real Live host, Max-transport and seek/loop asserted by nothing. D030 removes a defect from the
  gate, not a clause from the acceptance test.

### Files changed

| file | change |
|---|---|
| `crates/integrations/src/link.rs` | +45/-3: `PLAY_VISIBLE_MS`, `wait_playing`, both directions bounded, the initiator read bounded, +1 timeout guard test |
| `crates/xtask/src/bin/requirements.rs` | AT063 evidence and blocker rewritten against the measurement, line-count neutral so `evidence_routing` stays at :528-571 |
| `implementation/requirements.json` | regenerated; the diff is the AT063 record and nothing else (3 lines each way) |
| `implementation/evidence/link-flake-2026-11-15.txt` | new transcript: rates, probes, the SDK reading, the negative control, the limits |
| `implementation/BLOCKERS.md`, `UPSTREAM.md`, `FINAL-REPORT.md`, `STATE.json`, `docs/PERFORMANCE-READINESS.md` | the flake's sentence in each, rewritten; `link.rs:249` to `:288` wherever a row points at live code |

### Citation churn

`link.rs` grew net 42 lines (+45/-3, of which rustfmt's reflow of three `assert!` macros is 9), so
every citation into it below the first insertion moved by +39 and below the second by +42. The one
citation below the second is `single_master_tempo_wins_across_peers`, `:249` to `:288`, re-resolved
in `docs/PERFORMANCE-READINESS.md:240`, `implementation/BLOCKERS.md:33` and `:55`, `implementation/STATE.json:82`
and here at :815 (marked "then `:249`"). The pre-fix `:203` became a historical line number,
qualified "as it then stood" or pinned to `cf95a2f` wherever a record keeps quoting it — here at
:1143, :1260 and :1379, in `FINAL-REPORT.md` (whose own 6→7 counts at :228, :238 and :252 were this
lane's drift, now fixed), in the ledger's AT063 row and in `UPSTREAM.md`. Here at :919 "AT063's six
tests" stays: it is D027's count of the day it ran. Two numbers were checked and not touched —
`link.rs:131-143` (`set_playing`) and `link.rs:167` (`LINK_NET`) both sit above the insertions.
`requirements.rs` and `requirements.json` were kept line-count neutral by construction, so `:424`,
`:512`, `:528-571` and `requirements.json:3785` were re-checked, not assumed. No captured
transcript was rewritten: `link-2026-11-15.txt` still says `:203`, because that is where the line was.

### Not changed here

- The scratch probe (`crates/integrations/tests/link_stop_probe.rs`) is deleted, not promoted: its
  durable half is sections 1-2 of the transcript, and a test that prints instead of asserting is
  not a gate.
- Upstream is untouched. Patching `Controller.hpp` would fix the staleness rather than the claim
  about it, and this repo wraps upstream, it does not fork it.
- `wait_peers` and the two 5 s cross-peer deadlines stay as they were: they wait on discovery,
  which is a different clock and a different failure.

### Closing measurements

The 52 clean runs above are the flake experiment, not the harness. The lane closed with the
harness run the way a first-time user would meet it: `infrastructure/verify.ps1` full, at
`519c286`, tree clean before the run, `FLUX_ABLETON_LINK_LIB` never exported into the shell. It
printed `abl_link.lib resolved: D:\Projects\visual\genfx\_upstream\link\build\Release`, ran the
`link-integration` step as seven assertions and seven oks, and finished `VERIFY: PASS` / exit 0 /
**17 passed, 0 failed, 0 blocked_external**, with `verify.json` carrying `gitHash 519c286` and
`gitDirty false`. That is D029's resolver earning its keep: the gate is now keyed on a file on
disk, so the same command from a different shell gives the same verdict.

`cargo run -q -p xtask -- evidence` then exit 0 restamped `manifest.json`: **32 files, 9 name their
commit, 23 unattributed, 0 missing provenance**. The transcript this decision cites is the +1 file
and the +1 unattributed, and it is the fourth of its kind: `link-flake-2026-11-15.txt` names
`gitHash f49bb20` in its own `# provenance (F1)` header and still cannot be credited, because
`cmd_evidence` extracts provenance from `.json` only (`crates/xtask/src/main.rs:183-193`, `crates/xtask/src/main.rs:244-254` since D031). Lane 13
counted three such files; the count it recorded was right then and is four now. [D031 then read those headers into `header_other_commit`, and 23 unattributed became 19.] Fixing the
extractor to read a `# provenance` line out of text transcripts is a one-function change to a
writer other lanes own, so it stays on the open list rather than being done under cover of a Link
commit.

## D031 — 2026-11-15 four transcripts named a commit the manifest swore they did not

### The defect

`implementation/evidence/manifest.json` carries a `notes` array, and until this commit one of its
lines read: *"attribution=unattributed means nothing inside the file names a commit."* For four of
the files it was labelling `unattributed` — `ndi-2026-11-15.txt`, `spout-2026-11-15.txt`,
`link-2026-11-15.txt`, `link-flake-2026-11-15.txt` — the first line of the file is
`# provenance (F1): gitHash <sha>`. The count was not wrong; the sentence about what the count
means was, and a false explanation inside generated evidence is worse than a missing one because
it reads like a policy.

Lane 16 found this, wrote the number down (four, not three) and left the code alone rather than
changing an evidence writer under cover of a Link commit. This is that change, on its own.

### The choice, and the two wrong answers

The tempting ones. Fold the four into `stamped_current` / `stamped_other_commit` and the manifest
looks healthier — but `stamped_*` means *this crate wrote that SHA into the file while producing
it*, which is a guarantee a hand-typed line does not carry. Or leave the tiers alone and put the
exception in prose, which is exactly where the false sentence already was. Both were refused.

What shipped is a third strength plus a field that names the strength:

| `attribution` | `provenanceSource` | means |
|---|---|---|
| `stamped_current` / `stamped_other_commit` | `writer` | `write_evidence()` injected the SHA as the gate ran |
| `header_current` / `header_other_commit` | `header` | the file's own `# provenance` line names a SHA; a human typed it and nothing checked it |
| `unattributed` | `none` | nothing inside the file names a commit — now actually true |
| `no_provenance` | `none` | a file this crate writes named nothing: a bug, and the command exits 1 on it |

Two bounds are part of the contract, not accidents. Only `.txt` and `.md` are scanned: a PNG has
no business matching a hex-shaped byte run. And the scan stops at line 24, because a SHA in the middle of a transcript is usually a citation of somebody else's commit; it also takes the first
plausible token on the earliest line that matches, which is why `spout-2026-11-15.txt` reports `cf95a2f`
and not the 16-hex id printed further down its body. A short
HEAD also matches a full SHA in the file, so a transcript that quotes `gitHashFull` does not get
filed under another commit.

### Measured

- `cargo fmt --all -- --check` exit 0.
- `cargo test --workspace -- --test-threads=1` exit 0 — **214 passed / 0 failed / 0 ignored across
  the same 40 result lines**. Seven new tests, none of them a new result line: they are the first
  unit tests in `crates/xtask/src/main.rs`, whose bin target already appeared in that count as
  `running 0 tests`.
- `cargo run -q -p xtask -- evidence` exit 0 — **32 files, 9 stamped by their writer, 4 naming a
  commit in a header, 19 unattributed, 0 missing provenance**. All four header rows read
  `header_other_commit`, which is correct rather than flattering: a transcript is captured at a
  commit and committed afterwards, or not at all.
- Non-vacuity, by mutation and not by argument. Dropping `.take(24)` from `header_git_hash` reddens
  `stops_at_the_top_of_the_file` (1 failed / 6 passed). Making the header tier return `stamped_*`
  reddens `a_writer_stamp_and_a_typed_header_stay_apart` and `a_full_sha_matches_a_short_head`
  (2 failed / 5 passed). Both restored; `grep -c MUTATION` on the file is 0 and the seven are green.
- The full CI-parity harness re-ran at `f867b72` itself — clean tree, `FLUX_ABLETON_LINK_LIB`
  removed with `env -u` — and measured `VERIFY: PASS` / exit 0 / **passed=17 failed=0
  blocked_external=0**; `verify.json` carries `gitHash f867b72` with `gitDirty false`, and the
  `link-integration` step printed `abl_link.lib resolved: _upstream/link/build/Release` and executed
  all 7 tests ok, the D030 poll-guard among them. `cargo run -q -p xtask -- evidence` (exit 0) then
  restamped the manifest at that head — so the tier split above is what it measured at its own
  commit, not a stamp inherited from its parent.

### Not measured

Nothing can verify a hand-typed SHA after the fact — that is the content of the weaker tier, not a
TODO. Whether the four headers are *accurate* is a claim about their authors, and `ndi` / `spout` /
`link` were captured by lanes that recorded their own dirty state, which the reader can see in each
file. The 19 files that name nothing still name nothing; re-capturing them is F2/F4/G capture work
owned elsewhere.

### Files changed

| File | What moved |
|---|---|
| `crates/xtask/src/main.rs` | `header_git_hash`, `attribute`, the two-tier loop body, `provenanceSource` in each row, `header_named` in `counts`, the corrected `notes`, the new print line, and `mod evidence_provenance` (7 tests) |
| `implementation/evidence/manifest.json` | regenerated: the false note is gone, four rows moved to the header tier (a follow-up, `142f7e8`, then tightened the surviving note: it still called the remaining unattributed files "transcripts" when four are PNG captures and one is `diagnostics.json`) |
| `implementation/STATE.json` | `test_totals.measured` → 214 with the reason; `evidence_manifest` row carries the tier split; the lane-16 sentence about "cannot be credited" is marked superseded, not deleted |
| `implementation/DECISIONS.md` | this record; D027's and D030's citations of the moved extraction qualified |
| `docs/PERFORMANCE-READINESS.md` | the evidence row's attribution vocabulary, and a sixth entry in Test totals |
| `implementation/FINAL-REPORT.md`, `README.md` | 207 → 214, with what the +7 are |
| `docs/IMPLEMENTATION-ARCHITECTURE.md` | eight `xtask/src/main.rs` citations re-resolved (six were already mis-targeted since D024), the "two callers" of `read_preview` corrected to three, and the row kept at 888 lines |

### Citation churn

`cmd_evidence` grew by 61 lines above the extraction it used to do inline, so the block cited as `crates/xtask/src/main.rs:183-193` is `:244-254` now. All four citations (here at :1162 and
:1536, `STATE.json` `re_run_lanes` and `current_task`) were rewritten to name both, since the older
number is what those records saw. `docs/PERFORMANCE-READINESS.md` is where line-number citations
hurt most, so its evidence paragraph was reflowed inside its existing line budget and the file is
still 1076 lines; `README.md` (115), `FINAL-REPORT.md` (290), `STATE.json` (187) likewise.

`docs/IMPLEMENTATION-ARCHITECTURE.md` carried eight more `crates/xtask/src/main.rs` citations, and
grepping them turned up a defect older than D031: since D024 inserted `cmd_evidence` near the top of
the file they had been pointing into the wrong functions — the "catalog lints duplicate ids" note
read `:163-199`, which is `cmd_evidence`'s file walk, and the "test-gpu reports UNAVAILABLE" note read
`:242-287`, which is `cmd_evidence`'s tail and `cmd_doctor`. D031 shifted those targets a second time.
All eight are re-resolved here to the functions their prose names (`cmd_catalog:460-497`,
`cmd_test_gpu:539-585`, `cmd_doctor:348-392`, `cmd_fixtures:393-459`, `cmd_check:498-538`,
`BENCH_PROFILES:614-650`, `cmd_benchmark:806-962`, `cmd_package:1397-1524`), each range measured off
the function-start grep, not eyeballed. One citation was also a wrong *claim*: "`read_preview` has
exactly two callers — `tests.rs:164` and `main.rs:477`" — when `477` is `cmd_test_gpu` (which does not
call it) and the real callers are `tests.rs:164`, `main.rs:774` and `:1134` — three, none in the
shipped product, so the "no production caller" point stands. The file is kept at 888 lines so its own
`:661`/`:875` inbound citations (from `PERFORMANCE-READINESS.md:1076`) do not move.

### Not changed here

- `verify.ps1` and `ci.yml` still do not run `xtask evidence`, so the manifest is only as fresh as
  the last person who typed it. That is the same gap D028 recorded for `requirements.json`, and it
  wanted one decision covering both. [D032 made it: the ledger is byte-gated in both harnesses, the
  manifest is not, and its own notes say why — the second option, chosen rather than left open.]
- The `stamped` key keeps its old meaning (writer-injected only) rather than becoming
  stamped+header. A count that silently changes what it counts is how this lane started.

## D032 — 2026-11-15 one generated file is now freshness-gated, and the other is documented as ungatable

### The gap

Four lanes in a row recorded the same open item, and D031's "Not changed here" asked for one
decision covering both generated tracked files. `implementation/requirements.json` is written by
`cargo run -q -p xtask --bin requirements`, its statuses are produced by `at_status()` /
`at_blocker()` / `blocker_for()`, and neither `ci.yml` nor `infrastructure/verify.ps1` ran that
command — so a hand-edit of the ledger, or a status that drifted away from the arm supposed to
derive it, was invisible to every gate in the repo. The fix cannot be "run the generator in CI":
that rewrites a tracked file during a verification step, which is how a green job ends up shipping a
diff. The fix is a comparison that writes nothing.

### The two files are not the same kind of object, so they get different treatment

| file | what is in it | byte-gateable |
|---|---|---|
| `implementation/requirements.json` | a pure function of the tree: spec ids, catalog rows, statuses derived from paths that exist. No timestamp, no mtime, no git SHA; `generatedBy` is a fixed string. | **yes** — `--check`, now wired into both harnesses |
| `implementation/evidence/manifest.json` | a measurement of the run that wrote it: per-file mtimes, sha256 of four binaries, `gitHash`/`gitDirty`, and a row describing the manifest that existed *before* the writer ran. | **no** — by construction, not by oversight |

Gating the second would report drift on every second invocation of any gate with nothing broken, so
it is documented inside its own `notes` array instead: the note now says the file is deliberately
not freshness-gated, names the reason, and points at the command that gates the file that can be.
`xtask evidence` keeps the weaker check it already had — exit 1 only when an xtask-owned file was
written here without provenance.

### CRLF is not a hypothetical here, and that decided the shape of the comparison

`core.autocrlf=true` is not set by this repository — it comes from
`file:C:/Program Files/Git/etc/gitconfig`, the system gitconfig — and there is no `.gitattributes`.
Measured on this host: a fresh shallow clone checks `implementation/requirements.json` out with
4049 CR bytes, one per newline (the committed blob has 4049 newlines and no trailing one), while
this working tree has 0 because every writer in the tree emits LF, and `crates/xtask/src/main.rs`
clones at 1524 CR bytes for the same reason.
The generator emits LF. So a raw byte-compare would have called a clean checkout stale on the first
CI run, and the gate would have been switched off within a day. `--check` normalizes `\r\n` and
lone `\r` to `\n` on both sides before comparing, which is exactly the conversion git itself does on
commit — content is compared, encoding is not.

### What the gate says when it fails

Record-level, not a byte offset: `AT062.status: committed "implemented_unverified", generated
"blocked_external"`, up to 20 lines, plus the command that fixes it. Records present on only one
side are named, and if either side fails to parse — the worst case, a hand-edit that broke the JSON
— it falls back to the first differing line instead of panicking.

### Measured

- `cargo fmt --all -- --check` exit 0.
- `cargo test --workspace -- --test-threads=1` exit 0 — **218 passed / 0 failed / 0 ignored across
  the same 40 result lines**. The +4 are `mod freshness` in the `requirements` bin target, which
  already appeared in that count (it went from `running 2 tests` to `running 6`).
- `cargo run -q -p xtask --bin requirements -- --check` on the tree at HEAD: exit 0, prints
  `requirements: …\implementation/requirements.json is fresh`, writes nothing.
- Two real mutations, both against the committed ledger rather than a fixture. A status flipped to
  `implemented_unverified` → exit **1**, report named `AT062.status` with both values, and the check
  did not rewrite the file (the subsequent no-flag run printed `wrote …` and produced a file
  byte-identical to the pre-edit one — `cmp` exit 0, `git status` clean). The AT062 record then
  deleted outright → exit 1 and `AT062: in this tree, not in the committed file`, which is the
  one-sided-record branch exercised for real; the file was regenerated back to HEAD afterwards.
- `infrastructure/verify.ps1` with the new step: **`VERIFY: PASS`, exit 0, passed=18 failed=0
  blocked_external=0** — one step more than every run recorded before it. `verify.json` names
  `requirements-freshness` with `{ exit: 0, ok: true }`, its printed line is
  `requirements: …\implementation/requirements.json is fresh`, and the run resolved `abl_link.lib`
  from `_upstream` with `FLUX_ABLETON_LINK_LIB` and `FLUX_SPOUT_SHIM` both removed from the
  environment, `external` all true. Its `gitDirty true` is the self-stamp shape D026 documents: the
  uncommitted sources were this record and `STATE.json`, both writable only from that run's numbers.
- The same script re-run at `a91ebda`, the commit that contains those two files: **`VERIFY: PASS`,
  exit 0, passed=18 failed=0 blocked_external=0**, `verify.json` carrying `gitHash a91ebda` with
  `gitDirty false` — the gate's first run on a tree with nothing dirty outside
  `implementation/evidence/`. `cargo run -q -p xtask -- evidence` then exit 0 restamped the manifest
  at that same head: 32 files, 9 stamped by their writer (5 `stamped_current` — catalog, doctor,
  fixtures, test-gpu, verify — and 4 `stamped_other_commit`), 4 naming a commit in a header,
  19 unattributed, 0 missing provenance.
- Non-vacuity of the control: forcing `check_freshness()` to `return 0` at its first line left
  `committed_ledger_is_fresh` green and reddened exactly `one_status_flip_is_drift` (1 failed / 4
  passed — that probe ran on the five-test module, before the parse-fallback test below existed). A
  comparator that always says "fresh" therefore cannot pass the suite. Probe removed; all six are
  green.
- `a_broken_ledger_is_located_not_panicked` exercises the parse-failure fallback, so that branch is
  not dead code with a comment claiming it works.

### What this does not close

Two branches of `drift_lines` were reachable but not exercised when this was written. The reverse
one-sided arm — a record the committed file has and this tree no longer generates, printed as
`AT0xx: in the committed file, not in this tree` — was the same two-map code as the deletion that
had just been measured, in the other direction, and saying that was not the same as measuring it.
Neither was the `first differing line` fallback for sides that parse, are semantically equal and
still differ byte-wise. [D033 measured both, each with its own mutation control.]

No GitHub Actions run has been observed, so the `ci.yml` step is declared parity rather than
evidence: the two harnesses now invoke the same 18 checks, and only one of them has a record here.

The gate compares the artifact to its generator. It cannot compare either of them to reality: an
`at_status()` arm that derives a confident wrong status from a path that exists will regenerate the
same wrong bytes and pass. That is what `mod evidence_routing` is for, and it is a smaller claim
than "the ledger is true". Nothing in either harness runs `xtask evidence`, so `manifest.json` is
still only as fresh as the last person who typed it — now stated in the file itself rather than left
for a reader to infer.

### Files changed

| File | What moved |
|---|---|
| `crates/xtask/src/bin/requirements.rs` | `main()` split into `generate()` (returns the bytes) + `main()` (owns the disk); `normalize_newlines`, `check_freshness`, `drift_lines`, `records_by_id`, `union_keys`, `short`, `first_diff_line`; `mod freshness` (4 tests). 571 → 802 lines, all of it below the last cited line. |
| `crates/xtask/src/main.rs` | one existing `notes` element in `cmd_evidence` extended to state that `manifest.json` is deliberately not freshness-gated. Line count kept at 1524 so the eight citations D031 re-resolved did not move. |
| `infrastructure/verify.ps1` | new `Step 'requirements-freshness'` after `shaders-export` (218 lines) |
| `.github/workflows/ci.yml` | matching step, with the autocrlf reason in a comment above it |
| `implementation/evidence/` | the gate's own footprint: two harness runs rewrote `catalog`, `doctor`, `fixtures`, `test-gpu` and `verify.json` (the first at `3e4e825` with `gitDirty true`, the repeat at `a91ebda` with `false`) and `xtask evidence` restamped `manifest.json` at `a91ebda` [annotated by D036: no `verify.json` blob in any of this repo's 94 commits carries `gitHash 3e4e825`, and the tree committed at `a91ebda` still holds the 17-step `f867b72` file, so the "first at `3e4e825` with `gitDirty true`" names field values nobody can re-read — see D036's "The 18-0-0 tally, recounted from bytes"] |
| `AGENTS.md` | the command enters Canonical commands (it now *is* CI parity); the harness trap that said "nothing in `ci.yml` or `verify.ps1` runs that command, so a hand-edit survives CI silently" is rewritten to what is true now, and the 17-0-0 in the SDK-gate trap is put in the past tense with 18 named as the new ceiling |
| `implementation/DECISIONS.md` | this record; D025's writer citation re-pointed; D029's `verify.ps1:200-202` re-pointed to `:201-203`; D028's, D029's and D031's "Not changed here" bullets put in past tense and marked answered, not deleted |
| `implementation/STATE.json` | `test_totals.passed` was still **204** while its own `measured` prose said 214 — D031 updated the sentence and left the field a machine reads; both now say 218 with the +4, plus a lane-18 handover paragraph. Its `verify.ps1:35`/`:81` citations are above the inserted step and did not move. `at_head`, `counts`, `passing_steps`, `re_run_lanes`, the commands list and `known_gaps[13]` (D027's rotated-evidence gap, closed by D028 and gated by D032) are brought to the same state, and lanes 15/16's "both harnesses or neither" handovers are marked done. 187 lines kept |
| `docs/PERFORMANCE-READINESS.md` | a seventh Test-totals run inside the Test totals row, and the 18-step CI-parity run inside the CI row, both appended without adding a line. Held at 1076 lines, so its own inbound citations and the `IMPLEMENTATION-ARCHITECTURE.md:661`/`:875` targets do not move. |
| `docs/IMPLEMENTATION-ARCHITECTURE.md` | the catalog-is-authority sentence and the `xtask` bin inventory both name the comparison mode; 888 lines kept |
| `implementation/FINAL-REPORT.md`, `README.md` | 214 → 218 with what the +4 are, and the harness timeline carried from 17 steps to 18. FINAL-REPORT held at 290 lines so its inbound `:146`/`:151-152` citations do not move; README grew by two at 91-93, below every `README.md:NN` citation in the tree |

### Citation churn

The refactor was laid out to keep inbound line numbers valid: `generate()` occupies exactly the
span `main()` did (18-177), and `record`, `product_paths`, `blocker_for`, `task_status`,
`at_status` (290-465), `at_blocker` (472-526) and `mod evidence_routing` (528-571) were not touched,
so `requirements.rs:22-32`/`:26-32` (`IMPLEMENTATION-ARCHITECTURE.md:100`, `:330`), `:374`/`:393`/
`:396`/`:424`/`:445`/`:453`/`:472`/`:494`/`:512` (`DECISIONS.md:825`, `:1289`,
`PERFORMANCE-READINESS.md:245`, `:426`) and `:528-571` (`DECISIONS.md:1272`, `STATE.json:134`) are
all still exact. Three citations did move and all three are re-pointed rather than left:
`DECISIONS.md:805` (cites `requirements.rs:174`, the writer, now `:584`), `DECISIONS.md:1136` (cites
`verify.ps1:200-202`, shifted by one inserted step to `:201-203`) and D028's `ci.yml:35-45`, which
the inserted step split — those five commands were `:35`–`:41`/`:50` here and are `:42`–`:48`/`:50`
now; `ci.yml:29`/`:31` → `:42`/`:44` (D040). `verify.ps1:32`/`:35`/`:47-61`/`:77`/`:81`/`:99` hold.

Two files grew, both deliberately and both below every citation into them: `README.md` 115 → 117,
adding at 91-93 where the largest inbound reference is `README.md:60`, and `AGENTS.md` 197 → 198,
which nothing cites by line. `DECISIONS.md` grows only at 1302 and later, while every internal and
inbound reference into it sits at `:116`, `:805`, `:825`, `:1103`, `:1125`, `:1136`, `:1272` or
`:1289`. Two of those eight numbers are D036's, not this record's: its annotations at `:1196` and
`:1409` grew the file by seven lines above `:1196`, so `:1267` → `:1272` and `:1284` → `:1289`, and
the two pointers above were moved rather than left to rot the way `PERFORMANCE-READINESS.md:461` did.

## D033 — 2026-11-15 the two report shapes D032 reasoned are now measured, each with its own control

### The residue

D032 closed the freshness gap and then wrote, in its own "What this does not close", that two of
`drift_lines`' four report shapes were reachable but had no test: the reverse one-sided arm (a
record the committed ledger has and this tree no longer generates) and the fallback that fires when
both sides parse, mean the same thing and still are not the bytes the generator writes. The forward
arm had been exercised by hand — AT062 deleted from the file, exit 1, `AT062: in this tree, not in
the committed file` — and the fallback had never been reached at all: every mutation tried produced
a field-level difference, so `out` was never empty while the bytes differed. A branch that no
invocation has ever taken is not covered by a comment that describes it.

### Why the direction is not just symmetry

A stale ledger has two causes and the report is supposed to tell them apart. Adding a row to
`record()`/`at_status()` makes the committed file lag behind in one direction; deleting one — the
shape a reverted feature leaves behind — lags in the other. With only the forward arm tested, a
comparator that walked `cb` and forgot `ca` would pass the whole suite and go silent precisely when
a feature was removed, which is the failure this repository keeps hitting: a check that reports
nothing and is called a pass.

### What the two tests build

- `a_record_only_in_the_committed_file_is_named` parses the generated ledger, filters AT062 out of
  the `records` array, and hands the pair to `drift_lines` in the orientation a real `--check` uses:
  the full file on disk as committed, the shortened bytes as generated. It asserts the report is
  *exactly* one line — an absent record must not smear itself across the other 344 rows — and then
  that `check_freshness` returns 1 on that pair.
- `equal_meaning_in_different_order_reports_a_line` swaps two adjacent scalar field lines inside
  AT062's row. Both sides parse, `Value` equality says they are the same object, and the bytes are
  not. The assertion is on what the report says when no field moved: one entry,
  `first differing line is N`, with N the line the swap landed on.

### Treating a reordered ledger as drift is a decision, not an accident

The alternative was to compare parsed values and let key order through. Then the next
`xtask --bin requirements` run rewrites the order back, which is a tracked-file diff produced by a
verification step — the thing D032's whole design exists to avoid. So byte equality is the contract,
and the message gives a position rather than a guarantee, because the reader who has to fix a
reordered file needs to know where it diverges, not that something is wrong.

### Measured

- `cargo test -q -p xtask --bin requirements`: 8 passed / 0 failed — 6 of them in `mod freshness`.
- Two mutation controls, each against the real source rather than a fixture. Deleting the
  `ca`-minus-`cb` loop left **7 passed / 1 failed** and the failure was
  `a_record_only_in_the_committed_file_is_named`; deleting the `out.is_empty()` fallback left
  **7 passed / 1 failed** and the failure was `equal_meaning_in_different_order_reports_a_line`.
  Neither new test is decoration, and each names the branch it guards.
- `cargo fmt --all -- --check` exit 0, after rustfmt split two long `assert_*` calls; the file is
  870 lines, every added line below the last cited one.
- `cargo run -q -p xtask --bin requirements -- --check` exit 0 — the gate's verdict is unchanged,
  because no generator logic moved.
- `cargo test --workspace -- --test-threads=1` — exit 0, **220 passed / 0 failed / 0 ignored across
  the same 40 result lines**; the +2 are `mod freshness` going from 6 to 8 tests inside a bin target
  that was already one of those lines. `cargo fmt --all -- --check` exit 0 (it was red on the first
  draft of these two tests: two long assertion calls, split by the formatter, 864 → 870 lines).
- `cargo run -q -p xtask --bin requirements -- --check` exit 0 — "is fresh", unchanged from D032:
  these two tests build their divergent ledgers in memory and in `std::env::temp_dir()`, so the
  gate they exercise and the file it guards are not the same object.
- The `requirements` bin suite alone: `cargo test -q -p xtask --bin requirements` exit 0 at **8
  passed / 0 failed** (was 6). With each mutation reverted in turn: **7 passed / 1 failed**, naming
  respectively `a_record_only_in_the_committed_file_is_named` and `equal_meaning_in_different_order_reports_a_line`.
- The first control run also caught the test written backwards: with `dropped` as the committed
  side it reported `AT062: in this tree, not in the committed file`, which is the arm D032 had
  already measured by hand. Swapping the arguments is what made it test the unexercised direction.
- `infrastructure/verify.ps1`, full, clean tree, `FLUX_ABLETON_LINK_LIB` and `FLUX_SPOUT_SHIM`
  removed from the environment, run at `7adb5e6` — **`VERIFY: PASS` / exit 0 / 18 passed, 0 failed,
  0 blocked_external**, `generatedAt` `2026-11-15T22:10:42Z`, `gitHash 7adb5e6`, all three
  `external` probes true, and zero occurrences of `FAILED` or `error[` in the whole log.
  `requirements-freshness` recorded `{ exit: 0, ok: true }` while printing
  `requirements: …implementation/requirements.json is fresh` — the same gate, on a tree whose
  generator had just grown two tests, which is the point: `generate()` is unchanged, so adding
  tests to the file that generates the ledger must not move the ledger. `unit-tests` ran 158 lib
  tests (34 app, 14 audio, 16 catalog, 30 domain, 22 engine, 28 integrations, 6 media, 8 render),
  `link-integration` 7, `ndi-runtime-loads` 2, `spout-enumeration` 1. What that list does not
  contain is worth naming: `unit-tests` is scoped to eight `-p … --lib` packages, so the eight
  `requirements` bin tests this record adds run in CI's `cargo test --workspace` and in no
  `verify.ps1` step — true of D028's, D031's and D032's bin tests too, and unchanged here.
- Which head this evidence names, and which it cannot. `7adb5e6` contains the two tests and D033's
  record of them, which is what makes `gitDirty false` mean something; it does not contain this
  sentence, because a run is only writable from its own numbers, so the record of the run that
  names `7adb5e6` lands in the commit after it. D032 phrased the same limit correctly ("re-run at
  `a91ebda`, the commit that contains those two files"). D033's first draft of the bullet above
  said the evidence "names a head that already holds this paragraph", which is simply false —
  `git show 7adb5e6:implementation/DECISIONS.md` contains no occurrence of its own SHA — and the
  same overstatement was in `STATE.json`'s lane-19 text and in this lane's evidence commit message.
  The docs are corrected here; the commit message is history and stays wrong, because rewriting it
  is not on offer.
- `cargo run -q -p xtask -- evidence` exit 0 afterwards at that head: **32 files, 9 stamped by
  their writer, 4 naming a commit in a header, 19 unattributed, 0 missing provenance**,
  `headAtRun 7adb5e6`, `provenance.gitDirty false`, `generatedAtUtc 2026-11-15T22:11:39Z`.

### What this does not close

`drift_lines` still has report shapes with no invocation behind them. The top-level key loop is
reached only when `specVersion`, `generatedBy` or `counts` moves, and nothing has tried that —
`counts` is the interesting one, since it summarises the very records the per-record loop already
walks, so the two can disagree and only the top-level arm would say so. The `... and N more`
truncation has never fired either, because no mutation here produced more than 20 drifts.
[done: D034 drove both — `a_hand_edited_counts_block_is_named` for the summary block, and
`twenty_five_drifts_are_counted_record_by_record` for the cap, which turned out to report in sorted-id
order rather than file order. `specVersion` and `generatedBy` reach the same two lines and are still
unmeasured.] `check_freshness`' exit 2 — the file missing entirely — has no test; the module asserts
only 0 and 1, which is re-confirmed by reading all eight `check_freshness` assertions in the module.
Exit 2 is the shape still read off the code, which is what this record is supposed to stop doing. The
gate still cannot compare the generator to reality: a confident wrong `at_status()` arm regenerates
wrong bytes and passes, and that residue belongs to `mod evidence_routing`, not to this one.

### Files changed

| File | What moved |
|---|---|
| `crates/xtask/src/bin/requirements.rs` | `mod freshness` gains `a_record_only_in_the_committed_file_is_named` and `equal_meaning_in_different_order_reports_a_line`, and imports `drift_lines` alongside `check_freshness`. 802 → 870 lines, all of it below the last cited line. No comparator logic changed. |
| `implementation/DECISIONS.md` | this record; D032's residue paragraph put in the past tense and marked answered rather than deleted, one orphaned wrap in its CRLF section fixed, and "1302 and below" corrected to "and later" in its churn list. Followed by one correction to itself: the harness bullet's "the head already holds this paragraph" claim, disproved with `git show 7adb5e6:implementation/DECISIONS.md`, together with the same wording in `STATE.json`'s lane-19 text and a "byte-compared" phrase that described reading two logs. |
| `implementation/FINAL-REPORT.md`, `README.md`, `docs/PERFORMANCE-READINESS.md` | the `7adb5e6` run added beside `a91ebda`'s wherever the harness timeline is kept, and the `verify.ps1` row cites D019/D032/**D033** now. |
| `AGENTS.md` | new harness bullet: `verify.ps1`'s `unit-tests` step is eight `--lib` packages (158 tests), the workspace suite is 220, so **no step in the script executes a test that lives in a `[[bin]]` target** — which is where every D028-D033 guard sits. Found while measuring this lane's own "18 passed" claim, not invented as a caveat; nothing was changed to fix it, because the fix (a workspace step in `verify.ps1`) duplicates `ci.yml` and doubles the run. |
| `implementation/STATE.json`, `README.md`, `implementation/FINAL-REPORT.md`, `docs/PERFORMANCE-READINESS.md` | 218 → 220 with what the +2 are, and D033 entered wherever the lane handover lists what D032 left open: STATE's `test_totals.passed` field and its `measured` prose (which had disagreed once before, in D031), its `at_head`/`counts`/`passing_steps`/`re_run_lanes` and a lane-19 paragraph, README's totals clause, FINAL-REPORT §0's two totals, PERFORMANCE-READINESS §"Test totals" as an eighth chronologically-labelled run. `STATE.json` stays at 187 lines, `FINAL-REPORT.md` at 290 and `PERFORMANCE-READINESS.md` at 1076, so no inbound `:NNN` citation in them moved — FINAL-REPORT lost a date clause and two parentheticals to pay for the third number, because a wrapped bullet cannot grow without pushing §6b's `:146` and `:151-152` down. `README.md` grew one line at 91-93, below the highest citation into it anywhere in the tree (`README.md:60`). The remaining 218s in the tree are D032's own Measured rows in DECISIONS/STATE/P-R, which are dated runs at a named commit, not present-tense totals. |

### Citation churn

The insertion starts at line 803 of a file whose inbound citations end at `:584` and `:528-571`, so
nothing re-points: `requirements.rs:22-32`, `:374`, `:393`, `:396`, `:424`, `:445`, `:453`, `:472`,
`:494`, `:512`, `:528-571` and `:584` are all still exact, and the two SDK/harness step citations in
D032's churn list were untouched because no script or workflow changed here. `docs/` and
`implementation/` keep the line counts their own inbound citations depend on; DECISIONS.md grows only
below 1302, where nothing is cited.


## D034 — 2026-11-15 the last two unexercised report branches are driven, and one of them prints a truncation

### What D033 left, and why it was still worth driving

D033's own "What this does not close" named two report shapes with no invocation behind them:
the top-level key loop in `drift_lines` (`specVersion`, `generatedBy`, `counts` — reached only if
one of those moves, and nothing had ever moved one), and the `... and N more` cap in
`check_freshness`, which no mutation in D028-D033 had reached because every control produced one or
two drifts. Both are cheap to drive and both were saying something the code had not admitted.

### `counts` is a derived summary, so a disagreement there is invisible to the record loop

`counts` is computed from the spec arrays at `crates/xtask/src/bin/requirements.rs:165-170` while
`records` is built from the same arrays one loop lower, so the two are separate derivations of the
same source and can disagree. The per-record loop cannot see that class at all: it walks ids present
on both sides, and a summary-only edit changes no id. Before this lane, a hand-edit of the summary
block reported `first differing line is N` — the fallback, true but useless, since the whole point of
the message is to name what moved. Now:

```
counts: committed {"product":24,"effects":28,"effectFamilies":24,"generators":156,"tasks":28,"acceptance":84}, generated {"product":24,"effects":28,"effectFamilies":24,"generators":156,"tasks":28,"acceptance":85}
```

printed, by `check_freshness`, with two leading spaces of its own (`eprintln!("  {d}")`). Each side is
91 characters, so `short()` leaves it intact and unquoted; a value over 96 would arrive as an opening
`'`, the first 96 characters, then `...'` — which is how you can tell the two apart in a CI log.

### Twenty-five drifts is the size at which the cap becomes observable

`check_freshness` prints `diffs.iter().take(20)` and then `... and {} more`
(`crates/xtask/src/bin/requirements.rs:616-621`). Nothing produced more than two drifts until now, so
that `if` had never been evaluated. The new test mutates `status` on the first 25 records — the field
the gate exists to police, not a harmless scalar, so a comparator that special-cased status would
still be caught — and the run with `--nocapture` prints exactly 20 rows (`FX001` first, then
`R001`-`R019`) followed by `... and 5 more`. `FX001` first is not the order of the file: `records`
is indexed into a `BTreeMap` by id (`:685`), so the report walks ids in sorted order while the ledger's
own order is `R001`-`R024`, `FX001` twenty-fifth. That is worth knowing before reading a truncation,
because it decides which rows the cap drops — here `R020`-`R024`, the five with no room. Counting the
printed rows with a `^  R0` filter gives 19 and is wrong: the cap counts drift rows, not record ids
matching a pattern.

The test asserts `drift_lines().len() == 25` and that every row is a `<id>.status` carrying both
values; it does not assert the printed cap. That is deliberate: the cap is display code inside a
function that returns an exit code, and a test that re-derived the message text from the constant
would be a tautology. What behaviour *is* on the line is the alternative the cap protects against —
collapsing 25 real drifts into the line-number fallback — so the test asserts 25 named rows and
`!report.iter().any(|r| r.starts_with("first differing line"))`.

### A 96-character cap is now part of the contract, on purpose

`short()` truncates any reported value past 96 chars (`crates/xtask/src/bin/requirements.rs:715-724`),
and the real `counts` object is 91 — so the assertion that the row names `"acceptance"` on both sides
also asserts that the summary block still fits inside the message. If a spec category ever grows to
four digits, or an eighth key is added,
that test goes red and someone has to decide whether to widen `short()` or report `counts` field by
field. That is a better moment to have the conversation than a CI failure whose report says
`counts: committed {"product":24,...` and nothing about what moved.

### A native crash in the same tree, recorded separately from this lane

The first `cargo test --workspace -- --test-threads=1` on the D034 tree did not finish. It got
through 11 result lines and started a 12th binary, `-p flux-render --lib`, which died inside
`tests::row_stride_matches_preview_copy` with `exit code: 0xc0000005, STATUS_ACCESS_VIOLATION`, which
cargo surfaces as exit 139 and an `error: test failed, to rerun pass` note; the whole run stops there,
so 28 of the 40 result lines never appear — the other 17 test binaries and all 11 doctests. The test is
six lines of source
(`crates/render/src/tests.rs:176-181`): a 100x10 RGBA `render_offscreen`, an assert that the readback
is `100 * 10 * 4` bytes. Nothing in this lane can reach it — the two new tests mutate a `String` of
JSON in memory and a temp file.

Probes, before any re-run of the suite: the same binary alone ten times, `--test-threads=1`,
`cargo test -q -p flux-render --lib` → **10 exits of 0, 8 passed each time**, the stride test among
them. So it is not deterministic in isolation, and there is no mechanism here. What is true is only
what was measured: one workspace-length run produced a native fault inside a test whose whole body is a
GPU render and a readback, ten short runs did not, and the repo has no prior record of
`STATUS_ACCESS_VIOLATION` anywhere in its docs, evidence
or transcripts (`grep` over `*.md`, `*.json`, `*.txt` for `STATUS_ACCESS_VIOLATION`, `0xc0000005`,
`exit code: 139`, `signal: 11`: zero files). The temptation in this repository's own history — D027
guessed a quantum boundary for the Link flake and D030 had to read the vendored SDK to find the real
race — is the reason this paragraph stops at the counts.

### What this does not close

- [done: D035 drove `specVersion` and `generatedBy` together, and its control — a loop told to skip
  those two keys — reddened exactly that test. The same read-through found two shapes this list itself
  had missed, because the list was written from D033's named residues rather than from the branches:
  `short(None)`, reached when a key is present on one side only, serves both loops and is now
  asserted; and the forward one-sided id loop, which never had a test at all, and is the direction a
  real checkout drifts in. D035 also asserted the row the parse branch prints, not just its exit code —
  its control: the ten tests as this lane committed them stayed green, exit 0, with that branch
  rewritten to return no rows. See D035.] Among the three non-`records` top-level keys only `counts`
  was driven;
  `specVersion` and `generatedBy` reached the same two lines, so they were covered by the shape of the
  code and not by a measurement, and an edit that special-cased one of them would have passed.
- The cap's text stays unasserted, deliberately. Nothing reddens if `take(20)` becomes `take(10)`; what
  reddens is a report that stops naming records. That is the trade described above, not an oversight.
- The gate can only compare the committed bytes against what this tree's generator writes. It cannot
  tell you that an arm derives a status the spec no longer means — `AT060` and `AT063` were fixed that
  way at D025, by reading — so a status that is freshly generated and still wrong is invisible here by
  construction.
- Three workspace runs say nothing about how rare the render crash is. One in three long invocations is
  a count, not a rate, and the ten short runs were that binary alone, which is the weakest possible
  control for a fault that might need the GPU state left behind by the preceding 11 binaries.

### Measured

- `cargo test -q -p xtask --bin requirements -- --test-threads=1` — exit 0, **10 passed / 0 failed**
  (was 8).
- `cargo test --workspace -- --test-threads=1` — the first invocation on this tree exited 139 at
  `flux-render` (above). The second, with nothing changed between them, and a third after this
  record's doc comment had landed in the source, both exited 0 at **222 passed / 0 failed / 0 ignored
  across the same 40 result lines** (29 test binaries + 11 doctests), with
  `tests::row_stride_matches_preview_copy` `ok` inside each, and both D034 tests `ok` inside each. The
  runs went to scratch logs under the gitignored `target/`, so no line number in them is evidence
  anyone can re-open: this paragraph is
  the record, and the re-runnable claim is the command above. So the crash reproduced 0 times in 10
  short runs and 1 time in 3 long ones, and the +2 over D033's 220 are
  these two tests, in the `requirements` bin target that has been one of those 40 lines since D028.
- Probe A, the top-level key loop switched off with a local `const PROBE_SKIP_TOP_LEVEL: bool =
  true`: **9 passed / 1 failed**, `freshness::a_hand_edited_counts_block_is_named` panicking at the
  reported line 898, which is the `assert!` starting at `crates/xtask/src/bin/requirements.rs:897` in
  the file as committed — one line of difference, which is the one `const` the probe added above it.
  D035's two edits sit above it in the same module and moved it to `:955` (+23 for the parse-test
  assertions, +35 for the sixth test). The probe files were
  not kept, so that arithmetic cannot be re-derived; what was re-derived is the
  committed number, read off the reverted file rather than computed from the panic. Probe B, the per-field loop replaced by a single generic
  `{id}: differs` row: **9 passed / 1 failed**, `twenty_five_drifts_are_counted_record_by_record` at
  reported 944, which was `:940` in the file the probe ran against and is `:943` in the tree as D034
  committed it — `:1001` since D035 — which is this lane's own doc comment adding three lines above it,
  which is the churn rule
  biting inside a single lane rather than across two. That is worth noting on its own: probe B still
  produced exactly 25
  rows, so the `report.len() == 25` assertion passed under a broken comparator and it was the
  "every row names `<id>.status` with both values" assertion that caught it. A count alone would not
  have. Both probes reverted and the module re-measured at 10 passed; `git diff --stat` was then 83
  insertions and 0 deletions, which is how the revert was verified rather than trusted. The three
  comment lines added after that check make the committed diff 86 / 0.
- The truncation itself, observed rather than asserted: `cargo test -q -p xtask --bin requirements
  freshness::twenty_five_drifts_are_counted_record_by_record -- --exact --nocapture` — exit 0, and its
  block prints 20 rows, then `  ... and 5 more`, then the fix line. The same over
  `freshness::a_hand_edited_counts_block_is_named` prints the one `counts:` row quoted above. Those
  two numbers were re-taken that way after an earlier transcription of a whole-module `--nocapture`
  run, where ten tests print their drift blocks one under another and the row identities got
  attributed to the wrong block; `--exact` makes that impossible.
- `cargo fmt --all -- --check` exit 0. `requirements.rs` 870 → 956 lines, every added line below the
  last inbound citation into the file (`:584`, `:528-571`), so nothing re-points.
- `cargo run -q -p xtask --bin requirements -- --check` exit 0, "is fresh": the ledger is untouched,
  because both tests build their divergent copies in memory and in `std::env::temp_dir()`.

### The harness at the commit that holds this lane

`infrastructure/verify.ps1` then ran at `51bc077`, from a shell with `FLUX_ABLETON_LINK_LIB` and
`FLUX_SPOUT_SHIM` removed via `env -u`: **`VERIFY: PASS`**, `steps: passed=18 failed=0
blocked_external=0`, no `FAILED` line in a 1 930-line log. `requirements-freshness` recorded
`{ exit: 0, ok: true }` while printing that the ledger `is fresh` — the gate holding on a generator that
had just grown two tests and a comment. `link-integration` printed `abl_link.lib resolved:` naming
`_upstream\link\build\Release` and ran 7 tests; `ndi-runtime-loads` ran 2 with 29 filtered out;
`spout-enumeration` 1; `external` read `{ndiRuntime, abletonLinkLib, spoutShim}` all true. `unit-tests`
is the same 158 lib tests (app 34, audio 14, catalog 16, domain 30, engine 22, integrations 28, media 6,
render 8), which restates the scope limit D033 recorded: this script executed none of the four tests
this lane added, and the workspace run that did is in the bullet above.

`gitDirty` came back **true**, in `verify.json` and again in the restamped `manifest.json`, and it is
this record rather than the gate: D026's scope excludes `implementation/evidence/`, and the file that
was dirty was this one, which was being edited while the harness ran. `cargo run -q -p xtask -- evidence`
restamped at head `51bc077` — 32 files, 5 `stamped_current` (catalog, doctor, fixtures, test-gpu,
verify), 4 `stamped_other_commit`, 4 `header_other_commit`, 19 unattributed, 0 missing provenance. Its
`provenance.binary.mtimeUnix` is byte-for-byte the same value the D033 restamp recorded, so one
`xtask.exe` wrote both manifests, which is consistent rather than lucky:
`git diff --stat 7adb5e6..51bc077 -- crates/xtask/src/main.rs` is empty, because this lane touched the
`requirements` bin and not the evidence writer.

The self-stamp limit is re-checked, not inherited: `git show 51bc077:implementation/DECISIONS.md`
contains no occurrence of the string `51bc077`. So the evidence in this commit names a head that holds
the tests and the first 2 065 lines of this document, and *not* the paragraph that says so — which
arrives one commit later, as it did for D032 and D033.

### Files changed

| File | What moved |
|---|---|
| `crates/xtask/src/bin/requirements.rs` | `mod freshness` gains `a_hand_edited_counts_block_is_named` and `twenty_five_drifts_are_counted_record_by_record`. 870 → 956 lines, all of it inside the block appended after the last pre-existing line, plus three doc-comment lines above the second test. No comparator or `check_freshness` logic changed, and the two probes exist only in this document. |
| `implementation/DECISIONS.md` | this record, written across two commits: `51bc077` holds everything above the harness section, and this section plus "What this does not close" arrive with the evidence they describe. |
| `implementation/STATE.json`, `README.md`, `implementation/FINAL-REPORT.md`, `docs/PERFORMANCE-READINESS.md` | 220 → 222 with what the +2 are, and D034 entered in the lane handover in place of the residue D033 named. The truncated run is recorded in the three places that state a suite total and what it does not prove — `test_totals.caveat`, `known_gaps[15]`, `FINAL-REPORT.md`'s "green but weak" bullet and the `PERFORMANCE-READINESS` test-totals row — and not in `README.md`, which gets the number and nothing about how one invocation died. |
| `AGENTS.md` | the `STATUS_ACCESS_VIOLATION` shape added to the suite caveat, because a future agent who sees exit 139 after 11 result lines will otherwise assume their own change broke the build; and the harness-scope bullet's totals moved 220 → 222 with its D028-D033 range re-labelled D028-D034, so the count in that bullet stays the count the workspace run prints. |
| `implementation/evidence/` | `verify.json` from the run above, plus `catalog.json`, `doctor.json`, `fixtures.json` and `test-gpu.json` rewritten by their gates during it, plus `manifest.json` restamped by `cargo run -q -p xtask -- evidence`. Five of the 32 files are stamped at `51bc077` by the writer that produced them; the rest of the tiers are unchanged from D031's shape. These are measurements, not assertions: nothing in this group is edited by hand, and `manifest.json` is documented at D032 as the file the freshness gate deliberately does not cover. |

### Citation churn

`requirements.rs` grows only at the end (870 → 956), below `:584` and `:528-571`, so no citation into
it moves. The probe panics were reported at `:898` and `:944`, which are line numbers in files that no
longer exist; this record cites the `assert!`s at `:897` and `:943`, which is where they sat in the
tree this lane committed — D035 put them at `:955` and `:1001` — re-read off
the reverted tree — and `:943` is itself three lines below the `:940` that the probe ran against,
because this lane's own doc comment landed between them. That is the churn rule biting inside a single
lane rather than across two, and it is why the number in the paragraph is the one the file says. This
record is written at DECISIONS.md `:1923` and below — it was `:1919` when this paragraph was written,
and the +4 is this sweep's own `[done: D034 …]` annotation landing above it inside D033's record. The
deepest line cited into this file anywhere in the tree is `:1267`, and it is cited from `:1780` inside
this file; the only outside citation is `DECISIONS.md:116`, twice, from
`docs/IMPLEMENTATION-ARCHITECTURE.md:414` and `:869`, and both sit above every line this sweep edits.
`FINAL-REPORT.md` stays at 290 lines
and `docs/PERFORMANCE-READINESS.md` at 1076 — both forced by citations *above* the ones this lane
edits (`FINAL-REPORT.md:22` → `:151-152`, `STATE.json:157`/`:163` ceilings), so their edits are
in-place reflows rather than appends. `README.md` was 118 lines when this paragraph was written at
`51bc077`, and is 119 in the follow-up commit below. `STATE.json` goes 187 → 188: the crash
is a `known_gaps` element, which cannot be folded into a prose field without losing the pointer that
`test_totals.caveat` now uses, and nothing in the tree cites a STATE.json line deeper than `:163`, so
the +1 at `:166` re-points nothing — each of those five numbers was read back off the file, not
assumed from the edit. The harness rows (`at_head`, `counts`, `passing_steps`, `re_run_lanes`) and the
lane-20 handover are single JSON strings, so writing the `51bc077` run into them costs no lines;
`test_run_2026_11_14.verify_ps1_full.counts` now says four 18-0-0 runs — true of the commit that
wrote it, and D035's fifth is recorded in the same block below — and one number in that block
corrected itself on measurement — `ndi-runtime-loads` filtered 29 tests out at this run, where the row
had carried 28.

### After the evidence

`4d911c8` carries this lane's run and restamp, and it is not the last word the lane had: five files
were still dirty behind it, all of them the discharged-claim sweep rather than new measurement.

- D033's "What this does not close" is annotated `[done: D034 …]` in place, its residue kept in the
  past tense rather than deleted — and one sentence in it corrected on the way. It said "All three are
  read off the code", counting `check_freshness`'s exit 2 beside the two report branches D034 drove;
  after the annotation that "all three" read as though the third had been measured too. It now names
  exit 2 as the one shape still read off the code, backed by counting the module's eight
  `check_freshness` assertions and finding no `2` among them.
- `AGENTS.md`'s gate bullet described `--check` as exiting 1 "naming the record and field". After
  D034 the report has two shapes — per-record `<id>.<field>` rows plus a `counts` row, or one
  `first differing line is N` — and the bullet says both, plus the sorted-id ordering, because an
  agent reading only `AGENTS.md` would otherwise expect one line per drift in file order.
- `README.md:91-94` gained the `51bc077` run; that is its +1 line. The sentence could not stay
  "each … with `gitDirty false`" once the third run reads `gitDirty true`.
- `implementation/STATE.json`, twice, both times prose that had stopped matching the runs it lists.
  The harness block said "In all three, `FLUX_ABLETON_LINK_LIB` and `FLUX_SPOUT_SHIM` were removed
  with `env -u`" while the sentence above it had just grown to list four runs; `a91ebda`, the second
  of them, is the one whose shell is not recorded — no file in `implementation/evidence/` names it
  (`grep -l a91ebda` there is empty) and `verify.json` keeps only the newest run — so that claim now
  covers the three that do record a bare shell. And `test_totals.measured` called the 222 "the second
  invocation", which was true when two runs existed and went stale-by-omission when a third printed
  the same total; it now says second of three, which only re-reading the field against D034 caught.
- `docs/PERFORMANCE-READINESS.md`'s CI row carries the fourth **18-0-0** at `51bc077` with the
  `gitDirty true` cause. `FINAL-REPORT.md` gained nothing here: §0 states totals, not runs.

Measured after those edits, un-piped: `cargo fmt --all -- --check` exit 0, and `cargo run -q -p xtask
--bin requirements -- --check` exit 0 printing `is fresh` — the gate holding on a tree whose only
changes are prose. `implementation/STATE.json` parses at 188 lines, `README.md` is 119 with its
deepest inbound citation at `:60`, `FINAL-REPORT.md` at 290 and `docs/PERFORMANCE-READINESS.md` at
1 076, and this file was 2 159 lines at `53a66fc`, the commit that carries those edits — a claim that
had already gone stale in the working tree by the time D035 read it back, which is why the count it
turns into is recorded there rather than here.

---

## D035 — 2026-11-16 the freshness gate is read as a branch table, and six shapes turn out to have no invocation

D034's "What this does not close" named two residues and both were real. The method was the problem:
that list was written from D033's handover sentence and from the two tests the lane had just added, so
it could only report what the previous lane had already thought about. This lane opened
`check_freshness` (`crates/xtask/src/bin/requirements.rs:598-624`) and `drift_lines` (`:629-683`),
enumerated every `out.push`, `eprintln!` and `return` in them, and grepped the `freshness` module for
the assertion that pins each. Two entries came out of that which no residue list would have produced:
the loop at `:658-660`, for a record this tree generates and the committed file lacks — the single most
likely way a generated file goes stale in practice, a row added to `spec/` and `--check` never run —
and `short(None)` at `:710-712`, the word `absent`, reachable from both reporting loops and named by
no test.

| # | arm | line | what it can print | asserted before this lane | asserted now |
|---|---|---|---|---|---|
| 1 | ledger unreadable | `:599-604` | `cannot read …`, then exit 2 | nothing (D034's own list named it) | `a_missing_ledger_exits_two`, `:1061` |
| 2 | bytes equal | `:607-610` | `… is fresh`, exit 0 | `committed_ledger_is_fresh`, `:748` | unchanged |
| 3 | one side unparseable | `:634-639` | `one side is not valid JSON; first differing line is N` | the exit code only | row, prefix and parsed number, `:801-818` |
| 4 | top-level key `counts` | `:646-652` | `counts: committed {…}, generated {…}` | D034, `:936` | unchanged |
| 5 | top-level `specVersion`, `generatedBy` | same loop | same shape | nothing | `the_two_header_scalars_are_named`, `:1024` |
| 6 | id only in the committed file | `:655-657` | `<id>: in the committed file, not in this tree` | D033, `:831` | unchanged |
| 7 | id only in what this tree generates | `:658-660` | `<id>: in this tree, not in the committed file` | nothing | `a_record_only_this_tree_generated_is_named`, `:866` |
| 8 | field differs on a shared id | `:666-673` | `<id>.<field>: committed X, generated Y` | D032 `:761`, D034 `:978` | unchanged |
| 9 | key present on one side only | `:710-712`, via 5 and 8 | the word `absent` | nothing | `a_key_either_side_lost_is_reported_absent`, `:1078` |
| 10 | value longer than 96 chars | `:714-718` | the capped quote `'…...'` | D034 `:936` (cap exercised, text not asserted) | unchanged |
| 11 | equal meaning, unequal bytes | `:676-681` | `first differing line is N` | D033, `:900` | unchanged |
| 12 | every line pairs equal | `:730` | `N = min(lines) + 1`, one past EOF | nothing | `an_eof_newline_is_drift_reported_past_the_last_line`, `:1134` |
| 13 | more than 20 rows | `:616-621` | 20 rows, then `... and N more` | executed by `:978`, asserted by nothing | unchanged |

Thirteen arms, six newly asserted, five tests added plus one strengthened, one arm (`13`) still carrying
no assertion at all. Every line number in the left columns is identical in the tree D034 committed and
in this one: `git diff -U0` puts all four hunks inside `mod freshness` at or past committed `:791`, so
the comparator itself did not move and no citation into `generate()`, `check_freshness` or
`drift_lines` needed re-resolving.

### The controls

Each new test was run against a comparator mutated to break exactly it — one edit at a time, reverted
before the next, the source re-verified against `target/req.d035.base2.rs`
(`1e040300fb2acfbded6e30ac1dbe6884090951383f17e3821d8cd26923e7d5c7`) after every revert, and the clean
tree re-run at the end.

| probe | mutation | outcome |
|---|---|---|
| P1 | `:634-639` reduced to `return vec![]` | 14 / 1: `a_broken_ledger_is_located_not_panicked` at reported `:800`, which is committed `:801` — one line down, because the mutation is one line shorter than the four-line row it replaces |
| P2 | `:643` `key == "records"` → `key != "counts"` | 13 / 2: `the_two_header_scalars_are_named` at `:1040`, `a_key_either_side_lost_is_reported_absent` at `:1109` |
| P3 | `:603` `return 2` → `return 1` | 14 / 1: `a_missing_ledger_exits_two` at `:1069` |
| P4 | `:712` `"absent"` → `"null"` | 14 / 1: `a_key_either_side_lost_is_reported_absent` at `:1114` |
| P5 | `:730` fallback loses its `+ 1` | 14 / 1: `an_eof_newline_is_drift_reported_past_the_last_line` at `:1141`, `4050` against the expected `4051` |
| P6 | `:659` forward row given the reverse wording | 14 / 1: `a_record_only_this_tree_generated_is_named` at `:884` |

Two of them are worth more than the counts.

- **P2 did not silence the gate.** With the two header scalars skipped, the report that
  `the_two_header_scalars_are_named` received was `["first differing line is 2"]`: arm 11 caught what
  arms 5 and 9 stopped naming. So the mutation costs the reader the key name and both values, not the
  verdict — which is precisely the distinction D032 wrote `drift_lines` to make, and the reason the
  assertions in these tests are on row text rather than on exit codes.
- **The blindness control.** `git show HEAD:crates/xtask/src/bin/requirements.rs` — 956 lines, the ten
  tests exactly as D034 committed them — with P1 applied to it gives **10 passed / 0 failed, exit 0**. A
  comparator that returns *no rows at all* for an unparseable ledger, printing the header, the `fix:`
  line, and nothing that names where the file broke, was invisible to that suite; the pre-D035 test
  pinned the exit code and the fact of a message, not the message. That is the strongest argument in
  this lane for the shape of the edit: arm 3 became an assertion because `:796`'s body grew by twenty
  lines, and none of the five new tests would have reddened under P1 either — the parse test is the
  only thing in the module that reaches that arm.

### The EOF question, settled rather than smoothed

`generate()` returns `serde_json::to_string_pretty(&out).unwrap()` (`:176`) with no trailing newline,
and `normalize_newlines` (`:592-594`) folds carriage returns only. So an editor that insists on a final
newline writes bytes the generator does not, and the honest options were to call that drift or to trim
it. Trimming was rejected: the gate's contract, stated at D032, is byte equality with what this tree
generates, and forgiving one byte at EOF means printing `is fresh` about a file nothing produced. What
`an_eof_newline_is_drift_reported_past_the_last_line` asserts instead is the shape of the report when
that newline is the only difference — every line pairs equal, `out` is empty, and `first_diff_line`
(`:725-731`) counts one past the shorter side, naming line 4 051 of a 4 050-line file. The message
points at a line nobody can open. That is a real limit of the report, and after this lane it is a
tested limit rather than a surprise; P5's `left` value also re-measured the ledger's own shape, because
`body.lines().count()` printed 4 050.

### What this does not close

- Arm 13, `... and N more` (`:616-621`), still has no assertion. It executes — D034's 25-drift test
  reaches it and printed it under `--exact --nocapture` — but nothing reddens if the sentence is
  rewritten or `take(20)` becomes `take(10)`. D034 called that a deliberate trade; this lane keeps the
  trade and stops describing it as an oversight it fixed.
- Arm 1 is asserted in the module and unreachable from either harness: `verify.ps1:128` and
  `ci.yml:70` both point `--check` at a tracked path, and `git ls-files --error-unmatch
  implementation/requirements.json` succeeds, so exit 2 means only a deleted or never-checked-out
  ledger. The test exists so the verdict is not a lie if a checkout does hit it, not because a run
  produced it. Arm 12 is the reverse case: reachable — any tool that appends a final newline to the
  file would hit it — and never once produced here, because every run this lane made wrote the bytes
  either from `generate()` or from `to_string_pretty`.
- `records_by_id`'s `unwrap_or_default()` (`:693`) and the `unwrap_or_default()` for a record with no
  `id` (`:690`) have no invocation. What they do is reasoned from reading, and it is not silence: a
  one-sided non-array `records` makes every id one-sided, which is 345 rows, and a both-sides
  non-array falls through to arm 11's line number. Neither was measured, and no ledger in this tree is
  in either shape.
- Duplicate ids collapse silently in the BTreeMap (`:685`), and no test asserts that the 345 ids are
  unique. They are unique because nothing in `generate()` emits two rows with one id; nothing guards a
  change that does, and the loss would show up as a shorter report, not as a wrong one.
- The structural limit is untouched. The gate compares committed bytes against this tree's generator,
  so an arm that derives a status the spec no longer means — D025's `AT060` and `AT063`, fixed by
  reading — stays invisible here by construction. Five more tests cannot see it.
- All six probes ran against the `requirements` bin alone, which D034 already called the weakest
  possible control for the `flux-render` crash. This lane re-ran the workspace suite once and reported
  what it printed; it did not attempt a rate for a fault that has now appeared once in four long
  invocations across two lanes. The addendum below records a fifth, also clean, which moves the count
  and not the understanding: still no mechanism, and still no reason to blame a change before
  re-running.

### Measured

- `cargo test -p xtask --bin requirements -- --test-threads=1` — **15 passed / 0 failed**, exit 0, read
  from cargo's own status into `target/final-clean.log` rather than through a pipe. Was 10; the +5 are
  the five new tests. `mod freshness` now holds 13 of the 15, the other two being the
  `mod tests` routing guards from D028.
- `cargo test --workspace -- --test-threads=1` — exit 0, **227 passed / 0 failed / 0 ignored across the
  same 40 result lines**, with zero `FAILED`, zero `error[` and zero
  `STATUS_ACCESS_VIOLATION` lines in `target/ws-d035-final.log`, and
  `a_record_only_this_tree_generated_is_named` and `an_eof_newline_is_drift_reported_past_the_last_line`
  both `ok` inside it. The log is a gitignored scratch file, so the re-runnable claim is the command,
  not the number. The +5 over D034's 222 are this lane's five new tests, all in the `requirements` bin
  target that has been one of those 40 lines since D028 — and therefore executed by CI's workspace run
  and by no step of `infrastructure/verify.ps1`.
- `cargo run -q -p xtask --bin requirements -- --check` — exit 0 printing `is fresh`, on the tree that
  carries these five tests.
- `cargo fmt --all -- --check` — exit 0, after one `cargo fmt --all` pass that reformatted only the new
  test body.
- The six probes and the blindness control, each with the counts in the table above. Every one was
  followed by a revert and a `sha256sum -c` of the source against the pre-probe snapshot, `OK` each
  time, and no mutation survived into this commit.
- `requirements.rs` 956 → 1 154 lines, `199 insertions(+), 1 deletion(-)`: the deletion is one doc
  comment sentence at `:791` and every insertion is inside `mod freshness`.

### Files changed

| File | What moved |
|---|---|
| `crates/xtask/src/bin/requirements.rs` | Five tests added to `mod freshness` (`:866`, `:1024`, `:1061`, `:1078`, `:1134`) and `:796`'s parse test grown to assert its row and the number inside it. No comparator line changed: the four diff hunks all start at or past committed `:791`. |
| `implementation/DECISIONS.md` | this record, plus three in-place corrections to D034: its first "does not close" bullet annotated `[done: D035 …]` with the two shapes it had missed stated as missed, its probe-A and probe-B citations re-pointed to `:955` and `:1001`, and its own "this file is 2 159 lines" claim dated to the commit that made it true. |
| `implementation/STATE.json` | `test_totals` 222 → 227 with what the +5 are; the lane-20 "next open inside this gate" sentence discharged in place; a lane-21 paragraph added; the `:966` citation in lane-20's text re-pointed to `:1001`. |
| `README.md`, `implementation/FINAL-REPORT.md`, `docs/PERFORMANCE-READINESS.md`, `AGENTS.md` | the suite total and the D028-D034 range it carries, re-labelled for D035. |
| `implementation/evidence/` | only what a gate rewrites when it runs; nothing in this lane edits them by hand, and `manifest.json` stays outside the freshness gate for the reason recorded at D032. |

### Citation churn

`requirements.rs` grew inside the module rather than only at its end, which is the case the churn rule
exists for: `:860-893` (35 lines for arm 7's test) sits above four of the five new tests, so the
numbers written while they were being developed — `:989`, `:1024`, `:1043`, `:1099` — moved to
`:1024`, `:1061`, `:1078`, `:1134`, and the probes were re-run afterwards so that every line quoted in
this record is a line the committed file actually has. D034's two citations into the same module,
`:920` and `:966`, became `:955` and `:1001` for the same reason, and STATE's copy of the `:966`
citation moved with it. Into `requirements.rs` from elsewhere the citations that matter are
`docs/IMPLEMENTATION-ARCHITECTURE.md:100` (`:22-32`) and `:330` (`:26-32`), and
`docs/PERFORMANCE-READINESS.md:245` (`:424`, `:512`) and `:426` (`:393`, `:494`), all of them above
`:791`, so none of them moved. `AGENTS.md` stays at 214 lines and `STATE.json` at 188: the changes there are inside
single JSON strings and one bullet, not new lines. `README.md` stays at 119, `FINAL-REPORT.md` at 290
and `docs/PERFORMANCE-READINESS.md` at 1 076 — all three forced by the citation ceilings already
recorded at D034, so their edits are in-place reflows. This file is 2 336 lines at the commit that
carries this record, with the D035 heading at `:2176`; the only citation into it from outside is still `DECISIONS.md:116`, twice, from
`docs/IMPLEMENTATION-ARCHITECTURE.md:414` and `:869`, both above every line this lane touched.

### After the evidence

Everything above was written before the harness ran, which is the order D033's record of its own run
demands: a run is only writable from its own numbers. What the run then produced:

- `infrastructure/verify.ps1`, full and without `-Quick`, at `2361785` from a shell with
  `FLUX_ABLETON_LINK_LIB` and `FLUX_SPOUT_SHIM` removed via `env -u`: **VERIFY: PASS**, exit 0,
  `steps: passed=18 failed=0 blocked_external=0`, `verify.json` at `gitHash 2361785` with
  `gitDirty false` and `external` all true, `requirements-freshness` recording `{ exit: 0, ok: true }`
  while printing `is fresh`, `unit-tests` again at 158 (app 34, integrations 28, domain 30, engine
  22, catalog 16, audio 14, render 8, media 6), the integration groups 6/6/4/3, link 7, ndi 2 with 29
  filtered out, spout 1 — and zero occurrences of `FAILED` or `error[` in a 1 929-line log, each
  counted with `grep -c` rather than by eye. It is the fifth 18-0-0 run and the third of those five
  to measure a clean tree; the two that did not were dirty with their own records.
- `cargo run -q -p xtask -- evidence` — exit 0, restamping `manifest.json` at `headAtRun 2361785`,
  `generatedAtUtc 2026-11-16T00:27:47Z`, `provenance.gitDirty false`: 32 files, 9 stamped by their
  writer (5 `stamped_current` — catalog, doctor, fixtures, test-gpu, verify — and 4
  `stamped_other_commit`), 4 `header_other_commit`, 19 `unattributed`, 0 `missing_provenance`. The
  same 32 / 9 / 4 / 19 / 0 the D033 and D034 restamps reported, written by the same `xtask.exe`
  (`mtimeUnix 1794774257` = `2026-11-15T20:24:17Z`, read off this manifest and off `51bc077`'s in
  `git show`; D034 recorded the same value for `7adb5e6`) — and the shared binary is evidenced, not
  inferred from a shared timestamp: `git diff --stat 53a66fc..2361785 -- crates/xtask/src/main.rs`
  is empty, because this lane edited the `requirements` bin and not the evidence writer.
- **P7, which came after the table above was closed.** The forward arm (`:658-660`) had its loop body
  replaced with `let _ = id;` — the mutation the new test's doc comment predicts, and the one that
  does not silence the gate. 14 passed / 1 failed, exit 101, with
  `a_record_only_this_tree_generated_is_named` panicking at `:884` on
  `left: ["first differing line is 3773"]`. The number is unhelpful rather than wrong: line 3 773 of
  this ledger is `"id": "AT062"`, so it points at the record the report stopped naming, and only a
  reader holding both sides can tell. Reverted by copying `git show 2361785:…` back over the source,
  `sha256sum -c` reporting `OK`, `git status --short` then naming only the five evidence files a gate
  rewrites, and the module re-run at 15 passed / 0 failed.
- **The workspace suite was invoked a second time**, at this addendum's tree — docs and gate-written
  evidence only, no source between the two runs — and printed the same totals: exit 0, **227 passed /
  0 failed / 0 ignored across the same 40 result lines**, 713 lines of log with zero `FAILED`, zero
  `error[` and zero native-fault lines. That moves one number and no understanding: the crash rate in
  `known_gaps[15]` and in `AGENTS.md` is now 1 long invocation in 5, STATE's `test_totals.measured`
  says two invocations where it said one, and there is still no mechanism for the one fault D034 met.
- **The self-stamp limit is unchanged.** `git show 2361785:implementation/DECISIONS.md` contains no
  occurrence of `2361785`: the commit that carries a record cannot name its own SHA inside it. This
  section closes that gap the only way available — from the next commit, in the past tense — so the
  limit stands one level up, and this addendum cannot name the commit that carries it either.
- **The lane crossed midnight, and the dates moved rather than got footnoted.** `2361785` was made at
  `2026-11-16 01:24:40 +0100` and every measurement quoted above is later than it, so D035's heading,
  STATE's `test_totals.measured` and lane-21 date, README's `re-measured` clause, FINAL-REPORT's and
  `docs/PERFORMANCE-READINESS.md`'s row header were re-dated to 2026-11-16. D034's were not: its run
  stamps are UTC (`2026-11-15T23:24:21Z`), and 23:24Z *is* the 15th, so that record stays as written
  even though `51bc077` and `4d911c8` were both made after local midnight. The workable convention is
  that a decision carries the date its evidence was measured, and across a rollover the timestamps
  this file quotes with their `Z` outrank the heading.

`README.md` stays at 119 lines, `FINAL-REPORT.md` at 290, `docs/PERFORMANCE-READINESS.md` at 1 076,
`AGENTS.md` at 214 and `STATE.json` at 188: those edits sit inside lines already written, so no
inbound citation moved and no `requirements.rs` line number changed. This file grew by exactly the
section above — it is 2 396 lines at the commit that carries this addendum, which is why the 2 336
quoted two pages back names the commit carrying the record instead of this one.

---

## D036 — 2026-11-16 seventeen CLI sites exit nonzero now, and the first honest run of the Spout DX sender says it has been failing since November

`STATE.json` has carried the same sentence since D029: "the seven feature-off CLI branches that still
exit 0 (`crates/app/src/main.rs:531`, `:592`, `:689`, `:746`, `:810`, `:843`, `:880`)". D027 wrote it as
a judgement about a family it had just repaired once, and every lane after copied the seven numbers
without opening the file again. Opening it and enumerating every branch in `crates/app/src/main.rs` that
reports a non-execution and then hands back `Ok` gives **seventeen sites**, and the two the list was
missing were not subtle: the `link` branch prints *two* lines of prerequisite before exiting 0, and
`flux caps` prints a JSON error object and exits 0 — a shape change plus a status lie, which no residue
list mentioned because it never prints a sentence to notice.

| # | site | was | now | code | how it is known |
|---|---|---|---|---|---|
| 1 | `cmd_ndi_loopback` compiled out | `:531` | `:545` | `BlockedExternal` | measured exit 1 + `the_four_ndi_subcommands…` |
| 2 | `cmd_ndi_send` compiled out | `:592` | `:605` | `BlockedExternal` | same |
| 3 | `cmd_ndi_recv` compiled out | `:689` | `:701` | `BlockedExternal` | same |
| 4 | `cmd_ndi` compiled out | `:880` | `:923` | `BlockedExternal` | same |
| 5 | `cmd_spout_send_dx` compiled out | `:746` | `:778` | `BlockedExternal` | measured exit 1 + `the_three_spout…` |
| 6 | `cmd_spout_send` compiled out | `:810` | `:847` | `BlockedExternal` | same |
| 7 | `cmd_spout` compiled out | `:843` | `:887` | `BlockedExternal` | same |
| 8 | `cmd_link` compiled out | `:919` | `:961` | `BlockedExternal` | measured, and `link_names_the_library…` pins the env var |
| 9 | `cmd_depth` compiled out | `:1026` | `:1072` | `BlockedExternal` | measured exit 1 + `depth_refuses…` |
| 10 | DX helper exited nonzero | `:735` | `:753` | `SdkMissing` | measured: `0xc0000138`, exit 1 |
| 11 | DX helper failed to launch | `:736` | `:761` | `SdkMissing` | no invocation this lane; both files exist here |
| 12 | no DLL and no helper | `:740-741` | `:771` | `SdkMissing` | measured by running the CLI with `apps/spout-bridge` as cwd |
| 13 | `spout-send` shim not found | `:773-776` | `:808` | `SdkMissing` | measured from a directory with no `apps/spout-bridge` |
| 14 | `spout-send` accepted 0 frames | `:798-803` | `:836` | `BlockedExternal` | measured in the `--features ndi,spout` build |
| 15 | `spout` sender query timed out | `:829-831` | `:878` | `BlockedExternal` | not reached: the query answered (0 senders, exit 0) |
| 16 | `depth` model unavailable | `:997-999` | `:1040` | `SdkMissing` | not reached: `vision` was never built this lane |
| 17 | `caps` swallowed the worker error | `:1058` | `:1098` | propagates `MediaWorker::new()?` | measured exit 0 here + `caps_refuses_only_when_the_media_worker_does` |

Sites 1-9 share one constructor, `feature_off()` (`:150`), which puts the build command in `recovery`
instead of in a second `println!` the way `link` used to; 10-17 keep the D023 vocabulary — `SdkMissing`
for an apparatus that is not there, `BlockedExternal` for a run that was offered and took nothing.

### The boundary, stated once

D027 left the class open because "whether *this binary cannot* is a failure or a usage answer is one
decision for all seven". It is now one decision, and it is drawn at the apparatus, not at the outcome:

- **Nonzero** — the code is compiled out, the DLL or helper or model is not there, a child the command
  spawned exited nonzero, the library accepted none of the frames offered, or the registry query gave up
  before answering. None of these is an observation about the world; each is the command not having run.
- **Zero, still** — `flux ndi` with 0 sources (measured this lane), `flux spout` with 0 senders
  (measured), `flux recover` on a directory with no autosave (measured, exit 0), `flux caps` and
  `flux list` on a host that has FFmpeg. Finding nothing is an answer.
- **Two rows in `cmd_doctor` stay as they were** (`:196` GPU, `:207` FFmpeg): doctor exists to describe
  the machine, `unavailable` is a value in the same line shape as every other probe, and a report that
  exits 1 wherever it reports a limitation would make `doctor` unusable on exactly the hosts it is for.
  `caps` was different because its payload *changes shape* — a codec object versus `{"error":…}` — and
  an exit code of 0 invited a caller to parse the second as the first.

### Controls

Six tests now live in the `flux` bin target, which before this lane had none: the whole class was
unpinned by construction, not by an oversight in a particular assertion. Each was checked against a
mutation of `crates/app/src/main.rs`, one edit at a time, reverted, the source re-verified against
`target/app.d036.base.rs`
(`b5ab95892ad2512b09bf5ac13568bfc65612a6b4c2cf3b41b4c594b0ff57ec9f`) after every revert, and the clean
tree re-run at the end.

| probe | mutation | outcome |
|---|---|---|
| M1 | site 4 put back to `println!` + `Ok(())` | 4 passed / 2 failed: the ndi guard **and** `no_compiled_out_block_hands_back_a_success` |
| M2 | `feature_off` reports `SdkMissing` | 2 / 4: every refusal guard, because the code is asserted, not just the fact of an `Err` |
| M3 | `feature_off`'s default `recovery` dropped | 3 / 3: ndi, spout, depth — and `link` **survives**, because its call site re-states `FLUX_ABLETON_LINK_LIB` |
| M4 | a tenth compiled-out block added | 5 / 1: the scan's `assert_eq!(blocks, 9)`, which is what stops a new branch joining the class quietly |
| M5 | `caps` errors while the media worker succeeds | 5 / 1: the comparison in `caps_refuses_only_when_the_media_worker_does` is live, not vacuous |

M3 is the one worth reading twice: a guard that passes under a mutation is not a guard, and here the
reason it passes is that the same fact is carried in two places. M1 also earns the scan test its place —
the four runtime guards compile out feature by feature, so in a `--features ndi` build site 4 has no
test at all, while the scan still counts nine blocks and still rejects an `Ok` inside any of them.

### What an honest exit code revealed

Site 10 had never been *run* honestly before. `flux spout-send-dx` in a `spout` build prints
`DLL path unavailable; launching spout_sender.exe helper` and then, today, this:

```
flux: FluxError { code: SdkMissing, subsystem: "spout", severity: Error,
  message: "spout-send-dx: spout_sender.exe exited with exit code: 0xc0000138",
  recovery: "rebuild apps/spout-bridge", … }
```

Before this lane that sentence was printed and the process exited 0. Three independent launchers agree
the helper itself fails at process start — Rust's `Command` (the line above), `cmd /d /v:on`
(`-1073741512`) and PowerShell `Start-Process -PassThru` (same) — and `dx_test.exe`, the other 2026-11-09
C++ binary in that directory, fails identically. Two details narrow the mechanism, and both are measured:
the seven hand-shipped CRT DLLs beside them are version-identical to the system's (`14.51.36247.0`), so
this is not the redist mismatch `docs/PERFORMANCE-READINESS.md:458-465` predicted; and
`crates/integrations/src/spout.rs:63-74` prepends the shim's own directory to `PATH` before the child is
spawned, so a *find*-time failure is excluded and the remaining reading is bind-time — an ordinal that no
loaded DLL exports, in a binary that imports `SpoutLibrary.dll`. `spout_shim.dll`, the registry-only shim
that does not import it, loads and answers (`flux spout` → 0 senders, exit 0). So `implementation/evidence/spout-send-dx.txt`
(83 frames through a real D3D11 sender, 2026-11-09) — the evidence AT058 rests on — no longer reproduces
on this host, and the only reason anyone knows is that the exit code stopped lying. No gate in this repo
ever ran `flux spout-send-dx`: `verify.ps1` runs `spout-enumeration`, which is the registry path.

A trap surfaced by that sub-investigation, worth the same column as the pipe rule: in `cmd /c`,
`%ERRORLEVEL%` is expanded when the command line is *parsed*, so `prog.exe & echo %ERRORLEVEL%` reports
the previous status. Two first probes read `0` that way and were wrong; `/d /v:on` with `!ERRORLEVEL!`
gave the real `-1073741512`.

### What this does not close

- Sites 11, 15 and 16 have no invocation. 11 needs a helper that exists and cannot be launched, 15 needs
  a registry query that times out, 16 needs a `vision` build, which needs the `ort`
  `download-binaries` fetch this lane did not attempt. They are `Err` by construction and reasoned from
  reading, exactly the status D035 gave to `records_by_id`'s `unwrap_or_default()`.
- `main()` maps every `Err` to exit 1. `BlockedExternal` and `SdkMissing` are distinguishable in the
  printed code but not in the status, so a script cannot tell "rebuild with the feature" from "install
  the SDK" without parsing stderr. `verify.ps1` has a third counter for exactly this; the CLI does not.
- All three `missing()` closures (`cmd_ndi_loopback`: `:429`, `cmd_ndi_send`: `:553`, `cmd_ndi_recv`:
  `:614`) still build errors with an empty `recovery` — measured: `ndi-recv` prints `recovery: ""`.
  None calls `with_recovery`; the eight sites that do are elsewhere, and they exit nonzero (D023).
- The shim search is cwd-dependent (`crates/integrations/src/spout.rs:46-50`: env var, then
  `spout_shim.dll`, then `apps/spout-bridge/spout_shim.dll`). Sites 12 and 13 were only reachable *by
  moving directory*, which is the same shape as the AGENTS.md trap about cargo setting a test binary's
  cwd to the package directory, now measured from the CLI side. Not widened.
- These six tests land in a `[[bin]]` target: executed by CI's workspace run, by no step of
  `infrastructure/verify.ps1`. `VERIFY: PASS` cannot see this lane's work.
- The freshness gate cannot see any of it either. `at_status()`'s AT058/AT060/AT063 arms name no exit
  codes, so `requirements.json` stayed fresh through seventeen status changes and would have stayed
  fresh through the DX regression too. What caught it was running the binary.
- AT058's blocker string in the generated ledger now understates what this lane measured. It says
  "Flux's enumerator does not see its own DX sender"; the DX sender cannot start at all, so nothing
  should have been expected to see it. Correcting that means editing the `at_blocker()` arm and
  regenerating, which rotates a record that carries inbound `requirements.json:NNNN` citations, and it
  belongs with the lane that rebuilds `apps/spout-bridge` — not smuggled in beside an exit-code change.
  The status itself (`implemented_unverified`) is not over-claiming, which is why leaving it is not the
  D025 defect again.

### Measured

- `cargo test -q -p flux-app --bin flux -- --test-threads=1` — **6 passed / 0 failed**, exit 0, both
  before and after `cargo fmt --all`.
- `cargo test --workspace -- --test-threads=1` — exit 0, **233 passed / 0 failed / 0 ignored across 40
  result lines**, twice on this tree, zero `FAILED`, `error[` and `STATUS_ACCESS_VIOLATION` lines in
  logs of 719 and 718 lines. The +6 over D035's 227 are this lane's six guards, all in the `flux` bin
  target, whose result line already existed (it reported 0 tests). The native-fault rate is now 1 in 7.
- `cargo fmt --all -- --check` — exit 0 after one pass that reflowed only this lane's own code.
- `cargo run -q -p xtask --bin requirements -- --check` — exit 0, `is fresh`: the comparator and the
  generator were not touched.
- Nine subcommands against the default build (`target/d036/flux-default.exe`): `ndi`, `ndi-send`,
  `ndi-recv`, `ndi-loopback`, `spout`, `spout-send`, `spout-send-dx`, `link`, `depth` → **exit 1 each**,
  each printing `code: BlockedExternal` and a `recovery` naming `cargo build -p flux-app --features …`;
  `caps` and `list` → exit 0.
- The same binary family rebuilt `--features ndi,spout`: `ndi` exit 0 (runtime loaded, 0 sources),
  `ndi-loopback` exit 0 (921600 bytes back), `ndi-recv` exit 1 (`SdkMissing`, 0 sources), `spout` exit 0
  (0 senders), `spout-send` exit 1 (`BlockedExternal`, 0 frames accepted), `spout-send-dx` exit 1 (site
  10 above), `depth` exit 1 (`vision` still off), `caps` exit 0.
- Sites 12 and 13 driven by directory: from `apps/spout-bridge`, `spout-send-dx` → exit 1 naming both
  missing artifacts; from an empty directory with `FLUX_SPOUT_SHIM` removed, `spout-send` → exit 1
  `spout_shim.dll not found` and `spout` → exit 1. Boundary kept: `flux recover` on an empty directory →
  exit 0, printing that there was nothing to recover.
- The five controls above, each followed by a restore and a sha256 check against the snapshot; the final
  tree hashes identical to it before the test module was formatted.

### Files changed

| File | What moved |
|---|---|
| `crates/app/src/main.rs` | `feature_off()` added at `:150`; nine compiled-out branches, eight feature-on arms and `cmd_caps` now return errors; `#[cfg(test)] mod tests` (`:1133`) with six guards. 1 086 → 1 248 lines, `209 insertions(+), 47 deletions(-)`. |
| `implementation/DECISIONS.md` | this record, D027's "Not changed here" bullet annotated as discharged with the count corrected, D029's copy of the same sentence closed, D026's `:877` and D027's `:1132` citations re-pointed to the moved lines, and D032's two intra-file pointers at `:1786`-`:1787` re-pointed for the growth this record itself caused. |
| `implementation/STATE.json` | `test_totals` 227 → 233 with what the +6 are; the seven-branch item in `current_task` discharged; `known_gaps[15]` rate, `commands_to_resume` and the DX evidence claim corrected to what the binary now says. |
| `AGENTS.md` | the CLI-exit bullet extended with the seventeen and the boundary; the `%ERRORLEVEL%` parse-time trap added; the D028-D035 range and the workspace total relabelled; and, after this lane's own run, a harness-trap bullet that a run is named by the `gitHash` in its `verify.json` rather than by which commit was HEAD. 232 lines at `1a418a1`, 240 with this addendum. |
| `README.md`, `implementation/FINAL-REPORT.md`, `docs/PERFORMANCE-READINESS.md` | the suite total, the run range, the re-pointed `main.rs` citations, and the Spout rows corrected where this lane measured them. |
| `implementation/BLOCKERS.md` | the Spout2 bullet, which still said `flux spout-send-dx` "broadcasts real frames"; it is now past tense, names the `0xc0000138` re-measurement, and keeps its `:20-22`/`:23-24` claim lines on the lines that cite them. |
| `implementation/evidence/` | only what a gate rewrites when it runs; `manifest.json` stays outside the freshness gate for the reason recorded at D032. |

### Citation churn

`main.rs` grew at `:150` and then inside bodies, so every inbound citation into it below `:144` moved.
Re-resolved mechanically against `git show HEAD:crates/app/src/main.rs` with a line-mapping walk rather
than by eye: `cmd_ndi_loopback` `:403` → `:417`, its `(w, h)` `:439` → `:453`, `:440` → `:454`, its end
`:528` → `:542`; `cmd_ndi_send` `:536` → `:549`, `:558` → `:571`, the gradient loop `:568`-`:578` →
`:581`-`:591`; `cmd_spout_send_dx` `:694` → `:705`, `:703` → `:714`, `:710` → `:721`, `:716` → `:727`;
`cmd_ndi_recv`'s `match frame` `:666` → `:678`, `None =>` `:680` → `:692`; `cmd_osc_listen` `:937`-`:955`
→ `:977`-`:995`, its `recv_timeout` `:946` → `:986`; `cmd_spout`'s SdkMissing block `:821-827` →
`:857-863`. Nothing in `docs/IMPLEMENTATION-ARCHITECTURE.md` moved: every citation there points at
`:2-9`, `:30`, `:31-133`, `:58-74`, `:101`, `:116-132`, `:133-136` or `:140`, all at or above the
insertion point, so its 888 lines are unchanged.

Two citations *inside this file* moved, because this record's own annotations grew it: D032's
pointers at `:1786` and `:1787` named `DECISIONS.md:1267` and `:1284`, and the seven lines inserted
at `:1196` and `:1409` put that content at `:1272` and `:1289`. They are re-pointed, with the old
numbers kept in D032's own paragraph — a lane that grows a file above a pointer it also cites has
no business leaving the pointer for the next reader to find dead.

One citation was found **wrong** rather than stale. `docs/PERFORMANCE-READINESS.md:458-465` attributed
a DLL "fallback search order" to `main.rs:219-221`; at every revision this lane checked (`58887cd`,
`6f68295`, `93484db`, `e110381`, HEAD) those lines are `cmd_render`'s cancel field and progress
closure, and the order has always lived in `crates/integrations/src/spout.rs:46-50`. The same row's
other citation was the opposite case: `main.rs:666-680` for the helper shell-out was **correct** at
`58887cd`, moved to `:667` at `6f68295`, `:668` at `93484db`, `:726` at `e110381` and is `:737-766`
now — three renumbers survived by nobody opening the file at the number it names. That is what the
re-resolve rule is for.

This file is 2 610 lines at the commit that carries this record, with the D036 heading at `:2409`; the
only citation into it from outside is still `DECISIONS.md:116`, twice, from
`docs/IMPLEMENTATION-ARCHITECTURE.md:414` and `:869`, both far above every line this lane touched.

### After the harness

Everything above was written before the harness ran, which is the order D033's record of its own run
demands: a run is only writable from its own numbers. What the run then produced.

- `infrastructure/verify.ps1`, full and without `-Quick`, at `1a418a1` from a shell with
  `FLUX_ABLETON_LINK_LIB` and `FLUX_SPOUT_SHIM` removed via `env -u`: **VERIFY: PASS**, exit 0,
  `steps: passed=18 failed=0 blocked_external=0`, all 18 steps `{ exit: 0, ok: true }`, `verify.json`
  at `gitHash 1a418a1`, `gitDirty false`, `quick false`, `external` all three true, rustc 1.99.0,
  `requirements-freshness` recording that verdict while printing `is fresh` at log line 1408,
  `unit-tests` again at **158** (app 34, audio 14, catalog 16, domain 30, engine 22, integrations 28,
  media 6, render 8 — the same eight numbers D035 listed for the same step, in the order this log
  prints them), the integration groups 6/6/4/3, link 7 with 28 filtered out, ndi 2 with 29, spout 1
  with 28, and zero occurrences of `FAILED`, `error[` or `ACCESS_VIOLATION` in a 1 930-line log.
- That 158 is the blind spot confirmed rather than assumed. This lane added six tests and the step
  totals exactly what it totalled before them; the figure that moved is 227 → 233, and the command
  that reads it is the one this script does not run. Anyone who wants the six guards executed has to
  run the workspace suite, not `VERIFY: PASS`.
- `cargo run -q -p xtask -- evidence` — exit 0, `headAtRun 1a418a1`, `provenance.gitDirty false`: 32
  files, 9 stamped by their writer (5 `stamped_current` — catalog, doctor, fixtures, test-gpu, verify
  — plus 4 `stamped_other_commit`), 4 `header_other_commit`, 19 `unattributed`, 0 `missing`.
  `git diff --stat 2361785..1a418a1 -- crates/xtask` is empty, so the same writer produced the same
  32 / 9 / 4 / 19 / 0 rather than a re-tally of different bytes — evidenced by the diff, not by the
  shared `xtask.exe` mtime. And `manifest.json` still reports `gitHash 2361785` for *itself*, because
  a manifest cannot stamp the commit it is written into: D035's limit, met again rather than fixed.

### The 18-0-0 tally, recounted from bytes

D032's own Files-changed row (`:1772`) and `docs/PERFORMANCE-READINESS.md:1073` both record a run
**at `3e4e825`** as the first 18-step run, with `gitDirty true` "because its uncommitted sources were
DECISIONS.md and this file". Enumerating the `gitHash`, `counts` and `gitDirty` fields of every
committed `implementation/evidence/verify.json` across all 94 commits this repository's refs reach
(80 of them carry the file at all) says otherwise. The evidenced set, in order:

| run measured at | counts | `gitDirty` | `generatedAt` | first committed in |
|---|---|---|---|---|
| `a91ebda` | 18/0/0 | false | `2026-11-15T20:41:04Z` | `5763cdf` |
| `7adb5e6` | 18/0/0 | false | `2026-11-15T22:10:42Z` | `f710d90` |
| `51bc077` | 18/0/0 | **true** | `2026-11-15T23:24:21Z` | `4d911c8` |
| `2361785` | 18/0/0 | false | `2026-11-16T00:26:26Z` | `75ff9a9` |
| `1a418a1` | 18/0/0 | false | `2026-11-16T02:03:59Z` | the commit carrying this section |

No blob anywhere carries `gitHash 3e4e825`; the same sweep finds no blob with `passed: 16` either,
which is the shape D032 already annotated for `7ecc1f1`'s 16-0-1 — "that run's log was overwritten by
the next one". What is decidable is narrower than either claim: the tree committed at `a91ebda`
(21:39:36 local) still holds the 17-step `f867b72` file, so either no 18-step run executed at
`3e4e825`, or one ran, was left out of every commit, and was overwritten two minutes later by the run
recorded at `a91ebda`. Nothing in the repo separates those two histories, so the countable statement
is about **evidenced** runs: four before this lane, `1a418a1` the fifth, four of the five on a clean
tree. README (`:91`) and `implementation/FINAL-REPORT.md` (`:56`) had copied one another's list and
disagreed about `3e4e825` — one named it as a run, the other did not — and both now recite the five
rows above. D032's row at `:1772` is annotated rather than rewritten, because the mistake is
part of what that lane measured, and a record that reports a field value for bytes nobody can re-read
is the defect this lane exists to catch — one level above the CLI sites it closed.

This section sits below the D036 heading, so `:2409` still names it and the only inbound citation into
this file — `DECISIONS.md:116`, twice, from `docs/IMPLEMENTATION-ARCHITECTURE.md:414` and `:869` — is
above everything it grew. The file is 2 669 lines at the commit carrying this section.

## D037 — 2026-11-17 there is an installer for the first time, and the first release build of `flux spout` crashes where every debug build passed

AT082 has been `implemented_unverified` since the ledger existed, and the reason was not a missing
test but a missing *object*: `apps/desktop/src-tauri/tauri.conf.json` carried `"bundle": { "active":
false }`, and `cmd_package` built `cargo build --release -p flux-app --no-default-features` and wrote
`{ ok, exe, exists }`. So the word "package" named a CLI binary, `bundle.active = false` meant there
was nothing to install, and the row's clean-install / no-dev-PATH clauses had no artifact to be run
against — which is also why E4's `msvcp140*.dll` question stayed a hypothesis. This lane builds the
thing, gates the building of it, and then reports which halves of AT082 still need a second machine.

Five decisions, taken with the owner's "do all" and his recommended defaults: a plain-feature build
with opt-in feature forwarding; NSIS `currentUser` as the primary container (MSI opt-in);
exe-relative FFmpeg resolution with a visible refusal and **opt-in** bundling, because shipping
~100 MB of FFmpeg is a licensing call, not a build default; unsigned accepted with SmartScreen
documented rather than worked around; `embedBootstrapper` for WebView2.

| file | what changed | evidence it is real |
|---|---|---|
| `crates/platform/src/layout.rs` (new, 7 tests) | `exe_dir()` `:18`, `candidates()` `:64`, `locate()` `:81`, `locate_with_env()` `:88` — every sidecar resolves against `current_exe()`, then `bin/`, `resources/`, `ffmpeg/`, never the working directory | its own unit tests, which fail on a `cwd`-based base |
| `crates/integrations/src/{spout,ndi,ndi_io}.rs` | `LoadLibraryExW` + `LOAD_WITH_ALTERED_SEARCH_PATH` (`spout.rs:285`, `ndi.rs:198`) replaces `LoadLibraryW` + `set_var("PATH", …)`; candidates go through the layout | `crates/integrations/src/tests.rs:127` `sidecars_resolve_through_the_layout_not_the_working_directory` |
| `crates/media/src/lib.rs` | FFmpeg found the same way; absent means a refusal, not a silent fallback | the media unit tests in the workspace run |
| `apps/desktop/{package.json,package-lock.json}` (new) + `ui/package.json`, `ui/package-lock.json` (deleted) | the npm root moved **out of** `frontendDist`, because the NSIS bundler refuses a `frontendDist` that contains `node_modules` | `cargo:rerun-if-changed=../package.json` / `../package-lock.json` (`build.rs:154-155`), pinned by the watch-set gate in `src-tauri/src/lib.rs` |
| `tauri.conf.json:24-44` | `active: true`, `targets: ["nsis"]`, `installMode: currentUser`, `webviewInstallMode: embedBootstrapper`, `resources: []`, and a non-null
`csp` | the artifact itself, hashed into `evidence/package.json`; the `csp` is gated by nothing but
the launch leg below, which renders the editor from a payload built with it |
| `crates/xtask/src/main.rs:1532` `cmd_package` | parses `--bundles` / `--features` / `--with-ffmpeg`, builds the release CLI **and** the bundle, discovers the artifacts, writes `artifacts[]` with sha256/size/mtime; exit 0 installer, 1 build failed or bundler-exited-0-with-nothing (`status: "no_artifact"`), 2 bad usage, 3 no Tauri CLI (`status: "blocked_external"`) | 4 tests in `mod packaging`, two of them proven red by mutation |
| `.github/workflows/ci.yml`, `infrastructure/verify.ps1`, `AGENTS.md` | the verb joins the canonical command list, a CI step, and a Windows-gated `Step 'installer-bundle'` that `Blocked`s where NSIS cannot exist | `cargo run -q -p xtask --bin requirements -- --check` reads all three carriers |
| `crates/xtask/src/bin/requirements.rs:334`, `:556`, `:622` | AT082's evidence list, its blocker arm, and its freshness case (9 → 10 entries: `["package.json","nsis","layout"]`) | 22 tests in the `requirements` bin, incl. `:665` `no_acceptance_row_cites_a_command_as_its_evidence` |

### Why `custom-protocol` is restated rather than trusted

`tauri-2.12.5/build.rs` computes `let dev = !custom_protocol` and prints `cargo:dev={dev}`;
`tauri-build` turns that into `cfg(dev)` through `DEP_TAURI_DEV`. With the feature off, the shipped
binary serves `frontendDist` off disk — on a user's machine that is a blank window. `tauri build`
adds the feature itself (measured in this lane's `smoke/T2-tauri-build-1.log:8`, which is untracked,
so the line is quoted rather than cited:
`Running [tauri_cli] Command \`cargo build --bins --features tauri/custom-protocol --release\``), so
`bundle_features()` re-declares it and refuses to duplicate it, which turns a Tauri implementation
detail into a build assertion instead of a coincidence.

### What was actually run on this host

The three AT082 legs that need a machine which did not build Flux were **not** run — creating a
standard-user profile, disabling the network and installing into his account are his calls. What is
measurable from a checkout without touching OS state:

- the NSIS payload was extracted to a temp dir and launched there with `PATH` reduced to
  `C:\Windows\system32;C:\Windows`, cwd `C:\Windows`, and every `FLUX_*` variable removed: window
  created and visible, `:9000` bound, closed via `CloseMainWindow`. That was the 15:50 artifact; the
  same leg is re-measured against the installer the harness rebuilt at 21:22 in "After the harness"
  below, which is the run whose numbers a reviewer should use.
- `flux ndi` in that same extracted layout loaded `Processing.NDI.Lib.x64.dll` exe-relatively and
  returned a discovery list — the clause "DLLs found without dev PATH" answered for one runtime.
- the 7-Zip census of the installer: `flux-desktop.exe`,
  `$TEMP\MicrosoftEdgeWebview2Setup.exe` (1,897,688 B — the bootstrapper, hence the network
  dependency), five `$PLUGINSDIR` entries, `uninstall.exe`, and **no shim at all**, because
  `bundle.resources` is empty by default. Resolution is fixed; shipping is an owner decision.

### The release-only crash, and what it is not

`flux spout` faults in a release build. Six of six runs of `target/release/flux.exe spout` exit
`0xc0000005` with no stdout; WER bucket `e0543064afd06a0745e212203945e5c7` names
`SpoutLibrary.dll` at offset `0x232e3`. Two hypotheses were on the table and one is dead:

- **Not the loader change.** `git checkout HEAD -- crates/integrations/src/spout.rs` (the
  `LoadLibraryW` + `set_var("PATH")` form), rebuilt release, probed: 5 AVs, same exception. The
  mutated file was then restored byte-exactly and the restore proven against the pre-mutation hash
  `1c2f05ce2f197194c37e0ca67c5e20cffed0f5d54766d2f42cba78fccaa54329`. WER's own `LoadedModule` list
  closes the other half of the question: `spout_shim.dll`, `SpoutLibrary.dll`, `MSVCP140.dll` and
  `VCRUNTIME140_1.dll` all resolve from `apps\spout-bridge`, so the altered-search-path change does
  exactly what its doc comment claims.
- **It is the enumeration, and it is profile-bound.** `target/debug/flux.exe spout` on the same
  bytes prints `Spout sender registry: 0 sender(s)` and exits 0, and so does
  `target/d036/flux-features.exe` — whose control (same exe, cwd `C:\Windows`, shim unreachable)
  exits 1 `SdkMissing`, which is what makes that 0 an observation rather than a skip. The same
  release binary calls into the shim on the main thread without faulting: `flux spout-send` prints
  `offscreen GL ready: true` and only then fails for the unrelated D036 reason. So the fault is in
  the detached worker that `SpoutRuntime::senders_timeout` spawns (`spout.rs:129`), and it has one
  caller in the tree — `crates/app/src/main.rs:889`, the CLI, not the GUI.

Reachability: `spout` is compiled out of `default = []` in both `flux-app` and `flux-desktop`, and no
CI step builds release *with* features, so no artifact this repository has ever produced contained
the crashing path. `cargo xtask package --features spout` would create one, which is recorded in E4
and in `STATE.json` `known_gaps[16]` rather than fixed here: a third-party DLL faulting inside a
worker thread needs its own lane, and a packaging lane should not silently change a spout runtime.

### Controls and residues

Thirteen tests are new (4 `mod packaging`, 7 in `layout.rs`, 1 in the `requirements` bin's case
table, 1 sidecar-resolution test), and the two that could be reddened cheaply were: the mutation
logs are in `smoke/` (untracked) and the hashes they restore against are written above. Two residues
are deliberately not closed by this lane: `embedBootstrapper` needs network on a machine without the
WebView2 runtime — `offlineInstaller` is the alternative and it is an owner call — and
`apps/desktop/src-tauri/icons/icon.ico` is a single 32×32 image, so an installed Flux has no real
icon art. The workspace total moved 233 → 316 (+83) and the attribution is a count, not a proof: the
31 commits between `1a418a1` and HEAD add 78 `#[test]` attributes and this lane adds 13 (4
`mod packaging`, 7 in `layout.rs`, 1 in the `requirements` bin's case table, 1 sidecar-resolution
test), so 91 attributes against +83 executed tests. The two need not agree — a `#[test]` inside a
`#![cfg(feature = …)]` module does not run in a default-features workspace build, and an attribute
added to a file whose test count was already being counted moves nothing — and which of the 8
surplus attributes fall in that category is not enumerated here.

### After the harness

Everything above was written before the harness ran. These are the run's own numbers.

- `infrastructure/verify.ps1`, full and without `-Quick`, from a shell with both SDK variables left
  as this host has them: **`VERIFY: PASS`**, exit 0, `steps: passed=22 failed=0 blocked_external=0`,
  all 22 steps `{ exit: 0, ok: true }`, `verify.json` at `gitHash 6320d3e`, `gitDirty true` (the
  dirty side is this lane's sources, not the evidence), `quick false`, generated
  `2026-11-17T21:22:37Z`. The ceiling moved 18 → 22 and the step *names* say where: diffing this
  file's list against `git show 1a418a1:implementation/evidence/verify.json` gives exactly four
  additions — `installer-bundle` from this lane, and `live-audio-platform`, `live-audio-app`,
  `ui-bundle-cache-buster` from the 31 commits between `1a418a1` and HEAD. `installer-bundle`
  recorded exit 0, so the NSIS leg is now executed by the harness and not merely declared.
- The artifact that step produced: `target/release/bundle/nsis\Flux_0.1.0_x64-setup.exe`,
  **6,372,686 B**, sha256 `c697887a407183a729b598d13b427bc735fc029803b29d2f172ff31173c3ebf4`,
  mtime `2026-11-17T21:22:20Z`, with `features: ["custom-protocol"]` and `resources: []` in
  `evidence/package.json`. It is a different file from the one this lane built at 15:50
  (`909d6134…`, 6,372,285 B): the bundler is not byte-reproducible, so the hash a reviewer should
  census is the one in the evidence file, not a remembered constant.
- 7-Zip census of that installer — 8 entries, and the E4 claim holds exactly:
  `flux-desktop.exe` 19,397,120, `$TEMP\MicrosoftEdgeWebview2Setup.exe` 1,897,688,
  `uninstall.exe` 79,431, `$PLUGINSDIR\{nsis_tauri_utils.dll 34,304, modern-wizard.bmp 26,494,
  System.dll 12,288, nsDialogs.dll 9,728, StartMenu.dll 7,680}` — and no `spout_shim.dll`,
  `spoutdx_shim.dll`, `ndi_shim.dll`, `Processing.NDI.Lib.x64.dll`, VC++ runtime or `ffmpeg.exe`.
- The launch leg re-run against **that** payload (extracted to a temp dir, `PATH` reduced to
  `C:\Windows\system32;C:\Windows`, cwd `C:\Windows`, `FLUX_*` count after removal 0):
  `flux-desktop.exe` sha256 `1357956a2021b751c180361a6bad24c1a26217192465dcda70becf11c4897a7b`,
  PID 14408, window handle 1376×899 at `104,104`, `visible=True`, `UDP 127.0.0.1:9000` owned by
  14408, `CloseMainWindow` → `exited=True exitcode=0`. The capture is a `PrintWindow`
  (`PW_RENDERFULLCONTENT`) grab, and it shows the editor: five-node graph, a live preview inside the
  `Plasma` node, and a status bar reading `217 node types · 274 presets` and
  `build 6320d3e v0.1.0` — the running binary naming the commit it was built from.
- **One leg is declared superseded, not passed.** The first attempt at the same probe used
  `CopyFromScreen` over the window rect and the PNG it wrote contains the desktop wallpaper and a
  Windows Defender Firewall prompt for `target\debug\deps\flux_integrations-828e04100aa51bb4.exe`
  (spawned by the harness's own test binaries, not by the payload). That image is not evidence of
  anything about Flux, so it is named here as inconclusive and the `PrintWindow` leg replaced it.
  The dialog itself was left unanswered: allowing or cancelling an inbound-rule prompt is his call.
- CI-parity legs re-run on the final tree: `cargo fmt --all -- --check` exit 0 with an empty diff
  list; `node apps/desktop/ui/check-bundle.mjs` **29/29**; `cargo test --workspace --
  --test-threads=1` exit 0 at **316 passed / 0 failed / 0 ignored across 41 result lines**
  (30 binaries + 11 doctest lines). The only hit for the `ACCESS_VIOLATION|error\[|panicked` sweep
  over that log is the test *name* `freshness::a_broken_ledger_is_located_not_panicked`, which
  passed — no fault, no compile error, no panic.

The file is 2 818 lines at the commit carrying this section.

## D038 — 2026-11-17 the workflow file has not parsed since `d4e9a29`, so 29 of 49 CI runs died before creating a job

`ci.yml:151` (that blob; `:217` at `3667fd2`, `:228` here) read `run: cargo test -p flux-integrations --features`
`ndi ndi::`. A plain YAML scalar may not end with a colon that is followed by a line break: the
last `:` of `ndi::` is read as a mapping indicator, so `yaml.safe_load` raises `ScannerError:
mapping values are not allowed here … line 151, column 65`, the file yields no jobs at all, and
Actions fails the run in 0 s — measured on run 44189589142, whose `/jobs` array is empty. The
sibling legs at `:139`→`:205` (link) and `:169`→`:243` (spout) carry the same trailing `::` inside
literal block scalars, where it is harmless; `:151`→`:217` is the only line of the breaking shape.

### The census, because the claim I nearly wrote was wrong

The lane notes that first found this said CI "has never executed in this repository's history".
That is false and this section is where it gets corrected. Joined run-by-run — for all 49 runs,
`head_sha` from `gh` against a local `yaml.safe_load` of `ci.yml` at that blob, and `.jobs | length`
per run — the correlation is exact, with no exceptions in either direction:

| class | runs | jobs created |
|---|---|---|
| `ci.yml` at that head parses | 20 | 17 created 1 job, 3 created 2 |
| `ci.yml` at that head does not parse | 29 | 0 jobs, every one |

**49 runs, 49 failures**, and every run in history is still a failure — but the pipeline *did*
execute 20 times, all of them before the break: `git merge-base --is-ancestor <head> d4e9a29` is
true for all 20 heads, tested individually rather than sampled. Parsing by commit touched the file
12 times; it parses at `5eb655a`, `7a7baeb` and `d5d03d3` and at no commit after `d4e9a29`
(2026-11-09 15:01:58 +0100). The remote was created `2026-11-13T10:05:30Z`, so the 20 executing runs
are the pre-break commits being pushed for the first time, and every commit any lane has pushed
since has hit the parser wall. So the honest sentence is not "CI never ran" but: CI ran 20 times,
never once green, and has produced no job at all for 29 consecutive runs.

What the last executed job actually died on is worth keeping, because it is already repaired here.
Run 43514492265, job `verify (windows-latest)`, 15 steps: Set up job, checkout, `dtolnay/rust-toolchain`,
`choco install ffmpeg` and **Format check** all success; step 6 `Clippy (no deny)` failure; steps
7-13 skipped, so nothing after clippy ever reported. Its log ends
`error: could not compile 'flux-catalog' (lib) due to 22 previous errors`, the individual messages
all `approximate value of 'f{32, 64}::consts::PI' found` / `…TAU…` in `crates/catalog/src/generators_*.rs`
under clippy 1.100. Those 22 literals now live as named consts with per-const `allow`s at
`crates/catalog/src/builders.rs:31-38`, chosen precisely so a lint does not move a shipped parameter
range by 3.2e-3 — which is why this host's `cargo clippy --workspace --all-targets -- -A warnings`
exits 0 (measured today, clippy 0.1.99).

### The repair

One line, and the form is the point. A double-quoted scalar keeps the step's value byte-identical —
the parsed run value reprs as exactly `'cargo test -p flux-integrations --features ndi ndi::'` —
whereas the `run: |` block form the other two legs use would hand the runner the same text *plus a
trailing newline*. Repairing a parse failure by changing what a step receives is the wrong trade, so
the value is quoted and the command is untouched. The diff is 1 insertion / 1 deletion, so no
`ci.yml:NNN` citation in the docs moves; with it the file parses into `[verify, link-integration,
ndi-feature, spout-feature]` with 22 verify steps. Re-run after the change: `cargo run -q -p xtask
--bin requirements -- --check` exit 0 still reading 20 `cargo test` lines from 3 carriers
`[3, 6, 11]` — the carrier takes any non-comment line, so quoting the value does not delete the leg —
and `cargo fmt --all -- --check` exit 0. A one-line YAML change touches nothing else in the
canonical list.

### Two things this does not close

(1) The parity target had **no successful run** when this was written. The 49-run census all failed;
this push is the first since `d4e9a29` that can create jobs, and D040 reads what they say: a `verify`
job with all 25 steps green at #50, and at #54 a whole run concluding **`success`** — so "these gates
have never run hosted" is dead, the clippy worry below is retired for this image, and what ends a run
now is not a gate but the job's own clock: #53, byte-identical workflow, cancelled inside the
installer step at 60 min 10 s. The SDK legs, which this section left saying "blocked by design", did
not stay blocked: D039 reads their logs and D040 reads the gate. (2) No gate checks that
workflow parses. The cheapest deterministic form is another `requirements`-bin test that scans
`run:`-keyed plain scalars for a value ending in `:` — deliberately not a YAML-parser dependency,
which would be a build-time crate for one assertion. Left undone because the ask was the repair, not
a new gate.

The file is 2 889 lines at the commit carrying this section.

## D039 — 2026-11-17 the two "optional" legs were not optional and never skipped: they ran on every
push and died on a toolchain path the image does not have, so a machine probe now decides them

D038 repaired the parser, and the payoff is that there were logs to read. What they show is a
comment that no code implemented: `link-integration` carried "Runs only when a prebuilt
`abl_link.lib` is present (e.g. uploaded as an artifact); otherwise skipped — never fails for a
missing SDK." The job had no `if:`, no `needs:`, and no skip path of any kind, and nothing in it
looked for `abl_link.lib` before the clone. So it ran on every push and failed on every push.

### Run 44192530149, job by job (read from the Actions API and the two job logs, not from a lane note)

| job | result | where it stopped |
|---|---|---|
| `ndi (optional)` | **success** | step 4, "Build with ndi feature" — not, as this row first read, the only `success` the repository had: `verify` concluded green in the same run 43 min later, and D040 takes the census |
| `ableton-link (optional)` | failure | step 3 "Fetch Ableton Link and build abl_link"; step 4 reported `skipped` |
| `spout (optional)` | failure | step 3 "Fetch Spout2 and build the C++ shim"; step 4 reported `skipped` |
| `verify (windows-latest)` | **success** | steps 1–7 success — `Format check` and `Clippy (no deny)` among them — and step 8 "Workspace tests" still running when this row was written; the row's own result cell read `in progress` for one tranche, and the job finished 25 entries green at 23:26:55Z |

This is a job-level census, taken because D038's table counted runs: Actions had created 27 jobs in 50
runs — 25 `failure`, this one `success`, and `verify` still without a conclusion when the row above was
written. That is the state at `738641c`; the third push of this lane added four more concluded jobs and
all four are `success`, which the last section of this one reports. That sentence was a snapshot of
four of five: the same push also created `verify`, which concluded `cancelled` at 60 min 10 s, and the
push after it created five more, all `success`. D040's 37-job census is the one to quote.

Neither SDK failure is about an SDK. The link log, after the clone and `git submodule update` have
succeeded, ends `CMake Error at CMakeLists.txt:2 (project): Generator "Visual Studio 17 2022" could
not find any instance of Visual Studio.` — the generator name, not the source. The spout log echoes
its own command, whose second argument is
`call "C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"`,
and 1.4 s later prints `The system cannot find the path specified.` at exit 1. Both jobs run on image
`windows-2025-vs2026` (the first group of each log), and all five `apps/spout-bridge/*.bat` files call
`C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat` on their line
2. The repo's own build scripts moved to the new toolchain; the workflow did not.

### The gate: `sdk_probe` decides per leg, and a leg it cannot do is skipped

The new job runs `vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64
-property installationPath`, then gates each leg on what that machine actually has. `spout` is true if
`<install>\VC\Auxiliary\Build\vcvars64.bat` is a file; `link` is true only if that file exists *and*
cmake is on PATH *and* the version token two directories above the install maps to a cmake generator
name (`2019`→`Visual Studio 16 2019`, `2022`→`Visual Studio 17 2022`, `16` and `17` likewise, and
`18`→`Visual Studio 18 2026`). The legs gain `needs: sdk_probe` plus
`if: needs.sdk_probe.outputs.<leg> == 'true'`, and take the found values through step-level `env:`
(`GENERATOR`, `VCVARS`) instead of interpolating a `needs.*` output straight into a shell line, which
is the Actions script-injection pattern. Choosing *skip* over *fail* is AGENTS.md's rule, not taste: a
missing prerequisite is `blocked_external`, never a pass — and D027 had already settled that the
machine, not an environment variable, decides. The probe prints that itself, in the runner log:
"false means this runner cannot build that leg, so its job is SKIPPED. A skipped job is not a pass."

### Measured on this host before pushing

- The step body was re-extracted from the file by `yaml.safe_load` and executed verbatim, with
  `GITHUB_OUTPUT` pointed at a scratch file: exit 0, four `key=value` lines written, and
  `PROBE: link=true spout=true generator='Visual Studio 18 2026'` with
  `vcvars='C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat'` —
  the same directory the `.bat` files name, so the mapping and both `Test-Path` gates are proven
  against a real VS 18 install rather than reasoned from its layout.
- `cmake -G "Visual Studio 18 2026" -A x64` configures a scratch project at exit 0, and that string is
  in `cmake --help` on cmake 4.4.3, so the generator name was read out of cmake rather than guessed.
- Parse after the edit: jobs `[verify, sdk_probe, link-integration, ndi-feature, spout-feature]`, both
  legs (a PyYAML check only: it cannot read the `if:` expressions — see the next section)
  legs at 4 steps with their `if:` present, verify still at 22 steps, and the ndi run value still
  reprs as exactly `'cargo test -p flux-integrations --features ndi ndi::'`.
- Every `cargo test` command text in the file is byte-identical to before (`shell: pwsh` was the only
  addition), so the freshness carriers are untouched: `cargo run -q -p xtask --bin requirements --
  --check` exit 0 reading 20 lines from `[3, 6, 11]`, and `cargo fmt --all -- --check` exit 0.
- The only tracked file that still names a 2022 toolchain directory is `ci.yml:133`, inside the comment
  describing this defect; the other grep hits are under `_upstream/`, matched by `.gitignore:4`.

### The first push of this gate produced zero jobs, because a hyphen is not a property access

`801683e` was pushed, and its run 44196592517 failed in the same second it was created with an empty
`/jobs` array — exactly the D038 shape, for a different reason. The probe was introduced as job id
`sdk-probe` and both legs referenced `needs.sdk-probe.outputs.link`: the Actions expression grammar
accepts only an identifier after a `.`, and a hyphen is not one, so the whole file is rejected before
any job exists. PyYAML has no expression grammar, so the parse in the list above is a *YAML* check and
says nothing about the `if:` strings, which are a second language inside the file — the error was
writing it up as if it covered the workflow. The id is now `sdk_probe`, an underscore is a legal
identifier character, the display name stays `sdk-probe`, and the four `needs.*` references moved with
it. Reasoned, not measured: that this was the last remaining parse fault; the next run's job count is
the observation that will say so, and no check-run for that run is reachable through the API to quote
the message.

That reasoning did not survive the next push. `0fdae0b` is run 44196954753: completed in the same
second it was created, `/jobs` empty again, for a second and unrelated fault. The link test step
already ended with `shell: pwsh`, and this change had added `shell: pwsh` above its `run:` block, so
the step carried the key twice. Actions' parser treats a duplicate mapping key as an error; `safe_load`
treats it as a reassignment and keeps the last one — which is why the identical local check passed a
second time while the hosted file could not be read at all. The added line is gone and the step keeps
its original, and the pre-push battery is now a loader whose mapping constructor raises on a repeated
key (nine lines, subclassing `yaml.SafeLoader`) plus a scan that every `needs.<id>` dotted reference is
a legal identifier and every `needs:` names a job that exists. Both faults were invisible to the
instrument that reported success, and each one cost a CI run to find.

D038's line numbers moved with this edit and were re-resolved, not recomputed — against the file at
`801683e`, which is how three of them were stale again before this section was committed: at
`3667fd2` the anchors are `:217` (ndi), `:205` (link) and `:243` (spout), and D040's table says why.

### Five things this does not close

(1) There was still no green hosted **run** when this was written — the parity job had gone 25/25 at
#50, so that reason was already gone, and D040 reads the run that replaced it: #54, `success`, and #53
beside it `cancelled` by the clock rather than by a gate. (2) When this list was first written the probe had never run
on a runner — two pushes tried and neither workflow was readable; the third push got it to execute,
and the next section reads what it said. (3) A skip is not coverage: even when both legs run they
build from a fresh `_upstream/` clone, while the vendored `abl_link.lib` and `spout_shim.dll` stay
gitignored — so hosted CI still never compiles the bytes `verify.ps1` gates here, and closing that
needs an artifact hand-off or a runner that holds the SDKs, which is an owner call. (4) No gate checks
that the workflow parses (D038's residue, un-asked) — and it is no longer hypothetical, because this
section's own gate failed to parse twice and each fault was invisible to the check that reported it
fine. Nor does any gate check that a leg's `if:` names an
output key the probe actually writes; a typo there skips a leg quietly forever, the same silence class
as a gate that exits 0 on the broken artifact. (5) The spout step's `cmd /c` quoting survived its own
suspicion: given the probed path it compiled and linked the shim and the whole leg ended success, so
the fault was the path and nothing about the quoting.

### What the probe said on a runner, measured on the third push

`8c7ed59` is run 44197396087 and it created **five jobs**, which is the first thing in this lane that
proves the workflow parses. Job 134442296156 (`sdk-probe (decides the optional legs)`) ended success
having printed

    PROBE: link=true spout=true generator='Visual Studio 18 2026'
    PROBE: vcvars='C:\Program Files\Microsoft Visual Studio\18\Enterprise\VC\Auxiliary\Build\vcvars64.bat'

and both legs then entered `in_progress`, not `skipped` — the gate decides from the machine, and this
machine can do both. Look at what the second line says: the runner's install is the **Enterprise**
edition at the same `18` major as this host's Community one. The path that had failed for four lanes
was right about the edition and the drive letter and wrong about the version, which is precisely why
nothing in the gate repeats a path. Both legs then finished: `spout (optional)` and
`ableton-link (optional)` each ended **success** with every step green — the first hosted feature-leg
successes this repository has had, because before this push every one of those two jobs that ever got
to run had failed. The steps that had never passed now pass for both: the shim compiles against the
probed `18\Enterprise` vcvars, and cmake configures Ableton Link with the probed generator. What the
gate's other branch does — a runner with no MSVC, the job skipped — is still unexercised, because this
image has the toolchain. Still open: whether the vendored `abl_link.lib` and `spout_shim.dll` ever
reach CI at all (both legs build fresh clones, not these bytes), and the nine-line duplicate-key
detector that would have saved the two dead runs, which lives in a scratch directory instead of in
the repository.

The file is 3 029 lines at the commit carrying this section.
## D040 — 2026-11-18 the gate ran, one run came back green and one ran out of clock, so this is what a hosted installer actually carries, byte for byte

D039 stopped mid-sentence at "both legs then entered `in_progress`". This reads the rest of that run,
the run pushed after it, and the two artifacts each run uploads. Everything below comes from the
Actions API, from job logs, or from files downloaded between 23:30Z and 01:20Z. The first draft of
this section was written at the start of that window and left two jobs pending; this rewrite reads
their conclusions, taken at 00:14Z. The commands are given so a reviewer can
re-run them rather than take the table on trust.

### The parity target has been read green — as a job

Run #50 (`44192530149`, head `738641c`): the `verify (windows-latest)` job `134425386086`
**completed `success`**, started 22:41:47Z, finished 23:26:55Z (45 min 8 s), with **25 step entries
and no entry that is not `success`**. That is worth more than it reads, and the reason is a census of
the twenty earlier runs that got as far as creating a `verify` job: **every one of them died at step
7, `Clippy (no deny)`**, with no other failing step in any of them. So #50 is not the first green
*job* in the run (`ndi` finished 43 minutes earlier) — it is the first time that job's step 8
onward has ever executed on a runner:

`Workspace tests` (8) · `Catalog validation` (9) · `Doctor (capabilities)` (10) ·
`GPU catalog shader compile` (11) · `Shader harness export (Naga validate)` (12) ·
`Requirements ledger freshness (generated file matches its generator)` (13) ·
`Max for Live patch structure` (14) · `UI bundle cache-buster` (15) · `Media fixtures` (16) ·
`Desktop GUI build` (17) · `Release build` (18) · `Windows installer bundle` (19) ·
`Live-audio platform tests (WASAPI capture)` (20) · `Live-audio app tests` (21)

This is AGENTS.md's rule about the first check step, demonstrated rather than asserted: while one
early leg is red, no gate below it is reported at all, and this repository carried that blind spot
through twenty runs.

Those 25 against the 22 D038 counted in the file is not a contradiction, and it was checked rather
than assumed: entries 2–23 *are* the file's 22 steps, and entries 1 (`Set up job`), 46
(`Post Run actions/checkout@v4`) and 47 (`Complete job`) are Actions' own — the numbering skips
24–45 because Actions reserves those slots for the post steps it did not need.

The run #50 belongs to concluded `failure`, because it predates this gate and its two ungated legs
died at 22:42. So the sentence that replaced D038's residue (1) when this section was first written
read "a hosted *job* has now run the canonical list end to end and gone green; a hosted *run* has not
yet concluded green" — written at 23:41Z, before either of the two pending jobs had concluded, and
retired by the next section with the measured verdicts. D038's standing worry that "a hosted clippy
newer than 0.1.99 can still redden step 6" is retired twice over: the step (the file's 6, hosted's 7,
since Actions prepends `Set up job`) came back green in all three `verify` jobs read here on a runner
whose rustc is **1.100.0** — newer than this host's 1.99.0, which the `doctor.json` row below
measures — and its command is `cargo clippy --workspace --all-targets -- -A warnings`, so a warning
level cannot redden it by construction.

### Runs #53 and #54: one run green, one run out of clock, and a census that supersedes D039's

`8c7ed59` is #53 and `3667fd2` is #54 (`44198109118`). Each created five jobs. In each, the probe,
`ndi`, `spout` and `ableton-link` concluded **`success`** — four legs green, twice, which is the gate
doing its job rather than being argued for. Both `verify` jobs have since concluded, read at 00:14Z:
#54's ended **`success`** with all 25 step entries green in 55 min 1 s (23:17:55Z → 00:12:56Z), and
#53's ended **`cancelled`** at 60 min 10 s (23:13:06Z → 00:13:16Z) — steps 1–18 green, step 19
`Windows installer bundle` killed mid-step, steps 20–23 reported `skipped`. The run conclusions
follow their jobs: **#54 is `success`**, the first green hosted run this repository has had in 54
runs, and #53 is `cancelled`, which is not the same verdict as `failure` and is not a red gate.
A third data point arrived after this sentence was written: #55 (`44211444352`, at `e4578c2`) is the
second run to conclude `success`, and its `verify` job took **59 min 54 s** — six seconds inside the
bound this section calls marginal. D041 re-takes the census below rather than leaving it here.

What is measured here is not only the green. The two jobs ran the **same workflow**:
`git rev-parse 8c7ed59:.github/workflows/ci.yml` and the same against `3667fd2` both give
`47a50c8c694eeca46852c5d5024df2ae87ad2d1a`, and `git diff --stat 8c7ed59 3667fd2` is DECISIONS.md and
STATE.json and nothing else. Same image, same steps, started 4 min 49 s apart — and one finished just
under five minutes inside the `timeout-minutes: 60` bound while the other did not finish. So the
parity target is met, and the way it was nearly not met is now a measured fact rather than D038's
guess: the margin left by the slowest completed job is 4 min 59 s, and the tree cannot do anything
about it.

Re-taken census at 00:14Z, `gh api --paginate /actions/runs?per_page=60` (54 runs, all of them this
workflow) joined to `/jobs` per run: 23 runs created jobs, **37 jobs, all 37 concluded — 25
`failure`, 11 `success`, 1 `cancelled`**. Every one of the eleven successes belongs to #50 (ndi,
verify), #53 (probe, ndi, spout, link) or #54 (those four plus verify), and the single `cancelled` is
#53's verify. Run level: 52 `failure`, 1 `cancelled`, **1 `success`**. D039's table cell that called
`ndi` "the only job … that ended `success`" is superseded by its own run, and the `verify` row of the
same table, which read `in progress`, has a conclusion now.

### CI's installer against this host's: 8 entries, 7 of them the same bytes

`gh run download 44192530149 -n installer-windows` gives a 6,312,298 B zip holding one file,
`Flux_0.1.0_x64-setup.exe`, **6,329,553 B**. This host's, recorded by `evidence/package.json`
against a tree at `6320d3e` with `gitDirty true`, is 6,372,686 B (`c697887a…`). Those two were
extracted with `7z x` and hashed entry by entry:

| payload entry | hosted (#50, `738641c`) | this host (`6320d3e`) | verdict |
|---|---|---|---|
| `flux-desktop.exe` | 19,085,312 · `3c3ea42a…` | 19,397,120 · `1357956a…` | **differs** (+311,808 B local) |
| `$TEMP\MicrosoftEdgeWebview2Setup.exe` | 1,897,688 | 1,897,688 | identical `cf2f2445…` |
| `uninstall.exe` | 79,431 | 79,431 | identical `a07a736d…` |
| `$PLUGINSDIR\nsis_tauri_utils.dll` | 34,304 | 34,304 | identical `5ba143b5…` |
| `$PLUGINSDIR\modern-wizard.bmp` | 26,494 | 26,494 | identical `3ad2dc31…` |
| `$PLUGINSDIR\StartMenu.dll` | 7,680 | 7,680 | identical `39870682…` |
| `$PLUGINSDIR\System.dll` | 12,288 | 12,288 | identical `afe032f9…` |
| `$PLUGINSDIR\nsDialogs.dll` | 9,728 | 9,728 | identical `9c76f4d5…` |

Same entry set, same names, seven byte-identical, and one difference: the app binary. The green run
lets the two *hosted* builds be compared with each other, which says what that difference is.
`gh run download 44198109118 -n installer-windows` is a 6,307,484 B zip holding a **6,324,721 B**
`Flux_0.1.0_x64-setup.exe` (`f9955fd8…`); extracted with the same `7z x`, its eight entries are the
same eight names, seven of them identical to the #50 bytes above, and `flux-desktop.exe` is
19,085,312 B `742602e2…` — the same size as #50's and **32 bytes different**. `cmp -l` puts those 32
bytes in three kinds of place, and every kind is metadata: eight bytes are four copies of the PE
`TimeDateStamp` with only its low two bytes changing (at 256, 16,016,084, 16,016,112 and
16,016,140 — they decode to 1794957880 = 23:24:40Z and 1794960612 = 00:10:12Z, each inside its own
job's window); eight bytes are the commit string the binary embeds, in its two places at
3,761,899 (`641c` against `7fd2`) and 3,761,906 (`7386` against `3667`), which is
`apps/desktop/src-tauri/build.rs:79` running `git rev-parse --short HEAD` into `FLUX_GIT_HASH` for
the `gitHash` row of `apps/desktop/src-tauri/src/lib.rs:708`; and the last sixteen are the CodeView
PDB GUID at 16,017,744, immediately before the `flux_desktop.pdb\0` it names. The two trees differ in
six files and none of them is an input to that crate — `git diff --stat 738641c 3667fd2` is ci.yml,
AGENTS.md, README.md, one doc and the two evidence files — so 19,085,280 of 19,085,312 bytes of a
CI-built app binary came out of two runners 45 minutes apart unchanged, and the delta a reviewer
would call "a different build" is a clock, a hash and a GUID. That hash string is what the running app
reports as `gitHash` (`src-tauri/src/lib.rs:708`), so an installed Flux names its own commit out of its
own bytes — the payload has no provenance file in it, and the eight entries above are the whole of it.

This host's `1357956a…` is a different case, and the sentence this section first wrote about it —
"built from a different commit on a different machine" — has its first half measured and its second
half not: `6320d3e` is six commits *behind* `3667fd2` (`git rev-list --count`), and that range does
touch the crate, 15 source files deep including `apps/desktop/src-tauri/Cargo.toml`, `build.rs` and
`src/lib.rs`
(`git diff --stat 6320d3e 3667fd2 -- '*.rs' '*.toml' '*.ts' '*.js' '*.html' '*.wgsl' '*.css'`). So
the +311,808 B has a source cause available; nothing in this tranche separates the source part of it
from a runner part, and no control here should be read as doing so.

Note what the totals do *not* do — the payload differs by 311,808 B while the installers differ by
43,133 B, because the NSIS data block is `LZMA:23` and solid, so file sizes do not propagate. The
#50/#54 pair measures the same thing from the other end and harder: 32 payload bytes, of which 24 are
GUID and hash text, moved the outer installer by 4,832 B. Anyone comparing two installers by their
outer size is comparing compression, not contents.

What that is evidence for: the `Windows installer bundle` leg of the canonical list —
`cargo run -q -p xtask -- package` — runs to completion on a clean checkout of a committed tree on a
hosted image, and what it uploads carries the WebView2 bootstrapper (`$TEMP` entry, so
`embedBootstrapper` is in the hosted bytes too, not only in this host's) and carries **no**
`ffmpeg.exe`, no `spout_shim.dll`, no NDI or Link DLL and no VC++ runtime, which is `resources: []`
visible from outside the repository. One line of the #54 log belongs to AT082's offline clause and
should not be read backwards: at 00:10:13Z the bundler prints `Downloading
https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip` and then the
`nsis_tauri_utils-v0.5.3` DLL — that is the *build* reaching the network for NSIS itself, not the
installer reaching it for anything, and the `$TEMP` bootstrapper entry above is the only network
behaviour that ships. What it is not evidence for: nothing in CI installs it, launches
it or shows a window. AT082's title is a standard-user, offline, uninstall-preserving install on a
machine that did not build Flux, and its blocker still names exactly the legs that have not run; the
`installer-bundle` step in `verify.ps1` and this hosted step both prove the build, not the install.

### One control that validates the procedure, and one instrument that did not

The locally extracted `flux-desktop.exe` hashes to `1357956a…`, which is the value D037 recorded for
the same file when it launched it as a payload. Re-extracting today reproduces it exactly, so the CI
above comes from the same operation that produced D037's, which is the only reason the comparison
means anything.

The instrument that did *not* work is worth its own paragraph. `7z l -slt` prints a `Size` field for
these entries that is not the file size — it reported `$PLUGINSDIR\StartMenu.dll` at 13,316 B where
the extracted file is 7,680 B — which contradicted D037's census and made this lane's first draft of
that table wrong in one row. The `-ba` form is worse: its two size columns shift per row, so a
`paste`-based pairing hands names the wrong numbers. D037's numbers are right (all eight, confirmed
by extraction) and the listing is the unreliable witness; for a solid NSIS archive, extract and hash.

### `evidence-windows-latest`: 28 of its 33 files are this host's, and five are the runner's own

The other artifact is 622,401 B from #50 and 622,416 B from #54, 33 files each. Compared file by file
against `git show <head>:implementation/evidence/<file>` with line endings normalized on both sides —
the artifact is CRLF on disk, 4,774 B with 108 CR where the blob is 4,666 B with none, which is
`core.autocrlf=true` on `windows-latest` and the same reason `requirements.rs` normalizes before it
compares — **28 of the 33 are the checkout copy**, `verify.json` among them, and this section's first
draft called that "byte-identical", which is the wrong word for a pair that differs by 108 bytes: what
is identical is the *blob*, i.e. `git hash-object` of the downloaded file returns
`d70d77b55a04cbb78988293ca9453b0790c03ace` because git's clean filter ran first. That file's fields are
`gitHash 6320d3e`, `gitDirty true`, `generatedAt 2026-11-17T21:22:37Z`,
`counts {passed 22, failed 0, blocked_external 0}` — the run this host made 1 h 55 min before #54's job
started. `ci.yml` never executes the script (the three grep hits for `verify.ps1` in it are all comment
lines), so those 28 hosted files are a *copy*, not a measurement, and anyone citing "CI: 22 steps
passed" from that JSON is citing this host.

The five that are not a copy are the five CI's own steps write, and they are the first hosted
measurements this repository has had:

| hosted file | what the runner wrote (`3667fd2`, #54) | this host's tracked copy |
|---|---|---|
| `catalog.json` | 217 entries = 52 effects + 165 generators, `duplicates []`, `missingImplementation []` at 23:45:58Z | the same 217 |
| `doctor.json` | rustc **1.100.0** (0ef1bc386 2026-11-09), GPU **Microsoft Basic Render Driver**, backend **Dx12**, `deviceType Cpu`, driver 10.0.26100.33648, FFmpeg `C:\ProgramData\Chocolatey\bin\ffmpeg.exe`, seven encoders true | rustc 1.99.0 (b940084d7), **RTX 2080 Ti**, **Vulkan**, `DiscreteGpu`, 560.94 |
| `test-gpu.json` | profile `baseline`, `compiled: 217`, `failures: []` — on that CPU device | 217 on the RTX |
| `fixtures.json` | `test_10s_60fps.mp4`, `test_2997.mp4`, `sine_1k_48k.wav` under `D:\a\genfx\genfx\fixtures/generated` | the same three names |
| `package.json` | `bundleExit 0`, `bundles ["nsis"]`, `resources []`, artifact **`f9955fd8…`, 6,324,721 B** at 00:10:25Z, `gitHash 3667fd2`, `gitDirty true` | `c697887a…`, 6,372,686 B, `gitHash 6320d3e`, `gitDirty true` |

That `package.json` row closes a loop the installer table above only asserted. The sha256 the runner
recorded for the file it built — `f9955fd8…` — is the sha256 of the bytes `gh run download` gave me
two hours later, computed by different code at different times; #50's pair agrees the same way
(`468f538f…` = 6,329,553 B). "CI built the thing CI uploaded" now has two independent witnesses.

Three findings come only from those five files, and none of them was knowable from the step list.
(1) The runner's toolchain is **1.100.0** and this host's is 1.99.0, so the "a newer toolchain than the
verifying host's" worry is the standing state, not a forecast; its measured effect on this tree is one
warning this host has never printed — `unused field 'rust-version' in 'workspace.package'`, with
cargo's own note that `cargo::unused_workspace_package_fields` is warn-by-default — and the warning is
right: `Cargo.toml:20` sets `rust-version = "1.85"` under `[workspace.package]`, none of the 12 members
says `rust-version.workspace = true`, and the only declaration that binds is
`apps/desktop/src-tauri/Cargo.toml:6` repeating the literal, so the workspace has been advertising an
MSRV it enforces on one crate. A warn-by-default cannot redden a step whose command is
`cargo clippy --workspace --all-targets -- -A warnings` (`ci.yml:55`), which is the point of the
sentence rather than its comfort. Steps 7 and 8 came back green on 1.100.0 in all three runs read here. (2) The
hosted `test-gpu` green is a **software DX12 device**, so what CI proves about shaders is Naga → DX12
for all 217 entries — coverage no host-side run has claimed, since this host only ever compiles them
for Vulkan — and it proves nothing about the RTX path, and `--profile baseline` on a `Cpu` device is
not a frame-budget measurement in any sense. (3) All five say `gitDirty true` at a freshly cloned,
committed tree. The UI step says otherwise on the runner's own lips (`bundle cache-buster checks:
29/29 passed`, "REAL TREE: index.html has one tag and already matches the committed bundle"), the
evidence directory is excluded from the scope by D026, and no step prints `git status --porcelain`, so
the verdict is unattributable from anything CI records. A provenance field that reads `true` on a clean
checkout is a field that does not mean what its name says, and it is the reason the two `gitDirty`
cells in the row above are not evidence of anything.

The hosted test totals are in the job log rather than in any evidence file
(`gh run view --log --job 134444792239`, 1,121,942 B): step 8 `Workspace tests` printed **41
`test result: ok` lines totalling 316 passed**, step 20 `Live-audio platform tests` 2 lines / 13,
step 21 `Live-audio app tests` 3 lines / 81, and there is no `test result: FAILED` in the file — the
one grep hit for `panicked` is a test *name*, `a_broken_ledger_is_located_not_panicked`. Re-run on
this host at 01:33Z against this same tree — `cargo test --workspace -- --test-threads=1`, exit 0 —
the identical command prints the identical **41 result lines and 316 passed**, so the two sides of the
parity claim now carry one number instead of two, and this invocation did not hit the native fault
AGENTS.md counts.

### Six citations this tranche moved, re-resolved rather than recomputed

`ci.yml` went 111 → 170 → 244 lines across this lane: five commits (`6f00058`, `3df8bd3`, `55200c0`,
`7e39ced`, `280842f`) added 59 lines for the live-audio and UI-bundle gates and the installer step,
then the probe and the two gated legs added 74 more. Six documents still pointed at old coordinates. Each was
checked by printing the line a citation lands on, not by arithmetic:

| where | claimed | true at | true now |
|---|---|---|---|
| D038 (twice, and D039's re-resolution) | `ci.yml:215` / `:203` / `:241` | `801683e` | `:217` / `:205` / `:243` |
| D032's residue paragraph | `ci.yml:29`/`:31` "unchanged" | `1a418a1` | `:42` (fmt), `:44` (clippy) |
| D035's bullet | `ci.yml:46` runs `requirements -- --check` | `1a418a1` | `:59` |
| `implementation/STATE.json:131` | `ci.yml:31` runs clippy | `1a418a1` | `:44` |
| `docs/PERFORMANCE-READINESS.md:1150` | `ci.yml:29` is the first check step | `1a418a1` | `:42` |
| `PLANNING/AUDIO-REACTIVE-LANE-PLAN.md:116` | mirror "the `ndi` pattern (`ci.yml:85-92`)" | `d4e9a29` | the `ndi` job is `:209-217` |

D038 and D039 carry the corrected numbers in place; the last three are corrected in their own files.
Two things came out of doing it. (a) A citation qualified "at HEAD" is a claim about a moving target:
D039's paragraph re-resolved correctly, against the file at `801683e`, and was two commits stale by
the time it was committed — the qualifier has to name a blob. (b) The `verify.ps1:NNN` citations
were run through the same check: the six in D032's sentence (`:32`, `:35`, `:47-61`, `:77`,
`:81`, `:99`) plus `:123`, `:136` and `:201-203` all land on their subject, and the one dead thing
found is a *name* — D025's `Test-LinkLib` (`:77`) is `Resolve-LinkLib` at `:81` today, and `:77` is
the comment above it. Left as written, because it is D025's contemporaneous wording, not a claim here.

### Six things this does not close

(1) Was: a hosted *run* that concludes `success` had happened — #54, 00:12:57Z — and what did not close
is the margin around it: three `verify` durations, 45 min 8 s / 55 min 1 s / 60 min 10 s-cancelled,
against `timeout-minutes: 60` at the file's `:19` then. D041 closes the bound: its fourth measurement,
#55 at 59 min 54 s, lands six seconds inside that same bound, and `verify` now carries
`timeout-minutes: 75` at `ci.yml:23`. What that does *not* close is the reason a bound exists at all —
it is the only thing that ends a hung test, so the raise trades up to fifteen more runner-minutes on a
hang for margin the tree cannot otherwise reach, and D041 made that call under "fix anything you need",
not on an owner ruling about wall-clock spend. (2) A skip is still not coverage, and now the
shipping side says the same thing: both legs build fresh `_upstream/` clones, the vendored
`abl_link.lib` and `spout_shim.dll` stay gitignored, and the hosted installer's payload above has no
shim in it at all — so no hosted job compiles or ships the bytes `verify.ps1` gates here. (3) Was: no gate
checked that the workflow parses, or that a leg's `if:` names an output the probe actually writes, and
the nine-line duplicate-key detector that would have caught both dead runs lived in a scratch directory
— and that battery was itself wrong until this commit, because its D038 check tested *parsed* `run:`
values for a trailing colon, which flags the repaired line too (a quoted
`"… ndi::"` still ends in `:`) and cannot see quoting at all. D041 closes this: `.github/check-workflow.mjs`
is in both harnesses with eight legs and eleven controls, its leg 7 reads the *source* text of a plain
scalar rather than the parsed value, and `plainScalarColon` is the control that asserts the quoting half
of this mistake cannot come back. (4) Nothing hosted installs or launches the artifact, so AT082 stays `implemented_unverified` for the
same reasons its blocker names — and `embedBootstrapper` means a first install on a machine without
the WebView2 runtime needs a network, which no CI job tests. (5) The toolchain worry is measured
now, not feared: the runner is on 1.100.0 and this host on 1.99.0, and it survived hosted steps 6 and 7
(`Format check`, `Clippy (no deny)`) in each of #50, #53, #54 and #55 — `gh api …/actions/jobs/<id>` numbers them
that way at #54 and #55, and this sentence said 7 and 8 until D041 checked, which is `Clippy` and
`Workspace tests`. D041's gate is inserted as hosted step 3, so from the next run 6 and 7 *become* 7 and
8; the off-by-one was real, not a renumbering. It is not closed, because `-A warnings` buys silence rather than safety — the
`unused field 'rust-version' in 'workspace.package'` complaint is cargo reading this workspace's own
manifest, no gate anywhere reads it, and a future toolchain that *errors* would land at the same step
for the same reason. (6) The hosted `gitDirty: true` at a committed clean checkout is unattributed:
no step prints `git status --porcelain`, so every provenance block CI writes overstates dirtiness, and
D026's `implementation/evidence/` exclusion cannot be assumed to be the whole rule.

The file is 3 309 lines at `e4578c2`, the commit that wrote this sentence. D041 appends below it, so
that number is a property of the older blob and not of the one carrying both sections; the newer count
is in D041's own stamp.

## D041 — 2026-11-18 the workflow gate two sections called "un-asked" is in both harnesses now, and the bound it ships beside had six seconds left on it

D038 ended with a nine-line duplicate-key detector living in a scratch directory; D039's residual list
called a real gate "un-asked". This asks. It also takes the two things the lane had been carrying as
judgement — the 60-minute bound and an open Dependabot advisory — and turns both into measurements.
Everything below comes from a file in this commit, from `gh api`, or from a command whose full text is
given. The local harness run is `smoke/T17-verify.log`, and `smoke/` is ignored, so its numbers are
written here rather than cited.

### Eight legs, and the eleven controls that say they still fire

`.github/check-workflow.mjs` is Node-only and dependency-free: neither `Cargo.lock` nor
`apps/desktop` holds a YAML parser, so it scans the YAML subset this workflow actually uses (block
mappings, block sequences, block scalars, `#` comments) rather than pretending to be `safe_load`. The
legs, in the order the script prints them:

| leg | asserts | fault class it exists for |
|---|---|---|
| 0 | the file parses as nested block mappings at consistent depth | an over-indented key, a line that is not a key |
| 1 | no mapping scope holds the same key twice | the duplicated `shell:` that emptied run #52's `/jobs` |
| 2 | every `needs.X` is a dotted identifier | `needs.sdk-probe`, which Actions rejects at parse time |
| 3 | every `needs:` entry and `needs.X` names a declared job | a leg renamed without re-pointing its consumer |
| 4 | every `needs.X.outputs.Y` key is declared by X *and* X is in that job's `needs` | the silent class: an empty value makes `if:` false forever |
| 5 | every job output reads from a step id that exists and writes that key | a probe that stopped emitting one output, gating a leg off nobody notices |
| 6 | every job bounds and bases itself, every step has `uses:` or `run:` | an unbounded job, a step that runs nothing |
| 7 | no plain scalar ends in `:` or holds `: ` | D038's own fault: `run: … ndi::` yields zero jobs, zero logs |

Leg 5 is deliberately narrower than it could be. It accepts the write shape this workflow uses — a
PowerShell table entry `k = 'name'` fed to `Add-Content $env:GITHUB_OUTPUT` — and does not accept
`echo "name=$v"`, because nothing here writes that way and a clause no control exercises is coverage
claimed on paper. The comment at the leg says which line to widen if that ever goes red on a real leg.

Each leg is driven against a mutant of the committed file, and the battery compares **leg sets by
equality**, not "did something fire": a mutant that trips an extra leg fails the run. `control` asserts
the real file is clean, `inScalarControl` asserts a duplicated assignment inside a `run: |` body stays
clean — PowerShell, not YAML — and `plainScalarColon` unquotes the one line in the file that ends in
`::` and requires leg 7 *and only leg 7* to fire, which is the quoting half of D038's own mistake
frozen as an assertion. Eleven controls, each printing its own line:

```
node .github/check-workflow.mjs
```

```
leg 0: OK (block mapping over 256 lines, block scalars skipped)
…
workflow check: …/ci.yml clean — 5 jobs, 35 steps, 8 references, 102 plain scalars; 11 controls as required
```

Two things it does not do, stated so nobody reads the green as more than it is: it does not evaluate
`if:` expressions beyond the names inside them, so `== 'True'` compared against a probe that writes
`true` still passes; and its parser is a subset, so a flow-style mapping or a quoting shape this file
does not use today is opaque to it, not checked.

### The same gate in both harnesses, because D032 set the rule

`ci.yml:31-32` runs it as step 2, right after `actions/checkout@v4` and before anything that could burn
a toolchain install, and `verify.ps1:114` runs the identical command as `Step 'workflow-structure'`.
Gating a rule in one harness and not the other is the defect D032 closed for the ledger, so both carry
it. One command per `Step` block, as the trap list requires. It is a `Step`, not a `Blocked` leg: a
Node script that cannot run is a red, not a skip, and the ceiling moves 22 → 23 (D037's run recorded 22,
and this tranche's run below is the first with 23).

Being a `.mjs` and not a `[[bin]]`, it is also visible to `verify.ps1` — the trap that "a green
`verify.ps1` does not mean the workspace suite ran" is about cargo tests in binary targets, and this
gate is not one.

### The run: 23 steps, and the second one is the new one

`env -u FLUX_ABLETON_LINK_LIB -u FLUX_SPOUT_SHIM powershell -NoProfile -ExecutionPolicy Bypass -File
infrastructure/verify.ps1` — D035's form, so the Link and Spout verdicts come from the machine and not
from the shell that happens to hold the variables. It printed `steps: passed=23 failed=0
blocked_external=0` and `VERIFY: PASS`, exit 0. `verify.json` carries `gitHash: e4578c2`, `gitDirty:
true` over its stated scope, `external` all three true, and 23 step records that are each
`{exit: 0, ok: true}` — which is the ceiling the sentence above predicts, read off the run rather than
off the file. The new step is not last in the list, it is second:

```
== workflow-structure ==
leg 0: OK (block mapping over 256 lines, block scalars skipped)
leg 7: OK (102 plain scalars, none in the shape that makes YAML raise)
control control: clean as required
… eleven controls, each printing "fired as required" or "clean as required"
workflow check: .github/workflows/ci.yml clean — 5 jobs, 35 steps, 8 references, 102 plain scalars;
                11 controls as required
```

Then the three gates that read files rather than run code, each with its exit status taken un-piped:

- `cargo run -q -p xtask --bin requirements -- --check` → exit 0, `requirements.json is fresh`, and
  "20 `cargo test` command lines read from 3 carriers … AGENTS.md [3], ci.yml [6], verify.ps1 [11]".
  Adding a `node` line to `ci.yml` moved none of those three numbers, which is why the ledger gate stayed
  green without an edit.
- `cargo run -q -p xtask -- evidence` → exit 0, `33 files, 9 stamped by their writer, 4 naming a commit
  in a header, 20 unattributed, 0 missing provenance`. That is one file more than the 32/19 D036, D033,
  D034 and D035 all reported, and the reason is not the writer: `git ls-tree` gives 32 tracked files in
  `implementation/evidence/` at `1a418a1` (the last run's `headAtRun`) and 33 at HEAD, and the one that
  arrived between them is `live-capture-2026-11-16.txt`, committed six hours *after* `1a418a1` at
  `a8470d4`. So every earlier tally was correct for the tree it was taken on.
- The five `glib` probes in the section below, re-taken with the cargo lock free, each with its own exit
  status: `0` for both `cargo tree` legs, `101` for the `--precise 0.20.0 --dry-run`, `0` for the `7z`
  listing, and empty stdout for both greps.

For the record against the previous blob: the `verify.json` committed at `e4578c2` (this commit's parent,
`git show HEAD:implementation/evidence/verify.json`) reads `gitHash 6320d3e`, 22 steps, no
`workflow-structure` key and `passed: 22` — so the ceiling this section claims really did move 22 → 23,
and the 22 belongs to D037's tree rather than to anything in this tranche.

What this run does **not** measure is the workspace suite, and the two numbers must not be traded.
`verify.ps1`'s own log holds 20 `test result: ok` lines totalling 320 tests, because its `unit-tests`
step is eight `--lib` packages and never `cargo test --workspace`. The workspace figure this document
carries — 316 across 41 result lines — was re-measured at `3667fd2` (that tree plus uncommitted docs), and
`git diff --name-only 3667fd2..e4578c2 -- '*.rs'` prints nothing. This tranche adds no Rust file at all:
markdown, a `.mjs`, a workflow and the harness, so a replay would re-measure identical bytes. The `.mjs`
is not covered by that
argument and was run: it is the step above. The rebuilt installer is
`6,372,806` bytes, sha256 `9ea259ce2b765091bad12329dad9ff2c607a6aac38fe6e75bafcd83532c0e7e0`, and
`package.json` stamps it to `gitHash: e4578c2` with `gitDirty: true`.

### Four `verify` durations, and the six seconds that decided the bound

D040 left `timeout-minutes: 60` in place and called raising it "an owner call about wall-clock spend".
Then #55 ran:

| run | head | `verify` job | wall | conclusion |
|---|---|---|---|---|
| #50 | `738641c` | `134425386086` | 45 min 8 s | `success` (job), `failure` (run: a leg went red) |
| #53 | `8c7ed59` | `134442296098` | 60 min 10 s | `cancelled` mid-step at `Windows installer bundle` |
| #54 | `3667fd2` | `134444792239` | 55 min 1 s | `success` |
| #55 | `e4578c2` | `134490547846` | **59 min 54 s** | `success` |

Every one of the four reports 25 step entries — `Set up job`, the 22 steps the file had before this
tranche, `Post Run actions/checkout@v4` and `Complete job`, numbered 1…23 then 46 and 47 — and within
that array `Format check` is 6 and `Clippy (no deny)` is 7. The next run reports 26 and they become 7 and
8. #53 and #54 overlap: #53 started 23:13:06Z and #54 at 23:17:55Z, so the cancelled run and the green
one are the same workflow content racing the same bound from two pushes five minutes apart.

Read `startedAt`/`completedAt` yourself — there is no `timingDuration` field on `gh run view --json jobs`:

```
gh run view 44211444352 --json jobs -q '.jobs[]|"\(.name)|\(.conclusion)|\(.startedAt)|\(.completedAt)|\(.steps|length)"'
```

59 min 54 s against a 60-minute bound is a six-second margin on a job whose own steps are not the
variable. `verify` now declares `timeout-minutes: 75` at `ci.yml:23`, with the four durations above in
the comment so the next reader does not have to re-take them. The call is made here, under "fix anything
you need", and it is one line to undo. What it trades is stated in D040's list item (1): a timeout is
also the only thing that ends a hung test, so a hang now burns up to fifteen more runner-minutes.

Census re-taken twice over all 55 runs, at 01:55Z and again at 02:46Z with the same result
(`gh api --paginate` on `/actions/runs`, then `/jobs` per
run): **24 runs created jobs, 42 jobs — 25 `failure`, 16 `success`, 1 `cancelled`**, and **two runs
conclude `success`**, not the one D040 recorded. The `runs_with_jobs` count is 24 because 31 of the 55
runs created no job at all — every one of them a workflow GitHub could not read, which is what legs 0, 1
and 7 exist for.

### GHSA-wrw7-89jp-8q8g: in the lock, not in the graph, not in the artifact

The push that carried D040 turned on Dependabot's first alert here: `glib` 0.18.5, medium, unsoundness
in the `Iterator`/`DoubleEndedIterator` impls for `glib::VariantStrIter`, first patched at 0.20.0,
manifest `Cargo.lock`. It is an advisory against a crate this product compiles for a platform it does
not ship, and five commands say so — each run with its exit status read un-piped:

```
cargo tree -i glib --target x86_64-pc-windows-msvc -e normal   # 0, "nothing to print"
cargo tree -i glib --target x86_64-unknown-linux-gnu -e normal # 0, glib v0.18.5 under gtk ← tauri
cargo update -p glib --precise 0.20.0 --dry-run                # 101, gtk 0.18.2 requires glib ^0.18
grep -rn "VariantStrIter" crates apps spec docs implementation # empty (this file excepted)
grep -rn "glib::" crates apps spec docs                        # empty
```

- **Graph:** `glib` is reached only through `tauri → gtk ^0.18 / webkit2gtk ^2`, and those are
  target-gapped dependencies behind a `cfg(any(…))` that names Linux *and* the four BSDs (spelled out
  below). The Windows graph does not contain `glib` at all — that is what "nothing to print" means — so
  nothing here links it, and `cargo update` cannot raise it without `gtk` raising it, which `tauri` pins.
  The newest stable `tauri` on the registry (2.12.6) still requires `gtk ^0.18`.
- **Sources:** `VariantStrIter` and `glib::` appear in no file in this workspace — grep empty. The
  soundness hole is in an iterator over GVariant string arrays; nothing in Flux calls into glib.
- **Artifact:** `7z l -ba target/release/bundle/nsis/Flux_0.1.0_x64-setup.exe` exits 0 and lists eight
  entries — four NSIS plugin DLLs (`System`, `nsDialogs`, `nsis_tauri_utils`, `StartMenu`),
  `modern-wizard.bmp`, `$TEMP\MicrosoftEdgeWebview2Setup.exe`, `flux-desktop.exe` at 19 397 120 bytes and
  the `uninstall.exe` stub. A Rust crate compiled for another platform cannot be in it, and is not.
- **Ship surface:** `apps/desktop/src-tauri/tauri.conf.json` bundles a WebView2 bootstrapper on Windows.
  The Linux stack that would pull in `glib` is not built, packaged, tested or published here.

So the alert is a true statement about a crate this lockfile resolves and a build this repository does
not produce. It is **left open on GitHub** rather than dismissed: dismissing a shared visible state is
the owner's click, not this commit's. A `Cargo.lock` that carries an unreachable vulnerable crate is not
a defect to fix here, and `cargo update` cannot fix it anyway — that is the whole content of this
section, and the five commands above are how to re-check it before repeating any of it.

The `^0.18` is not a coincidence of this lockfile and it is not reachable from here. Re-taken against
the registry with `curl -A … https://index.crates.io/ta/ur/tauri`, the newest stable `tauri` (2.12.6)
and even `3.0.0-alpha.6` both require `gtk ^0.18` under
`cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))`
— so the gate is not merely Linux, it is every BSD-family target, and none of them is what this
repository builds, ships or tests. `cargo update -p glib --precise 0.20.0 --dry-run` therefore exits 101
naming `gtk v0.18.2` as the requirement holder, and the only way to change the answer is to change
upstream.

### Thirteen citation sites in five files, and the pair-half that a regex cannot see

`ci.yml` grew 11 lines and `verify.ps1` 5, so every citation at or past `ci.yml:22` and `verify.ps1:110`
moved. Each was re-resolved by printing the line a citation lands on against the line it named at
`HEAD`, not by arithmetic — and two of them turned out to have been stale *before* this tranche:
`IMPLEMENTATION-ARCHITECTURE.md:883` and the second `ci.yml:29` on `PERFORMANCE-READINESS.md:1150` still
pointed at the fmt step from before D040's own sweep, whose right-hand column they now disagree with.

| where | names | was | now |
|---|---|---|---|
| `PLANNING/AUDIO-REACTIVE-LANE-PLAN.md:116` | the `ndi` job block | `ci.yml:209-217` | `:220-228` |
| `docs/IMPLEMENTATION-ARCHITECTURE.md:883` | the fmt step | `:29` (stale at `e4578c2`: `:42`) | `:53` |
| `docs/PERFORMANCE-READINESS.md:1150` | the fmt step, twice | `:42`, `:29` | both `:53` |
| `docs/PERFORMANCE-READINESS.md:1150` | `requirements-freshness` | `verify.ps1:123` | `:128` |
| `implementation/STATE.json:131` | the clippy step | `ci.yml:44` | `:55` |
| `implementation/STATE.json:134`, `:164` | `requirements-freshness` | `verify.ps1:123` | `:128` |
| `implementation/DECISIONS.md:1136` | the spout Step and its `Blocked` arm | `verify.ps1:201-203` | `:206-208` |
| `implementation/DECISIONS.md:1408`, `:2273` | `requirements-freshness` | `:123` | `:128` |
| `implementation/DECISIONS.md:2274` | ci.yml's `--check` step | `:59` | `:70` |
| `implementation/DECISIONS.md:2822`, `:2959`, `:3235` | the ndi `run:` line, the 2022 comment, clippy | `:217`, `:122`, `:44` | `:228`, `:133`, `:55` |

What is deliberately *not* re-pointed: D040's own drift table and its `verify.ps1:200-202 → :201-203`
sentences. Those record one tranche's arithmetic against named blobs, and rewriting them would destroy
the record without fixing a pointer. A reader who wants today's coordinates reads the table above.
The heading's count is of *sites*, and the three counts are stated so they can be re-derived from the
rows beside them rather than carried: ten rows, thirteen distinct `file:NNN` locations in five files,
seventeen numbers rewritten once the two ranges (`:209-217` and `:201-203`) are counted by their
endpoints. `b05b4b6`'s message says "26 of them, nine carriers", which is none of those and is exactly
the number a later lane would copy — the table and this sentence are the record, the subject line is not.
AGENTS.md already warns that a `file:NNN` regex misses the second number of `:3819, 3829`; the same trap
has a cheaper cousin here — the `/`:31`` half of "`ci.yml:29`/`:31`" matches no pattern at all, so the
sentence around every hit got read rather than the pattern alone.

### Five things this does not close

(1) The gate checks the file it is pointed at. `FILE` defaults to `workflows/ci.yml` beside the script,
and nothing asserts that a second workflow file, if one ever appears, gets its own step — a new
`.github/workflows/*.yml` is unchecked by construction until someone adds a leg for it. (2) It validates
*names*, not *values*: `== 'True'` compared against a probe that writes `true` passes all eight legs and
still skips its leg forever on the runner. That is the silent class half-closed — the typo that renames
is caught, the typo that mis-compares is not, and no static scan of a `${{ }}` string can decide it
without running the probe. (3) The script is proven locally; the runner has not seen it yet. It is file
step 2 of the `verify` job and therefore hosted step 3, behind `Set up job` and `checkout`, and until
that run's log is read this section claims a local exit code and nothing more. (4) `timeout-minutes`
bounds a *job*, not a run, so a queue delay or a hung `sdk_probe` can still spend wall-clock the bound
never touches; and 75 minutes of
margin is margin, not a fix for the installer step's variance, which is still unexplained. (5) The
advisory is disposed as a *risk to this artifact*, not as a state change: GHSA-wrw7-89jp-8q8g stays open
on GitHub (dismissing shared visible state is the owner's click), `glib` 0.18.5 stays in `Cargo.lock`,
and closing it upstream needs `tauri` to move to `gtk 0.20` — which this repository does not control.

The file is 3 572 lines at the commit carrying this section.
