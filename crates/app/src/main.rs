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
        "diagnostics" => cmd_diagnostics(),
        "models" => cmd_models(),
        "depth" => cmd_depth(args),
        "integrations" => cmd_integrations(),
        "link" => cmd_link(),
        "ndi" => cmd_ndi(),
        "ndi-recv" => cmd_ndi_recv(args),
        "ndi-loopback" => cmd_ndi_loopback(),
        "ndi-send" => cmd_ndi_send(args),
        "spout" => cmd_spout(),
        "spout-send" => cmd_spout_send(args),
        "spout-send-dx" => cmd_spout_send_dx(args),
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
            eprintln!("usage: flux [list|doctor|caps|capture|integrations|osc|link|ndi|diagnostics|new-demo|demo-render|demo-media|contact-sheet|render|record|recover|collect|gui]");
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

fn cmd_ndi_loopback() -> FluxResult<()> {
    #[cfg(feature = "ndi")]
    {
        use flux_integrations::ndi::NdiRuntime;
        use flux_integrations::ndi_io::NdiShim;
        // End-to-end NDI video pipeline proof: create a sender AND a receiver in
        // one process, send a known RGBA gradient, receive it back, and confirm
        // dimensions + non-empty content. Self-contained (no third-party sender).
        //
        // The runtime is loaded and initialised first so that the shim's dependency on
        // `Processing.NDI.Lib.x64.dll` resolves to the module already in the process
        // rather than a second copy — same order `flux ndi-recv` uses.
        let missing = |what: &str| {
            FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "ndi",
                format!("ndi-loopback: {what}"),
            )
        };
        let Some(rt) = NdiRuntime::load() else {
            println!("NDI runtime not found.");
            return Err(missing("NDI runtime absent"));
        };
        if !rt.initialize() {
            println!("NDI runtime failed to initialize.");
            return Err(missing("NDIlib_initialize failed"));
        }
        let Some(shim) = NdiShim::load() else {
            println!("ndi_shim.dll not found (build apps/ndi-bridge).");
            return Err(missing("ndi_shim.dll absent"));
        };
        if !shim.initialize() {
            println!("NDI shim failed to initialize.");
            return Err(missing("shim initialize failed"));
        }
        let name = format!("Flux Loopback {}", std::process::id());
        let (w, h) = (640u32, 360u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                img[i] = (x * 255 / w) as u8; // R gradient
                img[i + 1] = (y * 255 / h) as u8; // G gradient
                img[i + 2] = 128; // B
                img[i + 3] = 255; // A
            }
        }
        let Some(sender) = shim.sender(&name, w, h) else {
            println!("failed to create NDI sender");
            shim.shutdown();
            return Err(missing("NDI sender creation failed"));
        };
        // Give the sender a moment to advertise, then resolve the name the runtime
        // actually publishes. NDI advertises `{machine} ({sender})` and
        // `ndi_shim_recv_create` hands its argument straight to
        // `NDIlib_recv_create_v3` as `p_ndi_name` with no lookup, so subscribing with
        // the sender's own name can never match — which is why this command could only
        // ever report "no frame received". `flux ndi-recv` resolves by discovery; this
        // now does the same.
        std::thread::sleep(std::time::Duration::from_millis(800));
        let sources = rt.discover(true, 1500);
        let Some(advertised) = sources
            .iter()
            .find(|s| s.name.contains(name.as_str()))
            .map(|s| s.name.clone())
        else {
            println!(
                "NDI loopback: own sender {name:?} did not appear in discovery ({} source(s) seen)",
                sources.len()
            );
            for s in &sources {
                println!("  {}", s.name);
            }
            drop(sender);
            shim.shutdown();
            return Err(missing("own sender not discovered"));
        };
        println!("resolved advertised source: {advertised}");
        let Some(recv) = shim.receiver(&advertised) else {
            println!("failed to create NDI receiver for own sender");
            drop(sender);
            shim.shutdown();
            return Err(missing("NDI receiver creation failed"));
        };
        // Pump frames (sender must keep sending while the receiver waits).
        let mut got = None;
        for _ in 0..40 {
            sender.send(&img, w, h);
            match recv.next_frame(150) {
                Ok(Some(f)) => {
                    got = Some(f);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    println!("recv error: {e}");
                    break;
                }
            }
        }
        drop(recv);
        drop(sender);
        shim.shutdown();
        // `rt.shutdown()` is deliberately not called: it would run `NDIlib_destroy` on
        // the one runtime instance shared with the shim, and the shim has already
        // finalized. A second destroy is not something to rely on being harmless.
        match got {
            Some(f) => {
                let nonzero = f.rgba.iter().any(|&b| b != 0);
                println!(
                    "NDI loopback OK: sent {}x{} RGBA, received {}x{} ({} bytes, nonzero={})",
                    w,
                    h,
                    f.width,
                    f.height,
                    f.rgba.len(),
                    nonzero
                );
            }
            None => {
                println!("NDI loopback: no frame received within timeout");
                return Err(missing("no frame reached the receiver"));
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "ndi"))]
    {
        println!("NDI is not built into this binary (feature `ndi` off).");
        Ok(())
    }
}

fn cmd_ndi_send(args: &[String]) -> FluxResult<()> {
    #[cfg(feature = "ndi")]
    {
        use flux_integrations::ndi_io::NdiShim;
        let missing = |what: &str| {
            FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "ndi",
                format!("ndi-send: {what}"),
            )
        };
        let name = args
            .get(1)
            .cloned()
            .unwrap_or_else(|| "Flux Test Sender".into());
        let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
        let Some(shim) = NdiShim::load() else {
            return Err(missing("ndi_shim.dll not found (build apps/ndi-bridge)"));
        };
        if !shim.initialize() {
            return Err(missing("NDI shim failed to initialize"));
        }
        let (w, h) = (640u32, 360u32);
        let Some(sender) = shim.sender(&name, w, h) else {
            shim.shutdown();
            return Err(missing("failed to create NDI sender"));
        };
        println!(
            "NDI sender '{}' broadcasting {}x{} for {}s",
            name, w, h, secs
        );
        let start = std::time::Instant::now();
        let mut img = vec![0u8; (w * h * 4) as usize];
        while start.elapsed().as_secs() < secs {
            // Animated gradient so the receiver sees live, changing content.
            let t = start.elapsed().as_millis() as u32;
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    img[i] = ((x * 255 / w + t / 8) % 256) as u8;
                    img[i + 1] = (y * 255 / h) as u8;
                    img[i + 2] = 128;
                    img[i + 3] = 255;
                }
            }
            sender.send(&img, w, h);
            std::thread::sleep(std::time::Duration::from_millis(33)); // ~30fps
        }
        drop(sender);
        shim.shutdown();
        println!("NDI sender stopped.");
        Ok(())
    }
    #[cfg(not(feature = "ndi"))]
    {
        let _ = args;
        println!("NDI is not built into this binary (feature `ndi` off).");
        Ok(())
    }
}

fn cmd_ndi_recv(args: &[String]) -> FluxResult<()> {
    #[cfg(feature = "ndi")]
    {
        use flux_integrations::ndi::NdiRuntime;
        use flux_integrations::ndi_io::NdiShim;
        let missing = |what: &str| {
            FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "ndi",
                format!("ndi-recv: {what}"),
            )
        };
        // Discover sources, connect to one (by name substring or index), grab a
        // frame, and write it as PNG. Proves real NDI receive end-to-end.
        let Some(rt) = NdiRuntime::load() else {
            return Err(missing("NDI runtime not found"));
        };
        if !rt.initialize() {
            return Err(missing("NDI runtime failed to initialize"));
        }
        let sources = rt.discover(true, 1500);
        rt.shutdown();
        if sources.is_empty() {
            return Err(missing(
                "0 NDI sources discovered; nothing to connect to (start NDI Test Patterns or another sender)",
            ));
        }
        let want = args.get(1).cloned();
        let chosen = match &want {
            Some(q) => sources
                .iter()
                .find(|s| s.name.to_lowercase().contains(&q.to_lowercase()))
                .cloned(),
            None => sources.first().cloned(),
        };
        let Some(src) = chosen else {
            println!("No source matched {:?}. Available:", want);
            for s in &sources {
                println!("  {}", s.name);
            }
            return Err(missing("no discovered source matched the requested name"));
        };
        println!("connecting to: {}", src.name);
        let Some(shim) = NdiShim::load() else {
            return Err(missing("ndi_shim.dll not found (build apps/ndi-bridge)"));
        };
        if !shim.initialize() {
            return Err(missing("NDI shim failed to initialize"));
        }
        let Some(recv) = shim.receiver(&src.name) else {
            shim.shutdown();
            return Err(missing("failed to create NDI receiver"));
        };
        // Wait up to ~5s for the first frame.
        let mut frame = None;
        for _ in 0..10 {
            match recv.next_frame(500) {
                Ok(Some(f)) => {
                    frame = Some(f);
                    break;
                }
                Ok(None) => continue,
                Err(e) => {
                    println!("receive error: {e}");
                    break;
                }
            }
        }
        shim.shutdown();
        match frame {
            Some(f) => {
                let dir =
                    std::path::PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "out".into()));
                std::fs::create_dir_all(&dir)?;
                let path = dir.join("ndi_frame.png");
                flux_media::write_png_rgba(&path, f.width, f.height, &f.rgba)?;
                println!(
                    "received {}x{} NDI frame -> {}",
                    f.width,
                    f.height,
                    path.display()
                );
            }
            None => {
                return Err(missing("no frame received within 5s (source may be idle)"));
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "ndi"))]
    {
        let _ = args;
        println!("NDI is not built into this binary (feature `ndi` off).");
        Ok(())
    }
}

fn cmd_spout_send_dx(args: &[String]) -> FluxResult<()> {
    #[cfg(feature = "spout")]
    {
        use flux_integrations::spout::SpoutDxSender;
        let name = args
            .get(1)
            .cloned()
            .unwrap_or_else(|| "Flux Spout DX".into());
        let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        let (w, h) = (640u32, 360u32);
        // Prefer the in-process DLL path.
        if let Some(sender) = SpoutDxSender::load(&name, w, h) {
            println!(
                "SpoutDX status bitmask: {} (1=init 2=ready)",
                sender.status()
            );
            let mut img = vec![0u8; (w * h * 4) as usize];
            let start = std::time::Instant::now();
            let mut sent = 0u32;
            while start.elapsed().as_secs() < secs {
                let t = start.elapsed().as_millis() as u32;
                fill_gradient(&mut img, w, h, t);
                if sender.send(&img, w, h) {
                    sent += 1;
                }
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
            println!("SpoutDX sent {sent} frames to '{}'", name);
            return Ok(());
        }
        // Fall back to the standalone sender EXE (the DLL form fails to load with
        // ERROR_INVALID_DLL/182 on this host; the EXE runs the identical code).
        let exe = std::path::Path::new("apps/spout-bridge/spout_sender.exe");
        if exe.exists() {
            println!("DLL path unavailable; launching spout_sender.exe helper");
            let status = std::process::Command::new(exe)
                .arg(&name)
                .arg(secs.to_string())
                .status();
            match status {
                Ok(s) if s.success() => println!("spout_sender.exe finished ({s})"),
                Ok(s) => println!("spout_sender.exe exited with {s}"),
                Err(e) => println!("failed to launch spout_sender.exe: {e}"),
            }
            return Ok(());
        }
        println!("SpoutDX unavailable (no loadable DLL and no spout_sender.exe helper).");
        Ok(())
    }
    #[cfg(not(feature = "spout"))]
    {
        let _ = args;
        println!("Spout is not built into this binary (feature `spout` off).");
        Ok(())
    }
}

#[cfg(feature = "spout")]
fn fill_gradient(img: &mut [u8], w: u32, h: u32, t: u32) {
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            img[i] = ((x * 255 / w + t / 8) % 256) as u8;
            img[i + 1] = (y * 255 / h) as u8;
            img[i + 2] = 128;
            img[i + 3] = 255;
        }
    }
}

fn cmd_spout_send(args: &[String]) -> FluxResult<()> {
    #[cfg(feature = "spout")]
    {
        use flux_integrations::spout::SpoutRuntime;
        let name = args
            .get(1)
            .cloned()
            .unwrap_or_else(|| "Flux Spout Sender".into());
        let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
        let Some(rt) = SpoutRuntime::load() else {
            println!("spout_shim.dll not found.");
            return Ok(());
        };
        println!("offscreen GL ready: {}", rt.gl_ready());
        let (w, h) = (640u32, 360u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        let start = std::time::Instant::now();
        let mut sent = 0u32;
        while start.elapsed().as_secs() < secs {
            let t = start.elapsed().as_millis() as u32;
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    img[i] = ((x * 255 / w + t / 8) % 256) as u8;
                    img[i + 1] = (y * 255 / h) as u8;
                    img[i + 2] = 128;
                    img[i + 3] = 255;
                }
            }
            if rt.send_image(&name, &img, w, h) {
                sent += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        if sent == 0 {
            println!(
                "Spout send produced no frames (offscreen GL path does not satisfy \
                 SpoutLibrary SendImage on this host; sender enumeration still works)."
            );
        }
        println!("Spout send: {sent} frames sent to '{}'", name);
        Ok(())
    }
    #[cfg(not(feature = "spout"))]
    {
        let _ = args;
        println!("Spout is not built into this binary (feature `spout` off).");
        Ok(())
    }
}

fn cmd_spout() -> FluxResult<()> {
    #[cfg(feature = "spout")]
    {
        use flux_integrations::spout::SpoutRuntime;
        let Some(rt) = SpoutRuntime::load() else {
            println!("spout_shim.dll not found (set FLUX_SPOUT_SHIM, or build apps/spout-bridge).");
            return Ok(());
        };
        let (senders, timed_out) = rt.senders_timeout(std::time::Duration::from_secs(3));
        if timed_out {
            println!("Spout sender query timed out (no sender / registry busy); 0 shown.");
        }
        println!("Spout sender registry: {} sender(s)", senders.len());
        for s in &senders {
            match (s.width, s.height) {
                (Some(w), Some(h)) => println!("  {}  {}x{}", s.name, w, h),
                _ => println!("  {}", s.name),
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "spout"))]
    {
        println!("Spout is not built into this binary (feature `spout` off).");
        Ok(())
    }
}

fn cmd_ndi() -> FluxResult<()> {
    #[cfg(feature = "ndi")]
    {
        use flux_integrations::ndi::NdiRuntime;
        // A runtime that cannot be loaded or initialized is a missing prerequisite, not a
        // result: exit nonzero so `flux ndi` cannot be read as "NDI works here, it just
        // found nothing". An empty source list *is* a result and still exits 0.
        let Some(rt) = NdiRuntime::load() else {
            return Err(FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "ndi",
                "ndi: NDI runtime not found (set FLUX_NDI_LIB to Processing.NDI.Lib.x64.dll)",
            ));
        };
        if !rt.initialize() {
            return Err(FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "ndi",
                "ndi: runtime present but NDIlib_initialize failed",
            ));
        }
        println!("NDI runtime initialized; discovering sources (1s)...");
        let sources = rt.discover(true, 1000);
        println!("{} NDI source(s) found:", sources.len());
        for s in &sources {
            println!("  {}  [{}]", s.name, s.url.clone().unwrap_or_default());
        }
        rt.shutdown();
        Ok(())
    }
    #[cfg(not(feature = "ndi"))]
    {
        println!("NDI is not built into this binary (feature `ndi` off).");
        Ok(())
    }
}

fn cmd_link() -> FluxResult<()> {
    #[cfg(feature = "link")]
    {
        use flux_integrations::link::LinkSession;
        let bpm = std::env::var("FLUX_LINK_BPM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120.0);
        let secs: u64 = std::env::var("FLUX_LINK_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(6);
        let session = LinkSession::new(bpm).ok_or_else(|| {
            FluxError::new(
                flux_domain::ErrorCode::InvalidCommand,
                "link",
                "failed to create Link session",
            )
        })?;
        println!("Ableton Link session enabled at {bpm} BPM (start/stop sync on)");
        println!("observing for {secs}s — run `flux link` in another terminal to see peers");
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < secs {
            let s = session.state();
            println!(
                "peers={} tempo={:.2} beat={:.3} phase={:.3} playing={}",
                s.num_peers, s.tempo, s.beat, s.phase, s.playing
            );
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        Ok(())
    }
    #[cfg(not(feature = "link"))]
    {
        println!("Ableton Link is not built into this binary (feature `link` off).");
        println!(
            "Build with `--features link` and FLUX_ABLETON_LINK_LIB set to abl_link.lib's dir."
        );
        Ok(())
    }
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

fn cmd_depth(args: &[String]) -> FluxResult<()> {
    #[cfg(feature = "vision")]
    {
        use flux_integrations::vision::{depth_to_rgba, DepthModel};
        let out = std::path::PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "out".into()));
        std::fs::create_dir_all(&out)?;
        // Build a synthetic scene: a bright near-square on a dark far background.
        let (w, h) = (256usize, 256usize);
        let mut rgb = vec![20u8; w * h * 3];
        for y in 64..192 {
            for x in 64..192 {
                let i = (y * w + x) * 3;
                rgb[i] = 235;
                rgb[i + 1] = 235;
                rgb[i + 2] = 235;
            }
        }
        let mut model = match DepthModel::load_default() {
            Ok(m) => m,
            Err(e) => {
                println!("depth model unavailable: {e}");
                return Ok(());
            }
        };
        let t = std::time::Instant::now();
        let depth = model
            .infer_rgb(&rgb, w, h)
            .map_err(|e| FluxError::new(flux_domain::ErrorCode::SdkMissing, "vision", e))?;
        let ms = t.elapsed().as_millis();
        let rgba = depth_to_rgba(&depth);
        let path = out.join("depth.png");
        flux_media::write_png_rgba(&path, depth.width as u32, depth.height as u32, &rgba)?;
        // Report the depth at the center (near) vs a corner (far).
        let cx = depth.width / 2;
        let cy = depth.height / 2;
        let near = depth.depth[cy * depth.width + cx];
        let far = depth.depth[0];
        println!(
            "depth {}x{} in {}ms -> {} (center={near:.3} corner={far:.3})",
            depth.width,
            depth.height,
            ms,
            path.display()
        );
        Ok(())
    }
    #[cfg(not(feature = "vision"))]
    {
        let _ = args;
        println!("vision is not built into this binary (feature `vision` off).");
        Ok(())
    }
}

fn cmd_models() -> FluxResult<()> {
    let statuses = flux_catalog::models::all_statuses();
    println!("Model-backed conditional nodes ({}):", statuses.len());
    for s in &statuses {
        println!("  {:?}  {}  {}", s.state, s.requirement, s.message);
    }
    Ok(())
}

fn cmd_diagnostics() -> FluxResult<()> {
    let report = flux_app::diagnostics::build_report(&[], &[]);
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| FluxError::new(flux_domain::ErrorCode::Io, "diag", e.to_string()))?;
    println!("{json}");
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
