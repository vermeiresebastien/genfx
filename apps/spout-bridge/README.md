# Spout2 bridge (C++ shim)

Spout2's `SpoutLibrary` exposes a **C++ pure-virtual** interface (`GetSpout()` → vtable).
Calling that vtable directly from Rust is unsafe, so this directory builds a tiny C++
`extern "C"` shim (`spout_shim.cpp`) that wraps the handful of calls Flux needs, and links
it into `spout_shim.dll`. Rust (`crates/integrations/src/spout.rs`, feature `spout`)
loads the DLL at runtime and calls the flat ABI.

## Build
Requires Visual Studio (MSVC) + the Spout2 SDK (`_upstream/spout2/SPOUTSDK/SpoutLibrary`).

```bat
build_shim.bat   :: compile spout_shim.cpp -> spout_shim.obj
link_shim.bat    :: link spout_shim.dll (SpoutLibrary.lib) + copy SpoutLibrary.dll here
```

Artifacts (`*.obj`, `*.dll`, `*.lib`, `*.exp`) are generated and git-ignored.

## Use
```powershell
$env:FLUX_SPOUT_SHIM = "$PWD\apps\spout-bridge\spout_shim.dll"
cargo build -p flux-app --features spout
target\debug\flux.exe spout      # enumerates the OS-wide Spout sender registry
```

## Scope / limitations (honest)
- Exposed: `spout_shim_create` (GetSpout), `spout_shim_get_sender_count`,
  `spout_shim_get_sender`, `spout_shim_get_sender_info`, `spout_shim_send_image`,
  `spout_shim_release`.
- **Sender enumeration** works (reads the Spout registry). `GetSenderCount` can block
  tens of seconds when no sender exists — not yet tuned for tight polling.
- **`SendImage`** requires a bound OpenGL context; without one the shim returns 0 rather
  than faking a successful send.
