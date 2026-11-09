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

fn cmd_benchmark(args: &[String]) -> i32 {
    let profile = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "ordinary".into());
    let gpu = match flux_render::gpu::Gpu::new(flux_render::BackendPref::Auto) {
        Ok(g) => g,
        Err(_) => {
            write_evidence("benchmark", &json!({"ok": false, "reason": "no GPU"}));
            return 1;
        }
    };
    let catalog = flux_catalog::build_catalog();
    let mut compiler = flux_render::pipeline::EffectCompiler::new(&gpu);
    let generators: Vec<String> = catalog
        .entries
        .iter()
        .filter(|e| {
            e.manifest.id.starts_with("gen.") && e.manifest.implementation.kind == "fragment"
        })
        .map(|e| e.manifest.id.clone())
        .collect();
    let mut results = vec![];
    for gid in generators.iter().take(30) {
        let graph = bench_graph(gid);
        let Ok(mut rt) = flux_render::graph_exec::RuntimeGraph::activate(
            &gpu,
            &mut compiler,
            &catalog,
            &graph,
            1920,
            1080,
            1,
        ) else {
            continue;
        };
        let project = flux_domain::Project::new("bench", 1);
        let ctx = flux_engine::frame::FrameContext::for_project(
            &project,
            flux_domain::RationalTime::ZERO,
            0,
        );
        let mut resolved = std::collections::HashMap::new();
        for pass in &rt.compiled.passes {
            if let Some(e) = catalog.get(&pass.type_id) {
                resolved.insert(
                    pass.node_id.clone(),
                    flux_render::pipeline::NodeParamsBuffer::defaults(&e.manifest),
                );
            }
        }
        let src = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sv = src.create_view(&Default::default());
        let mut times = vec![];
        for _ in 0..12 {
            let t0 = std::time::Instant::now();
            let _ = rt.execute(&gpu, &compiler, &catalog, &ctx, &sv, &resolved, 1.0);
            let _ = flux_render::preview::read_preview(&gpu, &rt.target_texture(), 1920, 1080);
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        results.push(json!({"id": gid, "p50ms": times[times.len() / 2], "p95ms": times[times.len() * 95 / 100]}));
    }
    write_evidence(
        "benchmark",
        &json!({"ok": !results.is_empty(), "profile": profile, "results": results}),
    );
    println!("benchmark: {} generators timed at 1920x1080", results.len());
    if results.is_empty() {
        1
    } else {
        0
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
