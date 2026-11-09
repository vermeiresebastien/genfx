# Blockers

## External prerequisites (cannot be supplied by this run)
- **NDI SDK/runtime** (R006, AT060): official SDK not installed; adapter/capability record
  present, real send/receive `blocked_external`.
- **Spout2 (Windows)** and **Syphon (macOS)**: SDKs/host not present; `blocked_external`.
- **Ableton Link**: license not resolved; internal/MIDI/Max transport only.
- **Max for Live**: host not installed; `.maxpat`-equivalent source only, `.amxd` blocked.
- **macOS hosts**: unavailable; macOS-only paths unverified.
- **Code signing / notarization**: no identity; builds are unsigned.
- **Optional vision models (G170–G177)**: not present; conditional nodes stay editable.

## Implemented, device/host-dependent for full verification
- **Windows capture**: WASAPI endpoint + PnP camera enumeration is real and verified on
  this host (5 mics, 6 loopback endpoints, 1 camera). Live frame/audio *capture* needs a
  device + OS permission, which this headless run could not exercise.
- **GUI end-to-end visual verification**: Tauri 2 + web UI builds, links, and launches;
  the Rust command surface passes the GUI acceptance flow (create→catalog→add→edit→save→
  reload). A webview screenshot was not captured headlessly.
- **SDF/particle producers**: correct and deterministic CPU implementations; not realtime
  at 4K (analytic WGSL generators are).
