# Lane plan — audio-reactive rendering + live capture (candidate D037)

Written 2026-11-16 against an uncommitted tree at HEAD `20e0ae9`. Every number
below was measured in this audit; anything reasoned from reading code is labelled.

## 0. What is actually in the tree

Nine tracked files modified, four files untracked (504 insertions in the tracked
ones, ~2,200 lines in the new ones). It is two lanes, not one:

**Lane A — media audio drives the picture (compiles today).**
`flux_audio::analyse_mono` / `point_at_sample` (`crates/audio/src/lib.rs:386-449`)
emit hop-aligned `FeaturePoint`s; `crates/app/src/audio_features.rs` (413 lines)
turns the export's own stereo mix into a `MediaAudioTimeline` addressable by exact
project time; `render_single_frame` gained an `audio` parameter
(`crates/app/src/export.rs:446`) and all three call sites (`:177`, `:339`, `:369`)
pass `Some(…)`; `FrameContext` gained `feature_source` plus `with_audio`
(`crates/engine/src/frame.rs:103-158`); `FeatureSource` is the new named boundary
(`crates/domain/src/params.rs:13-42`); `ExportResult` now reports
`feature_source` and `audio_windows`. This closes the hard stub the repo has
carried since November: `FeatureSnapshot::default()` at both resolve sites, which
meant every audio-reactive node body rendered zeros for a file-based export whose
audio was being decoded two lines away for the WAV mix.

**Lane B — live WASAPI capture (does not compile today).**
`crates/platform/src/capture.rs` (820 lines) — endpoint enumeration with real
`PKEY_Device_FriendlyName`, wire-format negotiation, shared-mode open, `drain_into`
with discontinuity and silent-packet accounting, plus a non-live stub whose every
call is `SdkMissing`. `crates/app/src/live_audio.rs` (770 lines) — ring +
analyser + status slot, four `FLUX_LIVE_AUDIO*` env vars, stall detection, no
auto-gain. Also fixes a `CoTaskMemFree` leak per device per enumeration in
`crates/platform/src/lib.rs:105-146`.

## 1. The defect this plan is really about

`cargo check --workspace --all-targets` exits 0 (measured, 58.98 s). That number
means less than it looks:

- `crates/app/src/lib.rs` has **no `mod live_audio;`** (measured: `grep` for
  `live_audio` outside the file returns nothing). A file that is not declared as a
  module is not in any build target. All 770 lines, including its 8 `#[test]`s,
  have never been compiled by anything.
- `crates/app/Cargo.toml` `[features]` is `default = []` plus
  `link/ndi/spout/vision`. There is **no `live-audio` feature in the app crate**, so
  even after declaring the module there is no way to ask for the live build.
- `flux-platform`'s `live-audio` feature is off by default and **nothing in the
  workspace enables it** — checked `ci.yml`, `verify.ps1`, and every `Cargo.toml`.
  The real implementation is `#[cfg(all(windows, feature = "live-audio"))] mod imp`
  (`capture.rs:170-755`), so 585 lines and its 3 tests are compiled out everywhere.
- `#[test]` census: 247 at HEAD → 270 in the working tree, **+23**. Eleven of the
  23 (8 + 3) sit in code no command in this repository executes. The other twelve
  are real and do run.
- `crates/tests/tests/media_audio_reactive.rs` (2 tests) runs in CI's workspace
  suite and in **no** `verify.ps1` step — that script names `producer_tests`,
  `export_matrix`, `dag_parallel`, `golden` individually (`:114-117`). Same blind
  spot AGENTS.md records for `[[bin]]` tests.

So the headline claim this lane can currently make is "the live half does not
exist as far as the compiler knows". The first job is to make it compile, the
second is to make a gate actually execute it, and only then to measure it.

## 2. Decisions taken here (so the build does not have to)

1. **Compile the live path, keep it out of the shipping default.** `live-audio`
   stays off in `default`; the fix is a gate that builds it, not making the default
   binary carry four more Win32 API surfaces.
2. **The determinism firewall is a type, not a comment.** `FeatureSource` is
   recorded on every `FrameContext` and on `ExportResult`; `MediaAudioTimeline` can
   only ever report `DocumentOnly` (measured: `feature_source()` at
   `audio_features.rs:112` returns the constant). A live reading reaches the
   document only through the same revision/undo seam as `GuiCommand::LiveOsc`.
   No export or preview render path may read the live slot. Keep it.
3. **No auto-gain, no silent fallback.** `input_db` defaults to `+0.0` and is the
   owner's explicit trim; an unsupported `WAVEFORMATEX` is an error, never a buffer
   of zeros. Keep it, and do not add a normaliser to make the demo look better.
4. **`AnalysisConfig` is built from the stream's own rate**, never from
   `AnalysisConfig::default()` (48 kHz), which would put every timestamp and tempo
   estimate 8.1 % out on a 44.1 kHz project. Keep and test.
5. **Feature lookup is by source sample position** (`point_at_sample`, hop-closed
   window), never wall clock — that is AT017's actual clause, and the reason the
   analysis happens before the render loop rather than inside it.
6. **Exit codes follow D036's boundary at the apparatus**: compiled-out feature or
   missing endpoint → nonzero (`BlockedExternal` / `SdkMissing`) with a `recovery`
   naming `--features live-audio`; an empty endpoint list is an answer → exit 0.

## 3. Steps

Each step names its own command. Never wrap one in a pipe or a trailing `echo`.

**Step 1 — Declare and enable.** Add `pub mod live_audio;` to
`crates/app/src/lib.rs` and `live-audio = ["flux-platform/live-audio"]` to
`crates/app/Cargo.toml`. Prove the module is now in a target:
`cargo check -p flux-app` (default) must still exit 0 with the live half
compiled out, and `cargo check -p flux-app --features live-audio` must run the
770 lines through the compiler for the first time. Expect this to be red at
first; that red is the point.

**Step 2 — Make the Windows 0.62 calls bind.** Iterate
`cargo check -p flux-platform --features live-audio` and
`cargo check -p flux-app --features live-audio --all-targets` to exit 0.
Check each signature against the vendored crate source before changing call
sites — the candidates are `IMMDevice::Activate`'s generic form, `PROPVARIANT`
union access and `PropVariantClear`, `PWSTR` ownership, and
`IAudioClient::Initialize`'s `hnsBufferDuration` type. The four added feature
strings (`crates/platform/Cargo.toml:11-14`) also have to exist in windows 0.62;
if one does not, the fix is the feature list, not the code.

**Step 3 — Execute the eleven orphaned tests.**
`cargo test -p flux-app --features live-audio --lib -- --test-threads=1` and
`cargo test -p flux-platform --features live-audio -- --test-threads=1`. Read the
counts off the output; the honest post-state is those tests passing in a feature
build, not the default suite growing by eleven.

**Step 4 — Turn "compiles" into "gated", with a falsifier.** Add one command per
block to `infrastructure/verify.ps1` and a `live-audio (optional)` job to
`ci.yml` mirroring the `ndi` pattern (`ci.yml:209-217`), gated on the machine
(Windows) and not on an environment variable (the D027 lesson). The falsifier is
a pair of CLI guards in the `flux` `[[bin]]` target: a default build of the live
subcommand exits 1 naming `--features live-audio`, and a feature build gets past
that branch and fails (or succeeds) for a device reason. A mutation that drops
`pub mod live_audio;` reddens the second guard — that is what stops the lane from
silently returning to its current state.

**Step 5 — CLI surface.** `flux live-audio list|probe|status` (extend
`cmd_capture`, `crates/app/src/main.rs:997`, or add a sibling — decide by line
count, `main.rs` is already 1 248 lines). `list` must print the friendly names,
which is the visible payoff of the `PKEY_Device_FriendlyName` work: today
`flux capture` prints `microphone { 00000000-… }` for what Windows calls
`Input (7- SSL 2 USB Audio Device)`. `probe` reports endpoint id, negotiated wire
format, device/buffer periods, frames drained, silent frames, discontinuities,
measured rate and peak, honouring `FLUX_LIVE_AUDIO_INPUT_DB` with no hidden trim.

**Step 6 — Measure on this rig.** This is the only step that can move AT015 and
AT017 off `implemented_unverified`, and it needs the machine, not a headless run.
Enumerate; open the SSL 2 in WASAPI shared mode; open a loopback tap of the render
endpoint while something actually plays (loopback has no clock — with nothing
rendering, `GetNextPacketSize` stays 0 and `STALL_LIMIT_MS` is what makes that a
reported state rather than a hang). Save the transcript to
`implementation/evidence/live-audio-2026-11-16.txt`. Note before starting: the
ASIO side of the SSL 2 is single-client, so close whatever holds it, and the WASAPI
wire rate may be 48 kHz even though the ASIO session runs 44.1 kHz — record which
rate the device actually offered instead of assuming the project's.
**Device availability is a gate, not a task** — if the rig cannot be handed over,
Steps 1-5 and 7-10 still land, and the ledger says `blocked_external`.

**Step 7 — Prove the media-audio picture path, red first.**
`cargo test -p flux-tests --test media_audio_reactive -- --test-threads=1`.
Then mutate: pass `FeatureSnapshot::default()` at `export.rs:461` instead of the
timeline's evidence and confirm `loud_media_audio_moves_the_picture_muted_does_not`
goes red. Revert, re-verify. A test that passes with the stub is not a test of the
stub's removal.

**Step 8 — Prove the firewall.** Add a determinism check: the same project and the
same media render byte-identical frames across two runs with audio attached, and
an assertion that `ExportResult::feature_source` can only be `DocumentOnly` on the
export path. Also bound the new memory shape: `mix_audio` now runs before the frame
loop, so a 10-minute 44.1 kHz stereo range (~212 MB of `f32`) is held for the whole
export — reasoned from the call order at `export.rs:157-163`, measured by timing
and peak RSS on one real export.

**Step 9 — Ledger through the generator, never by hand.** Edit the arm at
`crates/xtask/src/bin/requirements.rs:323` (`AT017`) and add an `AT015` arm —
measured: AT015 has no arm today, so its row comes from the default. Do not group
them: AT015 is callback/ring/device-loss and AT017 is file-audio-drives-visuals
(`spec/VERIFICATION.md:46,48`), different subjects, different evidence. Then
`cargo run -q -p xtask --bin requirements` to regenerate and
`cargo run -q -p xtask --bin requirements -- --check` to prove freshness. Expect
line-number churn: every `requirements.json:NNNN` citation below a grown record has
to be re-resolved, not eyeballed.

**Step 10 — Records and the discharged-claim sweep.** `implementation/DECISIONS.md`
D037 (measured vs reasoned kept in separate paragraphs, including the fact that the
live half existed for hours without ever being compiled); `STATE.json`
(`test_totals`, `known_gaps`, `commands_to_resume`); the `BLOCKERS.md` "Windows
capture" bullet, which currently says live audio capture could not be exercised;
`AGENTS.md` (add the trap: an undeclared module file is invisible to every gate,
and a `--workspace` green says nothing about an off-by-default feature);
`README.md`, `FINAL-REPORT.md`, `docs/PERFORMANCE-READINESS.md`. Sweep for every
present-tense claim that audio features are hard-stubbed — including
`crates/app/src/commands.rs:305`, already reworded in this tree.

**Step 11 — Full CI-parity verification.** `cargo fmt --all -- --check`; the
workspace suite `cargo test --workspace -- --test-threads=1` **twice** (the
1-in-7 `STATUS_ACCESS_VIOLATION` in `flux-render` is unresolved and a truncated
run must never be quoted); `xtask catalog`, `doctor`,
`test-gpu --profile baseline`, `export_shaders`, `requirements --check`;
`node apps/max-for-live/check-maxpat.cjs`; then `infrastructure/verify.ps1` full,
from `env -u FLUX_ABLETON_LINK_LIB -u FLUX_SPOUT_SHIM`, and read the printed
`steps: passed=N failed=N blocked_external=N` line, not just `ok`. Commit the
evidence the harness rewrote, and name the run by the `gitHash` inside
`verify.json`.

## 4. Not in this lane

Rebuilding `apps/spout-bridge` (`0xc0000138`) and the AT058 blocker-string
correction D036 explicitly deferred to that lane; a GUI live-audio meter or any
webview state; multi-track blending; macOS/`syphon`; three sites D036 left with no
invocation (`main.rs` 11, 15, 16).

## 5. Risk register

1. `live-audio` has never compiled — Step 2 is a guess until it runs. Budget
   several iterations.
2. The `windows` 0.62 feature paths in `crates/platform/Cargo.toml:11-14` are
   unverified names.
3. Adding a step to `verify.ps1` changes the `18-0-0` tally every document recites;
   the sweep in Step 10 must cover README `:91`, `FINAL-REPORT.md` `:56` and
   `docs/PERFORMANCE-READINESS.md` `:1073`.
4. Device measurement depends on the rig being free; do not relaunch anything that
   was closed to release the audio device.
5. A feature-gated CI job needs `cargo build --features live-audio` on
   `windows-latest` — if that pulls a heavy dependency, decide before merging, not
   during.
