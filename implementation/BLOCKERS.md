# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI SDK/runtime** (R006, AT060): **runtime discovered on this host** —
  `Processing.NDI.Lib.x64.dll` v5.5.1 (Resolume Arena install). `flux ndi` (feature `ndi`)
  runtime-loads it and **verifies discovery** (`NDIlib_initialize` +
  `NDIlib_find_*`, 0 sources when nothing broadcasts — evidence `ndi-discovery.txt`).
  Remaining blocker: **live send/receive** needs the full NDI header ABI (no headers
  shipped) plus an active stream; not faked. obs-ndi consulted as reference only.
- **Spout2 (Windows)** and **Syphon (macOS)**: Spout2 prebuilt `SpoutLibrary.{dll,lib}` is
  present (BSD-2); a C++ shim (`apps/spout-bridge`) builds `spout_shim.dll` and
  `flux spout` **verifies sender enumeration**. Remaining: `SendImage` needs a bound GL
  context, and `GetSenderCount` blocks when no sender exists (documented). Syphon is
  macOS-only.
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
