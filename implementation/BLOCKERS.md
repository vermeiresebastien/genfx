# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI** (R006, AT060): **Fully working — discovery, send, and receive verified.**
  NDI Tools v6.3.2 runtime auto-detected. `apps/ndi-bridge/ndi_shim.cpp` is compiled
  against the official NDI SDK headers (vendored via `ndi-sdk`), so structs are
  correct. Verified: discovery found 5 live sources; cross-process `flux ndi-send` →
  `flux ndi-recv` received a real 640×360 frame (`evidence/ndi-io.txt`,
  `ndi-received-frame.png`). No remaining NDI blocker.
- **Spout2 (Windows)** and **Syphon (macOS)**: Spout2 prebuilt `SpoutLibrary.{dll,lib}` is
  present (BSD-2); a C++ shim (`apps/spout-bridge`) builds `spout_shim.dll`, and
  `flux spout` **verifies sender enumeration** and offscreen GL-context init. Remaining:
  **`SendImage` fails via the offscreen WGL context** — Spout's GL path needs a sender in
  the application's real rendering context; the correct fix is a wgpu↔DX11 shared-texture
  (SpoutDX) path. Reported honestly (returns 0, never fakes). Syphon is macOS-only.
- **Ableton Link** (R015, AT063): **supported integration — built from upstream and
  verified locally** (two-process peer discovery + tempo/beat/phase/start-stop, see
  evidence/link-*.txt). **Licensing resolved**: the product owner has stated Flux is not
  sold/distributed commercially, so the GPLv2-or-later terms are acceptable (DECISIONS
  D012). No blocker remains; only "single-master" arbitration and seek/loop assertions are
  still not covered (AT063 stays `implemented_unverified`).
- **Max for Live**: host not installed; `.maxpat`-equivalent source only, `.amxd` blocked.
- **macOS hosts**: unavailable; macOS-only paths unverified.
- **Code signing / notarization**: no identity; builds are unsigned.
- **Optional vision models (G170–G177)**: not present; conditional nodes stay editable.

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
