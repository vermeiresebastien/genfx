# Blockers

## External prerequisites (cannot be supplied by this run)
- **Native GUI editor API mismatch** — the spec's editor UI (React Flow / Tauri) is not
  used. A native egui editor was attempted but egui 0.37 pins `wgpu 30`, while the
  verified renderer is pinned to `wgpu 27`; mixing both aborts on a windows-crate
  conflict inside wgpu-hal D3D12. The verified product path is the headless CLI
  (`flux render` / `demo-render` / `demo-media` / `contact-sheet`) which exercises the
  full engine, renderer, effects, generators, media and export. A GUI requires either
  upgrading the renderer to wgpu 30 (a rework) or adopting the Tauri 2 + React shell.
- **NDI SDK/runtime**, **Spout2 SDK (Windows)**, **Syphon (macOS)**: not installed;
  real send/receive verification blocked.
- **Ableton Link** license not resolved; audio transport uses internal/MIDI only.
- **Max for Live host**: not installed; `.amxd` packaging blocked.
- **macOS hosts**: unavailable; macOS-only paths unverified.
- **Code signing / notarization**: no identity.
- **Optional vision models** (G170–G177): not present; conditional nodes stay editable.

## Implemented but unverified
- SDF/mesh/particle/physics/fluid/vector/text generator rows: manifests complete, WGSL
  pipelines compile and present; their CPU/3D/raster producers are specified but not
  fully wired into export (they render as presentation passthrough of an uploaded
  texture, which is transparent without a producer). The 2D/math analytic generators
  (123 rows) render real content and are covered by integration tests.
- OS capture adapters (Windows camera/screen/WASAPI loopback): platform cfg scaffolding
  only; not runtime-verified on a device in this run.
