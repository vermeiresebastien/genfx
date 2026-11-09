# Flux Completion Plan

## 1. Objective

Complete the Flux implementation to the level defined by the specification, then
publish a reproducible verification package for the supported platforms.

The current headless product path is the baseline: the engine, renderer, catalog,
media pipeline, audio analysis, and offline export already operate through the
CLI. The remaining work falls into two categories:

1. product work required to make currently incomplete features functional;
2. environment-dependent verification that requires external SDKs, hosts,
   hardware, operating systems, or signing credentials.

## 2. Completion definition

The project is complete when all of the following are true:

- `cargo test --workspace` passes on every supported build profile.
- The CLI doctor command reports all required runtime dependencies clearly.
- All catalog entries have either a functioning producer or an explicit,
  specification-approved passthrough behavior.
- The GUI editor can create, edit, validate, preview, save, and render a Flux
  project without bypassing the authoritative engine and renderer.
- Windows capture and audio-input paths are verified on physical or representative
  devices.
- NDI, Spout2, Syphon, Ableton Link, and Max for Live are either implemented and
  verified or formally marked as optional integrations with documented limitations.
- macOS builds and macOS-only paths are verified on supported macOS hosts.
- Release artifacts are reproducible, packaged, and signed where credentials are
  available.
- Evidence is updated in `implementation/evidence/`, and `FINAL-REPORT.md`,
  `STATE.json`, `requirements.json`, and `BLOCKERS.md` agree with the observed
  implementation state.

## 3. Workstream A: finalize the authoritative baseline

### A1. Freeze the supported baseline

- Record the supported Rust toolchain, operating systems, GPU backends, FFmpeg
  requirements, and minimum hardware.
- Keep the verified renderer dependency versions in one workspace-level policy.
- Add a CI job for workspace tests, release compilation, catalog shader checks,
  media smoke tests, and evidence generation.
- Make `flux doctor` distinguish between required dependencies, optional
  integrations, and unavailable test environments.

### A2. Establish deterministic verification

- Run the unit and integration test suite from a clean checkout.
- Run GPU catalog compilation using the documented profile.
- Run representative render, media import/export, audio analysis, and contact
  sheet commands.
- Store command output, tool versions, input hashes, output hashes, and failure
  diagnostics in `implementation/evidence/`.
- Define acceptable image, audio, timing, and encoded-media tolerances so that
  verification is repeatable across GPUs.

### A3. Protect the existing CLI path

- Add regression tests for graph validation, feedback pairs, time mapping,
  modulation routing, frame ABI layout, ping-pong execution, and exact PTS
  handling.
- Add a golden-project test that exercises import, effects, generator content,
  audio modulation, and export in one run.
- Keep the CLI usable independently of the GUI and external live-performance
  integrations.

## 4. Workstream B: wire the incomplete generator families

The manifests and WGSL pipelines exist for the SDF, mesh, particle, physics,
fluid, vector, and text families, but the current export path can present an
uploaded texture instead of producing the requested content. Complete these in
the following order.

### B1. Common producer contract

- Define one producer interface for CPU, GPU, and hybrid generators.
- Specify resource ownership, resolution, color format, time inputs, random seed,
  frame lifetime, and error behavior.
- Map producer outputs into the existing graph and renderer resource model.
- Ensure preview and export use the same producer implementation, with only
  quality and resolution settings changing.

### B2. Text and vector generators

- Implement glyph layout, font loading, shaping, line wrapping, alignment, and
  raster/vector output according to the specification.
- Add deterministic font fallback and a clear missing-font diagnostic.
- Verify Unicode, multiline text, transforms, opacity, and animated parameters.

### B3. SDF and mesh generators

- Implement geometry or signed-distance evaluation and GPU buffer management.
- Add camera, transform, material, depth, and antialiasing behavior.
- Verify transparent backgrounds, alpha compositing, clipping, and export at
  resolutions different from preview.

### B4. Particle, physics, and fluid generators

- Implement stable simulation state across frames and deterministic reset/seek.
- Define fixed-step and variable-step behavior for offline rendering.
- Add bounded resource limits and diagnostics for excessive particle counts or
  simulation cost.
- Verify that seeking, looping, dropped preview frames, and final export produce
  predictable results.

### B5. Generator acceptance tests

For each family, add at least one fixture that proves:

- the node produces visible non-transparent content without an uploaded texture;
- parameters change the rendered result;
- animation changes across frames;
- seeking reproduces the same frame;
- the result survives a complete media export;
- invalid inputs fail with an actionable diagnostic.

## 5. Workstream C: choose and implement the GUI architecture

The current native GUI attempt is blocked by the `egui`/`wgpu` version mismatch.
Resolve this as an explicit architecture decision before implementation.

### C1. Recommended path: Tauri 2 + React

- Create a thin Tauri shell and React editor surface.
- Keep Rust as the authority for project state, validation, rendering, media,
  export, and file operations.
- Expose typed commands for project lifecycle, node catalog access, graph edits,
  preview rendering, timeline control, and export jobs.
- Use a versioned command schema so the GUI cannot diverge from the engine.
- Implement the editor in layers: project browser, node graph, inspector,
  timeline, preview, transport, and export dialog.
- Add save/load, undo/redo, validation errors, progress reporting, cancellation,
  and crash-safe project recovery.

### C2. Alternative path: upgrade the renderer dependency

Choose this only if the team accepts a renderer rework.

- Upgrade the authoritative renderer from `wgpu 27` to the version required by
  the selected native UI stack.
- Revalidate adapter selection, shader compilation, D3D12 behavior, readback,
  texture formats, and all evidence artifacts.
- Do not merge the UI until the full headless verification suite passes on the
  upgraded renderer.

### C3. GUI acceptance tests

- Create a project from an empty document.
- Add, connect, reorder, duplicate, and delete nodes.
- Edit typed parameters and observe validation errors.
- Preview at bounded resolution and switch to full-resolution export.
- Scrub, seek, loop, and reset simulations.
- Save, reopen, and render the same project from the CLI.
- Confirm that GUI-generated projects are schema-compatible and deterministic.

## 6. Workstream D: verify Windows capture and audio input

- Implement the Windows camera, screen capture, and WASAPI loopback adapters
  behind stable platform interfaces.
- Add permission, device enumeration, device selection, format negotiation,
  disconnect, and retry behavior.
- Verify timestamps, color conversion, frame drops, audio sample rate, channel
  layout, and shutdown behavior on representative hardware.
- Add a capture diagnostics command that records device metadata without storing
  user media unless explicitly requested.
- Add automated tests for mocked devices and a documented manual test matrix for
  physical devices.

## 7. Workstream E: external live-performance integrations

Treat each integration as optional and independently testable.

### E1. NDI

- Obtain the NDI SDK/runtime and confirm redistribution terms.
- Implement discovery, sender, receiver, format negotiation, frame timing,
  connection loss, and shutdown.
- Verify local loopback and two-process communication.

### E2. Spout2 and Syphon

- Obtain the Windows Spout2 SDK and a macOS Syphon environment.
- Implement texture-sharing adapters without copying through CPU memory when the
  platform permits.
- Verify sender/receiver interoperability with known reference applications.

### E3. Ableton Link

- Resolve licensing and distribution requirements.
- Implement transport state, tempo, beat phase, start/stop, peer changes, and
  loss/reconnect behavior.
- Verify that Link transport remains separate from the existing internal/MIDI
  transport and can be disabled without affecting offline rendering.

### E4. Max for Live

- Install a supported Max for Live host and define the packaging contract.
- Produce the `.amxd` artifact, validate metadata, and test device loading,
  parameter exchange, transport synchronization, and failure behavior.
- Document host and Max version requirements.

## 8. Workstream F: macOS support

- Provision supported macOS hosts and define the minimum macOS version.
- Build and test the renderer against Metal.
- Verify Syphon, camera/screen capture, audio input, file paths, permissions,
  and application lifecycle.
- Run the complete CLI and GUI acceptance suite on macOS.
- Record platform-specific limitations instead of treating unavailable hosts as
  successful verification.

## 9. Workstream G: optional vision models

- Decide whether G170–G177 are release-blocking or optional capabilities.
- If release-blocking, obtain the model packages, document licenses, define
  installation and cache behavior, and add model-version diagnostics.
- Implement unavailable-model behavior that preserves project editability and
  provides an actionable message.
- Add tests for model present, model missing, invalid model, and incompatible
  model-version cases.

## 10. Workstream H: release engineering

- Define versioning, changelog, artifact naming, and compatibility policy.
- Produce Windows release packages and, once available, macOS packages.
- Include the CLI, GUI, required runtime assets, catalog metadata, examples, and
  documentation.
- Add crash logs and user-facing diagnostics with privacy-conscious defaults.
- Obtain code-signing identities and notarization credentials where required.
- Verify installation, upgrade, uninstall, and clean-machine startup.
- Publish checksums and a release verification report.

## 11. Dependency and execution order

1. Freeze the baseline and CI evidence process.
2. Complete producer contracts and wire the missing generator families.
3. Decide the GUI architecture.
4. Implement the GUI against the stable engine command surface.
5. Verify Windows capture and audio input.
6. Run external integration work in parallel once SDKs and licenses are available.
7. Verify macOS paths on real macOS hosts.
8. Resolve optional vision models according to the release decision.
9. Package, sign, and perform clean-machine release verification.
10. Update all status and evidence files, then publish the final report.

## 12. Exit gates

### Gate 1: engine baseline

- Workspace tests pass.
- Catalog shader verification passes.
- CLI render and media export fixtures pass.
- Evidence is reproducible from a clean checkout.

### Gate 2: content completeness

- Every generator family has a real producer or an approved documented limit.
- Generator fixtures pass preview, seek, and export checks.

### Gate 3: editor completeness

- GUI project editing, validation, preview, save/load, undo/redo, and export pass.
- GUI output matches CLI output for the same project and settings.

### Gate 4: platform and integration verification

- Windows device paths pass on representative hardware.
- macOS paths pass on supported hosts.
- Each optional integration has a pass report or a clear release limitation.

### Gate 5: release readiness

- Packages install and run on clean machines.
- Signing and notarization are complete where applicable.
- Documentation, version metadata, evidence, and status files are consistent.

## 13. Required evidence updates

At the end of each workstream, update:

- `implementation/FINAL-REPORT.md` with observed results and limitations;
- `implementation/STATE.json` with machine-readable status;
- `implementation/requirements.json` with requirement-level evidence;
- `implementation/BLOCKERS.md` by removing resolved blockers and preserving
  external prerequisites that remain unavailable;
- `implementation/evidence/` with logs, test reports, hashes, screenshots, and
  platform metadata.

## 14. First execution batch

The next implementation batch should be:

1. Add CI and a clean-checkout verification script.
2. Define the common generator producer contract.
3. Implement one end-to-end text or vector producer as the reference producer.
4. Add a golden project covering the reference producer through export.
5. Decide between Tauri 2 + React and the renderer upgrade for the GUI.
6. Record the decision in `implementation/DECISIONS.md` before starting GUI work.

This sequence reduces rework: it stabilizes the current product path, proves the
missing-producer integration point, and prevents the GUI from becoming a second,
divergent rendering implementation.
