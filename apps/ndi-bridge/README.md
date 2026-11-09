# NDI bridge (C++ shim)

`ndi_shim.cpp` compiles against the **official NDI SDK headers** so every struct
layout (`NDIlib_recv_create_v3_t`, `NDIlib_video_frame_v2_t`, `NDIlib_send_create_t`,
…) is correct by construction. It exposes a flat `extern "C"` ABI that Rust
(`crates/integrations/src/ndi_io.rs`, feature `ndi`) loads at runtime:

| export | purpose |
|---|---|
| `ndi_shim_initialize` / `ndi_shim_destroy` | NDI init/shutdown |
| `ndi_shim_recv_create(source)` / `ndi_shim_recv_frame(...)` / `ndi_shim_recv_destroy` | receive a source → packed RGBA8 |
| `ndi_shim_send_create(name,w,h)` / `ndi_shim_send_frame(rgba,w,h)` / `ndi_shim_send_destroy` | send RGBA8 → NDI |

## Build
`build_shim.bat` — generates an import lib from the installed NDI runtime DLL
(`dumpbin /exports` + `lib /def`), then compiles/links `ndi_shim.dll` and copies the
runtime DLL next to it. Requires VS (MSVC) and the NDI 6 runtime.

The official headers come from the vendored copy in the `ndi-sdk` crate
(`.../ndi-sdk-0.2.0/vendored/include`) or an installed NDI SDK `Include/`.

## Use
```powershell
$env:FLUX_NDI_SHIM = "$PWD\apps\ndi-bridge\ndi_shim.dll"
cargo build -p flux-app --features ndi
target\debug\flux.exe ndi                       # discover sources
target\debug\flux.exe ndi-send MySource 10      # broadcast 640x360 for 10s
target\debug\flux.exe ndi-recv MySource out     # receive one frame -> out\ndi_frame.png
```

Verified cross-process: `ndi-send` → `ndi-recv` received a real 640×360 frame
(`implementation/evidence/ndi-received-frame.png`).
