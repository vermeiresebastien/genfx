# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI** (R006, AT060): **Fully working — discovery, send, and receive verified.**
  NDI Tools v6.3.2 runtime auto-detected. `apps/ndi-bridge/ndi_shim.cpp` is compiled
  against the official NDI SDK headers (vendored via `ndi-sdk`), so structs are
  correct. Verified: discovery found 5 live sources; cross-process `flux ndi-send` →
  `flux ndi-recv` received a real 640×360 frame (`evidence/ndi-io.txt`,
  `ndi-received-frame.png`). No remaining NDI blocker.
- **Spout2 (Windows)** and **Syphon (macOS)**: Spout2 (BSD-2) works **send-side**:
  `flux spout-send-dx` broadcasts real frames via a SpoutDX D3D11 sender — 83 frames sent
  (`evidence/spout-send-dx.txt`). Because the SpoutDX DLL form fails to load on this host
  (ERROR_INVALID_DLL/182), the sender runs as the `spout_sender.exe` helper (same code;
  verified `device OK` + `SendImage=1`). Sender *enumeration* works via `spout_shim.dll`.
  Not interoperating here: Flux's enumerator doesn't see the DX sender (different Spout2
  registration path); a production fix is to compile SpoutDX into the Flux binary directly.
  Syphon is macOS-only.
- **Ableton Link** (R015, AT063): **supported integration — built from upstream and
  verified locally** (two-process peer discovery + tempo/beat/phase/start-stop, see
  evidence/link-*.txt). **Licensing resolved**: the product owner has stated Flux is not
  sold/distributed commercially, so the GPLv2-or-later terms are acceptable (DECISIONS
  D012). No blocker remains; only "single-master" arbitration and seek/loop assertions are
  still not covered (AT063 stays `implemented_unverified`).
- **Max for Live**: host not installed; `.maxpat`-equivalent source only, `.amxd` blocked.
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
- **Ableton Link**: `flux link` runs a real session; two processes discover and sync.
  "Single-master" arbitration and seek/loop over Link are not yet asserted.
- **GUI**: Tauri 2 + web UI builds, links, and launches (dev + release); library/preview/
  inspector/timeline verified via screenshot and the GUI-backend e2e test.
- **SDF/particle producers**: correct and deterministic CPU implementations; not realtime
  at 4K (analytic WGSL generators are).
