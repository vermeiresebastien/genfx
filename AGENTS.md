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

## Architecture rules that are not negotiable

- **Rust is the only authority.** The GUI is a thin `dispatch` client. Do not
  implement validation, rendering, timing or file I/O in `apps/desktop/ui/`.
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
