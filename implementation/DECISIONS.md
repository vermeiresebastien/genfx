# Decisions (short records)

## D001 — GUI architecture: Tauri 2 + React over the Rust command surface
Revised decision (supersedes the initial native-egui attempt). The native egui editor
was abandoned because egui 0.37 pins `wgpu 30` while the verified renderer is pinned to
`wgpu 27`; mixing both aborts on a windows-crate conflict in wgpu-hal D3D12, and
upgrading the renderer to wgpu 30 is a large rework that would invalidate all GPU
evidence. **Decision: adopt the spec's Tauri 2 + React shell**, keeping Rust as the
single authority for project state, validation, rendering, media, export and file I/O.
The durable, testable part is the typed command/event surface (engine `Dispatcher`,
`Command`, `Project`), which is already implemented and covered by tests; the GUI is a
thin consumer of it. GUI implementation is staged after the producer contract and CI
are stable (Workstream C). Rationale: avoids a divergent second rendering engine and
the wgpu version conflict, and matches the product contract.

## D002 — Single wgpu renderer, one authoritative texture
One wgpu device/queue; graph evaluated once per frame; the resulting texture is copied
to (a) editor preview (RGBA readback, downscaled), (b) headless export readback,
(c) external outputs when available. This matches ARCHITECTURE 2/4/8. No browser
simulation runs the graph.

## D003 — Fragment effect template + shared WGSL preamble
Every FX/G row is a `process(uv, src) -> vec4` body. The generated shader supplies the
96-byte FrameUniform, manifest-packed NodeParams, AudioFeatures, and fixed sampler
bindings; final mix applies wet*mask in premultiplied linear. Uniform layout is generated
by `catalog::pack` from the same manifest that drives the UI, docs and tests.

## D004 — FFmpeg via CLI worker (process isolation)
ffmpeg-next links FFmpeg 9.0 shared libs; because the FFmpeg ABI tracks only its own
major version and the dev headers are the winget shared build, media is implemented
through a ffmpeg/ffprobe **process** worker (bounded queues, deterministic frame PTS
selection, cancel). This keeps media failures out of the render thread and avoids
ABI mismatch crashes; it is a documented deviation from `ffmpeg-next` in ARCHITECTURE 1.
Rationale recorded against R004/R024.

## D005 — Native fonts
Text rendering uses a bundled block font defined in code (`catalog::font5x7`), so text
never depends on system fonts and is byte-stable across hosts. A system font registry
hook is kept for future imports.

## D006 — Simulation workers are CPU fixed-step
Particles/physics/fluid/CA/reaction-diffusion run on deterministic CPU workers with
fixed dt from project `simulation_step`, then upload state textures. This is the same
evaluator path for preview and export (no second GPU engine).

## D007 — Generator producer contract
Generator rows that do not compute pixels directly in WGSL are *producers*: they compute
RGBA on the CPU (or a worker) and upload a texture that the presentation pass shows.
One interface (`flux_app::producer::Producer`) defines inputs (manifest id, resolved
native params, width×height px, local time seconds, seed), output (tightly packed RGBA8,
premultiplied, top-left origin, exactly w*h*4 bytes), determinism (identical inputs →
identical bytes, no wall clock), and ownership (caller owns the buffer; bounded to the
target size). The renderer accepts per-node `producer` textures via
`RuntimeGraph::execute_with_producers`, so the same producer implementation drives
preview and export with only resolution/quality differing. Reference producers:
typography (G070–G079, bundled block font), vector (G030–G045, analytic rasterizer),
SDF/mesh (G080–G107, CPU raymarch), particle/physics/field (G110–G148), control
(G150–G162).

## D008 — GUI command surface, persistence, export matrix, recording
The GUI is a thin consumer of `flux_app::commands::CommandHost`, which validates and
executes every request against the engine `Dispatcher` (revision-checked, typed
`GuiCommand`/`Request`/`Response`). The Tauri shell exposes `dispatch`, `render_preview`,
`export_project_cmd`, `capabilities`, `produce_generator_frame`; the webview never runs
its own engine. Persistence is `flux_app::persistence` (atomic save+backup, autosave with
retention/pruning, recovery manifest, relink search, collect-media). Export is a matrix
(PNG/JPEG sequence, WAV, H.264/HEVC MP4, ProRes MOV, GIF) with a frozen-snapshot job +
persisted queue (`job.rs`). Recording (`record.rs`) renders the deterministic exporter at
wall-clock budget and reports overrun honestly.

## D009 — Real where buildable, blocked where external
OSC and MIDI are implemented from crates (`rosc`, `midir`) and verified with real UDP
loopback and message parsing. Windows device enumeration uses the `windows` crate (WASAPI
COM endpoints + PnP camera class) and is verified on-host. NDI, Spout, Syphon, Ableton
Link, and Max for Live require SDKs/hosts/licenses that are absent; their adapters are
capability records marked `blocked_external`, never faked as working.

## D010 — Upstream audit (completion plan §2A)
Shallow clones audited on 2026-11-09 (recorded in `implementation/UPSTREAM.md`):

| Repo | URL | Commit | License | Decision |
|------|-----|--------|---------|----------|
| Ableton link | https://github.com/Ableton/link | `5ba907ae` | **GPLv2-or-later** | Build `abl_link` C-API from source, wrap behind `link` feature. GPLv2 is a **redistribution constraint**: shipping Flux with Link requires Flux to be GPL-compatible or Link to be a separately-loaded plugin. Local build+verify is fine. |
| Spout2 | https://github.com/leadedge/Spout2 | `6b68a5b4` | **BSD-2-Clause** | Permissive; wrap Spout2 shared-texture API behind `spout` feature on Windows. Needs Spout2 build + a DX11 device; not available in this headless run. |
| obs-ndi | https://github.com/Palakis/obs-ndi | `fa41b38f` | GPLv2 | Reference **only** for NDI lifecycle/timestamps; still needs official NDI SDK/runtime (separate license + redistribution terms). Not vendored. |
| Syphon | https://github.com/Syphon/Syphon-Framework | `f4761677` | BSD-2-Clause | macOS-only; wrap behind `syphon` feature; cannot build/verify on Windows. |

Only **Ableton Link** is buildable and verifiable on this host without a proprietary
SDK, so it is the first adapter implemented end-to-end (DECISIONS D011).

## D011 — Ableton Link adapter (thin FFI)
`flux-integrations::link` wraps the `abl_link.h` C API with raw `extern "C"` FFI and no
extra crate. Build path: clone `Ableton/link`, `cmake -G "Visual Studio 18 2026" -A x64`,
`cmake --build --target abl_link`, then link `abl_link.lib`. The Rust side links the lib
via a `build.rs` that reads `FLUX_ABLETON_LINK_LIB` (dir) and is only compiled with
`--features link`. When the feature is off or the runtime is absent, `protocol_status`
reports `unavailable` truthfully. Transport stays separate from the internal/MIDI
transport and can be disabled without affecting offline rendering.



