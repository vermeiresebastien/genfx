# Upstream integration audit

Audited 2026-11-09 against completion plan §2A. Repos shallow-cloned to `_upstream/`
(not vendored; `_upstream/` is git-ignored). This file records repository, commit,
license, what is reused, and the resulting Flux decision.

## Ableton Link
- **URL:** https://github.com/Ableton/link
- **Commit:** `5ba907ae027a8c2ec8824463e37296dcefc5e9be` (master, 2026-10-15)
- **License:** GPL-2.0-or-later (`LICENSE.md`, `GNU-GPL-v2.0.md`)
- **Reused:** the `abl_link` C API (`extensions/abl_link/include/abl_link.h`) and the
  built `abl_link.lib`. Build: `cmake -G "Visual Studio 18 2026" -A x64`, then
  `cmake --build . --config Release --target abl_link`. ASIO submodule
  (`chriskohlhoff/asio @ 8806a68`) fetched for the library build.
- **Flux decision:** wrap `abl_link.h` behind `flux-integrations` feature `link` with a
  thin raw-FFI adapter (`crates/integrations/src/link.rs`). **Licensing resolved:** the
  product owner has stated Flux is not sold/distributed commercially, so the GPLv2-or-later
  terms are acceptable (DECISIONS D012). Link is a **supported integration**; the feature
  flag remains only so SDK-less machines build cleanly.
- **Verification (re-measured 2026-11-15, D027 then D030):** the 7 tests all hold both sessions in
  one process, and they ran here for the first time under D027 — 11 of 13 invocations clean, 2 red
  at `link.rs:203` as it then stood. D030 found the cause (the SDK re-writes its client state from
  the IO thread with no timestamp preference, `evidence/link-flake-2026-11-15.txt`) and bounded the
  read-back; 52 clean runs since. The 2026-11-09 pair (`link-a.txt`, `link-b.txt`) proves discovery
  and tempo only: those transcripts print `playing=false` on every line, so they evidence no start/stop.

## Spout2
- **URL:** https://github.com/leadedge/Spout2
- **Commit:** `6b68a5b4fdeca6130b851e5f9366467a844860e5` (Version 2.007.018, 2026-10-10)
- **License:** BSD-2-Clause (`LICENSE`, Lynn Jarvis)
- **Reused (planned):** shared-texture sender/receiver + DX11 interop behind feature
  `spout` on Windows. Requires a built Spout2 DLL and a live GL/DX device; the adapter
  contract and capability status are present. The 2026-11-09 closer, "live sender/receiver
  is `blocked_external` until a Windows graphics session exists", is superseded on the send
  side only: `flux spout-send-dx` pushed 83 frames through a SpoutDX D3D11 sender running as
  the `spout_sender.exe` helper (`evidence/spout-send-dx.txt`).
- **Runtime status (2026-11-09; enumeration re-measured 2026-11-15):** the prebuilt
  `SpoutLibrary.{dll,lib}` (x64) is present, and a C++ `extern "C"` shim
  (`apps/spout-bridge/spout_shim.cpp`, linked against `SpoutLibrary.lib`) exposes a flat ABI.
  Both the library and the built `spout_shim.dll` are gitignored — this host has them, a fresh
  clone does not. `crates/integrations/src/spout.rs` (feature `spout`) runtime-loads the shim,
  and its enumeration call answers: **0 senders** on 2026-11-15
  (`evidence/spout-2026-11-15.txt`). An empty registry is not interop, so
  `capability_report()` answers `verified: false` for spout in every build (D027) rather than
  the old `cfg!(feature = "spout")`. `GetSenderCount` can block when no sender exists;
  `SendImage` needs a bound GL context (the shim returns 0 and never fakes success). Older
  evidence: `evidence/spout-registry.txt`.

## obs-ndi (reference only)
- **URL:** https://github.com/Palakis/obs-ndi
- **Commit:** `fa41b38f18f07acc3c041e382e091a196fc12db0` (2026-11-02)
- **License:** GPL-2.0
- **Reused:** none (not vendored). Consulted for NDI discovery/lifecycle/timestamp
  patterns. It does **not** replace the official NDI SDK/runtime, which carries its own
  license and redistribution terms.
- **Runtime status (re-measured 2026-11-15, D023):** the official NDI runtime
  (`Processing.NDI.Lib.x64.dll` v5.5.1) is installed here (Resolume Arena); Flux runtime-loads
  it (`crates/integrations/src/ndi.rs`, feature `ndi`) and verifies discovery. **Headers:** the
  shim compiles against official SDK headers from `ndi-sdk` 0.2.0 in the **cargo registry**
  (`apps/ndi-bridge/build_shim.bat:7`) — no Cargo.toml declares it, and `ndi_shim.dll` is
  gitignored (`.gitignore:53`), so a fresh clone rebuilds neither the headers nor the DLL. The
  2026-11-09 caveat "needs the full header ABI + an active stream" is discharged for synthetic
  640×360 frames, same-process and cross-process (`evidence/ndi-2026-11-15.txt`); a Flux render
  still never crosses this path, and no third-party consumer has received from Flux.

## Syphon
- **URL:** https://github.com/Syphon/Syphon-Framework
- **Commit:** `f4761677a45b8034a3c2069ec0f3d2553da81fba` (2026-09-21)
- **License:** BSD-2-Clause (bangnoise / vade)
- **Reused (planned):** Syphon server/client behind feature `syphon` on macOS only.
  Cannot be built or verified on the current Windows host; `blocked_external` (macOS).

## Reuse policy compliance
- No whole application/plugin copied; each integration is a small adapter behind a
  feature flag (`link`, `spout`, `syphon`, `ndi`).
- Official SDK/libraries preferred over third-party bindings; obs-ndi used only as
  reference.
- Absent SDK/host/runtime keeps the truthful `unavailable` status.

## Runtime availability found on this machine (2026-11-09)
- **NDI runtime present**: `C:\Program Files\Resolume Arena\Processing.NDI.Lib.x64.dll`
  (v5.5.1, 64-bit), with `Processing.NDI.Lib.Licenses.txt`. It exports 117 `NDIlib_*`
  functions including discovery (`NDIlib_initialize`, `NDIlib_find_create2`,
  `NDIlib_find_get_current_sources`, `NDIlib_find_wait_for_sources`). **No SDK headers are
  shipped**, so `crates/integrations/src/ndi.rs` defines only the small, stable structs it needs
  for discovery (`NDIlib_find_create_t`, `NDIlib_source_t`) — never guessing opaque layouts. That
  is still true of the repository: the headers the shim compiles against sit in the cargo
  registry (`apps/ndi-bridge/build_shim.bat:7`), and so does the gitignored prebuilt
  `ndi_shim.dll`. The 2026-11-09 closer — that send/receive "remains gated on the full header
  set + a live stream" — is discharged for 640×360 frames, same-process and
  cross-process: synthetic in `evidence/ndi-2026-11-15.txt` (D023), a Flux render since
  `evidence/ndi-loopback-2026-11-18.txt` (D049).
- **Spout2 prebuilt binary present**: `SPOUTSDK/SpoutLibrary/Binaries/SpoutLibrary.{dll,lib}`
  (x64, BSD-2), exposing a **C++ pure-virtual** interface (`SendImage`/`ReceiveImage`/
  `GetSenderCount`…). Wrapping it needs a small C++ `extern "C"` shim plus a live GL/DX
  context; the GPU is present (RTX 2080 Ti) but the shim/context work is not done yet.
- **Syphon**: macOS-only Xcode project; not buildable on Windows.
- **Ableton Link**: built from source and verified (see above).

