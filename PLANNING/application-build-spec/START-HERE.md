# Flux: autonomous application build specification

Specification version: 2.0. Date: 2026-11-09. Working product name: Flux.

Build a Windows and macOS application for video editing, audio-reactive visuals, visual programming, procedural generation, and live performance. Deliver a standalone application and Max for Live companions. This package supersedes the earlier implementation plan and the architectural suggestions in the conversation when they conflict.

## 1. Use this package

Put this directory in the implementation repository as `spec/`. Paste `AGENT-PROMPT.md` into the coding agent. The prompt assumes an existing authorized coding session. It does not depend on a specific command runner, undocumented CLI flags, a specific model context size, or a guaranteed model identifier. Select the intended DeepSeek model in the runner itself. No verified model-specific API or pricing claim is made here.

Read in this order:

1. This file: outcome, scope, and completion rules.
2. `ARCHITECTURE.md`: binding implementation contracts.
3. `CONTRACTS.md`: concrete data types, commands, binary layouts, and resource interfaces.
4. `TASKS.md`: ordered work units, outputs, and evidence gates.
5. `VERIFICATION.md`: acceptance scenarios and release conditions.
6. `EFFECTS.md` when implementing effects.
7. `GENERATORS.md` when implementing sources, animation, and simulations.
8. `REPOSITORIES.md` when resolving dependencies or integrations.

All paths within the specification are repository-relative. The source documents are deliverables; do not move or destroy the user's copies. Use direct instructions and terminology consistent with Google developer documentation: define terms, use active voice, describe preconditions, provide exact error states, and distinguish requirements from examples.

## 2. Required outcome

The user can start with a video file, camera, captured screen, external feed, image, SVG, Lottie animation, 3D scene, mathematical pattern, particle system, simulation, or data stream. They can arrange sources on a timeline, connect nodes, animate parameters, modulate them with audio or external controls, see live output, record a performance, and export an edited composition.

The complete product contains:

| ID | Required capability | Contract location |
|---|---|---|
| R001 | Windows x64 and macOS arm64/x64 standalone builds | ARCHITECTURE 1, VERIFICATION 8 |
| R002 | Max for Live audio-analysis and MIDI/control companions | ARCHITECTURE 12 |
| R003 | Native GPU rendering and WebGPU-compatible shader core | ARCHITECTURE 2, 7 |
| R004 | Video, image, audio, image-sequence, SVG, and Lottie import | ARCHITECTURE 6; GENERATORS |
| R005 | Webcam, display/window/region capture, microphone, system audio | ARCHITECTURE 6, 10 |
| R006 | Spout on Windows, Syphon on macOS, NDI send and receive | ARCHITECTURE 11 |
| R007 | Typed node graph with macros, groups, feedback, and reusable presets | ARCHITECTURE 4 |
| R008 | Conventional timeline editing and audio mixing | ARCHITECTURE 5 |
| R009 | Every one of the 28 requested named effects/fills | EFFECTS 2 |
| R010 | Analog, print, optical, material, glitch, and technical effects | EFFECTS 3 |
| R011 | Procedural 2D, SVG animation, image animation, typography | GENERATORS 2–5 |
| R012 | 3D blobs/SDFs, meshes, cameras, lights, imported models | GENERATORS 6–7 |
| R013 | GPU particles, physics, fluids, and evolving simulations | GENERATORS 8–10 |
| R014 | Audio-driven visuals, control nodes, OSC/MIDI input and output | ARCHITECTURE 9–10 |
| R015 | Internal, external, and recorded musical synchronization | ARCHITECTURE 3, 12 |
| R016 | Real-time editor preview, detached output, multiple monitors | ARCHITECTURE 2 |
| R017 | Recording and deterministic offline export | ARCHITECTURE 8 |
| R018 | Project/preset files, autosave, recovery, media relinking | ARCHITECTURE 13 |
| R019 | Performance diagnostics, resource limits, failure recovery | ARCHITECTURE 14 |
| R020 | Tests, sample projects, installers, documentation, evidence | VERIFICATION |
| R021 | External creative-code sources and recorded live data | GENERATORS 11–13 |
| R022 | Depth, motion, masking, and optional vision/model inputs | GENERATORS 12 |
| R023 | Conventional rates, aspect ratios, sizes, and capability detection | ARCHITECTURE 3, 14 |
| R024 | Repository reuse, pinned dependencies, distribution records | REPOSITORIES |

Create a machine-readable `requirements.json` in the implementation repository with one record per R ID, every effect ID, every generator ID, every task ID, and every acceptance-test ID. Store `status`, `implementation_paths`, `test_ids`, `evidence_paths`, and `blocker` on each record. Generate the final feature report from these records. A requirement is complete only when its evidence satisfies VERIFICATION.

## 3. Scope and sequence

This is a full product specification. The implementation stages organize one long run and are not permission to omit later stages. Finish portable features and all locally executable adapters. Keep moving on independent work when an external prerequisite is unavailable. Report blocked work accurately.

Use these classifications consistently:

- **Required:** core application, all effect rows, all generator rows except those marked conditional, timeline, rendering, editing, OSC/MIDI, persistence, recording/export, and the companion source patches.
- **Platform-required:** camera, screen, loopback audio, Spout/Syphon. Implement code and tests for the correct operating system; runtime verification requires that operating system.
- **Conditional:** NDI SDK, standalone Link license, Max/Live device packaging, signing/notarization, optional vision models, external creative-code engines, neural video generation. Implement the integration contract and adapter where the API is available. Mark actual missing dependency or host as a blocker. Do not call a detector or disabled menu a complete integration.
- **Extension:** VST3/CLAP bridge, full OpenFX host, ISF compatibility host, OTIO interchange, browser creative-code plugin marketplace, HDR mastering, and neural synthesis. Supply integration boundaries and documented hooks. Do not silently add them to the required release or replace required work with them.

The target hardware is a current midrange laptop or desktop. Target 1080p60 for ordinary graphs; generator resolution, particle counts, simulations, raymarch steps, and render samples are independent quality controls. 4K, 8K, 120/144/240 fps, and custom sizes are selectable when resource and codec limits allow them. A selectable setting is not a promise of real-time throughput for every scene.

## 4. Binding architecture decisions

1. Rust owns project state, graph evaluation, simulation time, timeline, media, audio, rendering, recording, export, and integration routing.
2. React and React Flow own editing UI. They submit commands and receive snapshots. They do not run an independent simulation.
3. One native `wgpu` renderer produces the authoritative textures for preview, output, external feeds, and recording.
4. The editor preview receives a bounded, reduced-resolution copy of that rendered output. The performance window presents the native texture directly. Avoid an early custom native-child-window integration project.
5. A WebGPU harness runs portable shaders and generator kernels at explicit timestamps. It is a conformance/tooling target. No duplicate TypeScript effect algorithms.
6. Jitter receives finished output and sends controls. It is an integration path. Max does not duplicate the GPU engine.
7. Every source enters through typed ports. Every temporal node has reset, step, checkpoint, restore, and seek semantics.
8. Use one exact rational-time model, one modulation fold, one alpha/color convention, and one export evaluator.
9. Build the first export early. All later effects and generators must pass through the same path.
10. Prefer library dependencies over large application forks. Use Clypra only as a bounded reference after inspection; do not merge a whole editor or telemetry subsystem into the product.

## 5. Completion and authority

Full authorization in the command runner means execute routine local implementation, dependency installation, builds, tests, generated fixtures, and packaging without repeated product questions. It does not supply absent SDKs, paid accounts, OS hosts, signing identities, test hardware, or permission to publish or purchase services.

Do not claim universal perfection, production readiness, macOS verification from Windows, or a working `.amxd` based only on a JSON patch. Produce measurable evidence. An unavailable external prerequisite must result in `blocked_external`, never `passed` or a fabricated output.

Status values:

`pending → implementing → implemented_unverified → verified`.

Also allow `failed`, `blocked_external`, and `extension`. Include the exact missing prerequisite and completed independent work in a blocker. A release can be a verified development build on one platform while the full cross-platform release remains incomplete.

## 6. Resource-efficient execution

Use one agent sequentially by default. Share typed schemas and shader primitives. Generate catalog UI, docs, test cases, and preset manifests from data. Run focused checks for each task, then full suites at integration gates. Do not reread the entire specification or reinstall dependencies at every stage. Keep bounded logs and durable progress state. See AGENT-PROMPT for the exact operating procedure.

## 7. Outputs of this planning task

This package specifies the application; it does not contain an implemented or tested application. The implementation agent must create the code, tests, artifacts, and evidence described here. Public project references are documented in REPOSITORIES. The private Weavy flows and inaccessible Figma pages have not been reverse engineered; implement the named visual behaviors and user workflow from original algorithms, without claiming pixel-identical copies.
