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
- **Verification:** two `abl_link` instances (and two separate processes) discover each
  other, agree on tempo/beat/phase, arbitrate a single tempo proposer, and propagate
  start/stop (6 tests + cross-process evidence `evidence/link-a.txt`, `link-b.txt`).

## Spout2
- **URL:** https://github.com/leadedge/Spout2
- **Commit:** `6b68a5b4fdeca6130b851e5f9366467a844860e5` (Version 2.007.018, 2026-10-10)
- **License:** BSD-2-Clause (`LICENSE`, Lynn Jarvis)
- **Reused (planned):** shared-texture sender/receiver + DX11 interop behind feature
  `spout` on Windows. Requires a built Spout2 DLL and a live DX11 device — not available
  in this headless run. Adapter contract + capability status are present; live
  sender/receiver is `blocked_external` until a Windows graphics session exists.

## obs-ndi (reference only)
- **URL:** https://github.com/Palakis/obs-ndi
- **Commit:** `fa41b38f18f07acc3c041e382e091a196fc12db0` (2026-11-02)
- **License:** GPL-2.0
- **Reused:** none (not vendored). Consulted for NDI discovery/lifecycle/timestamp
  patterns. It does **not** replace the official NDI SDK/runtime, which carries its own
  license and redistribution terms. NDI remains `blocked_external`.

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
