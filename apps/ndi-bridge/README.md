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
target\debug\flux.exe ndi-send MySource 10      # render gen.plasma+fx.bloom, broadcast it 640x360 for 10s
target\debug\flux.exe ndi-send MySource 10 C:\path\to\project.flux.json   # broadcast that document, at its canvas size
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
the earlier round — of a gradient. Since 2026-11-18 (D049) both commands build a `Project`
and render it through `export::PreviewRenderer`, so the bytes that cross are a Flux render;
`implementation/evidence/ndi-loopback-2026-11-18.txt` is that measurement — the frame comes
back within max R17 G6 B11 A0 of the render it was made from, alpha plane unchanged, and
21.9x further from the other render. It is not byte-exact, and the command prints the
residual instead of calling a tolerance a pass.

Not verified: a third-party receiver (Resolume, NDI Video Monitor, TouchDesigner) has still
never consumed a Flux sender, and nothing past same-machine loopback is exercised. Nor is
what goes out the image the desktop is presenting: it is the CLI's own one-shot render of a
document, because `trait LiveSink` has no implementor. `ndi_shim.dll` in this directory
was built 2026-11-09 and was not rebuilt for the runs above.
