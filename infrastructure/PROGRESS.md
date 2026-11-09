# Flux progress ledger

Spec version 2.0. Host: Windows 11-class (10.0.19045), AMD Ryzen 9 3900X, NVIDIA RTX 2080 Ti,
Rust 1.99.0 stable, Node 24.19.0, pnpm 11.21.0, FFmpeg 9.0 full build (CLI, libx264/libx265/ProRes/AAC),
WebView2 157, MSVC (VS 18 Community), CMake 4.4.3.

## T00 findings
- ffmpeg CLI + DLLs available; -full_build includes dev headers/libs in `include/` and `lib/`, in the winget
  package dir. ffmpeg-sys-next vendor builds against the shared FFmpeg.
- No Vulkan SDK; wgpu on Windows uses DX12 backend (primary) with Vulkan fallback.
- No NDI SDK, no Max/Live host, no macOS host, no signing identity -> external blockers.
- FFmpeg 9.0 ABI: use ffmpeg-sys-next without version feature (allow default newest that supports 9.x); if it fails, use only the ffmpeg CLI for media (process isolation) and record DECISION.

## Current task / next action
See implementation/STATE.json.
