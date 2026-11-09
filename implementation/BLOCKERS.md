# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI SDK/runtime** (R006, AT060): official SDK not installed; adapter/capability record
  present, real send/receive `blocked_external`. obs-ndi audited as reference only.
- **Spout2 (Windows)** and **Syphon (macOS)**: SDKs/host not present; `blocked_external`.
  Spout2 is BSD-2 and buildable once a Windows graphics session exists; Syphon needs macOS.
- **Ableton Link** (R015, AT063): **adapter built from upstream and verified locally**
  (two-process peer discovery + tempo/beat/phase/start-stop, see evidence/link-*.txt).
  Remaining blocker is **licensing, not code**: Link is GPLv2-or-later, so redistributing a
  Flux binary that links it is a derivative-work constraint (see DECISIONS D010). Not a
  functional blocker; a release/redistribution decision.
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
