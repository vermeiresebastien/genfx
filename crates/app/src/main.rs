//! Flux application entry point. Subcommands:
//!   flux list                     list catalog effects/generators
//!   flux doctor                   print toolchain/GPU/media capabilities
//!   flux new-demo <gen> <dir>     write a demo project file
//!   flux render <project> <dir>   export PNG+WAV+MP4 for a project file
//!   flux demo-render <gen> <dir>  build a demo and export it in one step
//!   flux demo-media <video> <dir> import a video and export it
//!   flux contact-sheet <dir>      render an effects contact sheet
//!   flux gui [project]            launch the native editor (not in this build)

use flux_app::{export, samples};
use flux_catalog::build_catalog;
use flux_domain::{Dispatcher, FluxError, FluxResult, Project};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = run(&args);
    match result {
        Ok(()) => {}
        Err(e) => {
            eprintln!("flux: {e:?}");
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> FluxResult<()> {
    let cmd = args.first().map(String::as_str).unwrap_or("gui");
    match cmd {
        "list" => cmd_list(),
        "doctor" => cmd_doctor(),
        "new-demo" => {
            let gen = args.get(1).cloned().unwrap_or_else(|| "gen.plasma".into());
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| ".".into()));
            cmd_new_demo(&gen, &dir)
        }
        "demo-render" => {
            let gen = args.get(1).cloned().unwrap_or_else(|| "gen.plasma".into());
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "out".into()));
            cmd_demo_render(&gen, &dir)
        }
        "demo-media" => {
            let video = PathBuf::from(args.get(1).cloned().ok_or_else(|| {
                FluxError::new(
                    flux_domain::ErrorCode::InvalidCommand,
                    "cli",
                    "demo-media needs a video path",
                )
            })?);
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "out".into()));
            cmd_demo_media(&video, &dir)
        }
        "contact-sheet" => {
            let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "out".into()));
            cmd_contact_sheet(&dir)
        }
        "render" => {
            let project = PathBuf::from(args.get(1).cloned().ok_or_else(|| {
                FluxError::new(
                    flux_domain::ErrorCode::InvalidCommand,
                    "cli",
                    "render needs a project path",
                )
            })?);
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "out".into()));
            let codec = args
                .iter()
                .position(|a| a == "--codec")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_else(|| "h264".into());
            cmd_render(&project, &dir, &codec)
        }
        "recover" => {
            let dir = PathBuf::from(args.get(1).cloned().ok_or_else(|| {
                FluxError::new(
                    flux_domain::ErrorCode::InvalidCommand,
                    "cli",
                    "recover needs a project dir",
                )
            })?);
            cmd_recover(&dir)
        }
        "collect" => {
            let project = PathBuf::from(args.get(1).cloned().ok_or_else(|| {
                FluxError::new(
                    flux_domain::ErrorCode::InvalidCommand,
                    "cli",
                    "collect needs a project path",
                )
            })?);
            cmd_collect(&project)
        }
        "caps" => cmd_caps(),
        "capture" => cmd_capture(),
        "integrations" => cmd_integrations(),
        "osc" => {
            let port = args
                .get(1)
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(flux_integrations::osc::DEFAULT_RECV_PORT);
            cmd_osc_listen(port)
        }
        "record" => {
            let project = PathBuf::from(args.get(1).cloned().ok_or_else(|| {
                FluxError::new(
                    flux_domain::ErrorCode::InvalidCommand,
                    "cli",
                    "record needs a project path",
                )
            })?);
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "rec_out".into()));
            let secs = args
                .iter()
                .position(|a| a == "--seconds")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(5.0);
            cmd_record(&project, &dir, secs)
        }
        "gui" | "editor" => {
            eprintln!("The GUI is not included in this build. Use `flux demo-render` / `flux render` for headless rendering.");
            std::process::exit(2);
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: flux [list|doctor|caps|capture|integrations|osc|new-demo|demo-render|demo-media|contact-sheet|render|record|recover|collect|gui]");
            std::process::exit(2);
        }
    }
}

fn cmd_list() -> FluxResult<()> {
    let c = build_catalog();
    let mut effects = 0;
    let mut generators = 0;
    println!("Catalog ({} entries):", c.entries.len());
    for e in &c.entries {
        let m = &e.manifest;
        let kind = if m.generator_id.is_empty() {
            "effect"
        } else {
            "gen"
        };
        if kind == "gen" {
            generators += 1;
        } else {
            effects += 1;
        }
        println!("  {:<28} {:<24} [{}]", m.id, m.name, m.category);
    }
    println!("\n{effects} effects/utilities, {generators} generators");
    Ok(())
}

fn cmd_doctor() -> FluxResult<()> {
    use flux_render::gpu::{BackendPref, Gpu};
    println!("== Flux doctor ==");
    println!("rustc: {}", env!("CARGO_PKG_VERSION"));
    match Gpu::new(BackendPref::Auto) {
        Ok(g) => {
            println!(
                "GPU:   {} ({}, {})",
                g.info.name, g.info.backend, g.info.device_type
            );
            println!("driver:{}", g.info.driver);
            println!("max 2D texture: {}", g.info.max_texture_2d);
            println!("rgba16f filterable: {}", g.info.supports_rgba16f_filter);
        }
        Err(e) => println!("GPU:   unavailable ({e:?})"),
    }
    match flux_media::MediaWorker::new() {
        Ok(w) => {
            let c = w.capabilities();
            println!("ffmpeg: {}", c.ffmpeg_path);
            println!(
                "encoders: h264={} hevc={} prores={} aac={} gif={} png={} wav={}",
                c.h264, c.hevc, c.prores, c.aac, c.gif, c.png, c.wav
            );
        }
        Err(e) => println!("ffmpeg: unavailable ({e:?})"),
    }
    println!("catalog entries: {}", build_catalog().entries.len());
    Ok(())
}

fn cmd_new_demo(gen: &str, dir: &std::path::Path) -> FluxResult<()> {
    let project = samples::demo_generator_project(gen, &["fx.bloom"], 4);
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.flux.json", sanitize(&project.name)));
    save_project(&project, &path)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn cmd_demo_render(gen: &str, dir: &std::path::Path) -> FluxResult<()> {
    let project = samples::demo_generator_project(gen, &["fx.bloom"], 4);
    let result = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.to_path_buf(),
            project_name: project.name.clone(),
            codec: flux_media::EncodeCodec::H264,
            width: 1280,
            height: 720,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        Some(Box::new(|i, n| {
            if i == n || i % 30 == 0 {
                println!("frame {i}/{n}");
            }
        })),
    )?;
    println!(
        "rendered {} frames, {} samples; frames in {}",
        result.frames,
        result.samples,
        result.frames_dir.display()
    );
    if let Some(v) = &result.video {
        println!("video: {}", v.display());
    }
    if let Some(w) = &result.wav {
        println!("wav:   {}", w.display());
    }
    Ok(())
}

/// Save project modal-free helper used by cmd_new_demo.
pub use save_project as save;

fn cmd_demo_media(video: &std::path::Path, dir: &std::path::Path) -> FluxResult<()> {
    let project = samples::demo_media_project(video, 3);
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.flux.json", sanitize(&project.name)));
    save_project(&project, &path)?;
    println!("wrote {}", path.display());
    let result = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.to_path_buf(),
            project_name: project.name.clone(),
            codec: flux_media::EncodeCodec::H264,
            width: 640,
            height: 360,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        Some(Box::new(|i, n| {
            if i == n {
                println!("frame {i}/{n}");
            }
        })),
    )?;
    println!(
        "rendered {} frames, {} samples",
        result.frames, result.samples
    );
    if let Some(v) = &result.video {
        println!("video: {}", v.display());
    }
    Ok(())
}

fn cmd_contact_sheet(dir: &std::path::Path) -> FluxResult<()> {
    let catalog = build_catalog();
    let effects: Vec<String> = catalog
        .entries
        .iter()
        .filter(|e| {
            (e.manifest.id.starts_with("fx.") || e.manifest.id.starts_with("preset."))
                && e.manifest.implementation.kind == "fragment"
        })
        .map(|e| e.manifest.id.clone())
        .collect();
    std::fs::create_dir_all(dir)?;
    let (w, h, px) = export::contact_sheet(&effects, 160, 90, 8)?;
    let path = dir.join("effects_contact_sheet.png");
    flux_media::write_png_rgba(&path, w, h, &px)?;
    println!(
        "contact sheet ({} effects, {}x{}) -> {}",
        effects.len(),
        w,
        h,
        path.display()
    );
    Ok(())
}

fn cmd_render(
    project_path: &std::path::Path,
    dir: &std::path::Path,
    codec: &str,
) -> FluxResult<()> {
    let project = load_project(project_path)?;
    let codec = flux_app::job::parse_codec(codec);
    let result = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.to_path_buf(),
            project_name: project.name.clone(),
            codec,
            width: project.settings.size.width,
            height: project.settings.size.height,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        Some(Box::new(|i, n| {
            if i == n || i % 60 == 0 {
                println!("frame {i}/{n}");
            }
        })),
    )?;
    println!(
        "rendered {} frames, {} samples",
        result.frames, result.samples
    );
    if let Some(v) = &result.video {
        println!("video: {}", v.display());
    }
    if let Some(w) = &result.wav {
        println!("wav:   {}", w.display());
    }
    Ok(())
}

fn cmd_record(project_path: &std::path::Path, dir: &std::path::Path, secs: f64) -> FluxResult<()> {
    let project = load_project(project_path)?;
    let r = flux_app::record::record_project(
        &project,
        &flux_app::record::RecordOptions {
            out_dir: dir.to_path_buf(),
            project_name: format!("{}_take", sanitize(&project.name)),
            codec: flux_media::EncodeCodec::H264,
            width: project.settings.size.width,
            height: project.settings.size.height,
            seconds: secs,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
    )?;
    println!(
        "recorded {} frames, {} samples, dropped={}, overrun={}",
        r.frames, r.samples, r.dropped_frames, r.overrun
    );
    if let Some(v) = &r.video {
        println!("video: {}", v.display());
    }
    if r.overrun {
        return Err(FluxError::new(
            flux_domain::ErrorCode::MediaDecode,
            "record",
            "RECORDING_OVERRUN: renderer could not sustain realtime",
        ));
    }
    Ok(())
}

fn cmd_recover(dir: &std::path::Path) -> FluxResult<()> {
    match flux_app::persistence::find_recovery(dir) {
        Some(m) => {
            println!(
                "recoverable snapshot: rev {} at {} (schema {})",
                m.revision, m.timestamp_utc, m.schema_version
            );
            println!("autosave: {}", m.autosave_path);
            let p = flux_app::persistence::load(std::path::Path::new(&m.autosave_path))?;
            let out = dir.join(format!("{}_recovered.flux.json", sanitize(&p.name)));
            flux_app::persistence::save_atomic(&p, &out)?;
            println!("recovered -> {}", out.display());
        }
        None => println!("no recoverable autosave in {}", dir.display()),
    }
    Ok(())
}

fn cmd_collect(project_path: &std::path::Path) -> FluxResult<()> {
    let mut project = load_project(project_path)?;
    let dir = project_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let n = flux_app::persistence::collect_media(&mut project, dir)?;
    flux_app::persistence::save_atomic(&project, project_path)?;
    println!(
        "collected {n} media files into {}",
        dir.join("media").display()
    );
    Ok(())
}

fn cmd_integrations() -> FluxResult<()> {
    for c in flux_integrations::capability_report() {
        println!(
            "{:<8} available={} implemented={} verified={}  ({})",
            c.id, c.available, c.implemented, c.verified, c.limitation
        );
    }
    Ok(())
}

fn cmd_osc_listen(port: u16) -> FluxResult<()> {
    let (tx, rx) = crossbeam_channel::bounded(256);
    let (recv, handle) = flux_integrations::osc::OscReceiver::start(port, tx)
        .map_err(|e| FluxError::new(flux_domain::ErrorCode::OscMalformed, "osc", e.to_string()))?;
    println!("listening for OSC on 127.0.0.1:{port} (Ctrl-C to stop)");
    let start = std::time::Instant::now();
    // Run for up to 30s or until 100 controls, printing each; demonstrates the
    // real receiver without any command/file execution from network input.
    while start.elapsed().as_secs() < 30 {
        match rx.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(c) => println!("{c:?}"),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    recv.stop();
    let _ = handle.join();
    Ok(())
}

fn cmd_capture() -> FluxResult<()> {
    let c = flux_platform::capture_capabilities();
    println!("platform: {}", c.platform);
    println!(
        "cameras: {}  microphones: {}  loopback: {}",
        c.cameras.len(),
        c.microphones.len(),
        c.loopback.len()
    );
    for d in c.cameras.iter().chain(&c.microphones).chain(&c.loopback) {
        println!("  [{}] {} default={}", d.kind, d.name, d.is_default);
    }
    println!(
        "camera_available={} loopback_available={}",
        c.camera_available, c.loopback_available
    );
    println!("note: {}", c.limitation);
    Ok(())
}

fn cmd_caps() -> FluxResult<()> {
    use flux_media::MediaWorker;
    match MediaWorker::new() {
        Ok(w) => {
            let c = w.capabilities();
            println!(
                "{{\"h264\":{},\"hevc\":{},\"prores\":{},\"aac\":{},\"gif\":{},\"png\":{},\"wav\":{}}}",
                c.h264, c.hevc, c.prores, c.aac, c.gif, c.png, c.wav
            );
        }
        Err(e) => println!("{{\"error\":{e:?}}}"),
    }
    Ok(())
}

/// Atomic project save (delegates to the persistence module).
pub fn save_project(project: &Project, path: &std::path::Path) -> FluxResult<()> {
    flux_app::persistence::save_atomic(project, path)
}

pub fn load_project(path: &std::path::Path) -> FluxResult<Project> {
    flux_app::persistence::load(path)
}

pub fn new_dispatcher(project: Project) -> Dispatcher {
    Dispatcher::new(project)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
