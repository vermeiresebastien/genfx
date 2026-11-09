# Dependency decisions and verified public references

Public documentation reviewed for this package on2026-11-09. These are candidate upstream components and API references, not a claim that their code was audited or tested inside Flux. The agent must pin selected versions in T00, read the matching API documentation and inspect the actual dependency graph. Do not clone every repository.

## 1. Adopt into the core

| Component | Upstream | Use and boundary |
|---|---|---|
| Desktop shell | [Tauri](https://github.com/tauri-apps/tauri) | UI/native commands/windows/bundling; core has MIT/Apache licensing options; retain notices. |
| Native window API | [Tauri WindowBuilder](https://docs.rs/tauri/latest/tauri/window/struct.WindowBuilder.html) | Current documented native builder is gated by `unstable`; isolate/pin/test in T00. Docs warn about Windows deadlock from synchronous command creation; use documented async/setup path. |
| Node editor | [xyflow](https://github.com/xyflow/xyflow) | `@xyflow/react` MIT UI package; Rust still owns execution/validation. Avoid depending on paid Pro examples. |
| GPU | [wgpu](https://github.com/gfx-rs/wgpu) | Rust cross-platform graphics abstraction, MIT/Apache options; WGSL core and real device limits. |
| Media | [FFmpeg](https://github.com/FFmpeg/FFmpeg) | decode/convert/resample/encode/mux; actual selected build/license controls shipped features. |
| Rust media bindings | [rust-ffmpeg](https://github.com/zmwangx/rust-ffmpeg) | `ffmpeg-next`; match wrapper/sys/native libraries and compiler. Wrapper licensing does not replace FFmpeg terms. |
| Audio devices | [CPAL](https://github.com/RustAudio/cpal) | device I/O, Apache-2.0; native loopback capture separate. |
| FFT | [RustFFT](https://github.com/ejmahler/RustFFT) | FFT primitive, MIT/Apache options; feature algorithms/timestamps remain app code. |
| OSC | [rosc](https://github.com/klingtnet/rosc) | OSC codec, MIT/Apache options; schedule/network router implemented in Flux. |
| MIDI | [midir](https://github.com/Boddlnagg/midir) | MIT cross-platform MIDI I/O; virtual ports differ by OS. |
| Vectors/Lottie | [ThorVG](https://github.com/thorvg/thorvg) | MIT C++ vector engine, SVG/Lottie rendering, native/WebGPU options; small ownership-safe C ABI. Start CPU/upload path if GPU interop unproven. |
| Physics | [Rapier](https://github.com/dimforge/rapier) | Apache-2.0 rigid-body2D/3D; app supplies render bridge/state replay; cloth/soft-body solver separate. |
| Audio retiming | [Signalsmith Stretch](https://github.com/Signalsmith-Audio/signalsmith-stretch) | MIT pitch/time library; selected C ABI wrapper, latency/drain compensation, exact output counts. |
| Text shaping | [HarfRust](https://github.com/harfbuzz/harfrust) | MIT HarfBuzz shaping port, matching read-fonts/skrifa outlines. Do not start with archived RustyBuzz; upstream directs users to HarfRust. |
| Model parsing | [gltf-rs/gltf](https://github.com/gltf-rs/gltf) | glTF2 parser; engine implements actual animation/skin/morph/render path and validates selected crate license/dependencies. |
| Windows video sharing | [Spout2](https://github.com/leadedge/Spout2) | BSD-2-Clause native sharing SDK, actual native interop/copy path tested. |
| macOS video sharing | [Syphon Framework](https://github.com/Syphon/Syphon-Framework) | Native shared-video framework; retain its actual [license](https://github.com/Syphon/Syphon-Framework/blob/main/License.txt). |

The descriptions above come from their upstream repositories. They establish intended purpose and licensing candidates, not implementation compatibility across arbitrary versions. Record transitive dependencies and selected build flags before shipping.

## 2. Required SDK/host boundaries

- [NDI SDK](https://docs.ndi.video/all/developing-with-ndi/sdk): use official SDK/runtime and actual terms. A public wrapper does not grant runtime redistribution rights. Missing SDK blocks real integration verification.
- [Ableton Link license](https://github.com/ableton/link/blob/master/LICENSE.md): GPLv2+ source route or contact Ableton for proprietary incorporation; do not assume GitHub availability grants unrestricted proprietary use. Do not enable this dependency in the default distribution until license choice is resolved.
- [Max for Live device creation](https://docs.cycling74.com/legacy/max8/vignettes/live_creatingdevices): `.amxd` uses Max for Live's own format/workflow. Supply source patches and use actual host to save/package/test. Consult current installed Max9 documentation during implementation.
- [Max udpreceive reference](https://docs.cycling74.com/reference/udpreceive/): OSC/full-packet/output-format behavior and low-priority defer controls vary by supported version; verify actual packets and keep networking off the DSP callback.

## 3. Optional adapters and bounded references

| Upstream | Decision |
|---|---|
| [Three.js](https://github.com/mrdoob/three.js) | MIT; optional scene/sketch runtime or algorithm/loader reference. A Three.js texture cannot be assumed to be a native wgpu texture. Cross boundary via explicit frame adapter/external feed. |
| [React Three Fiber](https://github.com/pmndrs/react-three-fiber) | MIT React/Three renderer; optional authoring runtime, no second authoritative simulation. |
| [three.quarks](https://github.com/Alchemist0823/three.quarks) | Particle/VFX reference/optional Three adapter; do not assume GPU/native compatibility or reuse license without checking selected package. |
| [SVG.js](https://github.com/svgdotjs/svg.js) | MIT SVG manipulation/animation; can help interactive authoring. Native canonical time and VectorScene still required. |
| [Lottie Web](https://github.com/airbnb/lottie-web) | MIT browser Lottie reference/harness; ThorVG native playback is baseline. |
| [resvg](https://github.com/linebender/resvg) | Rust SVG rasterization alternative/static thumbnails. It does not automatically implement SVG animation; select only if needed and audited. |
| [Vello](https://github.com/linebender/vello) | Rust vector-renderer alternative, multiple implementations with different maturity. Do not assume the old compute crate is the preferred production renderer; avoid adding a second vector stack without evidence. |
| [WebGL Fluid Simulation](https://github.com/PavelDoGreat/WebGL-Fluid-Simulation) | MIT reference for2D solver/pass structure; GLSL needs original WGSL port plus tests. |
| [webgpu-fluid-sim](https://github.com/jeroenwjb/webgpu-fluid-sim) | WGSL fluid example; research/reference only until license/build/code audit. |
| [fluid](https://github.com/jeantimex/fluid) | SPH/PIC/FLIP research reference; not proof of portable production performance. |
| [p5.js](https://github.com/processing/p5.js) | Optional creative-code source sandbox; verify selected license/distribution and time API. |
| [MediaPipe](https://github.com/google-ai-edge/mediapipe) | Optional vision tasks; exact platform/runtime/model support measured; no universal Rust-native API assumption. |
| [OpenCV](https://github.com/opencv/opencv) | Apache-2.0 current core, vision/flow/tracking adapter; review included modules/models separately. |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) | MIT inference runtime; weights have independent licenses; record actual execution provider and telemetry settings. |
| [OpenTimelineIO](https://github.com/AcademySoftwareFoundation/OpenTimelineIO) | Apache-2.0 timeline interchange extension. It stores cut/media-reference information, not whole procedural graph runtime/media. |
| [OpenFX](https://github.com/AcademySoftwareFoundation/openfx) | BSD-3 API headers/support; future plugin host is substantial engineering. Native plugins execute code, not data-only effects. |
| [Clypra](https://github.com/AIEraDev/Clypra) | MIT application reference for timeline/media/UI. Do not fork blindly or inherit performance/telemetry claims. Inspect actual modules/tests before any bounded reuse. |
| [MLT](https://github.com/mltframework/mlt) | Editing-framework alternative, LGPL core with module-specific licenses. Not part of chosen architecture; no second timeline/render engine. |
| [GStreamer](https://github.com/GStreamer/gstreamer) | Alternative media-pipeline framework, plugins vary. Not default dependency; use only if a documented capture gap justifies a bounded adapter. |
| [Bevy](https://github.com/bevyengine/bevy) | MIT/Apache game-engine reference. Frequent API changes and full engine architecture make it unnecessary as a second application core. |

## 4. Do not silently incorporate

[Hydra](https://github.com/hydra-synth/hydra) and its related packages require their actual selected licenses to be reviewed; do not assume public shader/routing code is permissive. [projectM](https://github.com/projectM-visualizer/projectm) is an optional LGPL music-visualizer backend/reference, with a different preset/OpenGL model. [LYGIA package metadata](https://github.com/patriciogonzalezvivo/lygia/blob/main/package.json) identifies Prosperity/Patron terms; do not treat it as a standard permissive shader dependency. [ISF-Files](https://github.com/Vidvox/ISF-Files) and [ISF specification](https://github.com/mrRay/ISF_Spec) are compatibility/reference sources; individual shader rights and GLSL semantics need review. No automatic universal GLSL→WGSL converter is required or assumed.

Implement the requested visual concepts with original kernels when no suitable approved source exists. Do not scrape private Weavy graphs, Figma plugin code or arbitrary ShaderToy snippets. The user-supplied effect names and workflow are sufficient functional requirements; exact unpublished internal algorithms have not been verified.

## 5. Reproducibility and distribution

Use package dependencies by default. Native libraries can be pinned source archives/submodules with checksums and repository-local build recipes. Record dependency name, exact version/commit, URL, SPDX/custom license, shipped files, native configure flags, modifications and required notice/source distribution. Inspect actual files rather than assuming the repository badge covers all assets.

FFmpeg's [legal guidance](https://ffmpeg.org/legal.html) and [license file](https://github.com/FFmpeg/FFmpeg/blob/master/LICENSE.md) distinguish LGPL default code, optional GPL components, and other linked-library consequences. Keep `--enable-gpl`/`--enable-nonfree` out of the default build unless explicit reviewed distribution strategy permits them. A sidecar executable is not a universal exemption from license obligations. Codec patent/product distribution questions require applicable advice at release time; do not invent legal clearance.

No unverified exact version numbers are prescribed for fast-moving libraries. T00 resolves a tested set and freezes it in lockfiles/version records, avoiding fabricated compatibility. Do not update dependencies halfway through the build just because a newer release appears.
