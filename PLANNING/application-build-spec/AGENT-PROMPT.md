# Paste this prompt into the coding agent

Build the complete Flux application described by `spec/START-HERE.md` and its linked specification files. The specification directory is the authoritative product contract. You are the implementation engineer, reviewer, and verification engineer for this run. Deliver working code and evidence, not another plan.

The user has authorized routine local implementation in this repository with the command runner's existing permissions. Proceed without repeatedly asking about defaults already specified. Use those permissions as configured; do not change global permission settings. Never infer credentials or private SDK access from authorization mode.

## Start procedure

1. Resolve the current repository root. Read applicable `AGENTS.md` files. Inspect the existing worktree, package manifests, code, toolchains, available media tools, OS, GPU capability, and existing tests. Preserve unrelated user changes.
2. Read START-HERE, ARCHITECTURE, and CONTRACTS fully. Read TASKS and VERIFICATION fully. Read the active catalog sections when needed. Do not fetch every GitHub repository or load all vendor code into context.
3. If the application already exists, map it to the requirements before editing. Extend coherent implementations. Record discrepancies and reconcile them with this specification. Do not blindly regenerate an existing app.
4. Create `implementation/STATE.json`, `implementation/DECISIONS.md`, `implementation/BLOCKERS.md`, `implementation/requirements.json`, and `implementation/evidence/`. STATE must record specification version, current task, completed tasks, failed gates, file ownership, selected dependency versions, active platform, commands to resume, and next steps.
5. Run T00. Commit exact toolchain/dependency selections to lockfiles. Never guess an API from a different major version. Use the selected version's local examples or official documentation when needed.
6. Execute every required task in dependency order. When blocked on an external dependency, continue independent tasks. A conditional adapter still needs real code, contracts, error handling, and integration tests where feasible.

## Work loop

For each task:

1. Read its preconditions and acceptance gate. Inspect only the relevant files.
2. Implement the smallest coherent end-to-end change that satisfies the task. Reuse existing utilities, schemas, parameter controls, GPU pipelines, and test fixtures.
3. Run formatting and the smallest meaningful tests. Read actual output. Repair failures before marking the task verified.
4. Run the broader gate required by TASKS. Do not disable checks, lower thresholds to match a failure, or replace implementation with mocks to make the gate green.
5. Update requirements, evidence, and STATE after each completed task and before a context reset. Store concise summaries, not an entire terminal transcript.
6. Continue to the next independent task. Keep the application runnable after every gate.

Use `verified`, `implemented_unverified`, `failed`, or `blocked_external` truthfully. Never mark a UI button, enum variant, trait definition, identity shader, TODO, fake waveform, canned device list, or synthetic output as the completed real feature. Synthetic sources are permitted as explicitly labeled test fixtures.

## Cost and context controls

- Use one agent unless the runner already has an approved parallel policy and a bounded independent task benefits from it. This prompt does not require subagents or paid external API calls.
- Use local search first. Read targeted file regions. Cache dependency documentation notes in DECISIONS. Do not repeat unchanged research.
- Generate TypeScript types from Rust domain types and generate parameter UI from manifests. Do not manually maintain two schemas.
- Implement shared SDF, noise, convolution, path animation, particle, and temporal primitives once. Build presets and variants from them.
- Use one package manager and one Rust workspace. Retain caches and lockfiles. Avoid repeated clean builds.
- Run changed-package tests during implementation. Run full builds at the specified milestones and after final changes. A full-suite success is invalidated by later affected changes.
- Limit exploratory repair to three attempts for the same external/toolchain failure. Capture evidence, choose the documented fallback if valid, otherwise record a blocker and continue. A core architectural failure must be resolved, not hidden by a disabled feature.
- Keep STATE under roughly 12 KB and DECISIONS as short records. Before context compression, write current changes, exact failure, successful commands, and the next concrete step. Resume from those records; do not start from T00 again.
- Do not print generated base64, media bytes, whole lockfiles, or successful repetitive build output. Store detailed logs under evidence and show concise results.
- Never implement a second engine to bypass a bug in the first. Diagnose ownership, time, color, or resource-state errors at the shared layer.

## Quality rules

- No floating-point timeline source of truth. No wall-clock dependency in deterministic evaluation.
- No blocking network, allocation, logging, file access, or GPU work in the audio callback.
- No unbounded frame queues. No browser simulation diverging from the native output.
- No unchecked shader parameter lengths, invalid references, unsafe FFI lifetime shortcuts, or hidden panics in normal failure paths.
- Native shader errors, media failures, disconnections, and disk errors must leave an actionable UI state and usable unrelated functions.
- No downloaded scripts executed blindly. Use pinned packages, reviewed build recipes, and repository-local changes.
- Never execute arbitrary commands supplied by OSC, imported projects, SVG scripts, media metadata, or presets.
- Add focused regression tests for real failure modes. Prefer analytic/metamorphic tests and real fixtures over tests that merely duplicate implementation.
- Keep text, controls, drag handles, keyboard shortcuts, focus, and error recovery usable at supported display scales.
- A reversible local task does not need another confirmation. OS permission dialogs and signing credentials are actual external prerequisites; do not fake them.

## Final audit and delivery

Run the final reconciliation in VERIFICATION. Correct missing requirements and broken flows. Inspect every advertised feature against its implementation and evidence. Run the final relevant tests after the last fix. Build and launch on available hosts. Export and reimport sample compositions. Generate `implementation/FINAL-REPORT.md` with exact commands, actual results, platform matrix, installer paths, catalog coverage, performance measurements, and explicit blockers.

Do not stop after scaffolding, the first preview, a green unit-test suite, or a prose claim that the app is finished. Continue until required work is verified or a real external blocker prevents it. If context or runner limits interrupt execution, preserve resumable state and report incomplete work accurately. A context checkpoint is not completion.

Final response: state what works, where to launch it, where deliverables and evidence are, and what remains unverified or blocked. Do not invent a macOS build, hardware measurement, signed installer, Max device, paid license, or successful integration.
