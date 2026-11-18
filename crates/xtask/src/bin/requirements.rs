//! Generate `implementation/requirements.json` from the spec index and the live
//! catalog: every R/FX/G/T/AT id becomes a record whose status is derived from
//! evidence, never fabricated. `--check` compares instead of writing (see main).

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn generate() -> String {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("spec/SPEC-INDEX.json")).unwrap(),
    )
    .unwrap();
    let catalog = flux_catalog::build_catalog();

    // Map requirement IDs -> catalog manifest ids for coverage.
    let mut req_to_manifests: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in &catalog.entries {
        for r in &e.manifest.requirement_ids {
            req_to_manifests
                .entry(r.clone())
                .or_default()
                .push(e.manifest.id.clone());
        }
    }

    let mut records: Vec<Value> = vec![];

    // Product requirements.
    if let Some(product) = spec["requirements"]["product"].as_array() {
        for p in product {
            let id = p["id"].as_str().unwrap_or("").to_string();
            let impls = product_paths(&id, &catalog);
            let status = if !impls.is_empty() {
                "implemented_unverified"
            } else {
                "pending"
            };
            let blocker = blocker_for(&id);
            records.push(record(
                &id,
                p["title"].as_str().unwrap_or(""),
                "required",
                status,
                impls,
                blocker,
            ));
        }
    }

    // Requested effects FX001–FX028.
    if let Some(effects) = spec["requirements"]["effects"].as_array() {
        for fx in effects {
            let id = fx["id"].as_str().unwrap_or("");
            let impls = req_to_manifests.get(id).cloned().unwrap_or_default();
            let status = if impls.is_empty() {
                "pending"
            } else {
                "implemented_unverified"
            };
            records.push(record(
                id,
                fx["title"].as_str().unwrap_or(""),
                "required",
                status,
                impls,
                None,
            ));
        }
    }
    // Effect families.
    if let Some(fams) = spec["requirements"]["effectFamilies"].as_array() {
        for fam in fams {
            let id = fam["id"].as_str().unwrap_or("");
            let impls = req_to_manifests.get(id).cloned().unwrap_or_default();
            let status = if impls.is_empty() {
                "pending"
            } else {
                "implemented_unverified"
            };
            records.push(record(
                id,
                fam["title"].as_str().unwrap_or(""),
                "required",
                status,
                impls,
                None,
            ));
        }
    }
    // Generators.
    if let Some(gens) = spec["requirements"]["generators"].as_array() {
        for g in gens {
            let id = g["id"].as_str().unwrap_or("");
            let class = g["class"].as_str().unwrap_or("required");
            let impls = req_to_manifests.get(id).cloned().unwrap_or_default();
            let status = if !impls.is_empty() {
                "implemented_unverified"
            } else if class == "conditional" {
                "blocked_external"
            } else {
                "pending"
            };
            let blocker = if class == "conditional" && impls.is_empty() {
                Some("external model/runtime or SDK required")
            } else {
                None
            };
            records.push(record(
                id,
                g["title"].as_str().unwrap_or(""),
                class,
                status,
                impls,
                blocker,
            ));
        }
    }
    // Tasks.
    if let Some(tasks) = spec["requirements"]["tasks"].as_array() {
        for t in tasks {
            let id = t["id"].as_str().unwrap_or("");
            let status = task_status(id);
            records.push(record(
                id,
                t["title"].as_str().unwrap_or(""),
                "required",
                status,
                vec![],
                None,
            ));
        }
    }
    // Acceptance tests.
    if let Some(ats) = spec["requirements"]["acceptance"].as_array() {
        for a in ats {
            let id = a["id"].as_str().unwrap_or("");
            let (status, ev) = at_status(id);
            let mut rec = record(
                id,
                a["title"].as_str().unwrap_or(""),
                "required",
                status,
                vec![],
                at_blocker(id),
            );
            rec["evidence_paths"] = json!(ev);
            records.push(rec);
        }
    }

    let out = json!({
        "specVersion": "2.0",
        "generatedBy": "cargo xtask report (requirements)",
        "counts": {
            "product": spec["requirements"]["product"].as_array().map(|a| a.len()).unwrap_or(0),
            "effects": spec["requirements"]["effects"].as_array().map(|a| a.len()).unwrap_or(0),
            "effectFamilies": spec["requirements"]["effectFamilies"].as_array().map(|a| a.len()).unwrap_or(0),
            "generators": spec["requirements"]["generators"].as_array().map(|a| a.len()).unwrap_or(0),
            "tasks": spec["requirements"]["tasks"].as_array().map(|a| a.len()).unwrap_or(0),
            "acceptance": spec["requirements"]["acceptance"].as_array().map(|a| a.len()).unwrap_or(0),
        },
        "records": records,
    });
    // generate() owns the bytes; main() owns the disk — either writes them or
    // compares them against what is committed. Nothing here touches the file.
    serde_json::to_string_pretty(&out).unwrap()
}

fn record(
    id: &str,
    title: &str,
    class: &str,
    status: &str,
    impls: Vec<String>,
    blocker: Option<&str>,
) -> Value {
    json!({
        "id": id,
        "title": title,
        "class": class,
        "status": status,
        "implementation_paths": impls,
        "test_ids": [],
        "evidence_paths": [],
        "blocker": blocker,
    })
}

fn product_paths(id: &str, catalog: &flux_catalog::Catalog) -> Vec<String> {
    let mut v = vec![];
    let has = |s: &str| catalog.entries.iter().any(|e| e.manifest.id == s);
    match id {
        "R001" => {
            v.push("crates/app/src/export.rs".into());
            v.push("apps/desktop".into());
        }
        "R003" => v.push("crates/render".into()),
        "R004" => v.push("crates/media".into()),
        "R007" => v.push("crates/engine/src/graph.rs".into()),
        "R008" => v.push("crates/engine/src/timeline.rs".into()),
        "R009" | "R010" => v.push("crates/catalog".into()),
        "R011" | "R012" | "R013" => {
            v.push("crates/catalog".into());
            v.push("crates/app/src/producer.rs".into());
        }
        "R014" => {
            v.push("crates/audio".into());
            v.push("crates/integrations".into());
        }
        "R015" => v.push("crates/integrations".into()),
        "R016" => {
            v.push("crates/render/src/preview.rs".into());
            v.push("apps/desktop".into());
        }
        "R017" => {
            v.push("crates/app/src/export.rs".into());
            v.push("crates/app/src/record.rs".into());
            v.push("crates/app/src/job.rs".into());
        }
        "R018" => {
            v.push("crates/domain/src/document.rs".into());
            v.push("crates/app/src/persistence.rs".into());
        }
        "R019" => {
            v.push("crates/app/src/diagnostics.rs".into());
            v.push("crates/platform".into());
        }
        "R020" => {
            v.push("implementation".into());
            v.push("docs".into());
        }
        "R021" => {
            v.push("crates/integrations/src/live.rs".into());
            v.push("apps/max-for-live".into());
        }
        "R022" => {
            v.push("crates/app/src/producer.rs".into());
            v.push("apps/shader-harness".into());
        }
        "R023" => v.push("crates/domain/src/time.rs".into()),
        "R024" => v.push("Cargo.lock".into()),
        _ => {}
    }
    if has("gen.solid") && matches!(id, "R003" | "R009" | "R011") {
        v.push("crates/catalog/src/effects_a.rs".into());
    }
    v.sort();
    v.dedup();
    v
}

fn blocker_for(id: &str) -> Option<&'static str> {
    match id {
        "R001" => Some("macOS hosts unavailable; Windows x64 built (GUI+CLI)"),
        "R002" => Some("Max for Live host not installed; source .maxpat only"),
        "R005" => Some("capture adapters implemented; device enumeration verified, live capture needs a device"),
        "R006" => Some("NDI SDK/Spout SDK/Syphon(macOS) not available"),
        "R015" => Some("Ableton Link built+verified (feature `link`); licensing resolved (not sold)"),
        "R021" => Some("external creative-code runtimes not installed"),
        "R022" => Some("model registry + unavailable/incompatible behavior verified; model files not shipped (optional, user-installed)"),
        _ => None,
    }
}

fn task_status(id: &str) -> &'static str {
    match id {
        "T00" | "T01" => "verified",
        "T22" | "T16" | "T21" | "T20" => "implemented_unverified",
        "T02" | "T03" | "T04" | "T05" | "T06" => "implemented_unverified",
        "T07" | "T08" | "T09" | "T10" | "T11" | "T12" | "T13" | "T14" | "T15" => {
            "implemented_unverified"
        }
        "T17" => "implemented_unverified",
        "T18" | "T19" | "T23" | "T24" => "implemented_unverified",
        "T25" | "T26" | "T27" => "implemented_unverified",
        _ => "pending",
    }
}

fn at_status(id: &str) -> (&'static str, Vec<String>) {
    // Map acceptance tests to actual evidence gathered this run.
    match id {
        // Domain: rational time, documents, commands/undo, migration.
        "AT001" | "AT002" | "AT003" => ("verified", vec!["crates/domain unit tests".into()]),
        // GPU frame / oracle / catalog compile.
        "AT004" => (
            "verified",
            vec![
                "implementation/evidence/test-gpu.json".into(),
                "crates/render solid_generator_color_oracle".into(),
            ],
        ),
        "AT023" => (
            "implemented_unverified",
            vec![
                "implementation/evidence/effects_contact_sheet.png".into(),
                "crates/tests effects_render_over_plasma_and_change_output".into(),
            ],
        ),
        "AT024" => (
            "implemented_unverified",
            vec!["implementation/evidence/effects_contact_sheet.png".into()],
        ),
        "AT028" => (
            "implemented_unverified",
            vec!["crates/tests dense_generators_produce_structure".into()],
        ),
        "AT007" | "AT009" => (
            "implemented_unverified",
            vec!["implementation/evidence/media-export.txt".into()],
        ),
        "AT014" => ("verified", vec!["crates/audio numeric tests".into()]),
        "AT017" => (
            "implemented_unverified",
            vec![
                "crates/tests/tests/media_audio_reactive.rs (decoded file audio reaches the analyser from fixture media: one features() per hop, and the measurement does not move when the export render speed does)".into(),
                "crates/app/src/audio_features.rs MediaAudioTimeline + crates/audio/src/lib.rs point_at_sample (features indexed by source sample, never by wall clock)".into(),
            ],
        ),
        "AT079" => (
            "implemented_unverified",
            vec!["implementation/evidence/benchmark.json".into()],
        ),
        "AT082" => (
            "implemented_unverified",
            vec![
                "implementation/evidence/package.json (artifacts[]: installer path, sha256, size and mtime written by `cargo xtask package`)".into(),
                "apps/desktop/src-tauri/tauri.conf.json > bundle (active, nsis, installMode currentUser, webviewInstallMode embedBootstrapper)".into(),
                "crates/platform/src/layout.rs + its tests (sidecars resolve from the executable's directory, never the working directory)".into(),
                "crates/integrations/src/tests.rs sidecars_resolve_through_the_layout_not_the_working_directory".into(),
            ],
        ),
        // Persistence: autosave/recovery/relink/collect (real, tested).
        "AT071" | "AT072" => (
            "verified",
            vec!["crates/app persistence tests (autosave/recovery/relink/collect)".into()],
        ),
        // Export matrix: each available codec produces a probeable artifact.
        "AT067" | "AT068" | "AT069" | "AT070" => (
            "implemented_unverified",
            vec![
                "crates/tests export_matrix tests".into(),
                "implementation/evidence/media-export.txt".into(),
            ],
        ),
        // OSC real UDP loopback + MIDI parsing.
        "AT049" | "AT050" => (
            "verified",
            vec!["crates/integrations osc tests (real UDP loopback, malformed/rate-limit)".into()],
        ),
        "AT051" => (
            "implemented_unverified",
            vec!["crates/integrations midi tests (parsing); device presence host-dependent".into()],
        ),
        // GUI command surface acceptance flow (engine side verified).
        "AT080" => (
            "implemented_unverified",
            vec!["crates/app commands gui_acceptance_flow test; Tauri app builds+launches".into()],
        ),
        // Three rows, three arms. They were one grouped arm until this run,
        // which is the D028 defect class exactly: a grouped arm cannot fail
        // loudly, so AT054 and AT055 were being handed the audio-endpoint
        // evidence that belongs to AT056 — while no webcam and no screen was
        // ever captured by anything. `evidence_routing` asserts the subject of
        // each row from now on.
        "AT054" => (
            "pending",
            vec![
                "no webcam capture exists: crates/platform/src/capture.rs opens WASAPI audio endpoints only, and modes/orientation/timestamps/disconnect have no code to test".into(),
            ],
        ),
        "AT055" => (
            "pending",
            vec![
                "no whole-surface, window, region or cursor grabbing exists: the capture path is audio-only, so crop/DPI and self-capture exclusion have no code to test".into(),
            ],
        ),
        "AT056" => (
            "implemented_unverified",
            vec![
                "crates/platform/src/capture.rs + crates/app/src/live_audio.rs (WASAPI shared mode; microphone and system-loopback taps; an idle loopback tap is reported stalled rather than as zeros, and the output-device-change and permission-recovery clauses are NOT covered by any code here)".into(),
                "crates/app live_audio tests (idle watchdog boundary, explicit non-auto trim, ring overflow counted as events, per-test sidecar isolation so the suite is parallel-safe)".into(),
                "implementation/evidence/live-capture-2026-11-16.txt (this host: SSL 2 microphone and S/PDIF loopback opened, streamed and stopped across processes; 1 kHz tone measured back at centroid=1000 Hz; +12 dB trim moved peak 0.0417 to 0.1658; 384000 frames in 8043 ms)".into(),
            ],
        ),
        "AT057" => (
            "implemented_unverified",
            vec![
                "implementation/evidence/capture.txt (enumeration only: no webcam or desktop surface has ever been taken from; the audio endpoints in that file were first opened for real on 2026-11-16, which is AT056's evidence and not this row's)".into(),
                "crates/app persistence tests (relink survives a moved file, on fixture media)".into(),
                "crates/tests export_matrix tests (fixture input -> probeable file)".into(),
            ],
        ),
        "AT058" => (
            "implemented_unverified",
            vec![
                "apps/spout-bridge (SpoutDX sender + spout_shim.dll over SpoutLibrary) + crates/integrations/src/spout.rs".into(),
                "implementation/evidence/spout-send-dx.txt (83 frames through a real D3D11 sender, 2026-11-09)".into(),
                "implementation/evidence/spout-2026-11-15.txt (bounded enumeration answers 0 senders: no peer on this host today)".into(),
            ],
        ),
        // No Syphon adapter exists. `syphon = []` is a flag with no code behind it,
        // so this row cannot read `implemented_unverified` even though the
        // capability and protocol rows answer for the protocol.
        "AT059" => (
            "blocked_external",
            vec![
                "crates/integrations/Cargo.toml:29 (`syphon = []`: the feature names no code)".into(),
                "crates/integrations/src/live.rs (protocol_status reports syphon unavailable off-macOS)".into(),
            ],
        ),
        "AT060" => (
            "implemented_unverified",
            vec![
                "apps/ndi-bridge (C++ shim over official SDK headers) + crates/integrations ndi_io".into(),
                "implementation/evidence/ndi-2026-11-15.txt (this run: in-process loopback, cross-process send->recv, negative-case exit codes)".into(),
                "implementation/evidence/ndi-io.txt + ndi-received-frame.png (2026-11-09 cross-process send/receive)".into(),
                "flux ndi-loopback (D049): two frames of a gen.plasma + fx.bloom PreviewRenderer render, sent and received back in-process; each received frame is the render it came from, and the residual against the other render is what says so — this row's subject is a Flux frame, not a hand-written gradient".into(),
                "implementation/evidence/ndi-loopback-2026-11-18.txt (the render run, with the per-channel residual named)".into(),
                "crates/app/src/main.rs ndi-fidelity tests (`the_margin_is_what_refuses_a_transport_barely_better_than_nothing` and three more), run by `cargo test -p flux-app --bin flux` and by no verify.ps1 step".into(),
            ],
        ),
        "AT061" => (
            "implemented_unverified",
            vec![
                "crates/integrations bridge tests (6: decode, non-bridge reject, sequence wrap, epoch, staleness, feature mapping)".into(),
                "apps/max-for-live/check-maxpat.cjs (structural: audio bridge, midi bridge, 52-byte OSC encoder)".into(),
                "crates/app/src/control.rs OSC UDP loopback tests (real socket -> document edit, 2026-11-15)".into(),
            ],
        ),
        // The `.maxpat` files are not a `.amxd`, and nothing in the tree packages one.
        "AT062" => (
            "blocked_external",
            vec![
                "apps/max-for-live (two .maxpat + bridge-osc.js; no .amxd exists in the tree)".into(),
            ],
        ),
        // Ableton Link: adapter built from upstream; tempo/beat/phase/peer
        // discovery, single-master tempo arbitration, and start/stop propagation
        // asserted by 6 tests that exist only inside a `link` feature build. That
        // build ran here for the first time on 2026-11-15 (D027): 11 of 13
        // invocations clean, 2 failures of `set_playing_reflected_in_state`.
        // Licensing resolved (product not sold; DECISIONS D012). The AC's
        // Max-integration and seek/loop clauses are asserted by nothing, so the
        // record cannot read `verified` — which is what implementation/BLOCKERS.md:32-41 says.
        "AT063" => (
            "implemented_unverified",
            vec![
                "crates/integrations link tests (7: discovery, tempo convergence, single-master, start/stop, clock, enable, poll guard) - feature `link` build, run here on 2026-11-15".into(),
                "implementation/evidence/link-2026-11-15.txt (11 of 13 invocations clean; 2 red at crates/integrations/src/link.rs:203 as it then was) + link-flake-2026-11-15.txt (mechanism, and 52 clean post-fix runs)".into(),
                "implementation/evidence/link-a.txt + link-b.txt (cross-process peers=1, 2026-11-09 build; playing=false throughout, so start/stop is not shown cross-process)".into(),
            ],
        ),
        "AT052" | "AT053" => (
            "verified",
            vec!["crates/app control tests (map/coalesce/apply)".into()],
        ),
        // AT077: conditional model/source absent -> preserved editable node,
        // actionable dependency error, never fake output. Implemented via the
        // model registry + non-fatal SdkMissing graph diagnostic.
        "AT077" => (
            "verified",
            vec![
                "crates/catalog models tests (missing/available/incompatible/invalid)".into(),
                "crates/engine missing_model_reports_warning_but_still_compiles".into(),
                "flux models CLI (actionable per-node install message)".into(),
            ],
        ),
        // AT075: actual vision inference — verified with a real ONNX model.
        "AT075" => (
            "verified",
            vec![
                "crates/integrations vision (ONNX Runtime; real model inference test)".into(),
                "implementation/evidence/vision-inference.txt + depth-demo/depth.png".into(),
            ],
        ),
        "AT074" => (
            "verified",
            vec!["crates/app diagnostics tests (redaction, no secrets)".into()],
        ),
        "AT035" | "AT042" | "AT043" | "AT044" | "AT045" | "AT046" | "AT047" | "AT048" => (
            "implemented_unverified",
            vec!["crates/tests producer tests (deterministic animated content)".into()],
        ),
        _ => ("implemented_unverified", vec![]),
    }
}

/// Which clause of an acceptance test keeps it from `verified`, or what external
/// prerequisite keeps a `blocked_external` record from running at all. Requirement
/// and generator rows use `blocker_for`; this is the only blocker hook acceptance
/// records have, so a bare `implemented_unverified` with no explanation is the gap
/// it exists to close.
fn at_blocker(id: &str) -> Option<&'static str> {
    match id {
        // Capture is enumeration-only: the AC's subject is a take that has never
        // happened, and relink is proven on fixture files rather than on a capture.
        "AT057" => Some(
            "no live source has ever been taken into an export; enumeration is all capture.txt \
             records, the export tests consume fixture media, and the timestamps the AC aligns \
             are the fixture's",
        ),
        "AT058" => Some(
            "no independent consumer has confirmed a Flux frame: the 2026-11-15 enumeration \
             answers 0 senders, Flux's enumerator does not see its own DX sender, and \
             capability_report() returns verified=false for spout in every build",
        ),
        // Not a missing runtime - missing code. The row exists in the capability
        // report and the protocol status, and nowhere else.
        "AT059" => Some(
            "needs macOS and an adapter that does not exist in this tree; the `syphon` feature \
             is an empty flag, so no OS change alone unblocks it. Smallest resume step: write a \
             `crates/integrations/src/syphon.rs` sender/receiver behind that flag and run \
             `cargo test -p flux-integrations --features syphon syphon::` on macOS",
        ),
        // The wgpu->NDI gap in this row's old blocker is closed: since D049 the bytes
        // handed to the sender come from `PreviewRenderer`, and the command refuses when
        // what returns is not that frame. What is left is the transport's own fidelity and
        // a consumer that is not our own CLI.
        "AT060" => Some(
            "loopback clause now measured on a real Flux render, but not byte-exactly: the \
             round trip costs max R17 G6 B11 and leaves alpha untouched, so the gate is \
             frame-identity-by-discrimination and the loss is reported, not tolerated \
             silently; release counts, stride/timestamps and missing-runtime behavior are \
             still unmeasured, and no NDI consumer that is not Flux has taken a frame",
        ),
        "AT061" => Some(
            "the OSC bytes are Flux's own encoder meeting Flux's own listener; no Max host has \
             ever sent one, and `multiple instances` is asserted in-process by the epoch test \
             rather than by two live patches",
        ),
        "AT062" => Some(
            "no .amxd exists, so pass-through, macro automation and Set-reopen are each \
             unexercised; the missing prerequisite is a manual save, not a missing machine. \
             Smallest resume step: open apps/max-for-live/*.maxpat in Max 8, Save As \
             Max Audio/MIDI Effect, load the result in Live and count packets with `flux osc 9000` \
             (BLOCKERS.md:42-46; no CLI path exists, so the ledger has no status for \
             \"blocked on a human clicking Save\")",
        ),
        // The 7 link tests do assert single-master arbitration and phase, and since
        // D027 they run here - but every one holds both sessions in a single process,
        // and no test or evidence file touches the AC's Max-transport or seek/loop
        // clauses. D030 found the flake was the test, not the transport.
        "AT063" => Some(
            "peer/tempo/phase/start-stop are asserted only in-process (both sessions in one \
             process); Max transport and seek/loop are asserted by nothing, and no real Link \
             host has ever joined. D030 bounded the read-back rather than fixing it: a capture \
             taken in the same instant as a commit can still return the previous value.",
        ),
        // The installer now exists, but every clause of this AC is about a machine that
        // did not build Flux, and that machine has never been booted. Read the row as
        // "the artifact is producible and inventoried", not "the artifact was accepted".
        "AT082" => Some(
            "no clean standard-user profile, scrubbed PATH or offline first launch has been \
             run on a machine that did not build Flux; the default bundle carries no sidecar \
             at all (bundle.resources is empty and the bridge DLLs are not bundled), so the \
             resources-found-without-dev-PATH clause is only exercised once something is \
             actually shipped — cargo xtask package --with-ffmpeg <dir> is the route, and \
             third-party DLLs in an installer are an owner decision, not a build default. \
             embedBootstrapper also downloads the WebView2 runtime at install when the machine \
             lacks it, so a no-network first install needs webviewInstallMode offlineInstaller. \
             Smallest resume step: install into a fresh standard-user profile with PATH \
             stripped to system32, launch offline, then uninstall and confirm the project \
             directory survives.",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod evidence_routing {
    use super::{at_status, generate, Value};

    fn evidence(id: &str) -> String {
        at_status(id).1.join(" | ").to_lowercase()
    }

    /// These five acceptance records were one `match` arm until D028, which meant
    /// Spout's evidence was cited by a Max AC and vice versa. A grouped arm cannot
    /// fail loudly, so the subject of each row is asserted here instead. The four
    /// rows added below are the same defect found again on 2026-11-16: AT054,
    /// AT055 and AT056 shared one arm, so a webcam AC and a desktop-capture AC
    /// were each citing WASAPI endpoint enumeration — evidence for a tap neither
    /// of them has.
    #[test]
    fn each_record_cites_its_own_subject() {
        let cases: [(&str, &[&str], &[&str]); 10] = [
            (
                "AT057",
                &["capture", "export"],
                &["spout", "syphon", "check-maxpat"],
            ),
            ("AT058", &["spout"], &["max", "syphon", "ndi"]),
            ("AT059", &["syphon"], &["spout", "max", "ndi"]),
            ("AT061", &["bridge", "maxpat"], &["spout", "syphon"]),
            ("AT062", &["amxd"], &["spout", "syphon"]),
            // The offline half is what Step 1 wired; the names here are the
            // fixture test and the two seams that make a measurement depend on
            // the source sample rather than on the clock.
            (
                "AT017",
                &[
                    "media_audio_reactive",
                    "point_at_sample",
                    "mediaaudiotimeline",
                ],
                &["spout", "ndi"],
            ),
            ("AT054", &["webcam"], &["microphone", "loopback", "cursor"]),
            ("AT055", &["cursor"], &["microphone", "loopback", "webcam"]),
            (
                "AT056",
                &["microphone", "loopback", "live-capture-2026-11-16"],
                &["webcam", "cursor", "spout", "ndi"],
            ),
            // The installer row. Its evidence used to be a sentence about a command; the
            // names here are the three artifacts the row actually depends on.
            (
                "AT082",
                &["package.json", "nsis", "layout"],
                &["benchmark.json", "media-export", "amxd", "maxpat"],
            ),
        ];
        for (id, must, must_not) in cases {
            let e = evidence(id);
            assert!(!e.is_empty(), "{id} cites nothing at all");
            for m in must {
                assert!(e.contains(m), "{id} lost its own subject {m:?}: {e}");
            }
            for n in must_not {
                assert!(!e.contains(n), "{id} cites {n:?} evidence: {e}");
            }
        }
    }

    /// `syphon = []` is a flag with no adapter behind it, and no `.amxd` exists, so
    /// neither row may read `implemented_unverified`: that says something was built.
    #[test]
    fn absent_code_is_blocked_not_implemented() {
        assert_eq!(at_status("AT059").0, "blocked_external");
        assert_eq!(at_status("AT062").0, "blocked_external");
    }

    /// The mirror of the same rule for the capture rows: enumeration of audio
    /// endpoints is not a webcam and not a desktop grab, so those two rows must
    /// say `pending`, while the row whose taps were actually opened on a device
    /// says `implemented_unverified` — built, measured once, not yet accepted.
    #[test]
    fn enumeration_does_not_become_a_camera_or_a_screen() {
        assert_eq!(at_status("AT054").0, "pending");
        assert_eq!(at_status("AT055").0, "pending");
        assert_eq!(at_status("AT056").0, "implemented_unverified");
        assert_eq!(at_status("AT017").0, "implemented_unverified");
    }

    /// An evidence entry is a witness, not an instruction. AT082 used to cite
    /// `cargo xtask package (unsigned dev binary)` — a command that at the time could not
    /// produce the artifact it named, because `bundle.active` was false and the verb built
    /// the CLI. A row may name a file or a test; a shell command tells a reader nothing
    /// about whether it ran, and the ledger has no field for that.
    #[test]
    fn no_acceptance_row_cites_a_command_as_its_evidence() {
        let ledger: Value = serde_json::from_str(&generate()).expect("the ledger parses");
        let records = ledger["records"].as_array().expect("records array");
        let mut rows = 0usize;
        let mut entries = 0usize;
        for rec in records {
            let id = rec["id"].as_str().unwrap_or("");
            if !id.starts_with("AT") {
                continue;
            }
            rows += 1;
            for ev in rec["evidence_paths"].as_array().expect("evidence array") {
                let s = ev.as_str().unwrap_or("");
                entries += 1;
                assert!(
                    !s.starts_with("cargo ") && !s.starts_with("node ") && !s.starts_with("npm "),
                    "{id} cites a command as evidence: {s}"
                );
            }
        }
        // A scan that silently matched nothing would pass forever, so the population is
        // asserted beside the rule.
        assert!(rows > 30, "the scan saw only {rows} acceptance rows");
        assert!(entries > 30, "the scan saw only {entries} evidence entries");
    }
}

/// No flag: write the ledger. `--check`: say whether the committed ledger is what
/// this tree would produce, whether the repo's own live-audio command lines are
/// serialised, and write nothing. The second form exists because neither `ci.yml`
/// nor `verify.ps1` used to run this generator at all, so a hand-edit of
/// `requirements.json` — or a status that drifted away from the `at_status()` arm
/// that is supposed to derive it — survived CI silently.
///
/// `--check` now answers two questions, because the second one has the same shape
/// as the first: a tracked carrier whose text nobody reads drifts. The exit code
/// is the worst of the two, and both reports print, so a ledger failure cannot
/// hide a command-carrier failure or the other way round. 2 is still reserved for
/// "there is no ledger to compare", since regenerating a file the repository does
/// not have fixes nothing.
fn main() {
    let body = generate();
    let path = root().join("implementation/requirements.json");
    if std::env::args().any(|a| a == "--check") {
        let ledger = check_freshness(&path, &body);
        let (findings, _) = check_test_command_carriers(&root());
        for f in &findings {
            eprintln!("requirements: {f}");
        }
        if !findings.is_empty() {
            eprintln!(
                "requirements: {} finding(s) in the command carriers — fix the line, do not \
                 relax this gate ({SERIALISE} is what all three carriers run, and D045's \
                 twelve unflagged runs bound the hang near one in five rather than closing it)",
                findings.len()
            );
        }
        let carriers = if findings.is_empty() { 0 } else { 1 };
        std::process::exit(ledger.max(carriers));
    }
    std::fs::write(&path, &body).unwrap();
    println!("wrote {}", path.display());
}

/// Git can hand Windows a CRLF working tree while this generator always emits LF.
/// That is an encoding of the same bytes, not a change of content, so both sides
/// are normalised before they are compared — otherwise the gate fails a clean
/// checkout and gets switched off.
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// 0 = the file on disk is what this tree generates, 1 = it is not,
/// 2 = there is no file to compare against.
fn check_freshness(path: &Path, body: &str) -> i32 {
    let on_disk = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("requirements: cannot read {}: {e}", path.display());
            return 2;
        }
    };
    let (committed, generated) = (normalize_newlines(&on_disk), normalize_newlines(body));
    if committed == generated {
        println!("requirements: {} is fresh", path.display());
        return 0;
    }
    eprintln!(
        "requirements: {} is not what this tree generates",
        path.display()
    );
    let diffs = drift_lines(&committed, &generated);
    for d in diffs.iter().take(20) {
        eprintln!("  {d}");
    }
    if diffs.len() > 20 {
        eprintln!("  ... and {} more", diffs.len() - 20);
    }
    eprintln!("  fix: cargo run -q -p xtask --bin requirements   (then commit the file)");
    1
}

/// Named field paths, not a byte offset: the value of this message is knowing
/// which record moved, and a diff of a 4 000-line pretty-printed JSON file tells
/// you nothing you would not rather find by reading the generator.
fn drift_lines(committed: &str, generated: &str) -> Vec<String> {
    let parsed = serde_json::from_str::<Value>(committed)
        .and_then(|a| serde_json::from_str::<Value>(generated).map(|b| (a, b)));
    let (a, b) = match parsed {
        Ok(v) => v,
        Err(_) => {
            return vec![format!(
                "one side is not valid JSON; first differing line is {}",
                first_diff_line(committed, generated)
            )]
        }
    };
    let mut out = vec![];
    for key in union_keys(&a, &b) {
        if key == "records" {
            continue;
        }
        if a.get(&key) != b.get(&key) {
            out.push(format!(
                "{key}: committed {}, generated {}",
                short(a.get(&key)),
                short(b.get(&key))
            ));
        }
    }
    let (ca, cb) = (records_by_id(&a), records_by_id(&b));
    for id in ca.keys().filter(|k| !cb.contains_key(k.as_str())) {
        out.push(format!("{id}: in the committed file, not in this tree"));
    }
    for id in cb.keys().filter(|k| !ca.contains_key(k.as_str())) {
        out.push(format!("{id}: in this tree, not in the committed file"));
    }
    for id in cb.keys().filter(|k| ca.contains_key(k.as_str())) {
        let (x, y) = (ca.get(id).unwrap(), cb.get(id).unwrap());
        if x == y {
            continue;
        }
        for field in union_keys(x, y) {
            if x.get(&field) != y.get(&field) {
                out.push(format!(
                    "{id}.{field}: committed {}, generated {}",
                    short(x.get(&field)),
                    short(y.get(&field))
                ));
            }
        }
    }
    if out.is_empty() {
        out.push(format!(
            "first differing line is {}",
            first_diff_line(committed, generated)
        ));
    }
    out
}

fn records_by_id(v: &Value) -> BTreeMap<String, &Value> {
    v["records"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| (r["id"].as_str().unwrap_or_default().to_string(), r))
                .collect()
        })
        .unwrap_or_default()
}

fn union_keys(a: &Value, b: &Value) -> Vec<String> {
    let keys = |v: &Value| -> Vec<String> {
        v.as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    keys(a)
        .into_iter()
        .chain(keys(b))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn short(v: Option<&Value>) -> String {
    match v {
        None => "absent".to_string(),
        Some(v) => {
            let s = v.to_string();
            let n = s.chars().count();
            if n > 96 {
                format!("'{}...'", s.chars().take(96).collect::<String>())
            } else {
                s
            }
        }
    }
}

fn first_diff_line(a: &str, b: &str) -> usize {
    a.lines()
        .zip(b.lines())
        .position(|(x, y)| x != y)
        .map(|i| i + 1)
        .unwrap_or_else(|| a.lines().count().min(b.lines().count()) + 1)
}

// ---------------------------------------------------------------------------
// Item 0 (I15 lane B2): the live-audio legs must be serialised, in the repo's
// own words and in CI.
// ---------------------------------------------------------------------------

/// The heading whose fenced block is `AGENTS.md`'s command list. Scanning only
/// that block is what keeps prose about `cargo test` out of the census: the file
/// names the command in five sentences that are not commands.
const CANONICAL_HEADING: &str = "## Canonical commands";

/// The legs this gate is about: a `cargo test` that turns on `live-audio`.
const LIVE_AUDIO_FEATURE: &str = "--features live-audio";

/// The flag that makes them finish. See `AGENTS.md` "Canonical commands" for the
/// measurement that says why, and do not read that measurement as "fixed": the
/// deadlock itself is open (tranche C item 9), which is exactly why the flag has
/// to be unable to quietly fall out of a carrier line.
const SERIALISE: &str = "--test-threads=1";

/// Every carrier that names a live-audio `cargo test` command. All three run on
/// this host without a device, a GPU or cmake, which is the property item 0 asked
/// for — and unlike a fresh `xtask` bin, all three are already wired into CI, so
/// this gate has a runner the day it is written: `ci.yml` runs `--check`, and so
/// does `verify.ps1`'s `requirements-freshness` step.
///
/// That is also why it lives in a ledger checker instead of a doc checker. The
/// honest cost is that `requirements -- --check` now answers two questions —
/// "is the ledger fresh" and "is the command list serialised" — and a failure in
/// either exits nonzero with its own report. The alternative homes were all
/// worse: `build.rs` and `check-bundle.mjs` need node and the UI devDependencies,
/// `cargo fmt`/`clippy` cannot see a doc, and a new bin would have needed new CI
/// wiring, which is a change to a lane this brief does not own.
fn test_command_carriers() -> [(&'static str, &'static str); 3] {
    [
        ("AGENTS.md", "the canonical-commands fenced block"),
        (".github/workflows/ci.yml", "any non-comment line"),
        ("infrastructure/verify.ps1", "any non-comment line"),
    ]
}

/// The shell/YAML/PowerShell part of a line: everything before the first `#`.
///
/// In all three carriers `#` opens a comment, and the only `#` that appears in a
/// command line here is the `#` of a comment after the command. A `#` inside a
/// quoted string would be mis-cut — measured, not assumed: `grep -c '#'` over the
/// three carriers' `cargo test` lines finds one, and it is `AGENTS.md`'s own
/// trailing comment.
fn code_part(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

/// The lines of a carrier that ARE commands, as `(1-based line, code part)`.
///
/// `AGENTS.md` is filtered to its canonical-commands fence; the other two are
/// whole files, because a command there is a command wherever it is written.
fn command_lines(carrier: &str, text: &str) -> Vec<(usize, String)> {
    let kept = |l: &str| code_part(l).contains("cargo test");
    if carrier != "AGENTS.md" {
        return text
            .lines()
            .enumerate()
            .filter(|(_, l)| kept(l))
            .map(|(i, l)| (i + 1, code_part(l).trim().to_string()))
            .collect();
    }
    let mut in_heading = false;
    let mut in_fence = false;
    let mut out = vec![];
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("## ") {
            in_heading = rest.trim() == CANONICAL_HEADING.trim_start_matches("## ");
            in_fence = false;
            continue;
        }
        if t.starts_with("```") {
            if in_heading && !in_fence {
                in_fence = true;
            } else if in_fence {
                return out;
            }
            continue;
        }
        if in_fence && kept(line) {
            out.push((i + 1, code_part(line).trim().to_string()));
        }
    }
    out // no closing fence: the caller sees an empty census and reports it
}

/// Findings for one carrier's text. Pure over `(carrier, text)` so a fixture can
/// drive the arms the committed tree must never contain.
fn serialisation_findings(carrier: &str, text: &str) -> Vec<String> {
    let lines = command_lines(carrier, text);
    let live: Vec<(usize, String)> = lines
        .iter()
        .filter(|(_, code)| code.contains(LIVE_AUDIO_FEATURE))
        .cloned()
        .collect();
    let mut findings: Vec<String> = live
        .iter()
        .filter(|(_, code)| !code.contains(SERIALISE))
        .map(|(n, code)| {
            format!(
                "{carrier}:{n} runs a live-audio `cargo test` without {SERIALISE}: {code} \
                 — that form hung once (63 of 65 ok lines, no `test result:` line, still \
                 spinning 2 h 38 m later); D045 re-ran it 12x at efdf253 with no hang, which \
                 bounds the rate near one run in five and does not close the fault"
            )
        })
        .collect();
    if lines.is_empty() {
        findings.push(format!(
            "{carrier} names no `cargo test` command line at all, so this gate is reading \
             nothing. If the command list moved, move this check with it — do not leave it \
             green and blind."
        ));
    } else if live.is_empty() {
        let n = lines.len();
        findings.push(format!(
            "{carrier} names {n} `cargo test` line(s) but none with {LIVE_AUDIO_FEATURE}: the \
             live-audio leg this gate exists for has left the carrier."
        ));
    }
    findings
}

/// Read every carrier and report. `(findings, lines_scanned)`: the second number
/// is what a red log has to be able to say out loud, so a run that read nothing
/// cannot be mistaken for a run that found nothing.
fn check_test_command_carriers(root: &Path) -> (Vec<String>, usize) {
    let mut findings = vec![];
    let mut scanned = 0;
    let mut read = vec![];
    for (path, where_) in test_command_carriers() {
        let full = root.join(path);
        let text = match std::fs::read_to_string(&full) {
            Ok(t) => t,
            Err(e) => {
                findings.push(format!(
                    "{path} could not be read ({e}) — the command carriers are this gate's \
                     input, and a missing one is a finding, not a skip"
                ));
                continue;
            }
        };
        let text = normalize_newlines(&text);
        let n = command_lines(path, &text).len();
        scanned += n;
        findings.extend(serialisation_findings(path, &text));
        read.push(format!("{path} — {where_} [{n}]"));
    }
    if findings.is_empty() {
        println!(
            "requirements: {scanned} `cargo test` command lines read from {} carriers, every \
             live-audio leg carries {SERIALISE}: {}",
            read.len(),
            read.join(", ")
        );
    }
    (findings, scanned)
}

#[cfg(test)]
mod ci_command_parity {
    use super::{
        check_test_command_carriers, command_lines, normalize_newlines, root,
        serialisation_findings,
    };

    /// The same assertion CI makes, inside the suite.
    #[test]
    fn the_committed_carriers_are_serialised() {
        let (findings, scanned) = check_test_command_carriers(&root());
        assert!(findings.is_empty(), "carrier findings: {findings:?}");
        // Three live-audio legs per carrier would be the floor; assert the census
        // moved so a reader can see the gate is reading real lines.
        assert!(scanned >= 9, "only {scanned} cargo test lines were read");
    }

    /// Control for the control: a gate that always returns no findings is
    /// indistinguishable from a passing one, so the exact defect must be visible
    /// on a fixture.
    #[test]
    fn a_live_audio_line_without_the_flag_is_a_finding() {
        let text = "run: cargo test -p flux-app --features live-audio\n";
        let f = serialisation_findings("x.yml", text);
        assert_eq!(f.len(), 1, "the missing flag must be named: {f:?}");
        assert!(f[0].contains("--test-threads=1"), "{}", f[0]);
        assert!(
            f[0].contains("x.yml:1"),
            "the finding names its line: {}",
            f[0]
        );
        // and the same line with the flag is clean — the gate is about the flag,
        // not about the feature.
        let fixed = text.trim_end().to_owned() + " -- --test-threads=1\n";
        assert!(
            serialisation_findings("x.yml", &fixed).is_empty(),
            "the serialised form must not be reported"
        );
    }

    /// Prose and comments are not commands. Without this leg the gate would either
    /// shout at `AGENTS.md`'s English or quietly scan only the first carrier.
    #[test]
    fn prose_and_comments_are_not_commands() {
        let agents = "See also `cargo test -p flux-app --features live-audio`, which is prose.\n\n\
                      ## Canonical commands\n\n```\n# cargo test -p flux-app --features live-audio\n\
                      cargo test -p flux-app --features live-audio -- --test-threads=1\n```\n";
        assert_eq!(
            command_lines("AGENTS.md", &agents).len(),
            1,
            "only the fence's real command may be read"
        );
        assert!(serialisation_findings("AGENTS.md", &agents).is_empty());
        let yml = "# run: cargo test -p flux-app --features live-audio\n";
        assert!(
            command_lines(".github/workflows/ci.yml", yml).is_empty(),
            "a commented-out command is not a command"
        );
    }

    /// A carrier whose block moved would leave the gate green while reading
    /// nothing, which is the same defect as a gate that prints a failure and
    /// exits 0. The census is therefore asserted, not just the flag.
    #[test]
    fn a_carrier_that_stopped_naming_the_leg_is_itself_a_finding() {
        // A carrier that still runs `cargo test`, just not the live-audio leg:
        // syntactically a command, so it is counted — and then named, because the
        // gate would otherwise be reading a command list that no longer has the
        // thing it exists to protect.
        let lost_the_leg = "run: cargo test --workspace -- --test-threads=1\n";
        assert_eq!(command_lines("x.yml", lost_the_leg).len(), 1);
        let f = serialisation_findings("x.yml", lost_the_leg);
        assert!(
            f.iter()
                .any(|s| s.contains("none with --features live-audio")),
            "the lost leg must be named: {f:?}"
        );
        // A carrier with no command lines at all (the only `cargo test` left is a
        // comment) must not pass silently either.
        let no_commands = "# run: cargo test -p flux-app --features live-audio\n";
        assert!(command_lines("x.yml", no_commands).is_empty());
        let f = serialisation_findings("x.yml", no_commands);
        assert!(
            f.iter().any(|s| s.contains("names no `cargo test`")),
            "an empty census must not pass silently: {f:?}"
        );
    }

    /// The real tree, mutated in memory: this is the arm the falsifier log drives
    /// on disk, asserted here so the probe has a named expectation.
    #[test]
    fn the_real_ci_carrier_reddens_when_the_flag_is_dropped() {
        let text = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
        let with = normalize_newlines(&text);
        assert!(serialisation_findings(".github/workflows/ci.yml", &with).is_empty());
        let without = with.replace(
            "cargo test -p flux-app --features live-audio -- --test-threads=1",
            "cargo test -p flux-app --features live-audio",
        );
        assert_ne!(
            without, with,
            "the needle is not in ci.yml — this control proves nothing"
        );
        let f = serialisation_findings(".github/workflows/ci.yml", &without);
        assert_eq!(f.len(), 1, "one line lost its flag, one finding: {f:?}");
    }
}

#[cfg(test)]
mod freshness {
    use super::{check_freshness, drift_lines, generate, normalize_newlines, root};
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "flux-requirements-{name}-{}.json",
            std::process::id()
        ))
    }

    /// The gate is only worth having if the committed ledger really is what the
    /// generator produces; this is the same assertion CI makes, inside the suite.
    #[test]
    fn committed_ledger_is_fresh() {
        let path = root().join("implementation/requirements.json");
        assert_eq!(
            check_freshness(&path, &generate()),
            0,
            "{} is not what `cargo run -q -p xtask --bin requirements` produces",
            path.display()
        );
    }

    /// Control for the control: without a mutation that must be caught, a check
    /// that always returns 0 would look exactly like a passing freshness gate.
    #[test]
    fn one_status_flip_is_drift() {
        let body = generate();
        let marker = "\"status\": \"implemented_unverified\"";
        let flipped = body.replacen(marker, "\"status\": \"verified\"", 1);
        assert_ne!(
            flipped, body,
            "no record was flipped — this control proves nothing"
        );
        assert_eq!(
            normalize_newlines(&flipped).lines().count(),
            normalize_newlines(&body).lines().count(),
            "the flip changed the shape of the file, not a status"
        );
        let path = scratch("drift");
        std::fs::write(&path, &flipped).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// CRLF in the working tree is git's doing, not a hand-edit.
    #[test]
    fn crlf_is_not_drift() {
        let body = generate();
        let path = scratch("crlf");
        std::fs::write(&path, body.replace('\n', "\r\n")).unwrap();
        assert_eq!(check_freshness(&path, &body), 0);
        let _ = std::fs::remove_file(&path);
    }

    /// A hand-edit that breaks the syntax is the worst case, so the report still
    /// has to say where the file diverges instead of panicking on a parse. The
    /// exit code alone never pinned that: a branch returning no rows at all would
    /// have printed nothing, exited 1, and left this test green, so the row is
    /// asserted and its line number is checked against the truncation point.
    #[test]
    fn a_broken_ledger_is_located_not_panicked() {
        let body = generate();
        let cut = body.chars().count() / 2;
        let broken = body.chars().take(cut).collect::<String>() + "\"trailing\": ,}";
        let report = drift_lines(&normalize_newlines(&broken), &normalize_newlines(&body));
        assert_eq!(
            report.len(),
            1,
            "one unparseable side is one row: {report:?}"
        );
        let prefix = "one side is not valid JSON; first differing line is ";
        assert!(
            report[0].starts_with(prefix),
            "not the parse branch: {report:?}"
        );
        let named: usize = report[0][prefix.len()..]
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("{} is not a line number: {e}", report[0]));
        assert_eq!(
            named,
            broken.lines().count(),
            "the row points away from where the file was cut"
        );
        let path = scratch("broken");
        std::fs::write(&path, &broken).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// The other one-sided arm: a record the committed file has and this tree no
    /// longer generates, which is what a deleted `at_status()` row or a dropped
    /// catalog entry leaves behind. D032 reasoned this branch from the two id maps
    /// and said so; this is the measurement it asked for.
    #[test]
    fn a_record_only_in_the_committed_file_is_named() {
        let body = generate();
        let mut generated: serde_json::Value = serde_json::from_str(&body).unwrap();
        let before = generated["records"].as_array().unwrap().len();
        let kept = generated["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["id"].as_str() != Some("AT062"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            kept.len(),
            before - 1,
            "AT062 was not the record dropped — this control proves nothing"
        );
        generated["records"] = serde_json::Value::Array(kept);
        let shortened = serde_json::to_string_pretty(&generated).unwrap();
        assert_eq!(
            drift_lines(&normalize_newlines(&body), &normalize_newlines(&shortened)),
            vec!["AT062: in the committed file, not in this tree".to_string()],
            "a committed-only record should be the only drift, and should be located"
        );
        let path = scratch("committed-only");
        std::fs::write(&path, &body).unwrap();
        assert_eq!(check_freshness(&path, &shortened), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// The mirror of the test above, and the direction a real checkout drifts in:
    /// a record this tree now generates that the committed ledger never had — a
    /// row added to `spec/` and the file not regenerated. Nothing asserted the
    /// second one-sided loop until this, so an edit that turned it into a
    /// no-op would have reported the missing record only as a line number.
    #[test]
    fn a_record_only_this_tree_generated_is_named() {
        let body = normalize_newlines(&generate());
        let mut committed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let before = committed["records"].as_array().unwrap().len();
        let kept = committed["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["id"].as_str() != Some("AT062"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            kept.len(),
            before - 1,
            "AT062 was not the record dropped — this control proves nothing"
        );
        committed["records"] = serde_json::Value::Array(kept);
        let shortened = serde_json::to_string_pretty(&committed).unwrap();
        assert_eq!(
            drift_lines(&normalize_newlines(&shortened), &body),
            vec!["AT062: in this tree, not in the committed file".to_string()],
            "a record this tree generated and the file lacks should be named, not located"
        );
        let path = scratch("tree-only");
        std::fs::write(&path, &shortened).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// The fallback for a ledger that parses, means the same thing and still is not
    /// the bytes the generator writes. The gate calls that drift on purpose — the
    /// file is generated, so byte equality is the contract — which is exactly why
    /// the report must give a location instead of naming a field that did not move.
    #[test]
    fn equal_meaning_in_different_order_reports_a_line() {
        let body = normalize_newlines(&generate());
        let mut lines: Vec<&str> = body.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.trim() == "\"id\": \"AT062\",")
            .expect("no AT062 row in the generated ledger");
        lines.swap(at + 1, at + 2);
        let swapped = lines.join("\n");
        assert_ne!(
            swapped, body,
            "the swap changed nothing — no branch is tested"
        );
        assert_eq!(swapped.lines().count(), body.lines().count());
        let a: serde_json::Value = serde_json::from_str(&swapped).unwrap();
        let b: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            a, b,
            "swapping two scalar fields changed meaning, not order"
        );
        let report = drift_lines(&swapped, &body);
        assert_eq!(report.len(), 1, "expected only the fallback: {report:?}");
        assert_eq!(report[0], format!("first differing line is {}", at + 2));
        let path = scratch("reordered");
        std::fs::write(&path, &swapped).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// The last report branch with no invocation behind it. `counts` is derived
    /// from the spec arrays rather than from the records, so a hand-edit of the
    /// summary block is drift the per-record loop cannot see at all — and the
    /// 96-char `short()` cap is asserted against here on purpose: if that block
    /// ever grows past it, the report stops naming the field it is complaining
    /// about, which is a defect in the report, not in the ledger.
    #[test]
    fn a_hand_edited_counts_block_is_named() {
        let body = normalize_newlines(&generate());
        let mut committed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let acceptance = committed["counts"]["acceptance"]
            .as_u64()
            .expect("counts.acceptance is not a number");
        committed["counts"]["acceptance"] = serde_json::json!(acceptance - 1);
        let edited = serde_json::to_string_pretty(&committed).unwrap();
        assert_ne!(
            normalize_newlines(&edited),
            body,
            "no counts field was changed — this control proves nothing"
        );
        let report = drift_lines(&edited, &body);
        assert_eq!(
            report.len(),
            1,
            "a summary-only edit must not drag record rows with it: {report:?}"
        );
        assert!(
            report[0].starts_with("counts: committed {")
                && report[0].contains(&format!("\"acceptance\":{}", acceptance - 1))
                && report[0].contains(&format!("\"acceptance\":{acceptance}")),
            "the top-level row must name both sides without truncating them: {}",
            report[0]
        );
        let path = scratch("counts");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// Twenty-five mutated records have to produce twenty-five rows rather than
    /// collapsing to the line-number fallback: that is the difference between
    /// "the generator stopped writing these fields, by id" and "these bytes
    /// differ somewhere". The rows come out in sorted-id order, because
    /// `records_by_id` is a `BTreeMap` — `FX001` first, though the ledger lists
    /// it twenty-fifth — so which five the cap drops is not the file's last five.
    /// It is also the only mutation so far large enough to
    /// reach the 20-line cap in `check_freshness`, whose `... and 5 more` line
    /// is display code and therefore observed with `--nocapture`, not asserted.
    #[test]
    fn twenty_five_drifts_are_counted_record_by_record() {
        let body = normalize_newlines(&generate());
        let mut committed: serde_json::Value = serde_json::from_str(&body).unwrap();
        {
            let recs = committed["records"]
                .as_array_mut()
                .expect("records is not an array");
            for rec in recs.iter_mut().take(25) {
                rec["status"] = serde_json::json!("verified_by_probe");
            }
        }
        let edited = serde_json::to_string_pretty(&committed).unwrap();
        assert_ne!(
            normalize_newlines(&edited),
            body,
            "no record status was changed — this control proves nothing"
        );
        let report = drift_lines(&edited, &body);
        assert_eq!(
            report.len(),
            25,
            "one row per mutated record, not the fallback: {report:?}"
        );
        assert!(
            report
                .iter()
                .all(|r| r.contains(".status: committed \"verified_by_probe\"")),
            "every row should be a record.status with both values: {:?}",
            &report[..2]
        );
        assert!(!report.iter().any(|r| r.starts_with("first differing line")));
        let path = scratch("truncation");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// D034 measured the `counts` row of the top-level loop and left the two
    /// scalars above it uninvoked. They are the drift a reader of `ci.yml` most
    /// wants named by key rather than by line: a `specVersion` the generator stops
    /// matching says the ledger was written against another spec, and a
    /// `generatedBy` that moved says it came from another command. Order is
    /// alphabetical here for a different reason than in the record loop —
    /// `union_keys` collects into a `BTreeSet` — so `generatedBy` precedes
    /// `specVersion` even though the file writes them the other way round.
    #[test]
    fn the_two_header_scalars_are_named() {
        let body = normalize_newlines(&generate());
        let original: serde_json::Value = serde_json::from_str(&body).unwrap();
        let mut committed = original.clone();
        committed["specVersion"] = serde_json::Value::String("9.9".to_string());
        committed["generatedBy"] = serde_json::Value::String("probe".to_string());
        let edited = serde_json::to_string_pretty(&committed).unwrap();
        assert_ne!(
            normalize_newlines(&edited),
            body,
            "no header field was changed — this control proves nothing"
        );
        let want = |key: &str, probe: &str| -> String {
            format!("{key}: committed \"{probe}\", generated {}", original[key])
        };
        let report = drift_lines(&edited, &body);
        assert_eq!(report.len(), 2, "two header rows, nothing else: {report:?}");
        for expected in [want("specVersion", "9.9"), want("generatedBy", "probe")] {
            assert!(
                report.contains(&expected),
                "the loop must name the key and both values; wanted {expected}, got {report:?}"
            );
        }
        let path = scratch("header");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// Exit 2 is the only verdict the module has never asserted, because every
    /// other test writes a file before asking about it. No harness reaches it
    /// either: `verify.ps1` and `ci.yml` both point `--check` at a tracked path,
    /// so 2 means the ledger is absent — a deleted file, or a checkout that never
    /// had it. The difference from 1 is not the gate's red/green, it is the
    /// instruction printed under it: 1 sends the reader to regenerate, and
    /// regenerating a file the repository does not have fixes nothing.
    #[test]
    fn a_missing_ledger_exits_two() {
        let path = scratch("missing");
        let _ = std::fs::remove_file(&path);
        assert!(
            !path.exists(),
            "{} exists, so nothing is being tested",
            path.display()
        );
        assert_eq!(check_freshness(&path, &generate()), 2);
    }

    /// D034 changed a top-level value and D032 changed a record field; neither
    /// removed a key, which is the drift a generator actually produces when an arm
    /// stops writing one. `short(None)` prints `absent` and is reached from both
    /// loops, so this drops `generatedBy` at the top and one record's `blocker`
    /// below, and asks for two rows that say which side lost what.
    #[test]
    fn a_key_either_side_lost_is_reported_absent() {
        let body = normalize_newlines(&generate());
        let mut committed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let generator = committed
            .as_object_mut()
            .expect("the ledger is not an object")
            .remove("generatedBy")
            .expect("generatedBy is not a top-level key");
        let records = committed["records"]
            .as_array_mut()
            .expect("records is not an array");
        let lost = records
            .iter()
            .position(|r| r.get("blocker").is_some())
            .expect("no record carries a blocker to drop");
        let id = records[lost]["id"]
            .as_str()
            .expect("a record has no id")
            .to_string();
        let blocker = records[lost]
            .as_object_mut()
            .expect("the located record is not an object")
            .remove("blocker")
            .expect("the located record has no blocker");
        let edited = serde_json::to_string_pretty(&committed).unwrap();
        assert_ne!(
            normalize_newlines(&edited),
            body,
            "no key was dropped — this control proves nothing"
        );
        let report = drift_lines(&edited, &body);
        assert_eq!(report.len(), 2, "one row per dropped key: {report:?}");
        for expected in [
            format!("generatedBy: committed absent, generated {generator}"),
            format!("{id}.blocker: committed absent, generated {blocker}"),
        ] {
            assert!(
                report.contains(&expected),
                "absent must be named on the side that lost it; wanted {expected}, got {report:?}"
            );
        }
        let path = scratch("absent");
        std::fs::write(&path, &edited).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// The one drift with no change of meaning behind it: an editor that insists on
    /// a final newline writes bytes this generator never produces — the committed
    /// ledger ends at its last `}`, and `normalize_newlines` folds carriage returns
    /// only. Both sides parse and every line pairs equal, so the fallback counts one
    /// past the shorter file and the report names a line nobody can open. That is
    /// asserted here rather than smoothed over, because the alternative — ignoring a
    /// trailing newline — would let the gate say "fresh" about bytes the generator
    /// does not write.
    #[test]
    fn an_eof_newline_is_drift_reported_past_the_last_line() {
        let body = normalize_newlines(&generate());
        let committed = format!("{body}\n");
        assert!(
            !body.ends_with('\n'),
            "the generator already ends with a newline, so there is no drift to test"
        );
        assert_eq!(
            drift_lines(&committed, &body),
            vec![format!(
                "first differing line is {}",
                body.lines().count() + 1
            )],
            "one fallback row, naming the line just past the end"
        );
        let path = scratch("eof-newline");
        std::fs::write(&path, &committed).unwrap();
        assert_eq!(check_freshness(&path, &body), 1);
        let _ = std::fs::remove_file(&path);
    }
}
