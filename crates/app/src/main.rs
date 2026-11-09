//! Flux application entry point. Subcommands:
//!   flux list                     list catalog effects/generators
//!   flux doctor                   print toolchain/GPU/media capabilities
//!   flux new-demo <gen> <dir>     write a demo project file
//!   flux render <project> <dir>   export PNG+WAV+MP4 for a project file
//!   flux demo-render <gen> <dir>  build a demo and export it in one step
//!   flux gui [project]            launch the native editor (default)

mod export;
mod samples;

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
            let video = PathBuf::from(
                args.get(1).cloned().ok_or_else(|| FluxError::new(flux_domain::ErrorCode::InvalidCommand, "cli", "demo-media needs a video path"))?,
            );
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "out".into()));
            cmd_demo_media(&video, &dir)
        }
        "contact-sheet" => {
            let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "out".into()));
            cmd_contact_sheet(&dir)
        }
        "render" => {
            let project = PathBuf::from(
                args.get(1).cloned().ok_or_else(|| FluxError::new(flux_domain::ErrorCode::InvalidCommand, "cli", "render needs a project path"))?,
            );
            let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "out".into()));
            cmd_render(&project, &dir)
        }
        "gui" | "editor" => {
            eprintln!("The GUI is not included in this build. Use `flux demo-render` / `flux render` for headless rendering.");
            std::process::exit(2);
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: flux [list|doctor|new-demo|demo-render|render|gui]");
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
        let kind = if m.generator_id.is_empty() { "effect" } else { "gen" };
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
            println!("GPU:   {} ({}, {})", g.info.name, g.info.backend, g.info.device_type);
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
            println!("encoders: h264={} hevc={} prores={} aac={} gif={} png={} wav={}", c.h264, c.hevc, c.prores, c.aac, c.gif, c.png, c.wav);
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
    println!("rendered {} frames, {} samples; frames in {}", result.frames, result.samples, result.frames_dir.display());
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
    println!("rendered {} frames, {} samples", result.frames, result.samples);
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
        .filter(|e| (e.manifest.id.starts_with("fx.") || e.manifest.id.starts_with("preset.")) && e.manifest.implementation.kind == "fragment")
        .map(|e| e.manifest.id.clone())
        .collect();
    std::fs::create_dir_all(dir)?;
    let (w, h, px) = export::contact_sheet(&effects, 160, 90, 8)?;
    let path = dir.join("effects_contact_sheet.png");
    flux_media::write_png_rgba(&path, w, h, &px)?;
    println!("contact sheet ({} effects, {}x{}) -> {}", effects.len(), w, h, path.display());
    Ok(())
}

fn cmd_render(project_path: &std::path::Path, dir: &std::path::Path) -> FluxResult<()> {
    let project = load_project(project_path)?;
    let result = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.to_path_buf(),
            project_name: project.name.clone(),
            codec: flux_media::EncodeCodec::H264,
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
    println!("rendered {} frames, {} samples", result.frames, result.samples);
    if let Some(v) = &result.video {
        println!("video: {}", v.display());
    }
    Ok(())
}

/// Atomic project save: write temp, flush, replace.
pub fn save_project(project: &Project, path: &std::path::Path) -> FluxResult<()> {
    let json = serde_json::to_string_pretty(project)
        .map_err(|e| FluxError::new(flux_domain::ErrorCode::Io, "persist", e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("flux.json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.flush()?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_project(path: &std::path::Path) -> FluxResult<Project> {
    let data = std::fs::read_to_string(path)?;
    let project: Project = serde_json::from_str(&data)
        .map_err(|e| FluxError::new(flux_domain::ErrorCode::ProjectCorrupt, "persist", e.to_string()))?;
    if project.schema_version > flux_domain::SCHEMA_VERSION {
        return Err(FluxError::new(
            flux_domain::ErrorCode::ProjectVersion,
            "persist",
            format!("project schema {} is newer than supported {}", project.schema_version, flux_domain::SCHEMA_VERSION),
        ));
    }
    Ok(project)
}

pub fn new_dispatcher(project: Project) -> Dispatcher {
    Dispatcher::new(project)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
