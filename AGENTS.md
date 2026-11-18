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
  asserts file sizes, not pixels. The mirror-image trap is a run that dies halfway:
  measured 2026-11-15 (D034), `cargo test --workspace -- --test-threads=1` exited 139 after 11
  result lines on `0xc0000005, STATUS_ACCESS_VIOLATION` inside `flux-render`'s
  `tests::row_stride_matches_preview_copy`, while that binary alone exited 0 ten times and the
  re-run finished at 222 passed; D035's two invocations printed 227 and D036's two printed 233, all
  without a fault, so the rate is 1 in 7.
  No mechanism is known — re-run before blaming your own change, and never quote a truncated run.
- **Missing prerequisite is `blocked_external`, never `passed`.** Detector-only is not
  integration. A `.maxpat` is not a `.amxd`. A codec test is not a control path.
- When code and documentation disagree, **the code wins** and the divergence gets
  recorded (see `implementation/DECISIONS.md` D018, `docs/IMPLEMENTATION-ARCHITECTURE.md` §8).

## Canonical commands

Match `.github/workflows/ci.yml`; anything else is not CI parity.

```
node .github/check-workflow.mjs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -A warnings
# --test-threads=1 is required on every cargo test line below, not optional
cargo test --workspace -- --test-threads=1
cargo test -p flux-platform --features live-audio -- --test-threads=1
cargo test -p flux-app --features live-audio -- --test-threads=1
cargo run -q -p xtask -- catalog
cargo run -q -p xtask -- doctor
cargo run -q -p xtask -- test-gpu --profile baseline
cargo run -q -p xtask --bin export_shaders
cargo run -q -p xtask --bin requirements -- --check
node apps/max-for-live/check-maxpat.cjs
cargo run -q -p xtask -- fixtures
cargo build -p flux-desktop
cargo build --release -p flux-app
cargo run -q -p xtask -- package
```

`--test-threads=1` is passed by hand — this list, `.github/workflows/ci.yml` and
`infrastructure/verify.ps1` all carry it, and there is no `.cargo/config.toml`
default, so a bare `cargo test` races GPU device creation. GPU tests are
serialised per test binary by a function-local mutex, which means different
files hold different mutexes.

On the live-audio legs the flag is what CI runs and nothing here licenses
removing it — but the stronger sentence this file carried, that it is "the only
form that finishes", is contradicted by measurement on this tree and is corrected
here rather than copied. Its witness is real and is not retracted: that command
without the flag once printed 63 of the 65 `ok` lines, no `test result:` line, and
was still spinning 2 h 38 m later. Re-taken at `efdf253` on a clean tree (D045):
twelve consecutive parallel invocations of that exact command all exited 0 — 65
passed / 0 failed, lib-suite 9.29–10.22 s, and all three carrier tests printing
`ok` in every one of the twelve logs — against 14.05 / 15.09 / 14.10 s for three
serial ones. So those carriers do return in parallel here, the hang stands
unreproduced with its mechanism still unnamed, and 0 of 12 only bounds it (≈ one
run in five at the 95% level); it does not prove it gone. The other leg never hung:
`cargo test -p flux-platform --features live-audio` gave 13 tests in 0.01–0.02 s
parallel and 0.03 s serial. Keep passing the flag — it is what
`.github/workflows/ci.yml` and `infrastructure/verify.ps1` run, and
`cargo run -q -p xtask --bin requirements -- --check` reads those carriers and
reddens a run where a live-audio `cargo test` line loses it. What this lane could
not check is the tree the original witness was taken on: the sentence named
`ecf0274` only for the serial re-measurement, and no record of the hang exists in
`implementation/`, `docs/` or `spec/` — it lived in this paragraph alone. Still the
only mechanism note in the repo, `crates/tests/tests/dag_parallel.rs:11-17`:
"GPU device creation is not safe concurrently from multiple test threads (it can
deadlock)", implemented as a function-local lock, so different files hold different
mutexes and nothing covers `flux-app`. Retiring the race rather than the flag means
one process-wide serialiser at the thing both forms call — `crates/render/src/gpu.rs:66`
— and that is D045's named next step, not D045's change.

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
  gate ran; a machine without the SDKs still exits 0. The mirror-image defect was live until
  D027: both gates were *decided by an environment variable* rather than by the machine, so
  this host recorded `blocked_external` for six days while holding `abl_link.lib` and
  `spout_shim.dll` on disk. They now test for the artifacts, and export the resolved shim
  path — cargo sets a test binary's cwd to the *package* directory, where the adapter's
  relative candidate does not resolve, so a repo-root "is the file there" probe reads present
  while the test silently skips and cargo calls it a pass. With that, this host ran 17-0-0 —
  but until D029 only from a shell that had exported `FLUX_ABLETON_LINK_LIB`: that gate checked
  for `abl_link.lib` *inside the directory the variable names*, so the same commit and the same
  clean tree recorded 16-0-1 from a bare shell (`7ecc1f1`). `Resolve-LinkLib` now searches the
  vendored candidates and exports the directory it finds, because `crates/integrations/build.rs:16`
  needs the variable *set*, not merely true; the run at `7235afd`, with that variable removed from
  the environment, is 17-0-0 again, and D032's `requirements-freshness` step made the ceiling 18.
  D037 took it to 22 and D041's `workflow-structure` step (`node .github/check-workflow.mjs`, the
  same script ci.yml runs as its own step 2) is the 23rd — so count the steps in the run you are
  reading, not in this sentence. That script read one named file until D042: its default is now a
  scan of `.github/workflows`, so a workflow added beside `ci.yml` is audited the moment it appears
  and a scan that finds no YAML there is leg 8, not a clean exit.
  CI has no blocked steps to record. Its `link` and `spout` legs are separate jobs that clone their
  own SDK sources, and the first hosted run whose workflow could be read measured both *failing* on
  a toolchain path the image does not contain — D039 gates them on a machine probe instead, and D040
  reads both legs green on a runner, `verify` end to end twice, and one run (`44198109118`) whose
  conclusion is `success` — the first here — beside its neighbour's `cancelled` at the 60-minute
  bound on a byte-identical workflow. That census is two runs wide now: `44211444352` is green too,
  and its `verify` job took 59 min 54 s of the same 60-minute bound, which is why D041 raises
  `verify` to 75 and why "the bound is marginal" is now a measurement rather than a worry. The census
  is three runs wide now: `44226667811` is green too, and its `verify` job took 61 min 53 s, which it
  was only allowed to spend because the bound had already moved. D042 re-takes it whole: 56 runs, 25
  of which created jobs, 47 jobs — 25 failure / 21 success / 1 cancelled — three runs `success`. A
  hosted run ending `cancelled` is not a red gate, and with 31 of those 56 runs never having created a
  job at all, "CI did not report" is not the same fact as "CI reported nothing wrong". The hosted
  installer is not the hosted `verify.json`: 28 of that artifact's 33 evidence files are this host's
  checkout copy, and the five the runner did write say
  its toolchain is rustc 1.100.0 and its graphics device is a Basic Render Driver over Dx12. Reading
  a hosted job is itself gated on that job finishing: `GET /actions/jobs/{id}/logs` for a running job
  redirects to a blob that answers `BlobNotFound` (measured 2026-11-18 on job `134541386792`, at
  02:59:01Z), while `GET /actions/runs/{id}/jobs` lists its steps and per-step conclusions the whole
  time. So a step's *conclusion* is readable for exactly as long as the job has left to run before
  its *output* is, and a sentence that says "the log says" must name which of the two it has.
  Annotations have their own carrier trap, and D043 walked into it: `gh run watch` *renders* them, and
  its rendering of run #56's node20 protest named only `actions/checkout@v4` — the `verify` job's own
  log names `actions/checkout@v4` **and** `actions/upload-artifact@v4`, and across the five job logs the
  protest appears four times (three jobs checkout-only, `verify` both, `sdk-probe` not at all, because
  it runs no action). Read an annotation from `gh run view --log --job <id>`; a watcher's stream is a
  count of its own polls and an incomplete copy of its text. D044 re-pinned them to `@v5`/`@v6`.
- **`verify.json` used to carry a UTF-8 BOM** because `Set-Content -Encoding UTF8` writes one
  in Windows PowerShell, so the "machine-readable" evidence failed a strict `JSON.parse`.
  Measured on the committed file at `75da874` and its predecessor. Written without BOM since
  D020, which is also when `gitHash` and `counts` were added to it.
- **Step names overpromise.** `ndi-discovery` used to run `ndi::`, which selects 2 tests that
  only prove the DLL loads and discovery does not crash — and both tests `return` early when
  the runtime cannot be dlopened, so a machine with no NDI recorded that as a *pass*. Renamed
  to `ndi-runtime-loads` and gated on the runtime DLL's presence 2026-11-15 (D023); the
  Blocked branch compiles the feature instead of testing it, so `ndi` stays type-checked.
  The same overpromise was live in `crates/integrations/src/lib.rs`: `spout` and `link` derived
  `verified` from `cfg!(feature = …)`, so a feature build claimed a measurement it had not made.
  Closed 2026-11-15 (D027): `verified` is a per-row constant there, and a source test
  (`capability_report_never_derives_verified_from_a_build_feature`) fails the pattern. Still true:
  **read the `limitation` string, not the boolean.**
- **A green `verify.ps1` does not mean the workspace suite ran.** Its `unit-tests` step is eight
  `-p … --lib` packages — 158 tests at `7adb5e6` — while `cargo test --workspace` is 316 across 41
  result lines (re-measured 2026-11-18 at `3667fd2` plus this tranche's docs, exit 0; D037 recorded
  the same numbers on the tree that produced them). The hosted side now reads the same figure: step 8
  of run 44198109118 prints 41 `test result: ok` lines totalling 316. Every test inside a `[[bin]]`
  target (the `requirements` and `xtask` guards D028 through D034 add, and D036's six in the `flux`
  binary) is therefore executed by CI's workspace run and by **no** step in this script.
  So after adding one, the honest claim is "`cargo test -p xtask --bin requirements` passes" plus a
  workspace run; `VERIFY: PASS` alone cannot see it.
- **A CLI that prints a failure and exits 0 is the same defect as a skipped gate counting
  as a pass.** `flux ndi-loopback` did that for its whole life: it subscribed with the bare
  sender name, NDI advertises `{machine} ({sender})`, so the match could never happen, the
  receive loop timed out, and the exit status said success. `ndi-send`/`ndi-recv`/`ndi`
  behaved the same on a missing runtime. Nonzero now means "this did not happen"; an empty
  discovery list is still exit 0 because finding nothing is an answer.
  Closed across the whole class 2026-11-16 (D036): **seventeen** sites in `crates/app/src/main.rs`,
  not the seven that every lane from D029 copied without opening the file. Nine compiled-out branches
  return one `feature_off()` error naming the `--features` build, eight feature-on arms answer
  `SdkMissing` or `BlockedExternal`, and `cmd_caps` propagates the worker error instead of printing a
  JSON error object at exit 0. The boundary is at the **apparatus, not the outcome**: compiled-out
  code, a missing DLL/helper/model, a child that exited nonzero, zero frames accepted, or a registry
  query that gave up are nonzero; an empty source list, an empty sender registry, a directory with no
  autosave, and the two `cmd_doctor` rows for an absent GPU or FFmpeg stay zero. Run the subcommand
  before quoting its status, and treat a `0` as proof only of the thing the code can observe —
  `flux spout-send-dx` printed its failure and exited 0 for four lanes, which is the only reason a
  sender that has been dead since November stayed unreported.
- **`cmd /c` expands `%ERRORLEVEL%` when the command line is *parsed*.** `prog.exe & echo
  %ERRORLEVEL%` reports the *previous* command's status, so a process that crashed at startup reads
  as exit 0. Use `cmd /d /v:on /c "prog.exe & echo !ERRORLEVEL!"`, or read the status from an
  un-piped PowerShell `Start-Process -PassThru`. Two probes in D036 said a dying helper exited 0 for
  exactly this reason.
- **`implementation/requirements.json` is generated, so its statuses are code.** For acceptance
  rows the strings come from `at_status()` and `at_blocker()`; `blocker_for()` serves the
  requirement and generator rows. Since D032 both harnesses run
  `cargo run -q -p xtask --bin requirements -- --check`: it compares the committed bytes with what
  the generator would produce and exits 1 with up to 20 named rows — `<id>.<field>` for a record,
  `counts` for the derived summary block (which the per-record loop cannot see, because a
  summary-only edit changes no id), or one `first differing line is N` when the two sides mean the
  same thing in different bytes. Rows come out in sorted-id order, not file order, because
  `records_by_id` is a `BTreeMap`, so a truncation drops the sort-order tail of the drift set rather
  than the last records in the ledger — D034 measured both shapes. A hand-edit, or a status that
  drifted from the arm that is supposed to derive it, therefore reddens CI instead of surviving
  silently. Line endings are normalized on both sides for a reason: `core.autocrlf=true` comes from
  the *system* gitconfig on the verifying host, so a fresh clone checks this file out as CRLF while
  the generator emits LF, and an unnormalized byte-compare would report drift on a clean checkout.
  `implementation/evidence/manifest.json` is deliberately **not** gated that way: it is a
  measurement of the run that wrote it (mtimes, binary hashes, `gitDirty`), so a byte-compare would
  fail with nothing broken. When a status overpromises, fix the generator arm (D025 did AT060 and
  AT063) and expect every line-number citation below the changed record to shift by the number of
  lines the record grew — re-resolve them rather than eyeballing, and remember a citation written
  as `requirements.json:3819, 3829` has a second number a `file:NNN` regex never matches.
- **A grouped `match` arm in that generator is a silent evidence-rotation bug** (D028). `"AT057" |
  "AT058" | "AT059" =>` gives one evidence list to a live take, Spout and Syphon, and nothing fails.
  Group only when the acceptance tests share a subject — AT067–AT070 are four clauses of one export
  matrix — and check a new group against the `spec/VERIFICATION.md` rows it claims to cover.
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
  after verification is normal and is not a code change. Since D026, `gitDirty` in those files
  is measured with `implementation/evidence/` excluded, so `gitDirty: true` means the *source*
  was uncommitted — do not "fix" the exclusion by widening it back.
- **A harness run is named by the `gitHash` inside `verify.json`, not by which commit was HEAD
  when the record was written.** Those are two different commits: the bytes a run writes are
  committed by the *next* one. Conflating them let D032 record a run "at `3e4e825`" with a
  `gitDirty` value, which four documents then copied, and no `verify.json` blob in any of this
  repository's 94 commits carries that hash — D036 enumerated them all, and the tree committed
  at `a91ebda` sixteen minutes later still holds the previous run's 17-step file. Say "the run
  recorded at `X`" only when you have read `gitHash: X` in a blob; otherwise say what is true:
  that a run happened with the tree at `X`, and that its bytes were never committed.

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
  `node bundle.mjs`, which versions exactly one `?v=` tag per served document — the
  bundle row (`index.html`) and the monitor row (`monitor.html`, plain `monitor.js`,
  added by I15 item 1) — each token hashed from the bytes of the script that document
  actually loads, and rewrites a tracked file only when its content changed
  (`index.html`/`monitor.html` are `cargo:rerun-if-changed` inputs of that build script;
  `app.bundle.js` is deliberately not, because it is its output). A bundler that cannot
  run, or cannot version exactly one tag in any row, stops the build. The gate also
  re-bundles `main.js` in memory through the same exported `BUNDLE_OPTIONS` and compares
  bytes, so a committed bundle its own sources do not produce is red rather than green
  (I15 item 2); `absWorkingDir` is pinned because esbuild's `// <path>` comments made the
  artifact depend on the directory the bundler was started from.
  Gate: `node apps/desktop/ui/check-bundle.mjs` (29 legs), in CI and `verify.ps1`.
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
