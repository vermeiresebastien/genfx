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
    std::fs::write(&path, serde_json::to_string_pretty(data).unwrap()).unwrap();
    println!("evidence: {}", path.display());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let code = match cmd {
        "doctor" => cmd_doctor(),
        "fixtures" => cmd_fixtures(),
        "catalog" => cmd_catalog(&args[1..]),
        "check" => cmd_check(&args[1..]),
        "test-gpu" => cmd_test_gpu(&args[1..]),
        "scenario" => cmd_scenario(&args[1..]),
        "benchmark" => cmd_benchmark(&args[1..]),
        "report" => cmd_report(),
        "package" => cmd_package(&args[1..]),
        _ => {
            println!("usage: cargo xtask <doctor|fixtures|catalog|check|test-gpu|scenario|benchmark|report|package>");
            2
        }
    };
    std::process::exit(code);
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
    let mut wanted: Vec<&str> = profile.chain.iter().map(|(id, _)| *id).collect();
    wanted.push(profile.generator);
    wanted.push("flux.output");
    let absent: Vec<&str> = wanted
        .iter()
        .filter(|id| catalog.get(id).is_none())
        .copied()
        .collect();
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
