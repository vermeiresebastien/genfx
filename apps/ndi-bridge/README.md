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
$env:FLUX_NDI_SHIM = "$PWD\apps\ndi-bridge\ndi_shim.dll"   # or run from the repo root:
# NdiShim::load() also tries "ndi_shim.dll" then "apps/ndi-bridge/ndi_shim.dll".
cargo build -p flux-app --features ndi
target\debug\flux.exe ndi                       # discover sources
target\debug\flux.exe ndi-send MySource 10      # broadcast 640x360 for 10s
target\debug\flux.exe ndi-recv MySource out     # receive one frame -> out\ndi_frame.png
target\debug\flux.exe ndi-loopback              # sender + receiver in one process
```

`ndi-recv` matches its argument case-insensitively as a substring of the **advertised**
name, which NDI forms as `{machine} ({sender})` — so `MySource` matches
`DESKTOP-XXXX (MySource)`, and a receiver created with the bare sender name matches
nothing. That mistake lived in `ndi-loopback` until 2026-11-15 and is why the command
printed a timeout while exiting 0 (D023).

Verified on this host, 2026-11-15, both cross-process (`ndi-send` → `ndi-recv`, real
640×360 frame written to PNG) and same-process (`ndi-loopback`, 921,600 bytes back with
non-zero content): see `implementation/evidence/ndi-2026-11-15.txt`. The 2026-11-09
transcript `evidence/ndi-received-frame.png` / `ndi-io.txt` records the same result from
the earlier round.

Not verified: no Flux render ever crosses this path (the frames above are generated in
`cmd_ndi_send` / `cmd_ndi_loopback`, and there is no wgpu→NDI producer), and no
third-party receiver app has consumed a Flux sender. `ndi_shim.dll` in this directory
was built 2026-11-09 and was not rebuilt for the runs above.
