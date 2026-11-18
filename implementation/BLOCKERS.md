# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI** (R006, AT060): **transport works and now carries a Flux render; the product path
  does not.** Re-measured 2026-11-15 (`evidence/ndi-2026-11-15.txt`): the NDI 6 Tools runtime
  loads and `NDIlib_initialize` succeeds, `flux ndi` runs discovery, and synthetic 640×360 RGBA
  frames come back both through a same-process loopback and across two processes
  (`ndi-send` → `ndi-recv`, 108,601-byte PNG whose sampled pixels match the sent gradient).
  Re-measured 2026-11-18 (`evidence/ndi-loopback-2026-11-18.txt`, D049) on a render instead of
  a pattern: `flux ndi-loopback` builds the gen.plasma + fx.bloom project, renders it through
  `export::PreviewRenderer` and sends those bytes, which come back within max R17 G6 B11 of
  the render they came from, 21.9x further from the other render, alpha plane unchanged.
  The 2026-11-09 line here read "**Fully working — discovery, send, and receive verified**"
  and "discovery found **5 live sources**"; that count was the network's state that
  afternoon (Premiere, the NVIDIA sender, Test Patterns), not a capability — today the same
  command finds **0** sources. Two things are still missing and no blocker list should hide
  them: there is **no producer on the running preview** — `trait LiveSink`
  (`crates/integrations/src/live.rs:37`) has no implementor, so what crosses is a CLI's own
  one-shot render and not what Flux is showing — and no third-party consumer has ever received
  from Flux (the peer in every run above is Flux's own receiver).
  `apps/ndi-bridge/ndi_shim.cpp` is compiled against the official NDI SDK
  headers (vendored via `ndi-sdk`), so structs are correct; `ndi_shim.dll` itself dates from
  2026-11-09 and was not rebuilt for these runs.
- **Spout2 (Windows)** and **Syphon (macOS)**: Spout2 (BSD-2) worked **send-side** — 83 frames
  through a SpoutDX D3D11 sender on 2026-11-09 (`evidence/spout-send-dx.txt`). D036 re-ran it
  2026-11-16: it no longer reproduces, `spout_sender.exe` dies at process start (`0xc0000138`).
  Because the SpoutDX DLL form fails to load on this host (ERROR_INVALID_DLL/182), the sender
  runs as that helper EXE (same code). Enumeration via `spout_shim.dll` ran 2026-11-15: 0 senders.
  Not interoperating here: Flux's enumerator doesn't see the DX sender (different Spout2
  registration path); a production fix is to compile SpoutDX into the Flux binary directly.
  Syphon is macOS-only.
- **Ableton Link** (R015, AT063): **supported integration, built from upstream, and run here
  since 2026-11-15** (D027 keyed the gate on `abl_link.lib`): the assertions executed for the
  first time in this repo's history, 2 of the first 13 invocations red. D030 established the
  mechanism (`evidence/link-flake-2026-11-15.txt`: the SDK's IO-thread handler re-writes client
  state without a timestamp guard) and bounded the read-back — 7 assertions now, 52 clean runs.
  Cross-process discovery and tempo are a 2026-11-09 artifact (`link-a.txt`, `link-b.txt`) printing
  `playing=false` on every line, so cross-process start/stop stays unevidenced. Licensing is
  resolved (D012). Single-master is asserted only by `link.rs:288`, which a plain build never
  compiles, and the AC's Max-transport and seek/loop clauses are asserted by nothing — hence
  AT063 `implemented_unverified`, as the generator now says (D025).
- **Max for Live**: source `.maxpat` devices are now **fully pre-wired** (node.script
  bridge-osc.js + loadbang + all connections) and structural checks pass. Remaining step:
  save each as `.amxd` via Max's GUI *Save As → Max Audio/MIDI Effect* (Max is
  JUCE-rendered; no CLI/UI-Automation path exists, so this is manual). Procedure in
  `apps/max-for-live/README.md`.
- **macOS hosts**: unavailable; macOS-only paths unverified.
- **Code signing / notarization**: no identity; builds are unsigned.
- **Optional vision models (G170–G177)**: **implemented and verified.** A model registry
  reports present/missing/incompatible (non-fatal `SdkMissing`, node stays editable — AT077).
  **Real inference works** (AT075): ONNX Runtime via the `ort` crate (statically linked,
  feature `vision`) runs Depth Anything v2 on `flux depth` in ~330 ms
  (`evidence/vision-inference.txt`, `depth-demo/depth.png`). Model weights are user-installed
  (`FLUX_MODEL_DEPTH`). No blocker remains for depth; other G17x models follow the same path.

## Implemented, device/host-dependent for full verification
- **Windows capture**: WASAPI endpoint + PnP camera enumeration is real and verified on
  this host (5 mics, 6 loopback endpoints, 1 camera). Live frame/audio *capture* needs a
  device + OS permission, which this headless run could not exercise.
  **Audio half discharged 2026-11-16, uncommitted working tree:** microphone and
  system-loopback taps were opened, streamed, read cross-process and stopped on this host
  (`evidence/live-capture-2026-11-16.txt`, AT056 now `implemented_unverified`). What still
  needs a device is *frame* capture — AT054 webcam and AT055 desktop have no code, and
  AT056's own output-device-change and permission-recovery clauses were never exercised.
  The ASIO-held-by-Live case is unmeasured because the owner was not present.
- **Ableton Link**: `flux link` runs a real session; two processes discover and sync, in a
  `link` feature build. Single-master is asserted too, but only by `link.rs:288`; seek/loop are not.
- **GUI**: Tauri 2 + web UI builds, links, and launches (dev + release); library/preview/
  inspector/timeline verified via screenshot and the GUI-backend e2e test.
- **SDF/particle producers**: correct and deterministic CPU implementations; not realtime
  at 4K (analytic WGSL generators are).
