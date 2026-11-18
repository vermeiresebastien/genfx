//! Flux application entry point. Subcommands:
//!   flux list                     list catalog effects/generators
//!   flux doctor                   print toolchain/GPU/media capabilities
//!   flux new-demo <gen> <dir>     write a demo project file
//!   flux render <project> <dir>   export PNG+WAV+MP4 for a project file
//!   flux demo-render <gen> <dir>  build a demo and export it in one step
//!   flux demo-media <video> <dir> import a video and export it
//!   flux contact-sheet <dir>      render an effects contact sheet
//!   flux gui [project]            launch the native editor (not in this build)

use flux_app::{export, live_audio, samples};
use flux_catalog::build_catalog;
use flux_domain::{Dispatcher, FluxError, FluxResult, Project};
use std::path::PathBuf;

/// The single render audit line for every CLI export path. Takes the whole
/// `ExportResult` (not five loose fields) so a mutation that replaces one of
/// the fields at a call site with a constant is caught: the caller no longer
/// sees the fields, and this function is called with a synthetic result in the
/// tests below. The `frames_dir` is named exactly when this run wrote PNG
/// frames, taken from `ExportResult::png_frames_written` — NOT from
/// `frames_dir.exists()`. The old `exists()` proxy was measurably wrong
/// (CR3 defect 1): `frames_dir` is a deterministic reused name, so a later
/// `--codec h264` run into the same out dir printed a path it wrote zero PNGs
/// into. The field is the sole authority for this decision.
fn render_audit_line(result: &export::ExportResult) -> String {
    if result.png_frames_written {
        format!(
            "rendered {} frames, {} samples; frames in {}; audio: {:?} ({} windows)",
            result.frames,
            result.samples,
            result.frames_dir.display(),
            result.feature_source,
            result.audio_windows
        )
    } else {
        format!(
            "rendered {} frames, {} samples; audio: {:?} ({} windows)",
            result.frames, result.samples, result.feature_source, result.audio_windows
        )
    }
}

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
        "capture" => cmd_capture(args),
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

/// A subcommand whose adapter is compiled out did not run, so it is not a result.
/// `crates/app` sets `default = []`, so in the binary CI builds every one of these
/// branches is the only path; exiting 0 there made "this binary cannot" indistinguishable
/// from "this worked", which is the defect D023 closed for `flux ndi` and D027 deferred
/// here because it covered all seven of them at once.
fn feature_off(subcommand: &str, feature: &str) -> FluxError {
    FluxError::new(
        flux_domain::ErrorCode::BlockedExternal,
        feature,
        format!("flux {subcommand}: not built into this binary (feature `{feature}` off)"),
    )
    .with_recovery(format!("cargo build -p flux-app --features {feature}"))
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
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        },
        Some(Box::new(|i, n| {
            if i == n || i % 30 == 0 {
                println!("frame {i}/{n}");
            }
        })),
    )?;
    println!("{}", render_audit_line(&result));
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
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        },
        Some(Box::new(|i, n| {
            if i == n {
                println!("frame {i}/{n}");
            }
        })),
    )?;
    println!("{}", render_audit_line(&result));
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
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        },
        Some(Box::new(|i, n| {
            if i == n || i % 60 == 0 {
                println!("frame {i}/{n}");
            }
        })),
    )?;
    println!("{}", render_audit_line(&result));
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
        // one process, render two frames of a deterministic project, send them and
        // confirm each comes back as the frame that was sent. Self-contained.
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
        // Two frames of a deterministic demo project, rendered after the sender is
        // created. This used to be a hand-written gradient baked in right here, which is
        // what let `AT060`'s blocker say that a Flux render is never what is sent — the
        // sentence D049 closed.
        let project = samples::demo_generator_project("gen.plasma", &["fx.bloom"], 4);
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
        // Render both frames now, from one renderer, so one wgpu device and one
        // catalog build back the pair this command compares. A device this host
        // cannot open is a missing prerequisite, so that error propagates.
        let rendered = (|| -> FluxResult<(Vec<u8>, Vec<u8>)> {
            let mut renderer = export::PreviewRenderer::new()?;
            let a = render_rgba_at(&mut renderer, &project, 1_000, w, h)?;
            let b = render_rgba_at(&mut renderer, &project, 2_000, w, h)?;
            Ok((a, b))
        })();
        let (sent0, sent1) = match rendered {
            Ok(pair) => pair,
            Err(e) => {
                drop(recv);
                drop(sender);
                shim.shutdown();
                return Err(e);
            }
        };
        // The point of two frames is that they are not the same frame. If the fixture
        // rendered identically, a receiver that echoed one buffer would pass.
        if sent0 == sent1 {
            drop(recv);
            drop(sender);
            shim.shutdown();
            return Err(missing(
                "the fixture renders identically at t=1s and t=2s, so no byte match here would prove motion",
            ));
        }
        let differing = sent0
            .iter()
            .zip(sent1.iter())
            .filter(|(a, b)| a != b)
            .count();
        // Pump frames (sender must keep sending while the receiver waits), and keep
        // pumping until the receiver hands back the exact bytes that were sent.
        let pump0 = pump_until_exact(&sender, &recv, &sent0, w, h, 60);
        let pump1 = pump_until_exact(&sender, &recv, &sent1, w, h, 60);
        drop(recv);
        drop(sender);
        shim.shutdown();
        // `rt.shutdown()` is deliberately not called: it would run `NDIlib_destroy` on
        // the one runtime instance shared with the shim, and the shim has already
        // finalized. A second destroy is not something to rely on being harmless.
        let (o0, o1) = match (pump0, pump1) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                println!("recv error: {e}");
                return Err(missing("NDI receive failed while pumping the render"));
            }
        };
        // Four distances: each received frame against its own render and against the
        // other one. Exactness is what the transport was asked for and it is tried
        // first; the cross terms say the frame that came back is the frame that went
        // up, and they are the part of the assertion no tolerance weakens.
        let own0 = o0.best.as_ref().map(|g| rgba_diff(g, &sent0));
        let own1 = o1.best.as_ref().map(|g| rgba_diff(g, &sent1));
        let cross0 = o0.best.as_ref().map(|g| rgba_diff(g, &sent1));
        let cross1 = o1.best.as_ref().map(|g| rgba_diff(g, &sent0));
        let report = |o: &PumpOutcome,
                      at: &str,
                      own: Option<FrameDiff>,
                      cross: Option<FrameDiff>| match (o.frames, own, cross) {
            (0, _, _) => println!("NDI loopback: at {at} the receiver returned no frame at all"),
            (_, Some(own), Some(cross)) => println!(
                "NDI loopback: at {at} {} frame(s) back, closest {}x{}, {} of them different from \
                 the first, best per-byte delta {} ({} with R/B swapped); vs its own render [{}]; \
                 vs the other render [{}]",
                o.frames,
                o.dims.0,
                o.dims.1,
                o.variants,
                o.best_delta,
                o.best_swap_delta,
                own.describe(),
                cross.describe()
            ),
            _ => println!("NDI loopback: at {at} no frame of the sent size came back"),
        };
        report(&o0, "t=1s", own0, cross0);
        report(&o1, "t=2s", own1, cross1);
        if o0.matched && o1.matched {
            println!(
                "NDI loopback OK: Flux render of gen.plasma+fx.bloom at t=1s and t=2s, {}x{} RGBA \
                 ({} B/frame), both received byte-exact after {} and {} frame(s); the two renders \
                 differ in {differing} bytes",
                w,
                h,
                w * h * 4,
                o0.frames,
                o1.frames
            );
            return Ok(());
        }
        // Byte-exact is not available on this transport, so the proof is carried by three
        // assertions, none of which is "some frames arrived".
        let dims_kept = o0.dims == (w, h) && o1.dims == (w, h);
        let pair = |own: Option<FrameDiff>, cross: Option<FrameDiff>| match (own, cross) {
            (Some(own), Some(cross)) => is_this_frame(&own, &cross),
            _ => false,
        };
        let carries_this_frame = pair(own0, cross0) && pair(own1, cross1);
        let received_differ = match (&o0.best, &o1.best) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        };
        // The one absolute the measurement does license: whatever NDI does to the colour
        // of a frame, it leaves the alpha plane alone. A transport that mangled alpha is
        // not this transport, and no comparative test would notice.
        let alpha_exact = [(&o0, &sent0), (&o1, &sent1)].iter().all(|(o, sent)| {
            o.best
                .as_ref()
                .map(|got| rgba_alpha_delta(got, sent) == 0)
                .unwrap_or(false)
        });
        if !dims_kept || !carries_this_frame || !received_differ || !alpha_exact {
            return Err(FluxError::new(
                flux_domain::ErrorCode::Io,
                "ndi",
                format!(
                    "ndi-loopback: the render did not survive the round trip (dimensions kept: \
                     {dims_kept}, each received frame closer to its own render than to the other: \
                     {carries_this_frame}, the two received frames differ: {received_differ}, the \
                     alpha plane unchanged: {alpha_exact}; the two renders differ in {differing} \
                     of {} bytes)",
                    sent0.len()
                ),
            )
            .with_recovery("check `flux ndi` for a peer sender advertising this machine's name"));
        }
        println!(
            "NDI loopback OK with a named loss: Flux render of gen.plasma+fx.bloom at t=1s and \
             t=2s, {}x{} RGBA ({} B/frame). Not byte-exact: the residual against its own render \
             is [{}], against the other render [{}]; alpha unchanged, dimensions unchanged, the \
             two received frames differ, and the transport returned {} frame(s), {} of them \
             differing from the frame immediately before. The two renders differ in {differing} \
             of {} bytes",
            w,
            h,
            w * h * 4,
            own0.map(|d| d.describe()).unwrap_or_else(|| "-".into()),
            cross0.map(|d| d.describe()).unwrap_or_else(|| "-".into()),
            o0.frames + o1.frames,
            o0.variants + o1.variants,
            sent0.len()
        );
        Ok(())
    }

    #[cfg(not(feature = "ndi"))]
    {
        Err(feature_off("ndi-loopback", "ndi"))
    }
}

fn cmd_ndi_send(args: &[String]) -> FluxResult<()> {
    #[cfg(feature = "ndi")]
    {
        use flux_integrations::ndi::NdiRuntime;
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
        // The runtime is loaded and initialised before the shim, in the order
        // `cmd_ndi_loopback` uses. `NdiShim::load` can be handed a relative candidate
        // (`bases()` ends in `.`), and a relative path makes the loader's
        // `LOAD_WITH_ALTERED_SEARCH_PATH` promise undefined, so the shim's import of
        // `Processing.NDI.Lib.x64.dll` only resolves against the module already in the
        // process. Without this line the sender cannot start on a host that has NDI.
        let Some(_rt) = NdiRuntime::load() else {
            println!("NDI runtime not found.");
            return Err(missing("NDI runtime absent"));
        };
        if !_rt.initialize() {
            println!("NDI runtime failed to initialize.");
            return Err(missing("NDIlib_initialize failed"));
        }
        let Some(shim) = NdiShim::load() else {
            return Err(missing("ndi_shim.dll not found (build apps/ndi-bridge)"));
        };
        if !shim.initialize() {
            return Err(missing("NDI shim failed to initialize"));
        }
        // What goes out is a Flux render. With no project argument that render is the
        // deterministic demo fixture, so the command still works with nothing to open;
        // with a path it broadcasts that document at its own canvas size.
        let project_path = args.get(3).cloned();
        let (project, source) = match &project_path {
            Some(p) => (
                load_project(std::path::Path::new(p))?,
                format!("project {p}"),
            ),
            None => (
                samples::demo_generator_project("gen.plasma", &["fx.bloom"], 4),
                "demo fixture gen.plasma + fx.bloom".into(),
            ),
        };
        let (w, h) = match project_path {
            Some(_) => (project.settings.size.width, project.settings.size.height),
            None => (640u32, 360u32),
        };
        let Some(sender) = shim.sender(&name, w, h) else {
            shim.shutdown();
            return Err(missing("failed to create NDI sender"));
        };
        println!(
            "NDI sender '{}' broadcasting {}x{} of {source} for {}s",
            name, w, h, secs
        );
        let start = std::time::Instant::now();
        let mut renderer = match export::PreviewRenderer::new() {
            Ok(r) => r,
            Err(e) => {
                drop(sender);
                shim.shutdown();
                return Err(e);
            }
        };
        let mut frames = 0u64;
        while start.elapsed().as_secs() < secs {
            let ms = start.elapsed().as_millis() as i64;
            let img = match render_rgba_at(&mut renderer, &project, ms, w, h) {
                Ok(f) => f,
                Err(e) => {
                    drop(sender);
                    shim.shutdown();
                    return Err(e);
                }
            };
            if !sender.send(&img, w, h) {
                drop(sender);
                shim.shutdown();
                return Err(missing("NdiSender::send refused the rendered frame"));
            }
            frames += 1;
            std::thread::sleep(std::time::Duration::from_millis(33)); // ~30fps
        }
        drop(sender);
        shim.shutdown();
        let elapsed = start.elapsed().as_secs_f64();
        println!("NDI sender stopped after {frames} rendered frames in {elapsed:.1}s.");
        Ok(())
    }
    #[cfg(not(feature = "ndi"))]
    {
        let _ = args;
        Err(feature_off("ndi-send", "ndi"))
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
        Err(feature_off("ndi-recv", "ndi"))
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
        // Resolved through the sidecar layout: an installed app has no `apps/`
        // tree, so a CWD-relative path could only ever work in a checkout.
        let helper = flux_platform::layout::locate(&[
            "spout_sender.exe",
            "apps/spout-bridge/spout_sender.exe",
        ]);
        if let Some(exe) = helper.as_deref() {
            println!("DLL path unavailable; launching spout_sender.exe helper");
            let status = std::process::Command::new(exe)
                .arg(&name)
                .arg(secs.to_string())
                .status();
            match status {
                Ok(s) if s.success() => println!(
                    "spout_sender.exe finished ({s}); the helper's own exit code is not \
                     proof that a receiver was connected"
                ),
                Ok(s) => {
                    return Err(FluxError::new(
                        flux_domain::ErrorCode::SdkMissing,
                        "spout",
                        format!("spout-send-dx: spout_sender.exe exited with {s}"),
                    )
                    .with_recovery("rebuild apps/spout-bridge"))
                }
                Err(e) => {
                    return Err(FluxError::new(
                        flux_domain::ErrorCode::SdkMissing,
                        "spout",
                        format!("spout-send-dx: failed to launch spout_sender.exe: {e}"),
                    )
                    .with_recovery("rebuild apps/spout-bridge"))
                }
            }
            return Ok(());
        }
        return Err(FluxError::new(
            flux_domain::ErrorCode::SdkMissing,
            "spout",
            "spout-send-dx: no loadable spout_shim.dll and no spout_sender.exe helper",
        )
        .with_recovery("build apps/spout-bridge, or set FLUX_SPOUT_SHIM"));
    }
    #[cfg(not(feature = "spout"))]
    {
        let _ = args;
        Err(feature_off("spout-send-dx", "spout"))
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
            return Err(FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "spout",
                "spout-send: spout_shim.dll not found (set FLUX_SPOUT_SHIM, or build apps/spout-bridge)",
            ));
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
            return Err(FluxError::new(
                flux_domain::ErrorCode::BlockedExternal,
                "spout",
                "spout-send: SpoutLibrary accepted 0 of the frames offered (the offscreen GL \
                 path does not satisfy SendImage on this host; sender enumeration still works)",
            )
            .with_recovery("use `flux spout-send-dx`, which sends through the D3D11 sender"));
        }
        println!("Spout send: {sent} frames sent to '{}'", name);
        Ok(())
    }
    #[cfg(not(feature = "spout"))]
    {
        let _ = args;
        Err(feature_off("spout-send", "spout"))
    }
}

fn cmd_spout() -> FluxResult<()> {
    #[cfg(feature = "spout")]
    {
        use flux_integrations::spout::SpoutRuntime;
        // Same rule as `flux ndi` (D023): a shim that cannot be loaded is a missing
        // prerequisite, not a result. An empty registry is a result and still exits 0.
        let Some(rt) = SpoutRuntime::load() else {
            return Err(FluxError::new(
                flux_domain::ErrorCode::SdkMissing,
                "spout",
                "spout: spout_shim.dll not found (set FLUX_SPOUT_SHIM, or build apps/spout-bridge)",
            ));
        };
        let (senders, timed_out) = rt.senders_timeout(std::time::Duration::from_secs(3));
        println!("Spout sender registry: {} sender(s)", senders.len());
        for s in &senders {
            match (s.width, s.height) {
                (Some(w), Some(h)) => println!("  {}  {}x{}", s.name, w, h),
                _ => println!("  {}", s.name),
            }
        }
        // An empty registry is an answer and still exits 0 (D023); a query that gave up
        // before answering is not one, and the count printed above says nothing.
        if timed_out {
            return Err(FluxError::new(
                flux_domain::ErrorCode::BlockedExternal,
                "spout",
                "spout: the sender query timed out after 3s, so the count above is not an \
                 answer about the registry",
            )
            .with_recovery("retry once no sender is mid-registration"));
        }
        Ok(())
    }
    #[cfg(not(feature = "spout"))]
    {
        Err(feature_off("spout", "spout"))
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
        Err(feature_off("ndi", "ndi"))
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
        Err(feature_off("link", "link").with_recovery(
            "cargo build -p flux-app --features link, with FLUX_ABLETON_LINK_LIB set to abl_link.lib's directory",
        ))
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

/// Live audio capture. `flux capture devices|start|status|stop`.
///
/// Every subcommand exits 1 when the build has no `live-audio`, rather than
/// printing an empty device list: the apparatus is what is missing, and an
/// empty list would look like a quiet machine (D036). `flux caps` keeps
/// reporting the always-compiled capability set.
fn cmd_capture(args: &[String]) -> FluxResult<()> {
    let sub = args.get(1).map(String::as_str).unwrap_or("devices");
    require_live_audio()?;
    match sub {
        "devices" => capture_devices(args),
        "start" => capture_start(args),
        "status" => capture_status(args),
        "stop" => capture_stop(args),
        other => Err(capture_bad(format!(
            "unknown capture subcommand {other:?}; use devices|start|status|stop"
        ))),
    }
}

fn require_live_audio() -> FluxResult<()> {
    if live_audio::LIVE_AUDIO_COMPILED {
        Ok(())
    } else {
        Err(FluxError::new(
            flux_domain::ErrorCode::SdkMissing,
            "capture",
            "live audio capture is not in this build: rebuild with --features live-audio \
             (Windows only); `flux caps` still reports the capability set",
        ))
    }
}

/// A usage or configuration fault on the capture path: exit 1 with a message
/// that says what was wrong, never a printed warning followed by exit 0.
fn capture_bad(message: impl Into<String>) -> FluxError {
    FluxError::new(flux_domain::ErrorCode::InvalidCommand, "capture", message)
}

fn capture_io(message: impl Into<String>) -> FluxError {
    let e = message.into();
    FluxError::new(flux_domain::ErrorCode::Io, "capture", e)
}

/// `--flag value` out of a capture argument list.
fn opt(args: &[String], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).cloned()
}

fn wants_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

/// The run's configuration: the machine's environment first, then the flags, so
/// an explicit `--tap`, `--device` or `--input-db` always beats what the shell
/// says. A value that will not parse is kept as a complaint and reported,
/// because silently falling back to a default is how an operator ends up
/// believing the wrong thing about a take.
fn capture_config(args: &[String]) -> FluxResult<(live_audio::LiveAudioConfig, Vec<String>)> {
    let (mut cfg, mut problems) = live_audio::LiveAudioConfig::from_env();
    if let Some(raw) = opt(args, "--tap") {
        match live_audio::LiveMode::parse(&raw) {
            Some(mode) => cfg.mode = mode,
            None => problems.push(format!("--tap {raw:?} is not one of loopback|mic")),
        }
    }
    if let Some(id) = opt(args, "--device") {
        cfg.device_id = Some(id);
    }
    if let Some(raw) = opt(args, "--input-db") {
        match live_audio::parse_db_input(&raw) {
            Some(v) => cfg.input_db = v,
            None => problems.push(format!(
                "--input-db {raw:?} is not a dB value in -24..=24; keeping {:+.1} dB",
                cfg.input_db
            )),
        }
    }
    Ok((cfg, problems))
}

fn capture_serialize<T: serde::Serialize>(value: &T) -> FluxResult<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| capture_io(format!("capture report did not serialize: {e}")))
}

fn capture_devices(args: &[String]) -> FluxResult<()> {
    let (cfg, problems) = capture_config(args)?;
    let list =
        live_audio::list_endpoints().map_err(|e| FluxError::new(e.code, "capture", e.message))?;
    if wants_json(args) {
        let report = serde_json::json!({
            "compiled": live_audio::LIVE_AUDIO_COMPILED,
            "requestedTap": cfg.mode.tap().map(|t| t.as_str().to_string()),
            "deviceId": cfg.device_id,
            "configProblems": problems,
            "endpoints": list,
        });
        println!("{}", capture_serialize(&report)?);
        return Ok(());
    }
    println!("live-audio: compiled");
    println!("endpoints: {}", list.len());
    for e in &list {
        println!(
            "  [{}] {}{}",
            e.tap.as_str(),
            e.name,
            if e.is_default { "  (default)" } else { "" }
        );
        println!("      id={}", e.id);
    }
    for p in &problems {
        println!("note: {p}");
    }
    if list.is_empty() {
        println!(
            "note: no active audio endpoint was enumerated. That is the apparatus answering \
             with nothing, not a build fault."
        );
    }
    Ok(())
}

fn capture_start(args: &[String]) -> FluxResult<()> {
    let (cfg, problems) = capture_config(args)?;
    if !problems.is_empty() {
        return Err(capture_bad(format!(
            "configuration errors:\n  {}",
            problems.join("\n  ")
        )));
    }
    if cfg.mode == live_audio::LiveMode::Off {
        return Err(capture_bad(
            "`capture start` needs --tap loopback|mic (or FLUX_LIVE_AUDIO=loopback|mic): \
             starting in `off` would run a capture that deliberately hears nothing",
        ));
    }
    let secs: u32 = match opt(args, "--secs") {
        None => 3,
        Some(raw) => raw.parse().map_err(|_| {
            capture_bad(format!(
                "--secs {raw:?} is not an integer number of seconds"
            ))
        })?,
    };
    if !(1..=600).contains(&secs) {
        return Err(capture_bad(format!(
            "--secs {secs} is outside 1..=600; a longer run belongs to the GUI, not a CLI capture"
        )));
    }
    let (snapshot, span) = live_audio::measure(&cfg, secs)?;
    report_capture(
        wants_json(args),
        secs,
        cfg.ring_ms,
        &problems,
        &snapshot,
        span,
    )
}

/// Print the capture report and decide the exit status. Kept separate from
/// `capture_start` so a test can drive both the `--json` and human branches
/// with the same synthetic zero-frame snapshot; without this seam, the two
/// `validate_capture_snapshot_rejects_*` helper tests would pass even if
/// production stopped calling the validator at all. Both branches now share
/// one `outcome` computed once, so no branch-local `return Ok(())` can
/// bypass it.
fn report_capture(
    as_json: bool,
    secs: u32,
    ring_ms: u32,
    problems: &[String],
    snapshot: &live_audio::LiveAudioSnapshot,
    span: std::time::Duration,
) -> FluxResult<()> {
    let outcome = validate_capture_snapshot(snapshot);
    if as_json {
        let mut json = snapshot.to_json();
        if let Some(obj) = json.as_object_mut() {
            obj.insert(
                "measuredFramesPerSecond".into(),
                serde_json::json!(snapshot.measured_rate(span)),
            );
            obj.insert("requestedSeconds".into(), serde_json::json!(secs));
            obj.insert("requestedRingMs".into(), serde_json::json!(ring_ms));
            obj.insert("configProblems".into(), serde_json::json!(problems));
        }
        println!("{}", capture_serialize(&json)?);
        return outcome;
    }
    let rate = snapshot.measured_rate(span);
    println!("tap={}", snapshot.tap);
    println!(
        "endpoint={} id={}",
        snapshot.endpoint_name, snapshot.endpoint_id
    );
    println!(
        "wire: {} Hz, {} ch, {} bit, tag {} ({}), extensible={}",
        snapshot.sample_rate,
        snapshot.channels,
        snapshot.bits_per_sample,
        snapshot.format_tag,
        if snapshot.float_wire {
            "float32"
        } else {
            "pcm16"
        },
        snapshot.extensible
    );
    println!(
        "buffer={} ms  device period default={:.1} ms min={:.1} ms",
        snapshot.buffer_ms, snapshot.period_default_ms, snapshot.period_min_ms
    );
    println!(
        "ring: asked {} ms, holds {:.1} ms at this rate",
        snapshot.ring_ms, snapshot.ring_ms_effective
    );
    println!(
        "input trim: {:+.1} dB (explicit, no auto-gain)",
        snapshot.input_db
    );
    println!("ran {} ms", snapshot.elapsed_ms);
    println!(
        "packets={} frames={} rate={:.1} frames/s silent={}",
        snapshot.packets, snapshot.frames, rate, snapshot.silent_frames
    );
    println!(
        "windows={} onsets={} overflowEvents={}",
        snapshot.windows, snapshot.onsets, snapshot.overflow_events
    );
    println!(
        "level: peak={:.4} rms={:.4} bass={:.4} mid={:.4} treble={:.4} centroid={:.0} Hz",
        snapshot.peak,
        snapshot.rms,
        snapshot.bands[0],
        snapshot.bands[2],
        snapshot.bands[4],
        snapshot.centroid_hz
    );
    println!(
        "beat: onset={} tempoBpm={:.1} confidence={:.2} beatPhase={:.3}",
        snapshot.onset, snapshot.tempo_bpm, snapshot.tempo_confidence, snapshot.beat_phase
    );
    println!("valid={} stalledMs={}", snapshot.valid, snapshot.stalled_ms);
    for p in problems {
        println!("note: {p}");
    }
    if let Some(e) = &snapshot.error {
        println!("error: {e}");
    }
    outcome
}

/// Validate that a capture snapshot represents a successful measurement.
/// Returns an error if the snapshot has an error, produced zero frames, or is invalid.
fn validate_capture_snapshot(snapshot: &live_audio::LiveAudioSnapshot) -> FluxResult<()> {
    if let Some(e) = &snapshot.error {
        return Err(capture_bad(format!("capture failed: {e}")));
    }
    if snapshot.frames == 0 {
        return Err(capture_bad(
            "capture produced zero frames; the device may be disabled or unavailable",
        ));
    }
    if !snapshot.valid {
        return Err(capture_bad(
            "capture snapshot is not valid; measurement failed",
        ));
    }
    Ok(())
}

fn capture_status(args: &[String]) -> FluxResult<()> {
    match live_audio::read_status_file() {
        None => {
            if wants_json(args) {
                println!("null");
            } else {
                println!("status: no live capture has reported one");
            }
            Ok(())
        }
        Some(v) => {
            if wants_json(args) {
                println!("{}", capture_serialize(&v)?);
                return Ok(());
            }
            // The sidecar a running bridge writes is `{pid, stopRequested,
            // status:{…}}`, so the reading has to be taken from the nested
            // object; the top level is the control channel.
            let top = |k: &str| v.get(k).cloned().unwrap_or(serde_json::Value::Null);
            let st = v.get("status").cloned().unwrap_or(serde_json::Value::Null);
            let g = |k: &str| st.get(k).cloned().unwrap_or(serde_json::Value::Null);
            let b = |k: &str| g(k).as_bool().unwrap_or(false);
            let u = |k: &str| g(k).as_u64().unwrap_or(0);
            let f = |k: &str| g(k).as_f64().unwrap_or(0.0);
            let s = |k: &str| g(k).as_str().unwrap_or("").to_string();
            println!("status file: {}", live_audio::status_file().display());
            println!(
                "pid={} stopRequested={} compiled={} running={} tap={}",
                top("pid"),
                top("stopRequested").as_bool().unwrap_or(false),
                b("compiled"),
                b("running"),
                s("tap")
            );
            println!("endpoint={} id={}", s("endpointName"), s("endpointId"));
            println!(
                "wire: {} Hz, {} ch, {} bit, tag {} ({}), extensible={}",
                u("sampleRate"),
                u("channels"),
                u("bitsPerSample"),
                u("formatTag"),
                if b("floatWire") { "float32" } else { "pcm16" },
                b("extensible")
            );
            println!(
                "buffer={} ms ring={} ms (holds {:.1} ms) trim={:+.1} dB",
                u("bufferMs"),
                u("ringMs"),
                f("ringMsEffective"),
                f("inputDb")
            );
            println!(
                "rate={:.1} windows={} onsets={} packets={} frames={} silent={}",
                f("measuredFramesPerSecond"),
                u("windows"),
                u("onsets"),
                u("packets"),
                u("frames"),
                u("silentFrames")
            );
            println!(
                "level: peak={:.4} rms={:.4} bass={:.4} mid={:.4} treble={:.4} centroid={:.0} Hz",
                f("peak"),
                f("rms"),
                g("bands").get(0).and_then(|v| v.as_f64()).unwrap_or(0.0),
                g("bands").get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
                g("bands").get(4).and_then(|v| v.as_f64()).unwrap_or(0.0),
                f("centroidHz")
            );
            println!(
                "beat: onset={} tempoBpm={:.1} confidence={:.2} beatPhase={:.3}",
                b("onset"),
                f("tempoBpm"),
                f("tempoConfidence"),
                f("beatPhase")
            );
            println!(
                "valid={} stalledMs={} elapsedMs={} overflowEvents={}",
                b("valid"),
                u("stalledMs"),
                u("elapsedMs"),
                u("overflowEvents")
            );
            if let Some(e) = g("error").as_str() {
                println!("error: {e}");
            }
            Ok(())
        }
    }
}

fn capture_stop(args: &[String]) -> FluxResult<()> {
    let landed = live_audio::request_external_stop();
    if wants_json(args) {
        println!(
            "{}",
            capture_serialize(&serde_json::json!({"stopRequested": landed}))?
        );
    } else if landed {
        println!("stop requested: true");
        println!(
            "note: the running capture stops its stream and ends its wait within \
             {} ms; `flux capture status` keeps the last reading it published.",
            live_audio::STOP_CHECK_INTERVAL_MS
        );
    }
    if landed {
        Ok(())
    } else {
        Err(capture_bad(
            "no live capture status file exists, so there is nothing to stop: \
             `flux capture start` is its own process and writes that file while it runs",
        ))
    }
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
                return Err(FluxError::new(
                    flux_domain::ErrorCode::SdkMissing,
                    "vision",
                    format!("depth: model unavailable: {e}"),
                )
                .with_recovery(
                    "drop model.onnx + model.json into FLUX_MODELS_DIR, or set FLUX_MODEL_<ID>",
                ));
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
        Err(feature_off("depth", "vision"))
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
    // A worker that could not be created is a missing prerequisite, not a report:
    // printing `{"error":…}` and exiting 0 made "no FFmpeg here" readable as
    // "here are the codecs" (the same defect D023 closed for `flux ndi`).
    let w = MediaWorker::new()?;
    let c = w.capabilities();
    println!(
        "{{\"h264\":{},\"hevc\":{},\"prores\":{},\"aac\":{},\"gif\":{},\"png\":{},\"wav\":{}}}",
        c.h264, c.hevc, c.prores, c.aac, c.gif, c.png, c.wav
    );
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

/// One packed RGBA8 frame of `project` at `ms` milliseconds, at exactly the shape
/// `NdiSender::send` asks for. `send` answers a bare `false` when the slice is not
/// `w*h*4`, so a short readback would otherwise be an invisible no-op; the length is
/// checked here, where the number that came back can be named.
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
fn render_rgba_at(
    renderer: &mut export::PreviewRenderer,
    project: &Project,
    ms: i64,
    width: u32,
    height: u32,
) -> FluxResult<Vec<u8>> {
    // A literal denominator cannot fail `new`'s only check; if ZERO were ever
    // taken, both frames would render identically and the caller says so.
    let time = flux_domain::RationalTime::new(ms, 1_000).unwrap_or(flux_domain::RationalTime::ZERO);
    let px = renderer.render(project, time, width, height)?;
    let expected = (width as usize) * (height as usize) * 4;
    if px.len() != expected {
        return Err(FluxError::new(
            flux_domain::ErrorCode::GpuLimit,
            "ndi",
            format!(
                "render at {ms} ms returned {} bytes for a {width}x{height} frame, not the \
                 packed RGBA8 {expected} bytes the sender requires",
                px.len()
            ),
        ));
    }
    Ok(px)
}

/// Largest per-byte difference between two packed RGBA frames. `usize::MAX` when the
/// two are not the same length, because a frame that arrived at the wrong size is not
/// "slightly off".
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
fn max_rgba_delta(got: &[u8], expect: &[u8]) -> usize {
    if got.len() != expect.len() {
        return usize::MAX;
    }
    got.iter()
        .zip(expect.iter())
        .map(|(a, b)| (*a as usize).abs_diff(*b as usize))
        .max()
        .unwrap_or(0)
}

/// The same delta after exchanging R and B in `got`. The shim sends and requests an
/// RGBA fourcc, so a byte-exact match *should* be available; if it is not, this says
/// whether the mismatch is a channel order rather than a corrupt transport, and turns
/// the failure into a named finding instead of a loosened assertion.
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
fn max_rgba_delta_rb_swapped(got: &[u8], expect: &[u8]) -> usize {
    if got.len() != expect.len() || got.len() % 4 != 0 {
        return usize::MAX;
    }
    got.chunks_exact(4)
        .zip(expect.chunks_exact(4))
        .map(|(g, e)| {
            [g[2], g[1], g[0], g[3]]
                .iter()
                .zip(e.iter())
                .map(|(a, b)| (*a as usize).abs_diff(*b as usize))
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0)
}

/// Per-channel disagreement between a received frame and a render: the largest and
/// the average absolute difference of each of the four channels. A transport that
/// decimates chroma shows up as a big B/R max with a near-zero A max; a transport
/// that garbles rows shows up as a big max in all four.
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
struct FrameDiff {
    max: [usize; 4],
    mean: [f64; 4],
}

#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
impl FrameDiff {
    /// The single largest per-byte difference over all channels.
    fn worst(&self) -> usize {
        self.max.iter().copied().max().unwrap_or(0)
    }

    /// The mean per-byte difference summed over all channels.
    fn mean_all(&self) -> f64 {
        self.mean.iter().sum()
    }

    fn describe(&self) -> String {
        format!(
            "max R{} G{} B{} A{}, mean {:.3} {:.3} {:.3} {:.3}",
            self.max[0],
            self.max[1],
            self.max[2],
            self.max[3],
            self.mean[0],
            self.mean[1],
            self.mean[2],
            self.mean[3]
        )
    }
}

/// `FrameDiff` between two packed RGBA frames of the same length; every channel at
/// `usize::MAX` when they are not, because a wrong-sized frame is not "slightly off".
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
fn rgba_diff(got: &[u8], expect: &[u8]) -> FrameDiff {
    if got.len() != expect.len() {
        return FrameDiff {
            max: [usize::MAX; 4],
            mean: [0.0; 4],
        };
    }
    let mut max = [0usize; 4];
    let mut sum = [0u64; 4];
    for (g, e) in got.chunks_exact(4).zip(expect.chunks_exact(4)) {
        for c in 0..4 {
            let d = g[c] as usize;
            let d = d.abs_diff(e[c] as usize);
            max[c] = max[c].max(d);
            sum[c] += d as u64;
        }
    }
    let pixels = (got.len() / 4).max(1) as f64;
    FrameDiff {
        max,
        mean: [
            sum[0] as f64 / pixels,
            sum[1] as f64 / pixels,
            sum[2] as f64 / pixels,
            sum[3] as f64 / pixels,
        ],
    }
}

/// How many times closer a received frame must sit to its own render than to the
/// other one, on the summed per-channel mean. Measured here at 21.9x, so the
/// requirement is a chosen strictness and not a fitted one.
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
const FRAME_MARGIN: f64 = 4.0;

/// Largest alpha-plane difference between two packed RGBA frames, ignoring RGB
/// entirely. Zero is what this transport gives; a length mismatch is `usize::MAX`.
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
fn rgba_alpha_delta(got: &[u8], expect: &[u8]) -> usize {
    if got.len() != expect.len() {
        return usize::MAX;
    }
    got.iter()
        .zip(expect.iter())
        .skip(3)
        .step_by(4)
        .map(|(a, b)| (*a as usize).abs_diff(*b as usize))
        .max()
        .unwrap_or(0)
}

/// Whether the frame that came back is the frame that went up rather than the other
/// render. Per channel it must be no further from its own render than from the other
/// one; ties are allowed, and only the fixture constant alpha channel ties. The
/// totals must be FRAME_MARGIN apart, which is what stops a transport that blurred
/// every frame toward the same thing from passing.
#[cfg_attr(not(feature = "ndi"), allow(dead_code))]
fn is_this_frame(own: &FrameDiff, cross: &FrameDiff) -> bool {
    (0..4).all(|c| own.max[c] <= cross.max[c])
        && (0..4).all(|c| own.mean[c] <= cross.mean[c])
        && own.mean_all() * FRAME_MARGIN <= cross.mean_all()
}

/// What one pump phase saw: whether the render came back byte-exact, how many frames
/// the receiver returned, and the closest frame's bytes so the caller can compare it
/// against the *other* render too.
#[cfg(feature = "ndi")]
struct PumpOutcome {
    matched: bool,
    frames: u32,
    best_delta: usize,
    best_swap_delta: usize,
    dims: (u32, u32),
    best: Option<Vec<u8>>,
    /// How many frames this pump saw that differed from the one immediately before
    /// them. Sending the same image repeatedly and getting varying answers means the
    /// transport is doing something rate-controlled, not a fixed transform; one change
    /// is the stale frame the previous phase left in the receiver.
    variants: u32,
}

/// Send `expect` until the receiver hands it back unchanged. `Err` carries the shim's
/// own words; a pump that runs out returns `Ok` with `matched: false` and the evidence
/// of what did arrive.
#[cfg(feature = "ndi")]
fn pump_until_exact(
    sender: &flux_integrations::ndi_io::NdiSender,
    recv: &flux_integrations::ndi_io::NdiReceiver,
    expect: &[u8],
    width: u32,
    height: u32,
    pumps: u32,
) -> Result<PumpOutcome, String> {
    let mut out = PumpOutcome {
        matched: false,
        frames: 0,
        best_delta: usize::MAX,
        best_swap_delta: usize::MAX,
        dims: (0, 0),
        best: None,
        variants: 0,
    };
    let mut prev: Option<Vec<u8>> = None;
    for _ in 0..pumps {
        if !sender.send(expect, width, height) {
            return Err(format!(
                "NdiSender::send refused a {}-byte frame at {width}x{height} (it wants {})",
                expect.len(),
                (width as usize) * (height as usize) * 4
            ));
        }
        if let Some(f) = recv.next_frame(150)? {
            out.frames += 1;
            if prev.as_ref().is_some_and(|x| *x != f.rgba) {
                out.variants += 1;
            }
            prev = Some(f.rgba.clone());
            let d = max_rgba_delta(&f.rgba, expect);
            let s = max_rgba_delta_rb_swapped(&f.rgba, expect);
            let improved = d < out.best_delta;
            if improved {
                out.best_delta = d;
                out.dims = (f.width, f.height);
            }
            if s < out.best_swap_delta {
                out.best_swap_delta = s;
            }
            if d == 0 && f.width == width && f.height == height {
                out.matched = true;
                out.best = Some(f.rgba);
                return Ok(out);
            }
            if improved {
                out.best = Some(f.rgba);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every guard here asserts the two things `main()` turns into an exit status:
    /// that the command refused, and that the refusal names how to make it work.
    #[allow(dead_code)]
    fn refused(result: FluxResult<()>) -> FluxError {
        let e = result.expect_err("a subcommand that cannot run must not report success");
        assert_eq!(e.code, flux_domain::ErrorCode::BlockedExternal, "{e:?}");
        assert!(
            !e.recovery.is_empty(),
            "the refusal must name the build that would make it run: {e:?}"
        );
        e
    }

    #[cfg(not(feature = "ndi"))]
    #[test]
    fn the_four_ndi_subcommands_refuse_with_a_build_recovery() {
        refused(super::cmd_ndi());
        refused(super::cmd_ndi_loopback());
        refused(super::cmd_ndi_send(&[]));
        refused(super::cmd_ndi_recv(&[]));
    }

    #[cfg(not(feature = "spout"))]
    #[test]
    fn the_three_spout_subcommands_refuse_with_a_build_recovery() {
        refused(super::cmd_spout());
        refused(super::cmd_spout_send(&[]));
        refused(super::cmd_spout_send_dx(&[]));
    }

    #[cfg(not(feature = "link"))]
    #[test]
    fn link_names_the_library_it_needs_to_be_built() {
        let e = refused(super::cmd_link());
        assert!(
            e.recovery.contains("FLUX_ABLETON_LINK_LIB"),
            "the Link refusal is the one place the old branch printed the \
             prerequisite; it has to survive into the error: {e:?}"
        );
    }

    #[cfg(not(feature = "vision"))]
    #[test]
    fn depth_refuses_with_a_build_recovery() {
        refused(super::cmd_depth(&[]));
    }

    /// `flux caps` may answer on a host with FFmpeg and refuse on one without; what it
    /// must never do is print an error payload and exit 0. Inverted here rather than
    /// forced, because this host has the binaries.
    #[test]
    fn caps_refuses_only_when_the_media_worker_does() {
        let worker = flux_media::MediaWorker::new().is_err();
        let caps = super::cmd_caps().is_err();
        assert_eq!(
            worker,
            caps,
            "MediaWorker::new() {} while `flux caps` {}",
            if worker { "failed" } else { "succeeded" },
            if caps { "failed" } else { "succeeded" },
        );
    }

    /// The nine compiled-out blocks are absent from the binary CI runs, so the guards
    /// above cover them one feature at a time. This scan covers all nine in every
    /// build, and counts them so a tenth branch cannot join the class quietly.
    #[test]
    fn no_compiled_out_block_hands_back_a_success() {
        let lines: Vec<&str> = include_str!("main.rs").lines().collect();
        // The guards below are the only other `#[cfg(not(feature))]` uses in the file;
        // the scan covers the command bodies, which are all above them.
        let stop = lines
            .iter()
            .position(|l| l.trim() == "#[cfg(test)]")
            .expect("the test module marker");
        let mut blocks = 0;
        let mut i = 0;
        while i < stop {
            if !lines[i].trim().starts_with("#[cfg(not(feature") {
                i += 1;
                continue;
            }
            assert_eq!(
                lines[i + 1].trim(),
                "{",
                "main.rs:{}: expected the cfg block to open on its own line",
                i + 2
            );
            let mut j = i + 2;
            let mut depth = 1;
            while j < lines.len() && depth > 0 {
                depth +=
                    lines[j].matches('{').count() as i32 - lines[j].matches('}').count() as i32;
                if depth > 0 {
                    let body = lines[j].trim();
                    assert!(
                        !body.contains("Ok(("),
                        "main.rs:{} returns success from a compiled-out subcommand: {body}",
                        j + 1
                    );
                }
                j += 1;
            }
            blocks += 1;
            i = j;
        }
        assert_eq!(
            blocks, 9,
            "expected the four ndi, three spout, one link and one vision refusal \
             branches; found {blocks}"
        );
    }

    #[test]
    fn capture_start_rejects_non_numeric_input_db() {
        // `flux capture start --input-db banana` must exit 1, not 0 with a note.
        // The parser adds a problem to the list; capture_start must fail on any
        // problem before attempting to run the capture.
        let args = vec![
            "--tap".into(),
            "loopback".into(),
            "--input-db".into(),
            "banana".into(),
            "--secs".into(),
            "1".into(),
        ];
        let result = super::capture_start(&args);
        assert!(
            result.is_err(),
            "non-numeric --input-db must fail, not succeed with a note"
        );
        let e = result.unwrap_err();
        assert_eq!(e.code, flux_domain::ErrorCode::InvalidCommand);
        assert!(
            e.message.contains("configuration errors"),
            "error must name the problem: {}",
            e.message
        );
        assert!(
            e.message.contains("--input-db"),
            "error must mention the flag: {}",
            e.message
        );
    }

    #[test]
    fn capture_start_rejects_out_of_range_input_db() {
        // `flux capture start --input-db 99` must exit 1 (range is -24..=24).
        let args = vec![
            "--tap".into(),
            "loopback".into(),
            "--input-db".into(),
            "99".into(),
            "--secs".into(),
            "1".into(),
        ];
        let result = super::capture_start(&args);
        assert!(
            result.is_err(),
            "out-of-range --input-db must fail, not succeed with a note"
        );
        let e = result.unwrap_err();
        assert_eq!(e.code, flux_domain::ErrorCode::InvalidCommand);
        assert!(
            e.message.contains("configuration errors"),
            "error must name the problem: {}",
            e.message
        );
    }

    #[test]
    fn validate_capture_snapshot_rejects_zero_frames() {
        // A snapshot with zero frames must fail validation, so capture start
        // exits nonzero when nothing was captured.
        let snapshot = flux_app::live_audio::LiveAudioSnapshot {
            running: false,
            compiled: true,
            tap: "loopback".into(),
            endpoint_id: "test".into(),
            endpoint_name: "test".into(),
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
            format_tag: 1,
            float_wire: false,
            extensible: false,
            buffer_ms: 10,
            period_default_ms: 10.0,
            period_min_ms: 3.0,
            ring_ms: 100,
            ring_ms_effective: 100.0,
            input_db: 0.0,
            valid: true,
            rms: 0.0,
            peak: 0.0,
            onset: false,
            flux: 0.0,
            bands: [0.0; 5],
            centroid_hz: 0.0,
            tempo_bpm: 0.0,
            tempo_confidence: 0.0,
            beat_phase: 0.0,
            windows: 0,
            onsets: 0,
            packets: 0,
            frames: 0, // Zero frames: must fail
            silent_frames: 0,
            overflow_events: 0,
            stalled_ms: 0,
            elapsed_ms: 1000,
            error: None,
            feature_source: flux_domain::params::FeatureSource::Live,
        };

        let result = super::validate_capture_snapshot(&snapshot);
        assert!(
            result.is_err(),
            "zero frames must fail validation, not succeed"
        );
        let e = result.unwrap_err();
        assert_eq!(e.code, flux_domain::ErrorCode::InvalidCommand);
        assert!(
            e.message.contains("zero frames"),
            "error must name the problem: {}",
            e.message
        );
    }

    #[test]
    fn validate_capture_snapshot_rejects_error_field() {
        // A snapshot with an error message must fail validation.
        let snapshot = flux_app::live_audio::LiveAudioSnapshot {
            running: false,
            compiled: true,
            tap: "loopback".into(),
            endpoint_id: "test".into(),
            endpoint_name: "test".into(),
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
            format_tag: 1,
            float_wire: false,
            extensible: false,
            buffer_ms: 10,
            period_default_ms: 10.0,
            period_min_ms: 3.0,
            ring_ms: 100,
            ring_ms_effective: 100.0,
            input_db: 0.0,
            valid: true,
            rms: 0.0,
            peak: 0.0,
            onset: false,
            flux: 0.0,
            bands: [0.0; 5],
            centroid_hz: 0.0,
            tempo_bpm: 0.0,
            tempo_confidence: 0.0,
            beat_phase: 0.0,
            windows: 0,
            onsets: 0,
            packets: 0,
            frames: 100,
            silent_frames: 0,
            overflow_events: 0,
            stalled_ms: 0,
            elapsed_ms: 1000,
            error: Some("device disconnected".into()), // Error: must fail
            feature_source: flux_domain::params::FeatureSource::Live,
        };

        let result = super::validate_capture_snapshot(&snapshot);
        assert!(result.is_err(), "snapshot with error must fail validation");
        let e = result.unwrap_err();
        assert!(
            e.message.contains("device disconnected"),
            "error must include the snapshot error: {}",
            e.message
        );
    }

    #[test]
    fn validate_capture_snapshot_rejects_invalid() {
        // A snapshot with valid=false must fail validation.
        let snapshot = flux_app::live_audio::LiveAudioSnapshot {
            running: false,
            compiled: true,
            tap: "loopback".into(),
            endpoint_id: "test".into(),
            endpoint_name: "test".into(),
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
            format_tag: 1,
            float_wire: false,
            extensible: false,
            buffer_ms: 10,
            period_default_ms: 10.0,
            period_min_ms: 3.0,
            ring_ms: 100,
            ring_ms_effective: 100.0,
            input_db: 0.0,
            valid: false, // Invalid: must fail
            rms: 0.0,
            peak: 0.0,
            onset: false,
            flux: 0.0,
            bands: [0.0; 5],
            centroid_hz: 0.0,
            tempo_bpm: 0.0,
            tempo_confidence: 0.0,
            beat_phase: 0.0,
            windows: 0,
            onsets: 0,
            packets: 0,
            frames: 100,
            silent_frames: 0,
            overflow_events: 0,
            stalled_ms: 0,
            elapsed_ms: 1000,
            error: None,
            feature_source: flux_domain::params::FeatureSource::Live,
        };

        let result = super::validate_capture_snapshot(&snapshot);
        assert!(result.is_err(), "invalid snapshot must fail validation");
        let e = result.unwrap_err();
        assert!(
            e.message.contains("not valid"),
            "error must name the problem: {}",
            e.message
        );
    }

    #[test]
    fn render_audit_line_names_every_field_of_the_export_result() {
        // SHORT-line gate AND the regression gate for the measured CR3 defect-1
        // run-2 defect: `png_frames_written = false` while the `frames_dir`
        // really does exist on disk. Branching on `exists()` (the old proxy)
        // would print the path; branching on the field must not. The directory
        // is `temp_dir()` itself, which always exists, so nothing is created or
        // removed here. Distinctive values (frames=7, samples=1234, windows=42)
        // and `feature_source` derived from the fixture via `{:?}` (I12
        // property, never a second literal) so no field swap or hard-coded enum
        // value hides. 2 enum iterations x 2 assertions.
        let present = std::env::temp_dir();
        assert!(
            present.exists(),
            "test precondition: temp_dir must exist to force field-vs-fs disagreement"
        );
        for src in [
            flux_domain::params::FeatureSource::Live,
            flux_domain::params::FeatureSource::DocumentOnly,
        ] {
            let r = flux_app::export::ExportResult {
                frames: 7,
                samples: 1234,
                frames_dir: present.clone(),
                wav: None,
                video: None,
                feature_source: src,
                audio_windows: 42,
                png_frames_written: false,
            };
            let line = super::render_audit_line(&r);
            let expected = format!(
                "rendered 7 frames, 1234 samples; audio: {:?} (42 windows)",
                src
            );
            assert_eq!(
                line, expected,
                "audit line must name every ExportResult field on the SHORT \
                 branch (feature_source={:?})",
                src
            );
            assert!(
                !line.contains(&present.display().to_string()),
                "SHORT line must NOT name an existing frames_dir the run did \
                 not write PNGs into (feature_source={:?}): {:?}",
                src,
                line
            );
        }
    }

    #[test]
    fn render_audit_line_names_frames_dir_only_when_this_run_wrote_pngs() {
        // LONG-line gate: `png_frames_written = true` while the `frames_dir`
        // does NOT exist. The field is the sole authority, so the audit line
        // must name the (non-existent) path anyway. Reverting the branch to
        // `frames_dir.exists()` (M3) or dropping the field makes this red,
        // because `exists()` is false here. `feature_source` still loops over
        // both enum values, expected value derived from the fixture. Distinctive
        // values 100/48000/93 so nothing collides with the SHORT gate.
        let missing = std::path::PathBuf::from("/tmp/flux_audit_missing_dir_zzz_9");
        assert!(
            !missing.exists(),
            "test precondition: frames_dir must not exist"
        );
        for src in [
            flux_domain::params::FeatureSource::Live,
            flux_domain::params::FeatureSource::DocumentOnly,
        ] {
            let r = flux_app::export::ExportResult {
                frames: 100,
                samples: 48000,
                frames_dir: missing.clone(),
                wav: None,
                video: None,
                feature_source: src,
                audio_windows: 93,
                png_frames_written: true,
            };
            let line = super::render_audit_line(&r);
            let expected = format!(
                "rendered 100 frames, 48000 samples; frames in {}; audio: {:?} (93 windows)",
                missing.display(),
                src
            );
            assert_eq!(
                line, expected,
                "audit line must name the frames_dir this run wrote PNGs into, \
                 even though it does not exist on disk (feature_source={:?})",
                src
            );
            assert!(
                line.contains(&missing.display().to_string()),
                "LONG line must name frames_dir (feature_source={:?}): {:?}",
                src,
                line
            );
        }
    }

    #[test]
    fn report_capture_zero_frames_exits_nonzero_on_both_branches() {
        // Both the `--json` branch and the human branch must run through the
        // same outcome. Removing `let outcome = validate_capture_snapshot(...)`
        // or replacing it with `Ok(())` reddens the JSON arm of this test —
        // that is the D036 defect: printing a failure payload and exiting 0.
        let snap = zero_frame_snapshot();
        let json_result = super::report_capture(
            true,
            1,
            100,
            &[],
            &snap,
            std::time::Duration::from_millis(500),
        );
        let human_result = super::report_capture(
            false,
            1,
            100,
            &[],
            &snap,
            std::time::Duration::from_millis(500),
        );

        let json_err = json_result.expect_err(
            "--json branch must not return Ok on zero frames; deleting the \
             validator call in report_capture would let `capture start --json` \
             print a failure payload and exit 0 (D036)",
        );
        assert!(
            json_err.message.contains("zero frames"),
            "--json error names the reason: {}",
            json_err.message
        );

        let human_err = human_result.expect_err("human branch must not return Ok on zero frames");
        assert!(
            human_err.message.contains("zero frames"),
            "human error names the reason: {}",
            human_err.message
        );
    }

    #[test]
    fn report_capture_error_field_exits_nonzero_on_both_branches() {
        let mut snap = ok_snapshot();
        snap.error = Some("device disconnected".into());
        assert!(
            super::report_capture(
                true,
                1,
                100,
                &[],
                &snap,
                std::time::Duration::from_millis(500)
            )
            .is_err(),
            "--json must reject an errored snapshot"
        );
        assert!(
            super::report_capture(
                false,
                1,
                100,
                &[],
                &snap,
                std::time::Duration::from_millis(500)
            )
            .is_err(),
            "human must reject an errored snapshot"
        );
    }

    #[test]
    fn report_capture_valid_snapshot_exits_zero_on_both_branches() {
        // The gate only bites on the failure shapes. A real success must be Ok
        // in both branches, so a mutation that inverts the exit status is also
        // caught (the two tests above assert Err for failure; this one asserts
        // Ok for success).
        let snap = ok_snapshot();
        assert!(
            super::report_capture(
                true,
                1,
                100,
                &[],
                &snap,
                std::time::Duration::from_millis(500)
            )
            .is_ok(),
            "--json must accept a good snapshot"
        );
        assert!(
            super::report_capture(
                false,
                1,
                100,
                &[],
                &snap,
                std::time::Duration::from_millis(500)
            )
            .is_ok(),
            "human must accept a good snapshot"
        );
    }

    fn base_snapshot() -> live_audio::LiveAudioSnapshot {
        live_audio::LiveAudioSnapshot {
            running: false,
            compiled: true,
            tap: "loopback".into(),
            endpoint_id: "test".into(),
            endpoint_name: "test".into(),
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
            format_tag: 1,
            float_wire: false,
            extensible: false,
            buffer_ms: 10,
            period_default_ms: 10.0,
            period_min_ms: 3.0,
            ring_ms: 100,
            ring_ms_effective: 100.0,
            input_db: 0.0,
            valid: true,
            rms: 0.0,
            peak: 0.0,
            onset: false,
            flux: 0.0,
            bands: [0.0; 5],
            centroid_hz: 0.0,
            tempo_bpm: 0.0,
            tempo_confidence: 0.0,
            beat_phase: 0.0,
            windows: 0,
            onsets: 0,
            packets: 0,
            frames: 100,
            silent_frames: 0,
            overflow_events: 0,
            stalled_ms: 0,
            elapsed_ms: 1000,
            error: None,
            feature_source: flux_domain::params::FeatureSource::Live,
        }
    }

    fn zero_frame_snapshot() -> live_audio::LiveAudioSnapshot {
        let mut s = base_snapshot();
        s.frames = 0;
        s
    }

    fn ok_snapshot() -> live_audio::LiveAudioSnapshot {
        base_snapshot()
    }

    fn synth_frame(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                out.extend_from_slice(&f(x, y));
            }
        }
        out
    }

    /// A frame two columns wide, so a chroma-style transform has something to alternate
    /// over, with R, G and B all distinct at every pixel.
    fn pair_of_renders() -> (Vec<u8>, Vec<u8>) {
        let a = synth_frame(64, 32, |x, y| {
            [
                ((x * 37 + y * 7) % 256) as u8,
                ((x * 11 + y * 5) % 256) as u8,
                ((x * y + 13) % 256) as u8,
                255,
            ]
        });
        let b = synth_frame(64, 32, |x, y| {
            [
                ((x * 89 + y * 61) % 256) as u8,
                ((x * 43 + y * 17) % 256) as u8,
                ((x * 7 * y + 200) % 256) as u8,
                255,
            ]
        });
        (a, b)
    }

    /// What the measured transport does: a small deterministic nudge that errs hardest
    /// in R and B, leaves G nearer the original and never touches alpha. Saturating,
    /// because a wrapping one is a 254-level jump on a byte that happened to be 0 and
    /// would model nothing.
    fn lossy_copy(src: &[u8]) -> Vec<u8> {
        let mut out = src.to_vec();
        for (i, b) in out.iter_mut().enumerate() {
            if i % 4 == 3 {
                continue;
            }
            let weight = match i % 4 {
                0 | 2 => 2,
                _ => 1,
            };
            *b = b.saturating_sub(weight);
        }
        out
    }

    #[test]
    fn discrimination_accepts_the_frame_that_was_actually_sent() {
        let (a, b) = pair_of_renders();
        let got = lossy_copy(&a);
        let own = rgba_diff(&got, &a);
        let cross = rgba_diff(&got, &b);
        assert!(
            is_this_frame(&own, &cross),
            "a lightly lossy copy of A must still be identified as A: own {} / cross {}",
            own.describe(),
            cross.describe()
        );
        assert!(
            !is_this_frame(&cross, &own),
            "the same test run backwards must refuse: the residual of A cannot be read as B"
        );
        assert_eq!(own.max[3], 0, "the synthetic loss must not touch alpha");
    }

    #[test]
    fn the_margin_is_what_refuses_a_transport_barely_better_than_nothing() {
        // `is_this_frame` has two halves. The per-channel comparison can be satisfied
        // by a transport that is only a little closer to the right frame than to the
        // wrong one, which is carrying a suggestion and not a render. This fixture is
        // built so that own beats cross on every channel by max and by mean, yet by
        // less than FRAME_MARGIN over all - so the margin is the only thing that
        // refuses it, and the only thing that can redden this test.
        let a = synth_frame(32, 16, |x, y| {
            [
                (100 + x % 20) as u8,
                (110 + y % 15) as u8,
                (120 + x % 11) as u8,
                255,
            ]
        });
        let b: Vec<u8> = a
            .iter()
            .enumerate()
            .map(|(i, v)| if i % 4 == 3 { *v } else { v.saturating_add(10) })
            .collect();
        let got: Vec<u8> = a
            .iter()
            .enumerate()
            .map(|(i, v)| match i % 4 {
                1 => v.saturating_sub(4),
                3 => *v,
                _ => v.saturating_sub(8),
            })
            .collect();
        let own = rgba_diff(&got, &a);
        let cross = rgba_diff(&got, &b);
        assert!(
            (0..4).all(|c| own.max[c] <= cross.max[c] && own.mean[c] <= cross.mean[c]),
            "the fixture must already satisfy the per-channel half: own {} / cross {}",
            own.describe(),
            cross.describe()
        );
        assert!(
            own.mean_all() * FRAME_MARGIN > cross.mean_all(),
            "the fixture must sit inside the margin, or this test proves nothing: \
             own {:.1} x{FRAME_MARGIN} vs cross {:.1}",
            own.mean_all(),
            cross.mean_all()
        );
        assert!(
            !is_this_frame(&own, &cross),
            "only the margin separates these two, so it must refuse: own {} / cross {}",
            own.describe(),
            cross.describe()
        );
    }

    #[test]
    fn a_frame_blurred_toward_both_renders_is_not_identified_as_either() {
        // The failure the margin exists for: a transport that drags every frame toward
        // the same thing stays closer to neither, so the comparative test refuses it
        // even though frames still arrive and still differ from each other.
        let (a, b) = pair_of_renders();
        let mid: Vec<u8> = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| ((*x as u16 + *y as u16) / 2) as u8)
            .collect();
        let from_a = rgba_diff(&mid, &a);
        let from_b = rgba_diff(&mid, &b);
        assert!(
            !is_this_frame(&from_a, &from_b) && !is_this_frame(&from_b, &from_a),
            "an equidistant frame must be refused in both directions: a={} b={}",
            from_a.describe(),
            from_b.describe()
        );
    }

    #[test]
    fn the_alpha_plane_is_compared_separately_from_the_colour() {
        let (a, _) = pair_of_renders();
        let mut same_alpha = lossy_copy(&a);
        assert_eq!(
            rgba_alpha_delta(&same_alpha, &a),
            0,
            "the colour may move but the alpha plane must not"
        );
        assert!(
            rgba_diff(&same_alpha, &a).worst() > 0,
            "the nudge has to have actually changed something for the zero above to mean anything"
        );
        // Saturating, and down: the fixture alpha is 255 everywhere, so +1 would wrap to
        // zero and the test would be measuring a delta of 255 while claiming one.
        same_alpha[7] = same_alpha[7].saturating_sub(1);
        assert_eq!(
            rgba_alpha_delta(&same_alpha, &a),
            1,
            "one alpha byte out of place must be visible to the alpha test"
        );
    }

    #[test]
    fn frame_distances_refuse_a_wrong_sized_frame_instead_of_calling_it_close() {
        let (a, _) = pair_of_renders();
        assert_eq!(max_rgba_delta(&a, &a[..a.len() - 4]), usize::MAX);
        assert_eq!(rgba_alpha_delta(&a, &a[..a.len() - 4]), usize::MAX);
        assert_eq!(rgba_diff(&a, &a[..a.len() - 4]).worst(), usize::MAX);
        let mut swapped = a.clone();
        for px in swapped.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        assert_eq!(
            max_rgba_delta_rb_swapped(&swapped, &a),
            0,
            "an R/B-exchanged frame must score zero once the channels are exchanged back"
        );
        assert!(
            max_rgba_delta(&swapped, &a) > 0,
            "...and the raw comparison must still see the swap"
        );
    }

    #[test]
    fn ndi_commands_load_the_runtime_before_the_shim() {
        // The shim imports `Processing.NDI.Lib.x64.dll`, and `NdiShim::load` can be handed
        // a relative candidate (`bases()` ends in `.`), which is documented undefined
        // behaviour for the loader flag it uses and does not resolve that import from the
        // shim's own directory. The runtime has to be in the process first. Scanned
        // because the symptom of the wrong order is "ndi_shim.dll not found" on a machine
        // that has NDI installed, which is an apparatus report that points at the wrong
        // missing thing.
        let src = include_str!("main.rs");
        for cmd in ["cmd_ndi_loopback", "cmd_ndi_send", "cmd_ndi_recv"] {
            let body = src
                .split(&format!("fn {cmd}("))
                .nth(1)
                .unwrap_or_else(|| panic!("{cmd} is not defined in main.rs"))
                .split("\nfn ")
                .next()
                .expect("a function body");
            match (
                body.find("NdiRuntime::load()"),
                body.find("NdiShim::load()"),
            ) {
                (Some(rt), Some(shim)) => assert!(
                    rt < shim,
                    "{cmd} loads the shim before the runtime, so the shim's import cannot resolve"
                ),
                (None, Some(_)) => panic!("{cmd} loads the shim with no runtime load above it"),
                _ => {}
            }
        }
    }
}
