# Flux — Performance Readiness: what still needs a real machine or real hardware

Scope: this document lists only what **cannot be proven from the current checkout** —
things that need this author's actual Windows box, his SSL 2, Ableton Live 12, a second
display, a Spout/NDI consumer, or a vendor account. Everything here is a gate to be closed
by observation, not by reading code.

Nothing in this file is verified. Nothing in it should be read as "works". Where the status
documentation says something works and the code says otherwise, the code wins and the
disagreement is named.

## 0. How to read this (truthfulness rules being applied)

- `spec/START-HERE.md:89` — an unavailable external prerequisite must produce
  `blocked_external`, never `passed` and never a fabricated output. `spec/START-HERE.md:93`
  defines the only legal ladder: `pending → implementing → implemented_unverified → verified`.
- `spec/START-HERE.md:95` — "Do not claim … macOS verification from Windows, or a working
  `.amxd` based only on a JSON patch."
- `spec/VERIFICATION.md:178` — "Do not claim the product is complete if required work is
  failed, unverified, or externally blocked."
- Owner column: **ALONE** = the author can clear this on his own rig tonight.
  **EXTERNAL** = needs something his machine cannot supply (a Max GUI is *not* external —
  he has Live 12; a Mac, a signing identity, a second VJ app, a second machine are).
- Two labels are used and are **not** the same thing:
  - `UNVERIFIED` — the code path exists and plausibly runs; nobody has watched it run.
  - `NOT WIRED` — the code exists in isolation and **nothing in the shipping binary calls it.**
    No amount of hardware testing proves a `NOT WIRED` item; it needs a code change first.
    These are called out explicitly because they are the trap: a checklist that says
    "test the OSC input" on a `NOT WIRED` item produces a confusing afternoon and a wrong
    conclusion.

## 1. The four facts that dominate everything below

Established by reading the integration code, not the status files.

> **Status note added 2026-11-15, supersedes the 2026-11-14 note below it.** The wire for
> Live → Flux OSC now exists and runs *inside the shipping desktop binary*:
> `flux_app::control::OscBridge` (`crates/app/src/control.rs:304`) owns an `OscReceiver`
> plus a listener thread, coalesces parameters on a 16 ms window and applies them through
> `CommandHost::apply_live_param` (`crates/app/src/commands.rs:250`) → `apply_to_dispatcher`
> → `Command::SetParameter`. The desktop starts it in its `setup` hook
> (`apps/desktop/src-tauri/src/lib.rs:474`) on `FLUX_OSC_PORT` or 9000, and reports it through
> the `osc_status` command (`apps/desktop/src-tauri/src/lib.rs:320`). A `live-control` event
> makes the webview re-pull the snapshot, so the inspector cannot disagree with the preview.
> Measured by loopback: 7 tests in `crates/app/src/control.rs` send real UDP bytes and assert
> the document, the revision and the counters move. **That is bytes → document. It is not
> document → pixel, and it is not Live → Flux: A1 is `UNVERIFIED`, no longer `NOT WIRED`.**
> Transport, seek, trigger, preset-recall and bridge features are decoded and counted with a
> stated reason but still change nothing, for the reasons in `apply_live_control`
> (`crates/app/src/commands.rs:284`).

> **Precision note added 2026-11-14 after verification.** "Nothing applies OSC
> controls" would be wrong. The apply half exists: `GuiCommand::LiveOsc`
> (`crates/app/src/commands.rs:556`) calls `control::apply_to_dispatcher`, and
> `LiveMidi` does the same for CC. What is missing is the **wire** — no code turns a
> received `OscControl` into a `LiveOsc` dispatch, and the desktop process opens no UDP
> listener — plus the precondition that a `LearnBinding` has set
> `control_map.graph_id`, or `LiveOsc` returns `changed: false`. Gate A items are
> therefore wiring jobs, not missing subsystems. See `implementation/DECISIONS.md` D018
> item 2 for the corrected wording. **The first half of that sentence is now false: the
> desktop does open a listener, and `LiveOsc` falls back to `active_graph` when nothing was
> learned.**

1. **The GUI has an OSC listener and nothing else.** `apps/desktop/src-tauri/src/lib.rs`
   contains OSC receive code and no MIDI, no Link, no bridge sender, no Spout and no NDI code
   (the only match for "link" in that file is `#[link(name = "kernel32")]` at line 557).
   `apps/desktop/ui/main.js` listens for the `live-control` event and nothing else
   integration-shaped. Every other integration is reachable only through the `flux` CLI.
2. **The CLI's integration commands send and receive synthetic test patterns, not Flux's
   render.** `flux ndi-send` broadcasts a `fill_gradient` at a hardcoded 640×360
   (`crates/app/src/main.rs:504`, `:516-520`); `flux spout-send-dx` does the same
   (`crates/app/src/main.rs:645`, `:652-661`). `live.rs:37-41` defines `trait LiveSink` and
   `live.rs:28-34` defines `trait LiveSource`; **neither trait has a single implementor in the
   tree.** So no code path exists that moves a rendered wgpu frame to NDI or Spout.
3. **`flux osc <port>` is still a 30-second print-only listener, and it is now the *second*
   OSC listener.** `crates/app/src/main.rs:865-883`: it binds, prints each decoded `OscControl`
   to stdout, and exits at `as_secs() < 30` (line 873). It never touches a `Dispatcher`, a
   project, or a frame — the desktop process does that now (see the status note above), which
   makes this command a diagnostic and not the feature. It shares `OscReceiver`, so it also
   inherited bridge-packet decoding. The function that *was* supposed to convert controls into
   edits, `apply_controls` (`crates/integrations/src/osc.rs:268`), still has **zero callers
   outside its own tests**; the live path uses `control::OscBridge` instead.
4. **Flux has no live audio input of any kind.** `crates/audio/Cargo.toml` depends on
   `rustfft`, `crossbeam-channel`, `serde`, `serde_json` and two workspace crates — no
   device/capture crate. `flux_audio::Analyser::push(&mut self, mono: &[f32])`
   (`crates/audio/src/lib.rs:182`) is the only way audio gets in, and the runtime hands it
   `FeatureSnapshot::default()` — see `crates/app/src/export.rs:403` and
   `crates/app/src/commands.rs:464`. `crates/platform/src/lib.rs:163-184` *enumerates*
   WASAPI endpoints; there is no `capture`/`read`/`start` function in that file at all.
   Grepping the entire tree for `ASIO` returns exactly one hit:
   `crates/integrations/build.rs:32`, a comment about Ableton Link's *networking* ASIO.
   Audio in Flux today is file-decode-for-offline-mix only (`crates/app/src/export.rs:873-934`).

Consequence: most "hardware needed" items in gates A and D are actually **NOT WIRED**.
The hardware step is real, but it comes after a code step, and this document says so per item.

---

## 2. Gate A — Ableton Live 12 wiring (OSC send/receive, transport, SSL 2 ASIO)

### A1 — Live → Flux OSC: the listener is in the app, the pixel is still unwitnessed. `UNVERIFIED`. Owner: ALONE
- **Proven by loopback, 2026-11-15.** Bytes on port 19011–19015 reach the authoritative
  document. `flux_app::control::OscBridge` binds, decodes with `osc::decode_control`, coalesces
  the latest value per parameter on a 16 ms window, and applies through
  `CommandHost::apply_live_param` → `apply_to_dispatcher` → `Command::SetParameter`. The tests
  assert the stored `ParamValue`, the revision bump (so the edit is undoable), one edit per
  parameter per window for a 10-packet burst, and that an unknown node is counted `unmapped`
  instead of vanishing.
- **Still unproven.** That a fader in Live changes a *pixel* on screen while the editor is
  open. No GPU frame was ever produced by this path in this session, and no Max device has
  sent a packet to Flux. The preview reads the graph from Rust every frame, so it should
  follow — that is a reasoned claim, not a measured one.
- **Shortest procedure.** (a) Build and open `target\release\flux-desktop.exe` on a project
  with a generator node; get the node id from the inspector. (b) Send
  `/flux/v1/project/<pid>/node/<node>/param/<param> 0.8` from Live's Max device, or with any
  OSC sender pointed at `127.0.0.1:9000`. (c) Watch the preview *and* the inspector.
  (d) Ask the app what it counted: call the `osc_status` IPC command — `bound`, `port`,
  `bindError` and every counter come back, so silence is distinguishable from a drop.
- **Pass.** The preview image visibly changes on the same beat the packet arrives, the
  inspector shows the new value without a click, and `osc_status.stats.applied` increments.
- **Failure looks like.** Three different things, now separable by numbers instead of by
  staring: `bound:false` + `bindError` naming `Address already in use` means something else
  owns 9000 — the usual suspect is `flux osc 9000`, which binds the same default and holds it
  for 30 s. `received` climbing with `unmapped` equal to it means the node id in the address is
  not a node in the *active* graph. `notApplied` with a `lastReason` naming transport or bridge
  is not a fault: those kinds have no engine target yet.
- **What this replaced.** Until 2026-11-15 this item was `NOT WIRED`: the desktop opened no
  socket, and `GuiCommand::LiveOsc` answered `changed:false` unless a `LearnBinding` had first
  set `control_map.graph_id`. Both halves are gone — an unlearned surface now targets
  `active_graph`.

### A2 — Max bridge packets are decoded and counted; their features still feed nothing. `UNVERIFIED`. Owner: ALONE
- **Fixed in code, 2026-11-15.** The discard defect was real: `parse_address`
  (`crates/integrations/src/osc.rs:65`) rejects any address whose third segment is not
  `project`, and the bridge speaks `/flux/v1/bridge/{id}/features`
  (`apps/max-for-live/README.md:19-21`), so a bridge packet decoded to `None` and vanished
  without a counter or a log line. `decode_bridge_packet` had zero callers outside its crate.
  There is now one entry point for every datagram, `osc::decode_control`
  (`crates/integrations/src/osc.rs:47`), which tries the bridge grammar first and returns
  `OscControl::Bridge(..)`; `OscReceiver` uses it, so both the CLI listener and the desktop
  bridge see bridge packets. A test asserts a `features` datagram increments `bridge` and
  leaves the document untouched.
- **Still not true.** That those features do anything. They are counted with the reason
  "the analyser is still handed `FeatureSnapshot::default()`" — see fact 4. The wire format is
  also still only proven against a packet this repo generated itself; whether Max emits exactly
  these bytes is a host question.
- **Shortest procedure.** With the Max device loaded in Live (Gate B), open Flux Desktop and
  call `osc_status`: `bridge` should climb once per heartbeat while audio plays. If it stays
  at zero, the packets are not arriving at all, and `bridge-osc.js`'s `HOST`/`PORT`
  (`apps/max-for-live/bridge-osc.js:49-50`, currently `127.0.0.1` / `9000`) is the first thing
  to check against a third-party monitor.
- **Pass.** `stats.bridge` increments while Live plays with the device loaded.
- **Failure looks like.** `bridge` stays 0. Since 2026-11-15 that is no longer ambiguous with
  "received but discarded": a received bridge packet is always counted.

### A3 — Flux → Live OSC output does not exist. `NOT WIRED`. Owner: ALONE
- **Unproven.** The reverse direction, which `spec/START-HERE.md:45` (R014) requires as
  "OSC/MIDI input **and output**". `crates/integrations/src/osc.rs:214 encode_param_feedback`
  is the only outbound encoder and its only call site is its own unit test
  (`crates/integrations/src/osc.rs:324`). There is no UDP socket that ever sends.
  `apps/max-for-live/flux-audio-bridge.maxpat:206` sets `"oscreceiveudpport" : 0` — the patch
  has no receive port open either. Feedback loop protection (AT050, marked `verified` in
  `implementation/requirements.json:3655`) therefore protects a channel that carries no traffic.
- **Shortest procedure.** After implementing: bind a listener in Live, move a Flux slider, check
  a packet arrives once (not per-frame) and that Flux does not react to its own feedback.
- **Pass.** Exactly one `/flux/v1/project/…/param/…` per committed edit; no echo loop.
- **Failure looks like.** Either nothing arrives, or a self-sustaining oscillation where Flux
  re-applies its own feedback and a parameter runs to a clamp.

### A4 — Ableton transport (play/seek) over OSC. `NOT WIRED`. Owner: ALONE
- **Unproven.** That pressing Space in Live starts/stops Flux's clock. What changed on
  2026-11-15 is the *reaching*, not the *acting*: `/transport/play` and `/transport/seek`
  are decoded (`crates/integrations/src/osc.rs:86-98`) and now arrive at the desktop
  listener, where they increment `notApplied` and record the reason
  "no Rust-owned playhead — the preview clock is the webview's rAF loop, and the document
  has no transport command" (`apply_live_control`, `crates/app/src/commands.rs:284`).
  They still move nothing. `apply_controls` (`crates/integrations/src/osc.rs:268`) remains
  dead code that matches only `OscControl::Param`.
- **New defect, measured.** The documented `seek` packet cannot carry its own time.
  `docs/FILE-FORMATS.md` specifies string seconds + int denominator; `decode_packet` keeps
  only numeric arguments, so seconds are dropped, the denominator is read as the seconds,
  and `parse_address` hardcodes `denominator: 1_000_000`. Pinned by
  `a_seek_packet_cannot_carry_its_seconds` (`crates/integrations/src/tests.rs`). Any future
  seek implementation must fix the wire format first, or it will seek to the wrong place
  with total confidence.
- **Shortest procedure.** Same rig as A1: send `/flux/v1/project/<pid>/transport/play 1`
  and watch `osc_status`. `notApplied` climbing with that reason is the *expected* result
  today — it proves the wire, not the feature.
- **Pass.** Playhead starts/stops in step with Live within one frame.
- **Failure looks like.** Identical to the pass condition of the wire: the counter moves and
  the preview stays frozen. Do not read a rising `notApplied` as transport working.
- **What a fix needs.** Either a Rust-owned clock that the webview follows, or a typed
  command the shell maps onto the existing rAF loop. Neither exists; this is design work,
  not wiring.

### A5 — Live 12's own audio clock vs. Flux's frame clock. UNVERIFIED. Owner: ALONE
- **Unproven.** That Flux's exact-rational time (`docs/USER-GUIDE.md:49-52`) tracks a Live
  transport that is at 44.1 kHz on the SSL 2 while Flux's project default is
  `sample_rate: 48_000` (`crates/domain/src/document.rs:67`; asserted at `:700`).
  Nothing in the tree resamples or compensates; `spec/VERIFICATION.md` AT008
  (`implementation/requirements.json:~230`) explicitly wants
  "error ≤ one project frame + 5 ms after known device latency compensation" and that
  compensation code does not exist.
- **Shortest procedure.** In the GUI set project sample rate to 44100, run a 60 s loop of a
  transient-heavy source, mark the first kick of bar 1 and bar 17 in Live, and mark the same
  visual event frame number in Flux. Compute drift in ms and in frames.
- **Pass.** Drift < 16.7 ms over 60 s and < 1 frame, with the same numbers reproducible
  between takes (not averaged away).
- **Failure looks like.** Progressive slide — visual lands later each bar (classic
  44.1/48 ratio drift is ~8.9 %: 60 s of audio becomes ~54.6 s of "time"). If the drift is
  *constant* rather than growing, it is fixed latency (compensate); if it grows, it is a rate
  mismatch.

### A6 — Ableton Link. UNVERIFIED in the binary you own. Owner: ALONE
- **Unproven.** That `flux link` runs at all on this machine right now.
  `crates/integrations/Cargo.toml:24-25` sets `default = []`; `crates/app/Cargo.toml:33`
  sets `default = []`. `link` is compiled **out** of any build that does not pass
  `--features link` plus `FLUX_ABLETON_LINK_LIB` (`implementation/STATE.json:36`).
  `live::protocol_status()` reports `available: cfg!(feature = "link")`
  (`crates/integrations/src/live.rs:82-90`) — so the binary's own report is the truth here,
  and the Nov 9 evidence (`implementation/evidence/link-a.txt`, 421 bytes, `peers` crossing 0→1)
  came from a *separately built* feature binary, not from `target\release\flux.exe`.
  Even in a feature build, Link is a standalone session in the CLI: it is not connected to the
  renderer (`crates/app/src/main.rs:101 → cmd_link`), and `apps/desktop` never mentions Link.
- **Shortest procedure.** (a) `target\release\flux.exe integrations` and read the `link` row.
  (b) If `available=false`, that is the whole answer for tonight. (c) With a feature build:
  `set FLUX_LINK_SECS=60` and run two `flux.exe link` terminals; then join the same session from
  Live 12's Link button (Live → Preferences → Look/Sync, or the Link view) and watch `tempo` and
  `beat/phase` converge.
- **Pass.** Live and Flux show the same tempo and the same beat within one frame; un-pausing
  Live resumes Flux's clock (start/stop propagation) on a real host, not in a unit test.
- **Failure looks like.** Flux never sees Live as a peer (multicast blocked — note `live.rs`
  binds loopback-oriented paths), or Flux's beat drifts a constant quarter off (phase anchor),
  or Live changes tempo and Flux holds its old value.
- **Doc conflict.** `implementation/BLOCKERS.md:18-23` calls Link "supported integration —
  built from upstream and verified locally" with "no blocker remains", and marks AT063
  `verified` (`implementation/requirements.json:3782`). `implementation/STATE.json:35` also
  lists "single-master tempo arbitration" as *verified*, while `BLOCKERS.md:22-23` and
  `BLOCKERS.md:43` say it is *not covered*. Those two files disagree with each other. Neither
  claims real-Live interop, and no evidence file contains Live's name. Treat "verified" here as
  "two Flux processes found each other".

### A7 — The SSL 2 ASIO single-client conflict, and what it actually costs Flux. UNVERIFIED. Owner: ALONE
- **Unproven.** Everything about Flux's audio input on this interface — because there is no
  audio input (Fact 4). The specific conflict: Live holds the SSL 2 as its ASIO client at
  44.1 kHz. ASIO is single-client by design, so while Live runs, no other process can open the
  SSL 2's ASIO driver. Flux would have to reach the same audio by a *different* route.
  The routes available in code today are: **none.** `crates/platform/src/lib.rs:169-170`
  enumerates `eCapture` and `eRender` WASAPI endpoints — enumeration only, there is no
  activation, no `IAudioClient`, no `GetNextPacketSize`, nothing that returns samples.
  `implementation/BLOCKERS.md:39-41` frames this as "live frame/audio *capture* needs a device
  + OS permission, which this headless run could not exercise". **The code says something
  narrower: no capture code exists to exercise.** `implementation/requirements.json:62` (R005,
  `pending`, blocker "capture adapters implemented; device enumeration verified, live capture
  needs a device") is accurate about the *device* part and misleading about the *adapter* part.
- **Shortest procedure.** (a) Determine what you can even see, with Live running and holding
  the interface: `target\release\flux.exe capture` and `target\release\flux.exe caps`.
  Check whether the SSL 2 appears as a WASAPI render endpoint and whether "What U Hear"-style
  loopback is listed. (b) Repeat with Live quit. (c) Only after capture code exists: point it
  at the loopback endpoint and confirm non-silence.
- **Pass.** (a) The SSL 2 endpoints are enumerated with Live holding the device, with correct
  names. (c) RMS in `crates/domain/src/params.rs:16 FeatureSnapshot` is non-zero when Live is
  playing, and drops to ~zero on mute.
- **Failure looks like.** `flux capture` prints the GUID-tail names only — the code derives
  display names from the endpoint id tail (`crates/platform/src/lib.rs:96-106`), so you will see
  `{0839dd8c-…}`-style strings, **not** "SSL 2", and you will not be able to tell which one is
  the interface. That is a UX defect that will burn time on rig day; `implementation/evidence/capture.txt`
  shows exactly this (`cameras: 1  microphones: 5  loopback: 6`, all GUID tails, and the camera
  entry is literally named `camera { Properties }`, i.e. the PnP key's leaf, not a camera name).
  Second expected failure: WASAPI loopback returns digital silence for the ASIO-held stream,
  because many ASIO drivers bypass the WASAPI render path entirely. If that happens, the only
  remaining routes are a virtual cable (loses the "judge by ear in Live" workflow), or Flux
  becoming the ASIO client *instead of* Live, which is not possible while Live is running.

### A8 — VST3 / plugin path. Not a gap, but record it so nobody re-asks. Owner: ALONE
- **Unproven / out of scope.** `spec/START-HERE.md:68` puts VST3/CLAP under **Extension**:
  "Supply integration boundaries and documented hooks. Do not silently add them to the required
  release." There is no VST3 code in the tree, so nothing about `Documents\VST3` is relevant to
  Flux today. Live's VST3 folder is where a *packaged* Flux analyzer would eventually land
  (`spec/ARCHITECTURE.md:283`) — do not test it and do not schedule it before Gates A1–A7 and B
  are closed.

---

## 3. Gate B — Max for Live device packaging: two `.maxpat` vs one real `.amxd`

### B1 — `.amxd` bytes do not exist. BLOCKED-ON-GUI, not on hardware. Owner: ALONE (he has Live 12)
- **Unproven.** That either device loads in Live at all. The tree ships
  `apps/max-for-live/flux-audio-bridge.maxpat` and `flux-midi-bridge.maxpat`;
  `implementation/STATE.json:24` is honest: "`.amxd` bytes need a Max host (external)".
  `implementation/requirements.json:27` keeps R002 at `pending` — the correct status, and
  `spec/START-HERE.md:89` forbids raising it without the bytes.
- **Exact manual procedure.** From `apps/max-for-live/README.md:48-52`, corrected for the real tree:
  1. Launch Live 12 (Suite) → Options → Preferences → Plug-ins: confirm Max/MSP is enabled.
  2. Open the **Max** application bundled with Live (not standalone Max 9 — `README.md:60`).
  3. Max → **File → Open…** → `D:\Projects\visual\genfx\apps\max-for-live\flux-audio-bridge.maxpat`.
  4. Max → **File → Save As…** → type **Max for Live Audio Effect** → keep the filename →
     save into `apps\max-for-live\`.
  5. Repeat 3–4 with `flux-midi-bridge.maxpat` → type **Max for Live MIDI Effect**.
  6. In Live, add `flux-audio-bridge` to an audio track fed from the SSL 2 input, and
     `flux-midi-bridge` to a MIDI track.
- **Pass.** Both appear in Live's device list; loading the audio device does not change the
  track's sound (`plugin~ → plugout~` unity, `README.md:29`); Live saves and reopens a Set
  containing them without error. `spec/ARCHITECTURE.md:273` forbids renaming `.maxpat` → `.amxd`;
  check the first four bytes are `ampf`, not `{`.
- **Failure looks like.** "Save As" offers no Max-for-Live device type (that is the standalone
  Max, `README.md:60`); the device loads but the Max window reports an error; or the audio
  device mutes/gates the track — which means `plugin~`/`plugout~` is not wired unity.

### B2 — The recorded `bootpath` points at a folder that does not exist. Owner: ALONE
- **Unproven.** That `node.script bridge-osc.js` resolves when the patch is opened on this
  machine. `apps/max-for-live/flux-audio-bridge.maxpat:198-199` records
  `"name" : "bridge-osc.js", "bootpath" : "D:/Projects/skeer-audio/genfx/apps/max-for-live"`.
  **`D:\Projects\skeer-audio\genfx` does not exist** (verified: the directory is absent); the
  repo is `D:\Projects\visual\genfx`. The `node.script` object itself
  (`flux-audio-bridge.maxpat:91`, `"text" : "node.script bridge-osc.js @autostart 1"`) uses a
  bare filename, so Max will normally resolve it against the patch's own folder — but the stale
  dependency record is exactly the kind of thing that produces "file not found" in a device
  saved into a `.amxd`, because a packaged device has no patch folder on disk to resolve against.
- **Shortest procedure.** During B1 step 4, after saving, open the *device* (not the patch) in
  Live, click the Wrench, and read the Max window. Then check whether `bridge-osc.js` was
  embedded into the package.
- **Pass.** No red "file not found: bridge-osc.js" in the Max window; OSC bytes leave the device.
- **Failure looks like.** The device loads, shows a red patch cord or an error line, and
  `flux osc 9000` receives nothing. Fix: `usewatchfolder`/`file (path)` the js explicitly, or
  set `@autoload 1` after copying `bridge-osc.js` next to the `.amxd`.

### B3 — The README's own verification command is broken in HEAD. Owner: ALONE (doc fix)
- **Unproven/false-by-construction.** The tree at `46cfd94` (2026-11-13, "Repoint the Max patch
  at the new tree") changed `apps/max-for-live/README.md:55` from the old
  `D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\...` to `D:\Projects\skeer-audio\genfx\apps\max-for-live\*.amxd`.
  That is a **third** path and it does not exist either. The commit message claims the old root
  was repaired and that the path "would have returned nothing and looked like a failed build" —
  the same failure it re-introduced. Follow step 5 verbatim and you conclude the `.amxd` build
  failed when it may have succeeded.
- **Shortest procedure.** Run the intended check with the real root:
  `Get-ChildItem "D:\Projects\visual\genfx\apps\max-for-live\*.amxd"` and
  `Format-Hex -Count 4` each file.
- **Pass.** Two `.amxd`, first four bytes `ampf`.
- **Failure looks like.** Zero files (wrong folder, or Save As wrote elsewhere — Live's default
  Max device folder is under `Documents\Ableton\…\Max for Live Devices`, not the repo), or a
  file that begins `7b` (`{`) — which would be a renamed JSON patch and is forbidden by
  `spec/ARCHITECTURE.md:273`.

### B4 — `udpsend` binary-OSC conformance has never been round-tripped in real Max. UNVERIFIED. Owner: ALONE (needs B1+B2)
- **Unproven.** The spec is explicit and unmet here: `spec/ARCHITECTURE.md:275` —
  "Use available OSC encoding object/helper verified in actual Max version; **`udpsend` alone
  does not establish binary OSC conformance. Round-trip packet tests with desktop rosc decoder
  are mandatory.**" The patch uses `udpsend` (`flux-audio-bridge.maxpat:4` occurrences) with
  `bridge-osc.js` doing hand-rolled OSC packing (`apps/max-for-live/bridge-osc.js:18` documents
  its own type table; `:57` `sock.send`). The desktop-side decoder is `rosc`
  (`crates/integrations/Cargo.toml` `rosc = "0.11"`), and `implementation/requirements.json:3756`
  keeps AT061 at `implemented_unverified`. The only round-trip evidence in the tree is
  Rust→Rust (`crates/integrations/src/tests.rs:8`), not Max→Rust.
- **Shortest procedure.** With the packaged device running in Live and
  `target\release\flux.exe osc 9000` open, play audio through the SSL 2 and record 10 s of raw
  UDP (a one-line PowerShell/Node sink, or the existing `flux osc` stdout). Then feed the
  captured bytes to `bridge::decode_bridge_packet` (`crates/integrations/src/bridge.rs:38`) in a
  test and assert the 8 floats arrive in the documented order — RMS, peak, bass, mid, treble,
  onsetStrength, onsetFlag, confidence (`README.md:24`).
- **Pass.** All 8 features decode, `seq` increments monotonically, and `transport` carries a
  plausible Live tempo. Watch for the known Max OSC traps: 64-bit `d` vs `f` (the address
  pattern uses `double seconds` + `float tempo`, `README.md:20-21`) and string alignment/padding.
- **Failure looks like.** `decode_packet` returns `None` (align/padding), or every value decodes
  but is 0, or `features` arrives while `hello` never does — then `BridgeState::on_event`
  (`crates/integrations/src/bridge.rs:137`) never accepts the epoch and
  `features_stale` (`:174`) stays true forever.

### B5 — The bridge UI the spec requires does not exist. NOT WIRED. Owner: ALONE
- **Unproven.** `spec/ARCHITECTURE.md:277` requires "host/ports, enabled/connect, status/heartbeat,
  instance UUID, feature gain/gate/attack/release, meters, **eight visual macros**, preset
  previous/next/recall, latency offset, transport forward toggle, record-controls indicator".
  `flux-audio-bridge.maxpat` contains 6 `newobj`, 1 `live.dial`, 1 `comment`
  (`:53-54`, `:183` registers one parameter named `rms`) and one
  `"parameterbanks" … "buttons" : ["-","-","-","-","-","-","-","-"]` at `:188` — the eight bank
  slots are all unfilled dashes. There is no host/port control and no enabled toggle, which means
  the hardcoded `127.0.0.1`/`9000` in `bridge-osc.js:49-50` cannot be changed without editing the
  patch source.
- **Shortest procedure.** During B1, after the device loads: count Live's macro slots on the
  device, and check whether any parameter other than `rms` is automatable.
- **Pass.** At least the 8 macros present and automatable in Live's automation list with stable
  names (`spec/ARCHITECTURE.md:277`: "Parameter names stable for Live automation").
- **Failure looks like.** One dial named `rms`, eight empty macro slots, and no way to retarget
  the UDP port from the Live GUI. This is a build task, not a test task — recording it here so
  the gap is not mistaken for a hardware limitation.

### B6 — Multiple simultaneous device instances. UNVERIFIED. Owner: ALONE (needs B1)
- **Unproven.** That two instances of the audio device on two tracks don't fight.
  `README.md:18-21` versions every address by `{instanceId}` and `BridgeState::new`
  (`crates/integrations/src/bridge.rs:113`) is per-instance, but nothing in the app holds a map
  of instances (Fact 1/A2: no consumer exists). `spec/ARCHITECTURE.md:277` asks for
  "multiple device instances coexist through UUIDs and configurable reply ports".
- **Shortest procedure.** After A1/A2 are wired: put the audio device on two tracks, play both,
  confirm two distinct `hello` handshakes and non-overlapping feature streams.
- **Pass.** Two instances, two UUIDs, both live; a dead one goes stale in
  `stale_timeout` (`bridge.rs:113`) and Flux stops reacting to its last values.
- **Failure looks like.** Only one track modulates anything — the second instance's packets
  collide on one reply port, or a single global `BridgeState` last-writer-wins.

---

## 4. Gate C — Spout / NDI on real receivers, and the SpoutDX-compiled-in blocker

### C1 — Neither sender can carry Flux's output. NOT WIRED. Owner: ALONE (code) — this is the real blocker
- **Unproven.** That a VJ app ever sees a Flux frame. `flux spout-send-dx` and `flux ndi-send`
  both emit a synthetic animated gradient at hardcoded 640×360
  (`crates/app/src/main.rs:645` and `:504`). `trait LiveSink` (`crates/integrations/src/live.rs:37-41`)
  has no implementor. There is no wgpu→shared-texture→sender path anywhere.
  `implementation/BLOCKERS.md:4` says "NDI … **Fully working — discovery, send, and receive
  verified**" and `implementation/requirements.json:3743` marks AT060 `verified`.
  **Both are describing the transport shim, not Flux's output.** Verified claim vs. code:
  code is authoritative — the verified thing is "a Rust process pushed a gradient through an
  NDI DLL and another Rust process got it back". Treat every NDI/Spout "VERIFIED" in
  `implementation/STATE.json:38-39` and `implementation/FINAL-REPORT.md:6` (§5 headline) as
  "shim works, Flux frames do not".
- **Shortest procedure.** First write the frame pump (a `LiveSink` fed from the existing render
  device), then: `flux-desktop` with a plasma graph, Spout enabled, and Resolume Arena (or
  any free Spout receiver) on the same machine.
- **Pass.** A receiver shows *the actual graph, at the project resolution*, updating in real
  time, and moving a slider changes the received image within one frame.
- **Failure looks like.** The receiver shows the test gradient (proving you re-ran the CLI, not
  the app), or shows Flux's output frozen at 640×360, or the process crashes on device loss.

### C2 — SpoutDX is not compiled into the Flux binary; the current workaround is a helper EXE. Owner: MIXED (code ALONE; interop proof needs a receiver)
- **Unproven.** The production path. `implementation/BLOCKERS.md:16` names it exactly:
  "a production fix is to **compile SpoutDX into the Flux binary directly**". Current state:
  the DLL form fails to load on this host with `ERROR_INVALID_DLL` / error **182**
  (`BLOCKERS.md:12-13`), so `crates/app/src/main.rs:666-680` shells out to
  `apps/spout-bridge/spout_sender.exe` — a separate process with its own D3D11 device that
  `crates/integrations/src/spout.rs:219-221` tries first via `FLUX_SPOUTDX_SHIM`,
  `spoutdx_shim.dll`, then `apps/spout-bridge/spoutdx_shim.dll`. That helper cannot access
  Flux's GPU textures, so it is structurally incapable of C1's fix.
  Additionally `BLOCKERS.md:15-16`: "Flux's enumerator doesn't see the DX sender (different
  Spout2 registration path)" — Flux cannot discover its own output.
- **Shortest procedure.** (a) `target\release\flux.exe integrations` — the `spout` row must show
  `available=true` or the feature is off (C3) and all further testing is void.
  (b) `target\release\flux.exe spout` — enumerate senders while a *known-good* third-party
  sender (e.g. Resolume's Spout out) is running. (c) After linking SpoutDX statically into
  `flux-desktop`: send one second of a moving plasma and watch a receiver.
- **Pass.** (b) Flux lists the third-party sender by name, proving the registry path is shared.
  (c) Receiver shows Flux's live render; `flux spout` also lists Flux's own sender.
- **Failure looks like.** Error 182 again on a DLL load (missing/incompatible CRT — note
  `apps/spout-bridge/` carries seven `msvcp140*/vcruntime140*/concrt140` DLLs dated **Sep 28**,
  i.e. the VC++ runtime is being shipped by hand next to the shim, and the shipping binary will
  not find them unless the working directory or PATH happens to match `main.rs:219-221`'s
  fallback search order); or `spout_sender.exe` launches and exits 0 with nothing on the
  receiver (the Nov 9 evidence `implementation/evidence/spout-send-dx.txt` is exactly that shape:
  "DLL path unavailable; launching spout_sender.exe helper … sent 83 frames … exit code 0" —
  **83 frames sent to nobody**, with no receiver named or confirmed anywhere in the file).

### C3 — `spout`, `ndi`, `syphon`, `link`, `vision` are all off by default. Owner: ALONE
- **Unproven.** Which binary the author is about to spend an evening testing.
  `crates/integrations/Cargo.toml:24-30` → `default = []`; `crates/app/Cargo.toml:32-37` →
  `default = []`. `apps/desktop/src-tauri/Cargo.toml:25` pulls `flux-integrations` with **no
  features**. So plain `cargo build` produces a `flux.exe` whose `cmd_ndi_send` /
  `cmd_spout_send_dx` bodies are `#[cfg]`-compiled away. `implementation/evidence/*.txt` for
  NDI/Spout/Link were produced by feature builds that are not the binaries in `target/`.
- **Shortest procedure.** Two commands, before touching any receiver:
  `target\release\flux.exe integrations` and `target\release\flux.exe doctor`.
- **Pass.** A row per protocol with `available=true` on the machine you are about to test.
- **Failure looks like.** `available=false  (built without feature 'ndi')` — and then someone
  spends the evening debugging a projector. Rebuild once, deliberately, with the features
  recorded next to the evidence.

### C4 — NDI over a real network, on real receivers. UNVERIFIED. Owner: EXTERNAL (a second machine / a consumer app)
- **Unproven.** Everything past same-machine loopback. `spec` AT060/AT057 want real
  interoperability; `implementation/evidence/ndi-io.txt` and `ndi-discovery-live.txt` show
  discovery of 5 sources (NDI Test Pattern etc.) and a cross-process **640×360** frame on one
  host. Not exercised: a second NIC, Wi-Fi, a VLAN, multicast vs. unicast discovery, bandwidth
  collapse, or a third-party consumer.
- **Shortest procedure.** Point `flux ndi-send <name>` (once C1's pump exists) at a second
  machine running NDI Video Monitor / VMC / a real VJ app. Then pull one network adapter,
  then join a Wi-Fi.
- **Pass.** Received frames on a non-sender machine at the project resolution; ≥25 fps sustained;
  receiver reconnects without restarting Flux when the adapter is re-enabled.
- **Failure looks like.** Source visible in discovery but "not connected" on subscribe (unicast
  blocked by firewall); or frame rate collapses to a few fps with rising latency (bandwidth /
  `Processing.NDI.Lib.x64.dll` — 29 MB, dated **Apr 14 2026** in `apps/ndi-bridge/` — being
  loaded from a path other than the SDK install); or it works on Ethernet and never appears on
  Wi-Fi (mDNS/multicast).

### C5 — Syphon is macOS-only and macOS has no host. BLOCKED. Owner: EXTERNAL
- `implementation/STATE.json:41` "syphon: interface present; blocked_external (needs macOS)";
  `BLOCKERS.md:17` "Syphon is macOS-only"; `live.rs:73-81` reports `available:
  cfg!(all(target_os = "macos", feature = "syphon"))` — on Windows this is `false` by
  construction. AT059 (`implementation/requirements.json:3729`) is the test that would cover it.
  Nothing to do here until G1 (a Mac) is resolved. Do not mark it failed on Windows; the honest
  status is `blocked_external` per `spec/START-HERE.md:89`.

---

## 5. Gate D — Live capture and audio input on Windows (`crates/platform`)

### D1 — There is no capture code; only an enumerator. NOT WIRED. Owner: ALONE (code)
- **Unproven.** Every sentence in the docs that begins "enumeration verified, live capture
  needs a device". `crates/platform/src/lib.rs` is 185 lines: `endpoints()` (`:72-110`) lists
  WASAPI devices, `camera_devices()` (`:114-161`) walks the registry key
  `HKLM\SYSTEM\CurrentControlSet\Control\DeviceClasses\{e5323777-f976-4f5b-9b55-b94699c46e44}`,
  `enumerate()` (`:163-184`) assembles them. **Nothing in the file opens a device.**
  The self-reported limitation is `:179-181`: "live frame capture needs a device and OS permission".
  `implementation/STATE.json:75` ("Live camera/screen capture needs a device") understates it.
  There is also no screen/window/region capture implementation at all: grepping the tree for
  duplication/BitBlt/`IMFSourceReader`/Windows.Graphics.Capture yields only `live.rs:32`'s trait
  method and `ndi_io.rs:153`'s `next_frame`. So AT054, AT055, AT056
  (`implementation/requirements.json:3668, :3680, :3692`) describe tests whose subject does not
  exist yet, and they are all `implemented_unverified` — the accurate label would be
  `implementing` for the capture portion.
- **Shortest procedure.** `target\release\flux.exe capture` on the rig is still worth running —
  not as proof of capture, but as the **device-naming audit** for A7. Then the pass criterion
  below belongs to the code that does not exist.
- **Pass (once written).** A microphone sample appears as a clip and its waveform is non-flat;
  Flux's own output heard through loopback modulates a visual; unplugging the device mid-take
  produces an actionable error and the app keeps running (`spec/VERIFICATION.md` AT056 wants
  "no dummy stream, output-device change and permission recovery").
- **Failure looks like.** Today: nothing to fail — nothing runs. Later: the classic silent-dummy
  failure, where capture "succeeds" with an all-zero buffer and every audio-reactive node pins
  at its minimum. Distinguish by asserting `dropped()` (`crates/audio/src/lib.rs:523`) and a
  non-zero `window_rms` (`:386`) before trusting any reactive behavior.

### D2 — Camera enumeration yields no usable identity. UNVERIFIED. Owner: ALONE
- **Unproven.** Which entry is the actual camera. `crates/platform/src/lib.rs:147-152` builds
  names from the raw PnP device-class key string, and `is_default: idx == 0` (`:151`) is just
  registry order, not an OS default. The recorded evidence `implementation/evidence/capture.txt`
  shows one entry, named `camera { Properties }` — the leaf of the key, not a model name.
- **Shortest procedure.** Plug the real camera (or confirm the built-in one), run
  `target\release\flux.exe capture`, and check you can tell the camera apart from a virtual
  camera (OBS/CamLinker type devices, which a live performer frequently does have installed).
- **Pass.** Every camera is identifiable by display name, and the name the user picks in the GUI
  is the device that opens.
- **Failure looks like.** Three GUID-tail rows, no way to choose, and the app opening whichever
  one was registry-first. This is also the failure mode where the author picks a virtual camera,
  sees black, and blames the GPU.

### D3 — 44.1 kHz SSL 2 vs. Flux's 48 kHz default. UNVERIFIED. Owner: ALONE
- **Unproven.** `crates/domain/src/document.rs:67` (`sample_rate: 48_000`, asserted at `:700`)
  against a rig fixed at 44.1 kHz. `crates/audio/src/lib.rs:534` `Mixer::new(sample_rate)` and
  `:394 estimate_tempo(flux, window_secs)` take a rate, but nothing resamples between the device
  and the analyser, and no resampler appears in `crates/audio/Cargo.toml`.
- **Shortest procedure.** With capture working (D1), run the A5 drift test at 44.1 k device /
  44.1 k project (should be clean) and 44.1 k device / 48 k project (should drift).
- **Pass.** Either exact rate match, or documented drift ≤ 1 frame over 10 minutes with the
  resampler's latency accounted for (`spec/VERIFICATION.md` AT008's "device latency compensation").
- **Failure looks like.** ~8.9 % time drift, or pitch-consistent but progressively late visuals —
  i.e. someone "fixed" it by time-stretching. Also: a 44.1 k device opened at 48 k returns
  `AUDCLNT_E_DEVICE_INVALIDATED`-class failures.

---

## 6. Gate E — Multi-display / external output and the monitor window

### E1 — The "detached output" is a webview, not a native texture. Owner: ALONE (code) then ALONE (test)
- **Unproven**, and contradicted by the binding architecture. `spec/START-HERE.md:77`
  (binding decision 4): "The editor preview receives a bounded, reduced-resolution copy of that
  rendered output. **The performance window presents the native texture directly.**"
  What exists: `apps/desktop/src-tauri/src/lib.rs:33-47 open_monitor()` creates
  `WebviewWindowBuilder::new(&app, "monitor", WebviewUrl::App("monitor.html"))` at
  `inner_size(960.0, 540.0)` — a second **WebView2** window, i.e. another HTML page, not the
  native texture. `toggle_monitor_fullscreen` (`:50-57`) just calls `set_fullscreen` on it.
  There is no native second window and no GPU surface handed to it.
- **Shortest procedure.** Build (after the running build finishes), launch
  `target\release\flux-desktop.exe`, open the monitor, drag it to the external display, click
  Fullscreen (`apps/desktop/ui/monitor.js:61` → `toggle_monitor_fullscreen`), and read the fps
  number the window title prints (`apps/desktop/ui/main.js:223`:
  `Flux | ok ${w}x${h} ${perf.lastMs}ms ${perf.fps}fps`).
- **Pass.** On the external panel: full-resolution, vsync-locked output with no letterboxing, no
  webview scrollbars, no window chrome, and the title-bar fps ≥ 50 for an ordinary graph.
- **Failure looks like.** Correct size but visibly juddering (it is a PNG blit, see E2); or the
  external display shows the desktop's DPI-scaled copy at a soft resolution; or fullscreen works
  on the primary and silently does nothing on the secondary.

### E2 — Frames reach the monitor as base64 PNG, downscaled to 480 px, throttled to ~11 Hz. Owner: ALONE (code)
- **Unproven / measured ceiling is low.** `apps/desktop/src-tauri/src/lib.rs:209-226`: the
  export frame callback returns early if `< 90` ms since last emit (`:213`), then
  `downscale_rgba(rgba, w, h, 480)` (`:217`), `flux_media::encode_png_rgba` (`:218`) and an
  `emit("export-frame", { dataUrl: "data:image/png;base64,…" })` (`:219-225`).
  Two things to notice: (a) the ~90 ms floor caps the monitor at ~11 fps **and only during
  export** — there is no live preview pump to the monitor at all in that code path; (b) PNG
  encode of a downscaled 480-px frame is CPU work on the render thread. The editor preview
  itself is also PNG-over-IPC (`render_preview` `:61-64` — "returns a PNG data URL for the
  webview").
- **Shortest procedure.** Do not trust `apps/desktop/ui/main.js:171-181`'s `perf.fps`, which is
  a *JS round-trip* counter (it measures request→response of an IPC call that includes PNG
  encode + base64 + IPC + decode + paint). Measure with an external camera/phone on the panel,
  or with a PresentMon/framerate overlay, and compare against the printed number.
- **Pass.** Independently measured on-panel rate within 10 % of the app's claimed rate, at
  1920×1080, sustained 120 s (`spec/VERIFICATION.md:134`), and the monitor's rate is reported
  separately from the editor copy as `spec/VERIFICATION.md:134` requires ("Report UI preview copy
  separately from native output").
- **Failure looks like.** The UI says 50 fps while the panel is a slideshow — the most likely
  outcome if the title-bar number is taken at face value. Also expect a large jump in CPU per
  frame from PNG encoding, and, during export, contention with the export's own GPU work
  (`README` in `main.js:132`: "~40ms keeps the preview live without backing" and `:199`
  "While exporting, the export thread owns the GPU").

### E3 — No display selection exists at all. Owner: ALONE
- **Unproven.** That the author can choose *which* screen gets the output — the single most
  likely thing to be needed on a gig. `grep available_monitors / primary_monitor` over `apps/`
  and `crates/` returns nothing. `apps/desktop/src-tauri/tauri.conf.json:9-18` declares exactly
  one window ("Flux", 1360×860) with no position, no monitor, and no fullscreen flag. The
  monitor window inherits the default placement of a fresh Tauri webview.
- **Shortest procedure.** `apps/desktop/src-tauri/tauri.conf.json:9-18` is where a second
  window would be declared; the code-side check is: enumerate monitors at launch and print
  count, resolution, DPI, and refresh rate, then move the monitor window by index.
- **Pass.** On a laptop + one external panel: index 0 and index 1 both selectable, each renders
  at the panel's native resolution and refresh rate, and the choice persists across restarts
  (AT065: "settings persist"; `spec/ARCHITECTURE.md:293` puts window bounds in app-data JSON).
- **Failure looks like.** Output lands on the laptop screen while the audience sees the projector
  mirror; or 1360×860 editor scaled to a 1920×1080 panel; or DPI 150 % producing a blurred
  fullscreen; or the window is on the external display but the *fullscreen call* maximizes the
  primary. `spec/VERIFICATION.md` AT065 additionally requires "consumer failure does not stop
  preview" — test by unplugging the external display mid-run.

### E4 — `bundle.active = false`: there is no packaged app to put on a second machine. Owner: ALONE
- `apps/desktop/src-tauri/tauri.conf.json:24-27` — `"bundle": { "active": false }`. So the only
  "app" is an `.exe` in `target\`, which explains why AT082
  (`implementation/requirements.json:4004`, clean-install/offline-launch/DLLs-without-dev-PATH)
  is `implemented_unverified` and why `implementation/STATE.json:79` lists "No signed installer".
  It also means C2's hand-carried `msvcp140*.dll` problem (and NDI's DLL search) is *untested*,
  because nobody has ever run a packaged Flux.
- **Shortest procedure.** Enable bundling, build the MSI/NSIS, install on a clean Windows user
  account, launch offline with no dev PATH.
- **Pass.** Installer runs as standard user; `flux-desktop` starts; `spout_shim.dll`,
  `spoutdx_shim.dll`, `ndi_shim.dll`, `Processing.NDI.Lib.x64.dll` and the VC++ runtime resolve
  **without** `apps/` present — note `crates/integrations/src/spout.rs:219-221` searches
  `apps/spout-bridge/spoutdx_shim.dll`, a source-tree-relative path that will not exist in an
  installed layout. User projects survive uninstall.
- **Failure looks like.** `ERROR_INVALID_DLL`/182 again, but now for the *installed* copy — and
  this is a strong candidate for the root cause of the very blocker in C2, since 182 is a DLL
  dependency-load failure, not a "the DLL is bad" failure. Test the installed layout's loader
  path before assuming 182 is inherent.

---

## 7. Gate F — Latency: preview fps, export timings, and what must be re-measured

### F1 — Every number in `implementation/evidence/` predates the current code. Owner: ALONE
- **Measured facts (file mtimes vs. git).** The tree measured was `46cfd94`, committed
  **2026-11-13 03:39** — a dated snapshot, not the current tip.
  Binaries: `target\release\flux.exe` **Nov 9 04:24**; `target\debug\flux.exe` Nov 10 01:37;
  `target\debug\flux-desktop.exe` Nov 10 04:43; `target\release\flux-desktop.exe` Nov 10 10:40.
  Evidence directory: **all 26 files dated Nov 9** (`benchmark.json`, `test-gpu.json`,
  `verify.json`, `media-export.txt`, `spout-*.txt`, `ndi-*.txt`, `link-*.txt`,
  `vision-inference.txt`, `gui-release.png`, `capture.txt`).
  Meanwhile 15 commits touched `crates/` or `apps/` **after** Nov 9 04:24, including the ones
  that change the frame and export paths outright:
  `3a2b43e` "Audit every node, fix black presets, **real video export, crisp 50fps preview**"
  (Nov 10 02:05), `ac73572` "**Sliders, monitor window**, and catalog/producer refinements"
  (Nov 10 09:25), `fe9e35e` "Bundle the webview…" (Nov 10 09:06), `8139007` "Fix param edits not
  re-rendering" (Nov 10 01:02), `3ee178b` "**stream export frames to the monitor**" (Nov 10 00:23),
  `2688195`/`7618d9e` (node-graph DAG executor, per-node textures, `render_graph` path).
  **Conclusion: the release CLI everyone will run was built before the preview and export paths
  it is credited with were written.** `implementation/FINAL-REPORT.md:75` still points at it as
  the artifact ("Release CLI `target/release/flux.exe`"), and
  `implementation/STATE.json:13` still reports `test_totals: {passed: 167, failed: 0}` from a Nov 9
  run. `implementation/BLOCKERS.md` (Nov 9 19:07) and `STATE.json` (Nov 9 18:41) are ~4 days
  behind the code, as the project brief already assumes.
- **Shortest procedure.** After the in-flight build completes: `cargo build -p flux-app
  --release` and `cargo build -p flux-desktop --release` so both artifacts are from `46cfd94`,
  then re-run the timing captures in F2–F4 and overwrite the evidence with the git SHA stamped
  in the filename. Confirm the SHA with the build-stamp path fixed in `f9e88ec`.
- **Partly discharged 2026-11-14.** The xtask gates were re-run from a binary built at
  `e84fe67`, so `catalog.json`, `doctor.json`, `test-gpu.json`, `fixtures.json` and
  `benchmark.json` in `implementation/evidence/` are current (they are regenerated by every
  gate run, which is also why a verification pass always leaves the tree dirty). Still stale:
  the **release** binaries (`target/release/flux.exe`, `flux-desktop.exe`), which `verify.ps1`
  does not build and `FINAL-REPORT.md:75` still credits, and every hand-captured
  `*.txt`/`*.png` from Nov 9 (`media-export.txt`, `spout-*`, `ndi-*`, `link-*`,
  `vision-inference.txt`, `gui-release.png`, `capture.txt`). F2 and F4 therefore remain open.
- **Pass.** Each evidence file names the commit SHA and the binary's mtime; numbers differ from
  the Nov 9 set or are explicitly restated as unchanged.
- **Failure looks like.** A 50 fps claim that reproduces only at 25 fps on the current tree —
  i.e. the regression hid in the DAG-executor work, and the frozen `benchmark.json` was the only
  record of the old behavior.

### F2 — Preview fps: `50fps` is a commit message, not a measurement. Owner: ALONE
- **Unproven.** `3a2b43e` claims "crisp 50fps preview"; `apps/desktop/ui/main.js:492-493`
  documents the removal of the earlier fixed 100 ms tick ("it runs as fast as frames can actually
  be produced (the old fixed 100 ms interval forced 10 fps regardless of how cheap a frame was)").
  The only number the app produces is `perf.fps` (`main.js:171-181`, displayed `:62`, `:223`),
  which measures the *JS request round trip* including PNG encode and IPC — see E2. There is no
  presentation-rate measurement, no p95, and no drop counter anywhere in the preview path.
- **Shortest procedure.** Warm up, then run the ordinary graph at 1920×1080 for 120 s
  (`spec/VERIFICATION.md:134`) and record: (a) the app's `perf.fps`, (b) an independent
  on-panel measurement (PresentMon, or a phone slow-motion on a beat-synced counter), (c)
  `perf.lastMs`. Repeat with the monitor window open and fullscreened on the external panel —
  which is the configuration that will actually be used live and which has never been measured.
- **Pass.** (b) ≥ 50 fps sustained on an ordinary graph with p95 frame time ≤ 16.67 ms and
  presentation misses < 1 % (`spec/VERIFICATION.md:138`), and the app's own number agrees with
  the independent one within 10 %.
- **Failure looks like.** (a) 50, (b) 20 — the app's self-report is measuring its own request
  loop, not frames on glass. Watch specifically for: the export thread stealing the GPU
  (`main.js:199`), `PreviewRenderer` cache misses (`b5889b9`), and per-frame PNG encode cost
  scaling with preview size.

### F3 — the benchmark harness, and what its numbers were actually measuring. Owner: ALONE
- **The Nov 9 headline dissolved. The gate is now measured on a real graph, and passes.**
- **What the Nov 9 file claimed, and why it could not be read.**
  `implementation/evidence/benchmark.json` (`"profile": "ordinary"`, 30 generators, Nov 9 02:27)
  reports per-node p50/p95 in ms. Two entries exceed the `p95 ≤ 16.67 ms` goal in
  `spec/VERIFICATION.md:138`: **`gen.concentric` p95 = 20.91 ms** and **`gen.clouds`
  p95 = 17.37 ms** (p50 for both ≈ 2.5–2.6 ms, so these are tail spikes, not throughput
  limits). The file records **no resolution, no adapter/driver, no p99, no presentation misses,
  no warm-up state** — all of which `spec/VERIFICATION.md:134` and `:136` require ("Record exact
  adapter/driver and actual path"). `implementation/BLOCKERS.md:46-47` separately warns
  "SDF/particle producers: correct and deterministic CPU implementations; **not realtime at 4K**"
  — so the CPU-producer class has a known, unquantified ceiling.
- **Re-measured 2026-11-14 at `e84fe67`, and the harness was the bigger problem.**
  `cargo run -q -p xtask -- benchmark --profile ordinary` on the freshly built xtask, same host
  (RTX 2080 Ti / Vulkan): 30 generators at 1920×1080, median worst-sample **4.98 ms**, and only
  **one** row over a 16.6 ms budget — `gen.concentric` (median 2.75 ms, worst 21.49 ms).
  `gen.clouds` is no longer over budget (worst 5.07 ms, was 17.37), so the Nov 9 spike did move;
  the `concentric` tail persists. Three things made even this weak evidence, and they were in
  `crates/xtask/src/main.rs`, not in the GPU:
  (a) `--profile` was parsed and written to the JSON but never used — resolution hardcoded
  1920×1080, the set was `.take(30)` fragment generators in catalog order, so `--profile 4k`
  did identical work;
  (b) every timed iteration wrapped a `read_preview` call, which `preview_size()` clamps to the
  960×540 preview cap (`crates/render/src/preview.rs:7-11`) — so the number was render **+ a
  2,073,600 B GPU→CPU copy**, not what a live frame costs and not comparable to the
  `p95 ≤ 16.67 ms` goal;
  (c) with 12 samples, `times[len*95/100]` is index 11 — the single worst sample — so the field
  named `p95ms` was `max`, and `p50ms` was index 6. There was no p99 and no warm-up discard.
  The other gap this gate needed was a **multi-node graph**: until 2026-11-15 nothing timed one.
  `crates/tests/tests/preview_perf.rs:12` renders `chain_graph("gen.plasma", &[])` — one
  generator, zero effects — at 320×180/352×198/640×360/960×540 only, and returns early on
  `no GPU; skipping`.
- **Harness defects (a)–(c) discharged 2026-11-15 (D021), and the multi-node gap is closed.**
  `--profile` now selects resolution, budget and graph from a profile table (`BENCH_PROFILES`,
  `crates/xtask/src/main.rs`), and an unknown profile is refused with exit 2 instead of silently
  benching 1080p. The timed path is
  `gen.plasma → fx.color_adjustment → fx.warp → fx.halftone → fx.vhs → fx.bloom → fx.composite →
  output`: 8 nodes, 8 compiled passes, with the generator also feeding `fx.composite` port `b` so
  the measured DAG genuinely merges. Render and readback are timed in separate windows, the
  render window ends in an explicit queue wait (because `execute()` only submits), 15 warm-up
  frames are discarded and 120 are timed, and `p95`/`p99` are emitted because n≥100 supports them.
  The per-generator sweep survives at 10 samples each, so it reports `median`/`max` and nothing
  that looks like a percentile.
- **Measured 2026-11-15 on this host (RTX 2080 Ti, Vulkan, driver 560.94), 8-node chain:**
  | profile | resolution | render median | render p95 | render p99 | render max | copy median | budget |
  |---|---|---|---|---|---|---|---|
  | `ordinary` run 1 | 1920×1080 | 1.53 ms | 2.32 ms | 3.37 ms | 3.40 ms | 2.52 ms | 16.67 ms |
  | `ordinary` run 2 | 1920×1080 | 2.09 ms | 3.71 ms | 7.49 ms | **19.57 ms** | 4.28 ms | 16.67 ms |
  | `ordinary` run 3 | 1920×1080 | 1.59 ms | 2.59 ms | 2.78 ms | 2.96 ms | 2.51 ms | 16.67 ms |
  | `4k` | 3840×2160 | 3.98 ms | 5.09 ms | 6.12 ms | 8.37 ms | 3.32 ms | 33.33 ms |
  Three conclusions, measured rather than reasoned. **(1)** The gate passes: p95 is 2.3–3.7 ms
  against a 16.67 ms budget at 1080p and 5.09 ms against 33.33 ms at 4K. **(2)** The old
  over-budget rows were a readback artefact, not a node — separating the windows puts all 30
  swept generators under 16.67 ms on render alone (worst 10.04 ms, `gen.fractal_noise`), while
  those same rows' `render+copy` max still reaches 30.58 ms (`gen.clouds`). The Nov 9
  `gen.concentric` p95 of 20.91 ms belonged to the copy. **(3)** The copy barely changes with
  resolution (2.5 ms at 1080p, 3.3 ms at 4K) because `preview_size()` caps it at 960×540 either
  way, which is precisely why the old number could not scale and could not be read as a frame cost.
- **What this still does not prove.** Run-to-run spread is a factor of ~6 in the tail (max 2.96 ms
  vs 19.57 ms on identical work), so one invocation is not a stable measurement and the
  single-frame hitch is unexplained. `presentation_misses` is recorded as
  `{"measured": false}` — headless graph execution has no swapchain, so nothing here says a frame
  reached glass (that is F2). There is no 120 s steady-state soak, no peak RAM/VRAM, no decode
  drops or audio underruns, and the `particles`/`sdf`/`fluid`/`recording` profiles of
  `spec/VERIFICATION.md:139-142` are still absent from the table — naming one of those is a
  refusal, not a measurement. The CPU-producer ceiling (`BLOCKERS.md:46-47`, "not realtime at 4K")
  is untouched. `benchmark.json` keeps only the most recent profile, since each run overwrites it.
- **Shortest procedure — the harness part is done; what is left is soak and coverage.**
  Remaining, in the order that buys the most truth per hour: (1) run the ordinary and 4K profiles
  as a 120 s steady-state soak rather than 120 frames and record peak RAM/VRAM, because the hitch
  in `ordinary` run 2 above is exactly the kind of thing a short sample cannot characterise;
  (2) add the `particles`/`sdf`/`fluid`/`recording` rows to `BENCH_PROFILES` so naming one
  measures it instead of refusing — the SDF/particle rows are where `BLOCKERS.md:46-47` expects
  the ceiling to be; (3) take the drop count from somewhere that has a swapchain (F2), since this
  harness structurally cannot produce one; (4) re-run `flux.exe doctor` and keep its adapter line
  beside the numbers (recorded in `test-gpu.json`: RTX 2080 Ti, Vulkan, driver 560.94) — on
  **this** machine, at the exact settings used live.
- **Pass.** Every node in the live set has a recorded p95 and p99 under the budget at the chosen
  resolution, on the exact adapter, with the numbers written down; anything over budget is either
  repaired or documented as a known capacity limit ("If not met, document actual sustainable
  quality/rate", `spec/VERIFICATION.md:142`).
- **Failure looks like.** p50 fine and p95 spiking to 20–30 ms — audibly/visibly *not* a slow
  machine, it is a hitch every 1–2 seconds, which on a live projection reads as a stutter. That
  is the failure mode a p50-only culture misses, and it is what a gig exposes. Note that the
  harness no longer hides this shape: `ordinary` run 2 above reported median 2.09 ms with a
  19.57 ms max, and the tool exits 1 when the chain tail breaches the budget rather than
  averaging it away.

### F4 — Export wall-clock timings are unrecorded for the path that ships. Owner: ALONE
- **Unproven.** `implementation/evidence/media-export.txt` (Nov 9 02:42) records only *decoded
  container facts* — `codec_name=h264 width=640 height=360 duration=3.000000 nb_frames=180` and
  a 1280×720/4 s/240-frame case. **No encode time, no fps-of-encode, no audio-video sync check,
  no failure injection.** `spec/VERIFICATION.md` AT066/AT067/AT069/AT070
  (`implementation/requirements.json:3815, 3825, …`) cover overload policy, CFR gaps, the codec
  matrix, cancel/disk-full, and fractional fps — all `implemented_unverified`. The video-export
  code itself landed in `3a2b43e`/`6ef3eaa`, *after* that evidence and after the release binary.
- **Shortest procedure.** On the rebuilt binary, render a known 60 s project at 1920×1080/60
  H.264 and time it end-to-end, then `ffprobe` the result for `nb_frames` (expect exactly 3600),
  duration, and PTS monotonicity. Repeat with the GUI's export running so the monitor is
  streaming frames (E2) — the configuration that steals the GPU. Then run
  `flux.exe render --codec hevc|prores|gif|none` once each and record which encoders actually
  exist on this box (`docs/USER-GUIDE.md:23`).
- **Pass.** Exact frame count, exact duration, audio and video lengths agree, encode rate ≥ 1×
  realtime for the profile you advertise, and the numbers logged with the commit SHA.
- **Failure looks like.** One or two frames short (CFR rounding), or AV drift of a few hundred ms
  in a long export, or a file that plays in VLC but not in the delivery target, or the GUI
  export being far slower than the CLI export because the monitor frame-callback (`:213-226`) is
  eating CPU. Also verify `3ee178b`'s "stream export frames to the monitor" did not change
  export throughput.

### F5 — 2 h soak, and the 44.1 k ASIO-held box. Owner: ALONE
- **Unproven.** AT081 (`implementation/requirements.json:3994`) wants "Soak 30min then 2h;
  stable cache/budgets/queue depths and no drift/leaks/crash under repeated edits and scene
  switches"; `spec/VERIFICATION.md:144` sets the trip wire at unexplained growth > 10 % or 64 MiB.
  `main.js:218` calls `monitorSize()` per frame and the export path allocates a PNG per emit;
  `crates/audio/src/lib.rs:523` exposes a `dropped()` counter nobody reads.
- **Shortest procedure.** 2 h of the live set: continuous playback/looping, scene recalls and
  parameter edits every 30 s, with the monitor fullscreen on the external panel; sample Working
  Set / GPU memory every 60 s; record `dropped()` at start and end.
- **Pass.** Growth < 10 % and < 64 MiB, `dropped()` bounded and explained, no presentation-miss
  creep between minute 5 and minute 115.
- **Failure looks like.** Steady climb in commit count with flat fps then a stall — or fps that
  decays slowly, which is the signature of a per-frame PNG/GC/base64 leak in the webview path
  rather than a GPU leak.

---

## 8. Gate G — macOS host, code signing, notarisation

### G1 — macOS paths are unverified because there is no Mac. BLOCKED. Owner: EXTERNAL
- **Unproven.** `implementation/BLOCKERS.md:29` "macOS hosts: unavailable; macOS-only paths
  unverified"; `implementation/STATE.json:74` "macOS builds unverified (no host)";
  `implementation/requirements.json:14` (R001) blocker "macOS hosts unavailable";
  `crates/platform/src/lib.rs:36-47` returns an empty capability set with
  `"capture adapters are implemented for Windows x64 only"`; `crates/integrations/src/live.rs:73-81`
  makes Syphon unavailability *structural* on Windows. AT083/AT084
  (`implementation/requirements.json:4016, :4026`) are the arm64/x64 packaged-launch tests and
  both are `implemented_unverified`. `spec/START-HERE.md:89` explicitly forbids claiming
  "macOS verification from Windows".
- **Shortest procedure.** Needs Apple hardware or a hosted macOS runner — nothing on this rig can
  substitute. When a Mac exists: build, launch, and confirm Syphon send/receive, `rpath`/dylib
  resolution, and that the file dialogs work; record "architecture/build alone is not runtime
  verification" (AT084's own words).
- **Pass.** `spec/START-HERE.md:32` R001 satisfied on arm64 *and* x64 by observation.
- **Failure looks like.** Not applicable yet — an untested assumption. Treat Syphon (C5) as
  downstream of this.

### G2 — Code signing / notarisation: no identity, and bundling is switched off. BLOCKED + ALONE
- **Unproven.** `implementation/BLOCKERS.md:30` "Code signing / notarization: no identity;
  builds are unsigned"; `implementation/STATE.json:79` "No signed installer".
  `apps/desktop/src-tauri/tauri.conf.json:25` `"active": false` — so beyond the missing identity
  there is no installer artifact to sign at all. Windows SmartScreen and macOS Gatekeeper
  behavior are therefore entirely unknown, and unsigned is exactly what breaks the clean-user,
  offline, dev-PATH-free launch AT082 (`implementation/requirements.json:4004`) wants.
- **Shortest procedure.** Partly alone: turn bundling on and produce an **unsigned** MSI/NSIS,
  then install it on a standard user account, offline, with the Rust toolchain not on PATH.
  That exercises everything except the certificate. Windows EV/OV cert and an Apple Developer
  ID (+ notarisation) are purchases/accounts — external.
- **Pass (unsigned stage).** Installer completes as standard user; app launches; all native
  shims load (E4). **Pass (signed stage).** No SmartScreen warning on Windows; on macOS the app
  passes notarisation and opens on first launch.
- **Failure looks like.** "Windows protected your PC", or an installed app that dies at load on
  `spout_shim.dll`/`ndi_shim.dll` because the loader resolves them relative to a source tree that
  is not there — see C2/C4's note about the manually-carried VC++ runtime.
- **Cost note:** external, and cheap to defer, but notarisation is the item with the longest
  lead time (account + certificate), so start it before the first gig where the app is handed to
  anyone else.

---

## 9. Gate H — Vision models (Depth Anything v2) and model-availability env vars

### H1 — The `vision` feature is off by default, so `flux depth` cannot exist in the binary you have. Owner: ALONE
- **Unproven.** `crates/integrations/Cargo.toml:25` `default = []`, `:30` `vision = ["dep:ort"]`,
  and `crates/integrations/src/lib.rs:15-16` `#[cfg(feature = "vision")] pub mod vision;`.
  `target\release\flux.exe` is Nov 9 04:24 (F1), built long before `93484db` (Nov 9 18:44) added
  the ONNX backend — so **the depth command is not in the release CLI**, whatever
  `implementation/BLOCKERS.md:31-36` and `implementation/requirements.json:3923` (AT075
  `verified`) say about the host.
- **Shortest procedure.** `target\release\flux.exe depth implementation/evidence/depth-demo`
  first — expect "unknown command" or a silent no-op; then rebuild
  `cargo build -p flux-app --release --features vision` and retry. Confirm with
  `flux.exe doctor`.
- **Pass.** The feature-off binary reports the absence truthfully, and the feature-on binary
  produces `depth.png` in a few hundred ms.
- **Failure looks like.** Someone concludes "depth doesn't work" from a feature-off binary.
  The `ort` build also needs the `download-binaries` fetch to have succeeded
  (`implementation/evidence/vision-inference.txt` lines 4-6: statically linked,
  "no runtime DLL ships", optional `DirectML.dll`) — so an offline box is a real failure mode.

### H2 — The env vars are not set on this machine and the weights are not in the repo. Owner: ALONE (model download = external service)
- **Unproven.** Resolution order is `FLUX_MODEL_DEPTH` → `FLUX_MODELS_DIR/<dir>`
  (`crates/integrations/src/vision.rs:117-128`), then `model.onnx` / `depth.onnx` / any `.onnx`
  in that dir, with `FLUX_DEPTH_ONNX` as an override (`:130-148`), and the inference size from
  `FLUX_DEPTH_SIZE` defaulting to 518 (`:58-61`). The Nov 9 evidence used
  `FLUX_MODEL_DEPTH=C:\flux-models\depth` (`implementation/evidence/vision-inference.txt:15`) —
  a path that is not in the repo and may not exist on the rig now. Model weights are
  user-installed by design (`implementation/STATE.json:20`, `BLOCKERS.md:35`); the registry keeps
  the node editable and emits a non-fatal `SdkMissing` rather than faking output (AT077,
  `implementation/requirements.json:3946`) — that behavior is the one genuinely good part of this
  gate.
- **Shortest procedure.** `set` (or `$env:`) the four variables, drop the ONNX file in, run
  `flux.exe models` and then `flux.exe depth <dir>`; time it.
- **Pass.** `flux models` reports `available` for depth and `missing` (with an actionable install
  message per node) for the others; `flux depth` writes a real depth map, and the reported time is
  recorded against the current binary rather than the "~330 ms" from Nov 9.
- **Failure looks like.** `no depth model dir (set FLUX_MODEL_DEPTH or FLUX_MODELS_DIR)`
  (`vision.rs:54-56`); or a `FLUX_MODELS_DIR` that exists but lacks a `depth` subdir — which
  silently falls through to `None` at `vision.rs:126` rather than reporting the near miss; or an
  `incompatible` shape error from `unexpected depth shape {dims:?}` (`vision.rs:88`) if the
  downloaded export has different I/O names than the assumed `pixel_values` /
  `predicted_depth` (`vision.rs:5-7`, `:39-44` reads inputs positionally and `:81-83` reads
  `outputs[0]` positionally).
- **Note:** downloading weights is a network dependency on
  `onnx-community/depth-anything-v2-small` (Apache-2.0, per `vision-inference.txt:10-12`) — the
  license record itself is fine, the fetch is not offline-hostile, so treat as ALONE-with-network.

### H3 — Depth polarity has only ever been exercised on a synthetic square. UNVERIFIED. Owner: ALONE (a real photo/camera needed)
- **Unproven, and the existing evidence is self-inconsistent.** `vision.rs:6-7` documents
  "output `predicted_depth` … relative depth (**higher = nearer**)" and `vision.rs:204`
  `depth_to_rgba` maps "normalized depth (0..1, nearer=higher) … near = bright".
  But `implementation/evidence/depth-inference.txt` records `center=0.001 corner=0.991` for a
  synthetic image whose near object is a **center** square (`vision.rs:255-260` builds exactly
  that: a bright 32×32 block in the middle of a 64×64 field), and
  `vision-inference.txt:17-19` describes it as "bright=far edges, dark=near center square" —
  i.e. the depth map is **inverted** relative to the code's own comment and mapping. The test
  that's supposed to catch this (`vision.rs:246-276 real_depth_inference_when_model_present`)
  only asserts the values *vary* (`max - min > 0.1`) — it never asserts which end is nearer.
  So Depth-Anything's disparity-vs-depth convention is **unproven**, and everything downstream
  (masking, parallax, DOF — R022, `implementation/requirements.json:275`) inherits the ambiguity.
- **Shortest procedure.** Run `flux depth` on one real photograph with an unambiguous near
  object and far background, and on a live camera frame of the author's own desk; look at whether
  the near thing is bright.
- **Pass.** The near object is bright, consistently, and the convention is written down with a
  named test that asserts the sign.
- **Failure looks like.** Background blur / depth-mapped effects working backwards (far edges
  sharp, the subject blurred). Cheap to fix, expensive to discover on stage.

### H4 — Only depth exists; the other model-backed nodes are registry rows. Owner: ALONE
- **Unproven / largely absent.** `crates/integrations/src/vision.rs` contains
  `DepthModel`, `DepthResult`, and nothing else. `BLOCKERS.md:36` says "other G17x models follow
  the same path" — a plan, not code. R022 (`implementation/requirements.json:275`) covers
  "Depth, motion, masking, and optional vision/model inputs"; `spec/START-HERE.md:67` lists
  optional vision models as **Conditional**. So the honest statement is: depth works in a
  feature build with a user-supplied model; motion, landmarks and masks have a registry entry and
  an install message, and no inference.
- **Shortest procedure.** `flux.exe models` and read every row's status string; then try to add
  each model-backed node to a graph with the model absent.
- **Pass.** Each absent model produces a non-fatal diagnostic, the node remains editable, and no
  node silently emits a placeholder frame (`spec/START-HERE.md:67` "Do not call a detector or
  disabled menu a complete integration").
- **Failure looks like.** A black or noise frame where a mask should be — which looks like a GPU
  bug from the outside. Check this explicitly, because it is the failure the whole project is
  most careful about claiming, and the easiest one to reintroduce.

---

## 10. Suggested order of work (each step is one machine, one evening)

| # | Gate | Why this order | Owner |
|---|---|---|---|
| 1 | F1 | Rebuild from `46cfd94`. Every other item is meaningless against Nov 9/Nov 10 binaries. | ALONE |
| 2 | C3, H1 | `flux integrations` + `flux doctor`: learn which features your binary actually has, before testing what isn't compiled in. | ALONE |
| 3 | A1, A2, A3, A4, A5 | **Code** the OSC receive path into the desktop process and route bridge + transport packets; then wire Live. Nothing in Gate A can be *tested* before this. | ALONE (code), ALONE (test) |
| 4 | Fact 4 / A7 / D1 | **Code** a real WASAPI loopback capture (and decide the ASIO coexistence policy for the SSL 2), then run A7's enumeration audit. | ALONE (code) + rig |
| 5 | B1 → B2, B3, B4, B5, B6 | Package the two devices, fix the dead `bootpath` and README path, round-trip real OSC bytes. | ALONE (has Live 12) |
| 6 | C1 → C2, C4 | **Code** a `LiveSink` fed by the render device, then Spout/NDI to a real receiver. | ALONE (code) + EXTERNAL (consumer app/machine) |
| 7 | E1, E2, E3 | Native output window + display selection; then measure on a second panel. | ALONE (code) + rig |
| 8 | F2, F3, F4, F5 | Re-measure preview fps, p95/p99, export wall-clock, soak — on the rebuilt binary, SHA-stamped. **F3's harness is fixed and its 1080p/4K graph numbers are recorded (D021); what remains here is the soak, the four unbuilt profiles and F2's drop count.** | ALONE |
| 9 | H2, H3, H4 | Model env vars, real-photo polarity check, per-node absence behavior. | ALONE (weights fetch needs net) |
| 10 | E4, G2 | Installer (unsigned) → certificates. | ALONE (bundle) + EXTERNAL (identity/notarisation) |
| 11 | G1, C5 | Everything macOS. | EXTERNAL |

## 11. Where the documentation and the code disagree (code wins)

| Topic | Docs say | Code says | Cite |
|---|---|---|---|
| NDI | "**Fully working** — discovery, send, and receive verified. No remaining NDI blocker." | The only sender transmits a synthetic 640×360 gradient; `trait LiveSink` has no implementor; Flux's output never reaches a receiver. | `implementation/BLOCKERS.md:4-9`, `implementation/requirements.json:3743` vs `crates/app/src/main.rs:504`-`:520`, `crates/integrations/src/live.rs:37-41` |
| Spout | "Spout2 works **send-side** … 83 frames sent" | Those 83 frames were a gradient, sent to no named or confirmed receiver, from a helper EXE outside the Flux process. | `implementation/BLOCKERS.md:10-16`, `implementation/evidence/spout-send-dx.txt` |
| OSC | "OSC (real UDP loopback, rate-limit) + MIDI parsing + live control bridge -> engine commands" | Half of it is now true, and the doc still describes the old half: `flux osc` prints for 30 s and quits, while the *desktop* is the thing that edits the document (`control::OscBridge`). `apply_controls` still has zero callers. Nobody has watched a Live fader move a Flux parameter. | `implementation/STATE.json:64`, `docs/USER-GUIDE.md:67-71` vs `crates/app/src/main.rs:865-883`, `crates/integrations/src/osc.rs:268`, `crates/app/src/control.rs:304` |
| Max bridge | "bridge_max: verified (protocol decode + maxpat structure)" | Structure verified ≠ traffic verified. The discard defect is gone (`osc::decode_control` tries the bridge grammar first, and the desktop listener counts bridge packets), but every bridge packet is `notApplied`: no feature value reaches an analyser, and no real Max packet has ever been decoded. | `implementation/STATE.json:29` vs `crates/integrations/src/osc.rs:47`, `crates/app/src/commands.rs:284`-`:306` |
| Ableton Link | AT063 `verified`; single-master arbitration both "verified" and "not yet asserted" | Feature off by default; `live.rs` reports availability from `cfg!(feature)`, so the shipping binary has no Link; no Link↔renderer connection exists. | `implementation/STATE.json:35` vs `:22-23`/`BLOCKERS.md:43`; `crates/integrations/Cargo.toml:25`, `crates/integrations/src/live.rs:82-90` |
| Capture | "WASAPI endpoint + PnP camera enumeration is real and verified … live capture needs a device" | Enumeration is complete and **capture code does not exist**; AT054/055/056 have no subject. | `implementation/BLOCKERS.md:38-41`, `implementation/requirements.json:62` vs `crates/platform/src/lib.rs:163-184` |
| GUI verified list | Seven behaviors "verified" incl. timeline, inspector, preview | That list predates the node-graph editor (`2688195`), the DAG executor (`7618d9e`), sliders + monitor window (`ac73572`) and the webview bundling (`fe9e35e`); the screenshots are Nov 9 10:33. | `implementation/STATE.json:46-54` vs `implementation/evidence/gui-release.png` mtime and the four commits |
| Test totals | "167 passed, 0 failed" (Nov 9) | **Resolved 2026-11-14, re-measured 2026-11-15:** `cargo test --workspace -- --test-threads=1` at `e84fe67` was 196 passed / 0 failed / 0 ignored, 40 binaries, exit 0; the same command on the tree that added the OSC receive bridge is **203 passed / 0 failed / 0 ignored, 40 binaries, exit 0** — the +7 are `crates/app/src/control.rs`'s loopback tests. The vacuity caveat in §0 still applies: 14 GPU integration tests return early on device-creation failure and count as passes. | `implementation/STATE.json` `test_totals` |
| CI / `verify.ps1` | "CI workflow + infrastructure/verify.ps1 (VERIFY: PASS)" | **Was VERIFY: FAIL at `e84fe67` (measured 2026-11-14, full run, exit 1); the blocking step is now cleared.** `cargo fmt --all -- --check` — the first check step in `ci.yml:29` — exited 1 with 56 diffs across 12 files, and in Actions that aborted the job before the suite ran. All 13 other executed local steps passed. `cargo fmt --all` has since been applied and `--check` exits 0. Both harness defects are repaired (D020): the chained `integration-tests` step is three steps, `Step` clears `LASTEXITCODE` before running, the skipped Link/Spout gates record `ok=null` + `status='blocked_external'`, and `verify.json` is written without a BOM with `gitHash` and the step counts in it. | `.github/workflows/ci.yml:29`, `infrastructure/verify.ps1:32`, `:35`, `:47-61` vs `implementation/evidence/verify.json`, `implementation/DECISIONS.md` D019 |
| `.amxd` | Honest: needs a Max host | — | `implementation/STATE.json:24`, `implementation/requirements.json:27` (still `pending`, correctly) |
| Depth polarity | "higher = nearer", "near = bright" | Recorded output of a synthetic near-center square: `center=0.001 corner=0.991`; the test only asserts variance. | `crates/integrations/src/vision.rs:6-7`, `:204` vs `implementation/evidence/depth-inference.txt`, `implementation/evidence/vision-inference.txt:17-19`, `crates/integrations/src/vision.rs:246-276` |
| Stale absolute paths | "Repoint the Max patch at the new tree" (`46cfd94`) | **Resolved 2026-11-14:** `flux-audio-bridge.maxpat:199` `bootpath` and `apps/max-for-live/README.md:55` now read `D:/Projects/visual/genfx/...`; `check-maxpat.cjs` still PASSes. The remaining `D:\Projects\skeer-audio\genfx` mentions in this document (§3, §4 of gate B) and in `IMPLEMENTATION-ARCHITECTURE.md:654`, `:863` are historical descriptions of the bug, not live paths. | `5fbfc1e`/`e84fe67` vs `apps/max-for-live/flux-audio-bridge.maxpat:199`, `apps/max-for-live/README.md:55` |
