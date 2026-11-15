//! `cargo xtask` project commands: doctor, fixtures, check, catalog, test-gpu,
//! scenario, benchmark, report, package. Each writes concise JSON evidence and
//! exits nonzero on failure.

use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn evidence_dir() -> PathBuf {
    let d = root().join("implementation/evidence");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_evidence(name: &str, data: &serde_json::Value) {
    let path = evidence_dir().join(format!("{name}.json"));
    let mut envelope = data.clone();
    // F1's pass criterion enforced at the writer, not in prose: a number that does not
    // name its commit and its binary cannot be told from a stale one. The SHA is the
    // commit the *run* started from, so a gate executed before committing its own tree
    // names its parent — which is why `gitDirty` is part of the stamp.
    if let Some(map) = envelope.as_object_mut() {
        map.insert("provenance".to_string(), provenance());
    }
    std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();
    println!("evidence: {}", path.display());
}

/// Runs git in the repo root; `None` means git is absent or the call failed, which is
/// recorded as an explicit null rather than guessed at.
fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .current_dir(root())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn file_mtime_utc(path: &std::path::Path) -> Option<(u64, String)> {
    let secs = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())?;
    Some((secs, flux_domain::utc_from_unix_pub(secs)))
}

/// Content is what answers "is this the build I think it is". A copy preserves mtime and
/// a rebuild changes it for reasons unrelated to the source, so mtime alone is only a
/// date; and because MSVC lands the link time and embedded build paths into the binary,
/// two compiles of identical source will not hash equal — a hash compares *copies*, and
/// only a symbol check proves a particular change is inside an artifact.
fn sha256_of(path: &std::path::Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

fn provenance() -> serde_json::Value {
    let exe = std::env::current_exe().ok();
    let binary = exe.as_ref().and_then(|p| {
        file_mtime_utc(p)
            .map(|(secs, utc)| json!({"path": p.display().to_string(), "mtimeUnix": secs, "mtimeUtc": utc}))
    });
    // Dirt is a property of the source that was measured, not of this tool's own output.
    // The gates run in sequence and each rewrites tracked files under
    // implementation/evidence/, so an unscoped check reports the first writer of a run as
    // clean and every writer after it as dirty at the same commit. The scope travels with
    // the value because the exclusion is not guessable from the boolean.
    let dirty_scope = ":(exclude,top)implementation/evidence";
    json!({
        "gitHash": git(&["rev-parse", "--short", "HEAD"]),
        "gitHashFull": git(&["rev-parse", "HEAD"]),
        "gitDirty": git(&["status", "--porcelain", "--", dirty_scope]).map(|s| !s.is_empty()),
        "gitDirtyScope": "working tree, excluding implementation/evidence/",
        "generatedAtUtc": flux_domain::now_utc_pub(),
        "binary": binary,
        "host": json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH}),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "doctor" => cmd_doctor(),
        "fixtures" => cmd_fixtures(),
        "evidence" => cmd_evidence(&args[1..]),
        "catalog" => cmd_catalog(&args[1..]),
        "check" => cmd_check(&args[1..]),
        "test-gpu" => cmd_test_gpu(&args[1..]),
        "scenario" => cmd_scenario(&args[1..]),
        "benchmark" => cmd_benchmark(&args[1..]),
        "soak" => cmd_soak(&args[1..]),
        "report" => cmd_report(),
        "package" => cmd_package(&args[1..]),
        _ => {
            println!("usage: cargo xtask <doctor|fixtures|evidence|catalog|check|test-gpu|scenario|benchmark|soak|report|package>");
            2
        }
    };
    std::process::exit(code);
}

/// The evidence files this crate writes. Only these are held to the provenance rule.
/// Everything else in the directory is a hand-saved transcript — `diagnostics.json`
/// included, because `flux diagnostics` prints to stdout and nobody in the tree writes
/// that file — and the honest disposition of a transcript is to name its age, not to
/// invent a SHA for it.
fn written_by_xtask(file: &str) -> bool {
    const OWN: &[&str] = &[
        "benchmark.json",
        "catalog.json",
        "check.json",
        "doctor.json",
        "fixtures.json",
        "manifest.json",
        "package.json",
        "test-gpu.json",
    ];
    OWN.contains(&file) || file.starts_with("scenario-") || file.starts_with("soak-")
}

/// The `# provenance (F1): gitHash <sha>` line a hand-saved transcript opens with. Bounded
/// to the head of the file on purpose: a SHA mentioned in the middle of prose is usually a
/// reference to somebody else's commit, not this capture's provenance.
fn header_git_hash(text: &str) -> Option<String> {
    for line in text.lines().take(24) {
        let Some(at) = line.find("gitHash") else {
            continue;
        };
        for token in line[at + "gitHash".len()..].split_whitespace() {
            let sha: String = token
                .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if (7..=40).contains(&sha.len()) {
                return Some(sha);
            }
        }
    }
    None
}

/// Two kinds of "this file names a commit", and the manifest must not blur them. A
/// `writer` stamp was injected by `write_evidence()` at the moment the gate ran, so it
/// cannot be wrong about its own tree. A `header` SHA was typed by whoever captured the
/// transcript and checked by nothing. `head` is the short SHA, so a file naming the full
/// SHA of the same commit still reads as current.
fn attribute(
    writer: Option<String>,
    header: Option<String>,
    own: bool,
    head: Option<&str>,
) -> (Option<String>, &'static str, &'static str) {
    let current = |sha: &str| match head {
        Some(h) => h.eq_ignore_ascii_case(sha) || sha.to_ascii_lowercase().starts_with(h),
        None => false,
    };
    match (writer, header) {
        (Some(sha), _) => {
            let status = if current(&sha) {
                "stamped_current"
            } else {
                "stamped_other_commit"
            };
            (Some(sha), "writer", status)
        }
        (None, Some(sha)) => {
            let status = if current(&sha) {
                "header_current"
            } else {
                "header_other_commit"
            };
            (Some(sha), "header", status)
        }
        (None, None) if own => (None, "none", "no_provenance"),
        (None, None) => (None, "none", "unattributed"),
    }
}

/// F1, the other half of it. Every file under `implementation/evidence/` either names the
/// commit it came from or is reported as unattributed, with its own mtime as the evidence
/// of its age. Naming a commit has two strengths, and the manifest keeps them apart:
/// `stamped_*` is a SHA this crate injected while producing the file, `header_*` is a SHA
/// a human typed into a transcript. Neither is invented here, and neither is rounded up:
/// a hand-capture that names nothing stays `unattributed`.
///
/// Exits nonzero only when a file this crate *does* write is missing its stamp, because
/// that is a defect in this contract. Unattributed hand-captures are reported, not
/// failed on: re-capturing them belongs to F2/F4/G, whose owners are other lanes.
fn cmd_evidence(_args: &[String]) -> i32 {
    let dir = evidence_dir();
    let mut paths = vec![];
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in read.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                paths.push(p);
            }
        }
    }
    paths.sort();

    let head = git(&["rev-parse", "--short", "HEAD"]);
    let mut files = vec![];
    let mut stamped = 0usize;
    let mut header_named = 0usize;
    let mut broken = vec![];
    for path in &paths {
        let name = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let (mtime_unix, mtime_utc) = file_mtime_utc(path).unwrap_or((0, String::new()));
        let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let found = name
            .ends_with(".json")
            .then(|| std::fs::read_to_string(path).ok())
            .flatten()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("gitHash")
                    .or_else(|| v.get("provenance").and_then(|p| p.get("gitHash")))
                    .and_then(|h| h.as_str())
                    .map(|s| s.to_string())
            });
        // A hand transcript's SHA lives in a header line its author wrote. Only text files
        // are scanned, so a PNG's bytes can never look like a provenance claim.
        let header = (name.ends_with(".txt") || name.ends_with(".md"))
            .then(|| std::fs::read_to_string(path).ok())
            .flatten()
            .and_then(|s| header_git_hash(&s));
        // A JSON gate written here must carry provenance; its absence is a bug, not a
        // limitation. A hand file that names nothing is a limitation, and reads as one.
        let own = written_by_xtask(&name);
        let (sha, source, status) = attribute(found, header, own, head.as_deref());
        match source {
            "writer" => stamped += 1,
            "header" => header_named += 1,
            _ => {}
        }
        if status == "no_provenance" {
            broken.push(name.clone());
        }
        files.push(json!({
            "file": name,
            "bytes": bytes,
            "mtimeUnix": mtime_unix,
            "mtimeUtc": mtime_utc,
            "gitHash": sha,
            "provenanceSource": source,
            "producer": if own { "xtask" } else { "hand-captured (no writer in the tree)" },
            "attribution": status,
        }));
    }

    let binaries = [
        "target/release/flux.exe",
        "target/debug/flux.exe",
        "target/release/flux-desktop.exe",
        "target/debug/flux-desktop.exe",
    ]
    .iter()
    .map(|rel| {
        let p = root().join(rel);
        let (unix, utc) = file_mtime_utc(&p).unwrap_or((0, String::new()));
        json!({
            "path": rel,
            "exists": p.exists(),
            "bytes": std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
            "mtimeUnix": unix,
            "mtimeUtc": utc,
            "sha256": sha256_of(&p),
        })
    })
    .collect::<Vec<_>>();

    let total = files.len();
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    write_evidence(
        "manifest",
        &json!({
            "ok": broken.is_empty(),
            "headAtRun": head,
            "rustc": rustc,
            "recheckCommand": "cargo run -q -p xtask -- evidence",
            "counts": {"files": total, "stamped": stamped, "header_named": header_named, "unattributed": total - stamped - header_named, "missing_provenance": broken.len()},
            "missingProvenance": broken,
            "notes": [
                "mtimeUtc is a date, not a proof of provenance: a copy preserves it and a rebuild changes it for reasons unrelated to the source. The sha256 identifies the file on disk; two compiles of identical source will not hash equal on MSVC, so neither alone names a commit.",
                "attribution comes in two strengths. stamped_current / stamped_other_commit: the SHA was injected by write_evidence() while this crate produced the file. header_current / header_other_commit: a hand-saved text transcript carries `# provenance (F1): gitHash <sha>` that its author typed and no writer checked — a claim about the capture, not a record of the run.",
                "attribution=unattributed means nothing inside the file names a commit at all. These are hand-saved transcripts with no writer in the tree, including diagnostics.json, which `flux diagnostics` only prints.",
                "stamped_other_commit is expected whenever a gate runs before its own work is committed; read provenance.gitDirty with it. header_other_commit is the normal state for a transcript, which is captured at a commit and committed later, or not at all.",
                "manifest.json's own row describes the manifest that was on disk when this scan ran — the writer cannot see the file it is about to produce. The provenance block of this payload is the first-party stamp for the current run.",
                "provenance.gitDirty is measured on the working tree with implementation/evidence/ excluded, because a gate rewriting its own tracked evidence is not an uncommitted source change.",
            ],
            "binaries": binaries,
            "files": files,
        }),
    );
    println!(
        "evidence: {} files, {} stamped by their writer, {} naming a commit in a header, {} unattributed, {} missing provenance",
        total,
        stamped,
        header_named,
        total - stamped - header_named,
        broken.len()
    );
    if !broken.is_empty() {
        eprintln!("evidence: files written here without provenance: {broken:?}");
        return 1;
    }
    0
}

fn cmd_doctor() -> i32 {
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let gpu = flux_render::gpu::Gpu::new(flux_render::BackendPref::Auto)
        .ok()
        .map(|g| g.info);
    let media = flux_media::MediaWorker::new()
        .ok()
        .map(|w| w.capabilities());
    let catalog_n = flux_catalog::build_catalog().entries.len();
    let data = json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "rustc": rustc,
        "gpu": gpu.as_ref().map(|g| g.to_json()),
        "ffmpeg": media.as_ref().map(|m| m.ffmpeg_path.clone()),
        "encoders": media.as_ref().map(|m| json!({"h264": m.h264, "hevc": m.hevc, "prores": m.prores, "aac": m.aac, "gif": m.gif, "png": m.png, "wav": m.wav})),
        "catalogEntries": catalog_n,
        "blockers": ["NDI", "Spout", "Syphon(macOS)", "Max for Live host", "macOS hosts", "code signing", "vision models"]
    });
    let ok = gpu.is_some() && media.is_some();
    write_evidence("doctor", &json!({"ok": ok, "data": data}));
    if let Some(g) = &gpu {
        println!("GPU: {} ({}, {})", g.name, g.backend, g.device_type);
    }
    println!("catalog entries: {catalog_n}");
    println!(
        "doctor: {}",
        if ok {
            "PASS"
        } else {
            "FAIL (gpu or ffmpeg missing)"
        }
    );
    if ok {
        0
    } else {
        1
    }
}

fn cmd_fixtures() -> i32 {
    let dir = root().join("fixtures/generated");
    std::fs::create_dir_all(&dir).unwrap();
    let Some(ffmpeg) = flux_media::find_ffmpeg() else {
        println!("ffmpeg missing");
        return 1;
    };
    let mut made = vec![];
    let targets: [(&str, &[&str]); 3] = [
        (
            "test_10s_60fps.mp4",
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=60:duration=10",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
        ),
        (
            "test_2997.mp4",
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=30000/1001:duration=5",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
        ),
        (
            "sine_1k_48k.wav",
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=48000:duration=2",
                "-ac",
                "2",
            ],
        ),
    ];
    for (name, extra) in targets {
        let p = dir.join(name);
        if !p.exists() {
            let mut args = vec!["-hide_banner", "-loglevel", "error", "-y"];
            args.extend_from_slice(extra);
            args.push(p.to_str().unwrap());
            let _ = Command::new(&ffmpeg).args(&args).status();
        }
        if let Ok(meta) = std::fs::metadata(&p) {
            println!("fixture {name}: {} bytes", meta.len());
        }
        made.push(name);
    }
    write_evidence(
        "fixtures",
        &json!({"ok": true, "dir": dir.display().to_string(), "files": made}),
    );
    0
}

fn cmd_catalog(_args: &[String]) -> i32 {
    let c = flux_catalog::build_catalog();
    let mut ids = std::collections::BTreeSet::new();
    let mut dupes = vec![];
    let (mut fx, mut gens) = (0, 0);
    let mut missing_impl = vec![];
    for e in &c.entries {
        if !ids.insert(e.manifest.id.clone()) {
            dupes.push(e.manifest.id.clone());
        }
        if e.manifest.generator_id.is_empty() {
            fx += 1;
        } else {
            gens += 1;
        }
        if e.manifest.implementation.path.is_empty() {
            missing_impl.push(e.manifest.id.clone());
        }
    }
    let ok = dupes.is_empty() && missing_impl.is_empty();
    write_evidence(
        "catalog",
        &json!({"ok": ok, "entries": c.entries.len(), "effectsAndUtilities": fx, "generators": gens, "duplicates": dupes, "missingImplementation": missing_impl}),
    );
    println!(
        "catalog: {} entries ({} fx/util, {} generators), duplicates={}",
        c.entries.len(),
        fx,
        gens,
        dupes.len()
    );
    if ok {
        0
    } else {
        1
    }
}

fn cmd_check(args: &[String]) -> i32 {
    let scope = args
        .iter()
        .position(|a| a == "--scope")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "all".into());
    let packages: Vec<&str> = match scope.as_str() {
        "domain" => vec!["flux-domain"],
        "media" => vec!["flux-media"],
        "render" => vec!["flux-render"],
        "audio" => vec!["flux-audio"],
        "catalog" => vec!["flux-catalog"],
        _ => vec![
            "flux-domain",
            "flux-catalog",
            "flux-engine",
            "flux-media",
            "flux-audio",
            "flux-render",
        ],
    };
    let mut ok = true;
    for pkg in packages {
        let status = Command::new("cargo")
            .args(["test", "-p", pkg])
            .current_dir(root())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        println!("check {pkg}: {}", if status { "ok" } else { "FAIL" });
        ok &= status;
    }
    write_evidence("check", &json!({"ok": ok, "scope": scope}));
    if ok {
        0
    } else {
        1
    }
}

fn cmd_test_gpu(args: &[String]) -> i32 {
    let profile = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "baseline".into());
    let gpu = match flux_render::gpu::Gpu::new(flux_render::BackendPref::Auto) {
        Ok(g) => g,
        Err(e) => {
            write_evidence(
                "test-gpu",
                &json!({"ok": false, "reason": format!("{e:?}")}),
            );
            println!("test-gpu: UNAVAILABLE (no GPU)");
            return 1;
        }
    };
    let catalog = flux_catalog::build_catalog();
    let mut compiler = flux_render::pipeline::EffectCompiler::new(&gpu);
    let mut fails = vec![];
    let mut compiled = 0;
    for e in &catalog.entries {
        if e.manifest.implementation.kind != "fragment" {
            continue;
        }
        match compiler.ensure_pipeline(&gpu, e) {
            Ok(_) => compiled += 1,
            Err(err) => fails.push(format!("{}: {}", e.manifest.id, err.message)),
        }
    }
    let ok = fails.is_empty();
    write_evidence(
        "test-gpu",
        &json!({"ok": ok, "profile": profile, "compiled": compiled, "failures": fails, "gpu": gpu.info.to_json()}),
    );
    println!(
        "test-gpu: compiled {compiled} shaders, {} failures",
        fails.len()
    );
    if ok {
        0
    } else {
        1
    }
}

fn cmd_scenario(args: &[String]) -> i32 {
    let id = args.first().cloned().unwrap_or_else(|| "AT000".into());
    write_evidence(
        &format!("scenario-{id}"),
        &json!({"id": id, "note": "see tests and evidence/"}),
    );
    println!("scenario {id}: recorded");
    0
}

/// One `--profile` value, and the three things a profile is supposed to decide:
/// resolution, the frame budget it is judged against, and the graph that is timed.
/// Before this, `--profile` was parsed, echoed into the JSON and never read again.
struct BenchProfile {
    name: &'static str,
    width: u32,
    height: u32,
    budget_ms: f64,
    spec: &'static str,
    /// `(node type, image-input port)` in chain order. The input port is `source`
    /// everywhere except the two-input compositor, whose first required input is `a`
    /// (`crates/render/src/graph_exec.rs:365`).
    chain: &'static [(&'static str, &'static str)],
    /// Generator feeding the chain, and how many generators to also time singly.
    generator: &'static str,
    sweep: usize,
}

const BENCH_PROFILES: &[BenchProfile] = &[
    BenchProfile {
        name: "ordinary",
        width: 1920,
        height: 1080,
        budget_ms: 16.67,
        spec: "spec/VERIFICATION.md:138",
        chain: &[
            ("fx.color_adjustment", "source"),
            ("fx.warp", "source"),
            ("fx.halftone", "source"),
            ("fx.vhs", "source"),
            ("fx.bloom", "source"),
            ("fx.composite", "a"),
        ],
        generator: "gen.plasma",
        sweep: 30,
    },
    BenchProfile {
        name: "4k",
        width: 3840,
        height: 2160,
        budget_ms: 33.33,
        spec: "spec/VERIFICATION.md:143",
        chain: &[
            ("fx.color_adjustment", "source"),
            ("fx.warp", "source"),
            ("fx.halftone", "source"),
            ("fx.vhs", "source"),
            ("fx.bloom", "source"),
            ("fx.composite", "a"),
        ],
        generator: "gen.plasma",
        sweep: 10,
    },
];

fn opt_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Nearest-rank percentile over an ascending sample set.
fn nearest_rank(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    let rank = (p * n as f64).ceil().max(1.0) as usize;
    sorted[(rank - 1).min(n - 1)]
}

/// A summary whose field names only claim what the sample count can support: `p95`
/// appears at 20+ samples and `p99` at 100+, because a nearest-rank estimate below
/// that is the max wearing a percentile's name.
fn summarise(times: &[f64]) -> serde_json::Value {
    let n = times.len();
    if n == 0 {
        return json!({"samples": 0});
    }
    let mean = times.iter().sum::<f64>() / n as f64;
    let mut out = json!({
        "samples": n,
        "median_ms": nearest_rank(times, 0.5),
        "mean_ms": mean,
        "max_ms": times[n - 1],
    });
    if n >= 20 {
        out["p95_ms"] = json!(nearest_rank(times, 0.95));
    }
    if n >= 100 {
        out["p99_ms"] = json!(nearest_rank(times, 0.99));
    }
    out
}

fn bench_source_texture(gpu: &flux_render::gpu::Gpu, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench-source"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Timed run of one graph: `render_ms` is execute plus a queue wait, `copy_ms` is the
/// preview readback alone, so the two are no longer added together into one number.
struct BenchRun {
    render: Vec<f64>,
    copy: Vec<f64>,
    frame: Vec<f64>,
    errors: usize,
    passes: usize,
}

fn time_graph(
    profile: &BenchProfile,
    gpu: &flux_render::gpu::Gpu,
    compiler: &mut flux_render::pipeline::EffectCompiler,
    catalog: &flux_catalog::Catalog,
    graph: &flux_domain::Graph,
    warmup: usize,
    samples: usize,
) -> Result<BenchRun, String> {
    let mut rt = flux_render::graph_exec::RuntimeGraph::activate(
        gpu,
        compiler,
        catalog,
        graph,
        profile.width,
        profile.height,
        1,
    )
    .map_err(|e| format!("activate: {e:?}"))?;
    let passes = rt.compiled.passes.len();
    let project = flux_domain::Project::new("bench", 1);
    let ctx =
        flux_engine::frame::FrameContext::for_project(&project, flux_domain::RationalTime::ZERO, 0);
    let mut resolved = std::collections::HashMap::new();
    for pass in &rt.compiled.passes {
        if let Some(e) = catalog.get(&pass.type_id) {
            resolved.insert(
                pass.node_id.clone(),
                flux_render::pipeline::NodeParamsBuffer::defaults(&e.manifest),
            );
        }
    }
    let src = bench_source_texture(gpu, profile.width, profile.height);
    let sv = src.create_view(&Default::default());
    let compiler = &*compiler;
    let mut run = BenchRun {
        render: vec![],
        copy: vec![],
        frame: vec![],
        errors: 0,
        passes,
    };
    for frame in 0..(warmup + samples) {
        let t0 = std::time::Instant::now();
        if rt
            .execute(gpu, compiler, catalog, &ctx, &sv, &resolved, 1.0)
            .is_err()
        {
            run.errors += 1;
        }
        // execute() only submits. Without draining the queue the "render" number is
        // CPU submission and the GPU is still working, so the wait belongs in here.
        let _ = gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let render_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let t1 = std::time::Instant::now();
        let _ = flux_render::preview::read_preview(
            gpu,
            &rt.target_texture(),
            profile.width,
            profile.height,
        );
        let copy_ms = t1.elapsed().as_secs_f64() * 1000.0;
        if frame >= warmup {
            run.render.push(render_ms);
            run.copy.push(copy_ms);
            run.frame.push(render_ms + copy_ms);
        }
    }
    run.render.sort_by(|a, b| a.partial_cmp(b).unwrap());
    run.copy.sort_by(|a, b| a.partial_cmp(b).unwrap());
    run.frame.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(run)
}

/// A profile that silently benches a node the catalog does not have would report fewer
/// passes than the graph it claims to represent.
fn profile_absent(profile: &BenchProfile, catalog: &flux_catalog::Catalog) -> Vec<&'static str> {
    let mut wanted: Vec<&'static str> = profile.chain.iter().map(|(id, _)| *id).collect();
    wanted.push(profile.generator);
    wanted.push("flux.output");
    wanted
        .iter()
        .filter(|id| catalog.get(id).is_none())
        .copied()
        .collect()
}

fn cmd_benchmark(args: &[String]) -> i32 {
    let profile_name = opt_value(args, "--profile").unwrap_or_else(|| "ordinary".into());
    let Some(profile) = BENCH_PROFILES.iter().find(|p| p.name == profile_name) else {
        let known: Vec<&str> = BENCH_PROFILES.iter().map(|p| p.name).collect();
        eprintln!("benchmark: unknown profile '{profile_name}' (known: {known:?})");
        write_evidence(
            "benchmark",
            &json!({"ok": false, "reason": "unknown profile", "profile": profile_name, "known_profiles": known}),
        );
        return 2;
    };
    let samples: usize = opt_value(args, "--samples")
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    let warmup: usize = opt_value(args, "--warmup")
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let gpu = match flux_render::gpu::Gpu::new(flux_render::BackendPref::Auto) {
        Ok(g) => g,
        Err(_) => {
            write_evidence("benchmark", &json!({"ok": false, "reason": "no GPU"}));
            return 1;
        }
    };
    let catalog = flux_catalog::build_catalog();
    let mut compiler = flux_render::pipeline::EffectCompiler::new(&gpu);

    // A profile that silently benches a node the catalog does not have would report
    // fewer passes than the graph it claims to represent.
    let absent = profile_absent(profile, &catalog);
    if !absent.is_empty() {
        eprintln!(
            "benchmark: profile '{}' names nodes absent from the catalog: {absent:?}",
            profile.name
        );
        write_evidence(
            "benchmark",
            &json!({"ok": false, "reason": "profile names absent catalog nodes", "profile": profile.name, "absent": absent}),
        );
        return 2;
    }

    let chain_graph = bench_chain_graph(profile);
    let chain = time_graph(
        profile,
        &gpu,
        &mut compiler,
        &catalog,
        &chain_graph,
        warmup,
        samples,
    );
    let chain = match chain {
        Ok(run) => run,
        Err(err) => {
            eprintln!("benchmark: profile graph failed to activate — {err}");
            write_evidence(
                "benchmark",
                &json!({"ok": false, "reason": "graph activation failed", "profile": profile.name, "error": err}),
            );
            return 1;
        }
    };

    // Per-generator rows: kept, because the old evidence file was nothing but these,
    // but timed as single-node graphs at the profile resolution with a small sample
    // count, so they report median/max and not a percentile they cannot support.
    let generators: Vec<String> = catalog
        .entries
        .iter()
        .filter(|e| {
            e.manifest.id.starts_with("gen.") && e.manifest.implementation.kind == "fragment"
        })
        .map(|e| e.manifest.id.clone())
        .collect();
    let mut rows = vec![];
    for gid in generators.iter().take(profile.sweep) {
        let graph = bench_graph(gid);
        let Ok(run) = time_graph(profile, &gpu, &mut compiler, &catalog, &graph, 3, 10) else {
            rows.push(json!({"id": gid, "error": "activation failed"}));
            continue;
        };
        rows.push(json!({
            "id": gid,
            "render": summarise(&run.render),
            "frame": summarise(&run.frame),
            "execute_errors": run.errors,
        }));
    }

    let render = summarise(&chain.render);
    let tail = render
        .get("p95_ms")
        .or(render.get("max_ms"))
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::MAX);
    let over_budget = tail > profile.budget_ms;
    let ok = chain.errors == 0 && !over_budget;

    write_evidence(
        "benchmark",
        &json!({
            "ok": ok,
            "profile": profile.name,
            "spec_profile": profile.spec,
            "resolution": {"width": profile.width, "height": profile.height},
            "frame_budget_ms": profile.budget_ms,
            "warmup_frames": warmup,
            "timed_frames": samples,
            "graph": {
                "source": profile.generator,
                "chain": profile.chain.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                "merge": "the generator also feeds fx.composite port b",
                "nodes": profile.chain.len() + 2,
                "compiled_passes": chain.passes,
            },
            "render_ms": render,
            "preview_copy_ms": summarise(&chain.copy),
            "render_plus_copy_ms": summarise(&chain.frame),
            "tail_over_budget": over_budget,
            "execute_errors": chain.errors,
            "presentation_misses": {"measured": false, "why": "headless graph execution has no swapchain; a frame here is not a presented frame (docs/PERFORMANCE-READINESS.md F2)"},
            "copy_caveat": "read_preview builds a shader module, bind group layout and pipeline on every call (crates/render/src/preview.rs:32), so preview_copy_ms is an upper bound on the copy, and render_plus_copy_ms is the only number that bounds the GPU.",
            "generators": rows,
            "gpu": gpu.info.to_json(),
        }),
    );
    println!(
        "benchmark[{}]: {}x{} graph of {} nodes — render median {:.2} ms, {} — budget {} ms: {}",
        profile.name,
        profile.width,
        profile.height,
        profile.chain.len() + 2,
        render["median_ms"].as_f64().unwrap_or(f64::NAN),
        if render.get("p95_ms").is_some() {
            format!("p95 {:.2} ms", render["p95_ms"].as_f64().unwrap())
        } else {
            format!("max {:.2} ms", render["max_ms"].as_f64().unwrap())
        },
        profile.budget_ms,
        if over_budget { "OVER" } else { "ok" },
    );
    println!(
        "  preview copy median {:.2} ms over {} samples (see copy_caveat); {} generators swept at this resolution",
        summarise(&chain.copy)["median_ms"].as_f64().unwrap_or(f64::NAN),
        samples,
        rows.len()
    );
    if ok {
        0
    } else {
        1
    }
}

/// Peak resident set of this process, and the device's GPU memory in use.
#[cfg(windows)]
fn process_memory() -> Option<(u64, u64)> {
    use windows::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            std::ptr::from_mut(&mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    };
    if ok.0 != 0 {
        Some((
            counters.WorkingSetSize as u64,
            counters.PeakWorkingSetSize as u64,
        ))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn process_memory() -> Option<(u64, u64)> {
    None
}

/// `nvidia-smi` reports device-wide memory, which is not per-process: it is recorded as
/// what it is. A Vulkan renderer's allocations do not appear in `--query-compute-apps`,
/// so no number here attributes VRAM to Flux.
fn nvidia_memory_used_mib() -> Option<u64> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used",
            "--format=csv,noheader,nounits",
            "--id=0",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .parse::<u64>()
        .ok()
}

/// F3/F5's steady-state half: the same graph the benchmark times, but run against a
/// wall-clock deadline instead of a frame count, with peak and growth of the process's
/// working set sampled alongside. 120 frames cannot see a leak or a hitch that appears
/// at minute two, which is why `benchmark` is not this.
///
/// Deliberately not part of `verify.ps1`: it is a two-minute measurement, not a gate,
/// and the trip wire it checks (growth under 10 % and under 64 MiB,
/// `spec/VERIFICATION.md:144`) belongs to the soak run, not to every CI pass.
fn cmd_soak(args: &[String]) -> i32 {
    let profile_name = opt_value(args, "--profile").unwrap_or_else(|| "ordinary".into());
    let Some(profile) = BENCH_PROFILES.iter().find(|p| p.name == profile_name) else {
        let known: Vec<&str> = BENCH_PROFILES.iter().map(|p| p.name).collect();
        eprintln!("soak: unknown profile '{profile_name}' (known: {known:?})");
        write_evidence(
            &format!("soak-{profile_name}"),
            &json!({"ok": false, "reason": "unknown profile", "profile": profile_name, "known_profiles": known}),
        );
        return 2;
    };
    let seconds: u64 = opt_value(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    if seconds == 0 {
        eprintln!("soak: --seconds must be greater than 0");
        return 2;
    }
    let warmup: usize = opt_value(args, "--warmup")
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let evidence = format!("soak-{}", profile.name);

    let gpu = match flux_render::gpu::Gpu::new(flux_render::BackendPref::Auto) {
        Ok(g) => g,
        Err(_) => {
            write_evidence(
                &evidence,
                &json!({"ok": false, "reason": "no GPU", "profile": profile.name}),
            );
            return 1;
        }
    };
    let catalog = flux_catalog::build_catalog();
    let absent = profile_absent(profile, &catalog);
    if !absent.is_empty() {
        eprintln!(
            "soak: profile '{}' names nodes absent from the catalog: {absent:?}",
            profile.name
        );
        write_evidence(
            &evidence,
            &json!({"ok": false, "reason": "profile names absent catalog nodes", "profile": profile.name, "absent": absent}),
        );
        return 2;
    }

    let mut compiler = flux_render::pipeline::EffectCompiler::new(&gpu);
    let graph = bench_chain_graph(profile);
    let mut rt = match flux_render::graph_exec::RuntimeGraph::activate(
        &gpu,
        &mut compiler,
        &catalog,
        &graph,
        profile.width,
        profile.height,
        1,
    ) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("soak: profile graph failed to activate — {e:?}");
            write_evidence(
                &evidence,
                &json!({"ok": false, "reason": "graph activation failed", "profile": profile.name, "error": format!("{e:?}")}),
            );
            return 1;
        }
    };
    let passes = rt.compiled.passes.len();
    let project = flux_domain::Project::new("soak", 1);
    let ctx =
        flux_engine::frame::FrameContext::for_project(&project, flux_domain::RationalTime::ZERO, 0);
    let mut resolved = std::collections::HashMap::new();
    for pass in &rt.compiled.passes {
        if let Some(e) = catalog.get(&pass.type_id) {
            resolved.insert(
                pass.node_id.clone(),
                flux_render::pipeline::NodeParamsBuffer::defaults(&e.manifest),
            );
        }
    }
    let src = bench_source_texture(&gpu, profile.width, profile.height);
    let sv = src.create_view(&Default::default());

    let mut render: Vec<f64> = vec![];
    let mut copy: Vec<f64> = vec![];
    let mut errors = 0usize;
    let mut rss: Vec<u64> = vec![];
    let mut peak_rss: u64 = 0;
    let mut vram: Vec<u64> = vec![];
    let mut gpu_probes = 0usize;
    let mut frames = 0usize;
    let start = std::time::Instant::now();
    let mut last_rss = start;
    let mut last_gpu = start;
    let deadline = start + std::time::Duration::from_secs(seconds);

    while std::time::Instant::now() < deadline {
        let t0 = std::time::Instant::now();
        if rt
            .execute(&gpu, &compiler, &catalog, &ctx, &sv, &resolved, 1.0)
            .is_err()
        {
            errors += 1;
        }
        let _ = gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let render_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let t1 = std::time::Instant::now();
        let _ = flux_render::preview::read_preview(
            &gpu,
            &rt.target_texture(),
            profile.width,
            profile.height,
        );
        let copy_ms = t1.elapsed().as_secs_f64() * 1000.0;
        frames += 1;
        if frames > warmup {
            render.push(render_ms);
            copy.push(copy_ms);
        }
        // Probing is never inside a frame's timing window: `nvidia-smi` is a process
        // spawn costing tens of milliseconds, and putting it between t0 and the wait is
        // exactly how the old harness made a copy appear in a render number.
        if last_rss.elapsed() >= std::time::Duration::from_millis(500) {
            if let Some((working, peak)) = process_memory() {
                rss.push(working);
                peak_rss = peak_rss.max(peak).max(working);
            }
            last_rss = std::time::Instant::now();
        }
        // One probe immediately, then every 10 s. Counting attempts rather than results
        // is what stops a missing `nvidia-smi` from being re-spawned on every frame.
        if gpu_probes == 0 || last_gpu.elapsed() >= std::time::Duration::from_secs(10) {
            if let Some(used) = nvidia_memory_used_mib() {
                vram.push(used);
            }
            gpu_probes += 1;
            last_gpu = std::time::Instant::now();
        }
    }
    let elapsed_at_end = start.elapsed();
    render.sort_by(|a, b| a.partial_cmp(b).unwrap());
    copy.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let render_sum = summarise(&render);
    let copy_sum = summarise(&copy);

    let growth_mib = if rss.len() >= 2 {
        Some(rss[rss.len() - 1].saturating_sub(rss[0]) / (1024 * 1024))
    } else {
        None
    };
    let growth_pct = if rss.len() >= 2 && rss[0] > 0 {
        Some((rss[rss.len() - 1] as f64 - rss[0] as f64) * 100.0 / rss[0] as f64)
    } else {
        None
    };
    // spec/VERIFICATION.md:144 — unexplained growth over 10 % or 64 MiB is the trip wire.
    let leaked = match (growth_mib, growth_pct) {
        (Some(mib), Some(pct)) => mib > 64 || pct > 10.0,
        _ => false,
    };
    let tail = render_sum
        .get("p99_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::MAX);
    // F3's failure mode is a hitch, not a slow mean, so the count of frames that
    // individually breached the budget is part of the record even when p95 is fine.
    let over_budget_frames = render.iter().filter(|v| **v > profile.budget_ms).count();

    let ok = errors == 0 && !leaked && render_sum.get("p95_ms").is_some();
    write_evidence(
        &evidence,
        &json!({
            "ok": ok,
            "profile": profile.name,
            "requestedSeconds": seconds,
            "wallSeconds": elapsed_at_end.as_secs_f64(),
            "headlessFramesPerSecond": frames as f64 / elapsed_at_end.as_secs_f64(),
            "frames": frames,
            "timedSamples": render.len(),
            "warmupFrames": warmup,
            "passes": passes,
            "resolution": [profile.width, profile.height],
            "frameBudgetMs": profile.budget_ms,
            "render": render_sum,
            "previewCopy": copy_sum,
            "executeErrors": errors,
            "p99OverBudget": tail > profile.budget_ms,
            "framesOverBudget": over_budget_frames,
            "memory": {
                "processWorkingSetSamplesBytes": rss,
                "peakWorkingSetBytes": peak_rss,
                "growthMiB": growth_mib,
                "growthPct": growth_pct,
                "tripWire": "growth must stay under 10% and under 64 MiB (spec/VERIFICATION.md:144)",
                "measured": !rss.is_empty(),
                "measureFailureReason": if rss.is_empty() { "K32GetProcessMemoryInfo unavailable on this host" } else { "" },
            },
            "gpuMemory": {
                "deviceUsedMiBSamples": vram,
                "peakDeviceUsedMiB": vram.iter().max(),
                "probeCount": gpu_probes,
                "measured": !vram.is_empty(),
                "measureFailureReason": if vram.is_empty() { "nvidia-smi gave no parseable memory.used: absent, non-zero exit, or non-numeric" } else { "" },
                "caveat": "device-wide memory in use from nvidia-smi, not per-process: a Vulkan renderer's allocations are not attributed to Flux by nvidia-smi. Other processes on this GPU are inside the number.",
                "processVram": {"measured": false, "reason": "no Vulkan-memory telemetry in this build; NVAPI not linked"},
            },
            "presentationMisses": {"measured": false, "reason": "headless graph execution has no swapchain (same limit as benchmark, F2)"},
            "audio": {"measured": false, "reason": "this soak drives the render loop only; no device is opened, so dropped() is not exercised"},
            "gpu": gpu.info.to_json(),
        }),
    );
    let ms = |key: &str| -> String {
        render_sum
            .get(key)
            .and_then(|v| v.as_f64())
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    };
    println!(
        "soak {} @ {}x{}: {} frames in {:.1} s wall, {} execute errors, render p50 {} p95 {} p99 {} max {} ms against {:.2}",
        profile.name,
        profile.width,
        profile.height,
        frames,
        elapsed_at_end.as_secs_f64(),
        errors,
        ms("median_ms"),
        ms("p95_ms"),
        ms("p99_ms"),
        ms("max_ms"),
        profile.budget_ms,
    );
    println!(
        "  working set peak {} MiB, growth {} MiB ({} %) over {} samples; device GPU memory peak {} MiB over {} probes (device-wide, see gpuMemory.caveat)",
        peak_rss / (1024 * 1024),
        growth_mib.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
        growth_pct
            .map(|p| format!("{p:.2}"))
            .unwrap_or_else(|| "-".into()),
        rss.len(),
        vram.iter()
            .max()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".into()),
        gpu_probes,
    );
    if ok {
        0
    } else {
        1
    }
}

/// The profile's graph: `generator -> fx -> fx -> ... -> output`, with the generator
/// also wired into `fx.composite`'s second input so the timed path is a merging DAG
/// rather than a straight line. This is the measurement the tree never had: every
/// previous number here came from a one-node graph.
fn bench_chain_graph(profile: &BenchProfile) -> flux_domain::Graph {
    use flux_domain::{Graph, GraphEdge, GraphNode};
    use std::collections::BTreeMap;
    let mut nodes = vec![GraphNode {
        id: "g".into(),
        type_id: profile.generator.into(),
        type_version: 1,
        position: [0.0, 0.0],
        params: BTreeMap::new(),
        bypass: false,
        missing_type: false,
    }];
    let mut edges = vec![];
    let mut prev = "g".to_string();
    for (i, (type_id, input_port)) in profile.chain.iter().enumerate() {
        let id = format!("e{i}");
        nodes.push(GraphNode {
            id: id.clone(),
            type_id: (*type_id).into(),
            type_version: 1,
            position: [(240.0 * (i as u32 + 1) as f32), 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        });
        edges.push(GraphEdge {
            from_node: prev.clone(),
            from_port: "color".into(),
            to_node: id.clone(),
            to_port: (*input_port).into(),
        });
        if *input_port == "a" {
            edges.push(GraphEdge {
                from_node: "g".into(),
                from_port: "color".into(),
                to_node: id.clone(),
                to_port: "b".into(),
            });
        }
        prev = id;
    }
    nodes.push(GraphNode {
        id: "o".into(),
        type_id: "flux.output".into(),
        type_version: 1,
        position: [240.0 * (profile.chain.len() as u32 + 1) as f32, 0.0],
        params: BTreeMap::new(),
        bypass: false,
        missing_type: false,
    });
    edges.push(GraphEdge {
        from_node: prev,
        from_port: "color".into(),
        to_node: "o".into(),
        to_port: "source".into(),
    });
    Graph {
        id: format!("bench:chain:{}", profile.name),
        name: format!("{} chain", profile.name),
        nodes,
        edges,
        output_node: "o".into(),
        keyframes: BTreeMap::new(),
    }
}

fn bench_graph(gen_id: &str) -> flux_domain::Graph {
    use flux_domain::{Graph, GraphEdge, GraphNode};
    use std::collections::BTreeMap;
    Graph {
        id: format!("bench:{}", gen_id),
        name: gen_id.into(),
        nodes: vec![
            GraphNode {
                id: "g".into(),
                type_id: gen_id.into(),
                type_version: 1,
                position: [0.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
            GraphNode {
                id: "o".into(),
                type_id: "flux.output".into(),
                type_version: 1,
                position: [240.0, 0.0],
                params: BTreeMap::new(),
                bypass: false,
                missing_type: false,
            },
        ],
        edges: vec![GraphEdge {
            from_node: "g".into(),
            from_port: "color".into(),
            to_node: "o".into(),
            to_port: "source".into(),
        }],
        output_node: "o".into(),
        keyframes: BTreeMap::new(),
    }
}

fn cmd_report() -> i32 {
    let a = cmd_catalog(&[]);
    let b = cmd_doctor();
    if a == 0 && b == 0 {
        0
    } else {
        1
    }
}

fn cmd_package(_args: &[String]) -> i32 {
    println!("package: building release binary...");
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "flux-app",
            "--no-default-features",
        ])
        .current_dir(root())
        .status();
    let ok = status.map(|s| s.success()).unwrap_or(false);
    let exe = root().join("target/release/flux.exe");
    write_evidence(
        "package",
        &json!({"ok": ok, "exe": exe.display().to_string(), "exists": exe.exists()}),
    );
    println!(
        "package: {}",
        if ok {
            "built target/release/flux.exe"
        } else {
            "FAILED"
        }
    );
    if ok {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod evidence_provenance {
    use super::{attribute, header_git_hash};

    /// The header form the 2026-11-15 hand transcripts actually open with. Before D031 the
    /// manifest called these files `unattributed` and asserted in its own notes that nothing
    /// inside them named a commit, which this line shows to be false.
    #[test]
    fn reads_the_f1_header_a_hand_transcript_uses() {
        let line = "# provenance (F1): gitHash f49bb20, working tree DIRTY at capture time";
        assert_eq!(header_git_hash(line).as_deref(), Some("f49bb20"));
        let quoted = "# provenance (F1): gitHash \"cf95a2f\", gitDirty true";
        assert_eq!(header_git_hash(quoted).as_deref(), Some("cf95a2f"));
    }

    /// A header that names nothing must read as naming nothing. Returning a guess here is
    /// how a provenance field turns into folklore.
    #[test]
    fn names_nothing_when_there_is_no_sha_to_name() {
        assert_eq!(header_git_hash("# peers=0 tempo=120.0 frames=83"), None);
        assert_eq!(
            header_git_hash("# provenance (F1): gitHash not-recorded"),
            None
        );
        assert_eq!(header_git_hash("# gitHash abc"), None);
        assert_eq!(header_git_hash(""), None);
    }

    /// The scan is bounded to the head of the file on purpose: a SHA in the middle of prose
    /// is usually a citation of somebody else's commit. This test is what makes `take(24)`
    /// load-bearing rather than decorative.
    #[test]
    fn stops_at_the_top_of_the_file() {
        let mut text = String::new();
        for i in 0..40 {
            text.push_str(&format!("# body line {i}\n"));
        }
        text.push_str("# provenance (F1): gitHash f49bb20\n");
        assert_eq!(header_git_hash(&text), None);
    }

    /// The two strengths may not be blended: a writer stamp outranks a header, and a header
    /// is never reported as if a writer had injected it.
    #[test]
    fn a_writer_stamp_and_a_typed_header_stay_apart() {
        let (sha, source, status) = attribute(
            Some("aaaaaaa".into()),
            Some("bbbbbbb".into()),
            false,
            Some("aaaaaaa"),
        );
        assert_eq!(
            (sha.as_deref(), source, status),
            (Some("aaaaaaa"), "writer", "stamped_current")
        );

        let (sha, source, status) = attribute(None, Some("abc1234".into()), false, Some("abc1234"));
        assert_eq!(
            (sha.as_deref(), source, status),
            (Some("abc1234"), "header", "header_current")
        );

        let (_, _, status) = attribute(None, Some("abc1234".into()), false, Some("9999999"));
        assert_eq!(status, "header_other_commit");
    }

    /// A full SHA in the file still has to read as current against a short HEAD, or every
    /// transcript that quotes `gitHashFull` would be filed under another commit.
    #[test]
    fn a_full_sha_matches_a_short_head() {
        let full = "90d9e59a000000000000000000000000000000ff";
        let (_, _, status) = attribute(None, Some(full.into()), false, Some("90d9e59"));
        assert_eq!(status, "header_current");
    }

    /// Absence is reported with two different words, and which one applies is a property of
    /// the file's producer, not of the file: this crate writing a gate without provenance is
    /// a bug in this crate (`no_provenance`, and the command exits nonzero), while a hand
    /// capture that names nothing is a limitation worth stating plainly.
    #[test]
    fn missing_provenance_is_a_defect_only_for_files_this_crate_writes() {
        let (sha, source, status) = attribute(None, None, true, Some("abc1234"));
        assert_eq!((sha, source, status), (None, "none", "no_provenance"));
        let (sha, source, status) = attribute(None, None, false, Some("abc1234"));
        assert_eq!((sha, source, status), (None, "none", "unattributed"));
    }

    /// With no readable HEAD nothing can be called current. The SHA is still reported, so a
    /// reader can compare it by hand instead of trusting a tier this run cannot compute.
    #[test]
    fn an_unreadable_head_credits_nothing() {
        let (_, _, status) = attribute(Some("abc1234".into()), None, true, None);
        assert_eq!(status, "stamped_other_commit");
    }
}
