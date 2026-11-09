# Planning review and handoff

Review date:2026-11-09. Specification version2.0. This is a review of a written specification, not a verified application release.

## Review outcome

The package replaces conflicting earlier architecture suggestions with one native authoritative render/simulation path, a bounded editor preview copy, explicit typed adapters, and a timestamp-driven WebGPU conformance harness. It includes24 product requirements,28 individually specified effects/fills,24 additional effect families,156 generator/animator/control/adapter rows,28 implementation tasks, and85 acceptance scenarios. Generator rows include142 required and14 conditional entries. The index initializes everything as pending; these counts are specification coverage, not implemented feature counts.

## Corrections made during review

1. Removed two independently advancing browser/native runtimes. Multiple outputs consume one completed live texture.
2. Replaced assumed native↔browser texture sharing with explicit bounded readback for embedded preview; full-quality output stays native.
3. Made Tauri native-window API/version/feature support an initial build spike. Documented its current `unstable` gate and synchronous Windows creation hazard.
4. Replaced approximate timeline seconds/fps with exact rational time and VFR presentation-timestamp selection.
5. Defined stateful simulation fixed ticks, canonical feedback cadence, checkpoint identity, warm-up, seeking, and output-rate behavior.
6. Defined keyframe replacement/fold order, local time domains, and shape-preserving trim/split behavior.
7. Defined shared linear-premultiplied image convention and96-byte frame ABI, explicit native resource lifetimes and cross-language packing tests.
8. Required source fingerprint validation for export snapshots and recorded takes for live inputs; queueing a project does not freeze external files.
9. Made all28 requested effects explicit, including Gooey merge and Moiré. Added algorithms, controls and distinguishing tests; omitted creator columns.
10. Added native SVG/Lottie paths, path/image/text animation,3D SDF/mesh scenes, particles, fluids, physics, data and external runtime contracts.
11. Made optical-flow image animation work through a non-neural required baseline; optional model availability cannot hide a missing core feature.
12. Distinguished `.maxpat` source from real Max-packaged `.amxd`, source implementation from host verification, and unsigned artifacts from signed distributions.
13. Recorded official Link/NDI/FFmpeg constraints and actual missing-host outcomes. No license approval or SDK access is fabricated.
14. Replaced broad repeated agent instructions with ordered tasks, focused verification, pinned dependency selection, reusable primitives, durable progress state, and final evidence reconciliation.

## Automated checks on the specification

Run `node spec/validate-spec.mjs` after copying the directory to `spec/`. The read-only validator checks required documents, catalog/task/test ID uniqueness and references, expected primary counts, code-fence balance, and referenced specification filenames. `--index` prints the machine-readable index. It does not execute application tests or prove architecture correctness.

The planning run also checked JSON code examples and SPEC-INDEX parsing, task/test coverage references, and relative document links. A package archive is provided for copying the whole specification into the implementation repository. START-HERE and AGENT-PROMPT are the entry points.

## Known implementation boundaries

Actual dependency versions are resolved and frozen in T00 because plausible-looking untested pins would be less reliable than a required compatibility spike. Public upstream purposes/licenses were reviewed; no third-party source code was integrated or audited here. The private reference flows were not reverse engineered. Exact visual behavior is specified through the effect catalog and original implementation requirements.

Full functionality needs real Windows/macOS hosts, actual devices/integration runtimes and Max/Live where relevant. Signing, NDI, standalone Link licensing and optional model/runtime adapters may need external prerequisites. A coding session's authorization mode cannot supply these. The agent has direct instructions to complete independent work, preserve resumable progress and report remaining blockers without calling them finished.

The scope is substantial professional software. One long run can be an execution arrangement, but cannot guarantee perfection, uninterrupted runner availability, a particular spend, or all-platform validation. Quality is assessed through the specified behavior, real artifacts, meaningful tests and honest evidence.
