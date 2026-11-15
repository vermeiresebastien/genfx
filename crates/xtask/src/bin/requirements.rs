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
            vec!["crates/tests determinism test".into()],
        ),
        "AT079" => (
            "implemented_unverified",
            vec!["implementation/evidence/benchmark.json".into()],
        ),
        "AT082" => (
            "implemented_unverified",
            vec!["cargo xtask package (unsigned dev binary)".into()],
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
        // Windows capture enumeration verified; live capture needs a device.
        "AT054" | "AT055" | "AT056" => (
            "implemented_unverified",
            vec!["implementation/evidence/capture.txt (real WASAPI+PnP enumeration)".into()],
        ),
        "AT057" => (
            "implemented_unverified",
            vec![
                "implementation/evidence/capture.txt (WASAPI + PnP enumeration only: no device was ever taken from)".into(),
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
        // record cannot read `verified` — which is what implementation/BLOCKERS.md:26-35 says.
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
        // Four clauses in this AC; only the first is measured. The rest need a
        // host without the NDI runtime, and a consumer that is not our own CLI.
        "AT060" => Some(
            "loopback clause verified on synthetic frames; release counts, stride/timestamps and \
             missing-runtime behavior unmeasured, and there is no wgpu->NDI path so a Flux render \
             is never what is sent",
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
             (BLOCKERS.md:36-40; no CLI path exists, so the ledger has no status for \
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
        _ => None,
    }
}

#[cfg(test)]
mod evidence_routing {
    use super::at_status;

    fn evidence(id: &str) -> String {
        at_status(id).1.join(" | ").to_lowercase()
    }

    /// These five acceptance records were one `match` arm until D028, which meant
    /// Spout's evidence was cited by a Max AC and vice versa. A grouped arm cannot
    /// fail loudly, so the subject of each row is asserted here instead.
    #[test]
    fn each_record_cites_its_own_subject() {
        let cases: [(&str, &[&str], &[&str]); 5] = [
            (
                "AT057",
                &["capture", "export"],
                &["spout", "syphon", "check-maxpat"],
            ),
            ("AT058", &["spout"], &["max", "syphon", "ndi"]),
            ("AT059", &["syphon"], &["spout", "max", "ndi"]),
            ("AT061", &["bridge", "maxpat"], &["spout", "syphon"]),
            ("AT062", &["amxd"], &["spout", "syphon"]),
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
}

/// No flag: write the ledger. `--check`: say whether the committed ledger is what
/// this tree would produce, and write nothing. The second form exists because
/// neither `ci.yml` nor `verify.ps1` used to run this generator at all, so a
/// hand-edit of `requirements.json` — or a status that drifted away from the
/// `at_status()` arm that is supposed to derive it — survived CI silently.
fn main() {
    let body = generate();
    let path = root().join("implementation/requirements.json");
    if std::env::args().any(|a| a == "--check") {
        std::process::exit(check_freshness(&path, &body));
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
    /// has to say where the file diverges instead of panicking on a parse.
    #[test]
    fn a_broken_ledger_is_located_not_panicked() {
        let body = generate();
        let cut = body.chars().count() / 2;
        let broken = body.chars().take(cut).collect::<String>() + "\"trailing\": ,}";
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
}
