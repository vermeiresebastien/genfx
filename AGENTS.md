# AGENTS.md — Flux / genfx

Working rules for anyone (human or agent) editing this repository. Read this
before changing code or docs. It exists because the repo's status documents
drifted badly from the tree between 2026-11-09 and 2026-11-13.

## What this is

A Windows-first Rust engine and wgpu renderer for audio-reactive video, node-graph
compositing, procedural generation and deterministic offline export, plus a Tauri 2
desktop GUI. The product spec is `spec/` (v2.0). Verifying host: RTX 2080 Ti on
wgpu Vulkan, driver 560.94, rustc 1.99.0 stable, Node 24.19.0, FFmpeg 9.0.

## Truthfulness rules (the important ones)

- **Docs are claims, not evidence.** `implementation/`, `docs/` and `spec/` describe
  intent and history. Before repeating any "verified" line, re-run the command that
  would prove it. If you cannot, say `unverified`.
- **Report measured versus reasoned separately.** A finding you derived by reading
  code is not a finding you measured.
- **A passing suite is not proof of GPU work.** 14 GPU integration tests return early
  on device-creation failure ("no GPU; skipping") and count as passes. `golden.rs`
  asserts file sizes, not pixels.
- **Missing prerequisite is `blocked_external`, never `passed`.** Detector-only is not
  integration. A `.maxpat` is not a `.amxd`. A codec test is not a control path.
- When code and documentation disagree, **the code wins** and the divergence gets
  recorded (see `implementation/DECISIONS.md` D018, `docs/IMPLEMENTATION-ARCHITECTURE.md` §8).

## Canonical commands

Match `.github/workflows/ci.yml`; anything else is not CI parity.

```
cargo fmt --all -- --check
cargo test --workspace -- --test-threads=1        # the flag is required, not optional
cargo run -q -p xtask -- catalog
cargo run -q -p xtask -- doctor
cargo run -q -p xtask -- test-gpu --profile baseline
cargo run -q -p xtask --bin export_shaders
node apps/max-for-live/check-maxpat.cjs
cargo build -p flux-desktop
```

`--test-threads=1` is only in CI and `infrastructure/verify.ps1`; there is no
`.cargo/config.toml` default, so a bare `cargo test` races GPU device creation.
GPU tests are serialised per test binary by a function-local mutex, which means
different files hold different mutexes.

Never wrap a verification command in a pipe or a trailing `echo` — it replaces the
tool's exit status with the echo's. Check the exit code of the thing you are claiming.

## Traps inside this repo's own verification harness

Measured on 2026-11-14 (D019). Do not trust these to tell you the truth without reading
them first.

- **`cargo fmt --all -- --check` was red at HEAD for four days** (56 diffs across 12
  files, from `fe9e35e` 2026-11-10 until the commit that rewrites this bullet). It is the
  first check step in CI, so while it is red the job dies before the test suite runs and
  every later gate goes unreported — which is how "CI is green" survived next to a tree
  that could not be merged. Re-run this step before repeating any CI claim.
- **One command per `Step` block in `verify.ps1`.** `Step` reads `$LASTEXITCODE` once, after
  the whole block, so a `;`-chained pair is judged only by its last command. This was a live
  defect: step `'integration-tests'` chained three `cargo test` runs and recorded only
  `dag_parallel`'s exit. Fixed 2026-11-15 (D020) by splitting them into three steps, and
  `Step` now clears `LASTEXITCODE` before running, because a block containing no external
  command used to inherit the *previous* step's verdict. Keep the rule anyway: never put two
  commands in one `Step` block. Treat a `verify.json` `ok` as weaker evidence than the
  printed `VERIFY:` line.
- **A skipped gate is `blocked_external`, not `ok: true`.** `link-integration` and
  `spout-enumeration` used to write `{ ok = true; skipped = true }` when their env vars were
  unset, which is the repo's own truthfulness rule broken inside the harness. They now write
  `ok = null` plus `status = 'blocked_external'`, and the run prints
  `steps: passed=N failed=N blocked_external=N`. Check that third number before saying a
  gate ran; a machine without the SDKs still exits 0.
- **`verify.json` used to carry a UTF-8 BOM** because `Set-Content -Encoding UTF8` writes one
  in Windows PowerShell, so the "machine-readable" evidence failed a strict `JSON.parse`.
  Measured on the committed file at `75da874` and its predecessor. Written without BOM since
  D020, which is also when `gitHash` and `counts` were added to it.
- **Step names overpromise.** `ndi-discovery` runs `ndi::`, which selects 2 tests that only
  prove the DLL loads and discovery does not crash.
- **`xtask benchmark` numbers need their labels read.** The three harness defects D019 found were
  fixed 2026-11-15 (D021): `--profile` now selects resolution/budget/graph (`ordinary`, `4k`;
  anything else exits 2), render and preview-copy are timed in separate windows, warm-up is
  discarded, and percentiles are nearest-rank over 120 samples. Two traps remain. (1) The single
  number that used to be quoted — `p95ms` per generator — is gone; per-generator rows now carry
  `median`/`max` over 10 samples only, and the merge-style percentiles belong to the 8-node chain.
  (2) Tail numbers move by ~6x between runs on identical work (measured: max 2.96 ms, then
  19.57 ms), so one invocation is not a frame-budget verdict; and `presentation_misses` is
  `{measured: false}` because headless execution has no swapchain.
- A gate pass **rewrites `implementation/evidence/*.json`**, which are tracked. A dirty tree
  after verification is normal and is not a code change.

## Architecture rules that are not negotiable

- **Rust is the only authority.** The GUI is a thin `dispatch` client. Do not
  implement validation, rendering, timing or file I/O in `apps/desktop/ui/`.
  Live control follows the same rule: the OSC listener runs in the Rust process and
  edits the document through `CommandHost`, and the webview is only told an external
  edit happened. Do not route received packets through JS, and do not add a second
  state that the preview reads instead of the document.
- **A tracked UDP port has exactly one owner.** Flux Desktop binds `9000` at startup
  (`FLUX_OSC_PORT` moves it); `flux osc 9000` will then fail to bind, and "the fader
  does nothing" is a port fight rather than a decoder bug. `osc_status` reports
  `bound`, `bindError` and a counter per outcome.
- **The webview ships `app.bundle.js`, not `main.js`.** `src-tauri/build.rs` runs
  `node bundle.mjs` and rewrites the `?v=` hash in `index.html`, and it reports a
  bundle failure as `cargo:warning` while continuing. A green `cargo build -p
  flux-desktop` is not proof the UI change shipped: grep the bundle for the symbol.
- **Exact rational time.** `RationalTime{i64 num, u32 den}`, always reduced,
  numerator serialized as a decimal string. No wall clock in deterministic kernels.
- **The frame ABI is 96 bytes** and is currently hand-written in three places
  (`engine/src/frame.rs`, `render/src/pipeline.rs`, `catalog/src/shader_gen.rs`) with
  nothing binding them together. Editing shader text is an ABI edit. Check all three.
- **The catalog is the single source of truth** for UI metadata, uniform layout,
  shader generation, docs and tests. 217 entries; confirm with `flux doctor`. Adding a
  node means adding a row plus a `process()` body — never touch the renderer to do it.
  Procedure: `docs/IMPLEMENTATION-ARCHITECTURE.md` §3.6.
- Node conventions a shader body must respect: a second image input must be named `b`,
  and a generator must have no `source` input.
- Typed commands carry `expected_revision`; undo/redo must not replay network sends or
  reopen devices.

## Feature flags

`crates/app` and `crates/integrations` both set `default = []`. `link`, `ndi`, `spout`,
`syphon` and `vision` are **compiled out of a plain build**. Do not describe them as
available without checking `flux integrations`.

## Repo hygiene

- **Never commit render output.** `media_out/ rec_out/ sdf_out/ txt_out/ test_out/
  smoke/ fixtures/generated/ apps/shader-harness/shaders.json` and scratch `*.txt` are
  ignored as of 2026-11-14 (eight of those rules were previously dead because they were
  wrapped in quotes). Untrack, do not delete, generated artifacts you did not author.
- Do not rewrite history to reclaim the ~1 GB `.git` without being asked.
- `infrastructure/` holds spent one-shot migration scripts. Several contain absolute
  paths to a directory that no longer exists **on purpose** — for the spent migrations
  that dead path is the only guard against a destructive rewrite of the catalog crate.
  Do not "fix" them. See `infrastructure/README.md`.
- The repo root is `D:\Projects\visual\genfx`. Any path referencing
  `D:\Projects\skeer-audio\genfx` or `D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX` in a
  config or doc is stale; the Max patches and their README were corrected once already.
- Binaries in `target/` are not evidence. If you claim a build, rebuild.

## Commits

One commit per coherent change, with a body that threads the interleaved parts and
states what was verified and what was not. Never push unless asked.
