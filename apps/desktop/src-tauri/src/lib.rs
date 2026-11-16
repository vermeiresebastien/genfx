//! Flux desktop shell (Tauri 2).
//!
//! Rust is the authority: this shell exposes the engine's typed command surface
//! (`flux_app::commands::CommandHost`) over Tauri IPC. The webview only submits
//! requests and renders snapshots + preview frames; it never runs an independent
//! simulation or renderer.

use flux_app::commands::{CommandHost, GuiCommand, Request, Response};
use flux_app::control::OscBridge;
use flux_app::export::{export_project_with_frames, ExportOptions, PreviewRenderer};
use flux_app::producer::produce_node;
use flux_domain::{Project, RationalTime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

struct AppState {
    host: Arc<Mutex<CommandHost>>,
    /// Cached GPU preview renderer (created lazily on first preview so a missing
    /// GPU doesn't break app startup). Reusing it avoids recreating the device
    /// and recompiling shaders on every frame.
    preview: Mutex<Option<PreviewRenderer>>,
    /// True while a background export is running (prevents concurrent GPU work).
    exporting: Arc<AtomicBool>,
    /// The live-control listener, and why it is not running if it is not.
    osc: Mutex<OscRuntime>,
}

/// Owned by this process, not by the webview: a listener in the UI layer would
/// put control authority on the wrong side of the IPC boundary.
struct OscRuntime {
    port: u16,
    bridge: Option<OscBridge>,
    bind_error: Option<String>,
}

/// The UDP port live control listens on.
///
/// `FLUX_OSC_PORT` moves it, which matters because the port has exactly one
/// owner and `flux osc <port>` uses the same default of 9000.
fn osc_port() -> u16 {
    let default = flux_integrations::osc::DEFAULT_RECV_PORT;
    match std::env::var("FLUX_OSC_PORT") {
        Ok(raw) => raw.trim().parse::<u16>().unwrap_or_else(|_| {
            eprintln!(
                "FLUX_OSC_PORT={:?} is not a port number; listening on {default}",
                raw
            );
            default
        }),
        Err(_) => default,
    }
}

#[tauri::command]
fn dispatch(state: tauri::State<AppState>, request: Request) -> Response {
    let mut host = state.host.lock().unwrap();
    host.handle(request)
}

#[tauri::command]
fn open_monitor(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("monitor") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "monitor", WebviewUrl::App("monitor.html".into()))
        .title("Flux Preview")
        .inner_size(960.0, 540.0)
        .min_inner_size(320.0, 180.0)
        .resizable(true)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_monitor_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    let window = app
        .get_webview_window("monitor")
        .ok_or_else(|| "monitor window is not open".to_string())?;
    let fullscreen = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window
        .set_fullscreen(fullscreen)
        .map_err(|e| e.to_string())?;
    Ok(fullscreen)
}

/// Render a single preview frame of the active project at time `t` (seconds)
/// at reduced resolution; returns a PNG data URL for the webview.
#[tauri::command]
fn render_preview(
    state: tauri::State<AppState>,
    time_seconds: f64,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let (project, graph) = {
        let host = state.host.lock().unwrap();
        let g = host
            .dispatcher
            .project
            .graphs
            .get(&host.active_graph)
            .cloned();
        (host.dispatcher.project.clone(), g)
    };
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    let mut guard = state.preview.lock().unwrap();
    if guard.is_none() {
        *guard = Some(PreviewRenderer::new().map_err(|e| format!("gpu init: {e:?}"))?);
    }
    let renderer = guard.as_mut().unwrap();
    // Render the node-graph editor's active graph (fall back to timeline render
    // if the project has no graph).
    let rgba = match &graph {
        Some(g) => renderer.render_graph(g, &project, t, width, height),
        None => renderer.render(&project, t, width, height),
    }
    .map_err(|e| format!("render: {e:?}"))?;
    // Encode the RGBA to PNG in memory (no disk round-trip).
    let png =
        flux_media::encode_png_rgba(width, height, &rgba).map_err(|e| format!("png: {e:?}"))?;
    if png.len() > 4 * 1024 * 1024 {
        return Err("preview too large".into());
    }
    Ok(format!("data:image/png;base64,{}", b64(&png)))
}

fn b64(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Nearest-neighbour downscale of packed RGBA for the live export preview.
fn downscale_rgba(src: &[u8], w: u32, h: u32, max_w: u32) -> (u32, u32, Vec<u8>) {
    if w == 0 || h == 0 || w <= max_w {
        return (w, h, src.to_vec());
    }
    let dw = max_w;
    let dh = (((h as u64) * (dw as u64) + (w as u64) / 2) / (w as u64)).max(1) as u32;
    let mut out = vec![0u8; (dw as usize) * (dh as usize) * 4];
    let need = (dw as usize) * (dh as usize) * 4;
    if src.len() < need {
        return (w, h, src.to_vec());
    }
    for y in 0..dh {
        let sy = (y as u64 * h as u64 / dh as u64) as u32;
        for x in 0..dw {
            let sx = (x as u64 * w as u64 / dw as u64) as u32;
            let si = ((sy as usize) * (w as usize) + (sx as usize)) * 4;
            let di = ((y as usize) * (dw as usize) + (x as usize)) * 4;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (dw, dh, out)
}

/// Start an export on a background thread. Progress, live frames and completion
/// are reported through events (`export-progress`, `export-frame`, `export-done`,
/// `export-error`) so the webview can show the real output while rendering.
#[tauri::command]
fn export_project_cmd(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    out_dir: String,
    width: u32,
    height: u32,
    codec: Option<String>,
    keep_frames: Option<bool>,
) -> Result<String, String> {
    let project = {
        let host = state.host.lock().unwrap();
        host.dispatcher.project.clone()
    };
    if state.exporting.swap(true, Ordering::SeqCst) {
        return Err("an export is already running".into());
    }
    let exporting = state.exporting.clone();
    // Release the live preview GPU device before exporting so only one device is
    // active at a time (two concurrent wgpu devices can crash some drivers).
    if let Ok(mut guard) = state.preview.lock() {
        *guard = None;
    }
    // Resolve relative output dirs against the process CWD so the reported path
    // is absolute and the user can find the file.
    let out_dir = {
        let p = std::path::PathBuf::from(&out_dir);
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p)
        }
    };
    let codec = flux_app::export::codec_from_name(codec.as_deref().unwrap_or("h264"));
    let keep = keep_frames.unwrap_or(false);

    std::thread::spawn(move || {
        let progress_app = app.clone();
        let progress: flux_app::export::ProgressFn = Box::new(move |done, total| {
            let _ = progress_app.emit(
                "export-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        });
        let opts = ExportOptions {
            out_dir,
            project_name: project.name.clone(),
            codec,
            width,
            height,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        };
        // Stream the actual export frames to the monitor while rendering. The UI
        // pauses its own preview loop during export, so only one GPU device is
        // doing work at a time.
        let frame_app = app.clone();
        let last_emit = std::cell::Cell::new(std::time::Instant::now());
        let frame_cb: flux_app::export::FrameFn = Box::new(move |done, total, rgba, w, h| {
            if done < total && last_emit.get().elapsed().as_millis() < 90 {
                return;
            }
            last_emit.set(std::time::Instant::now());
            let (dw, dh, small) = downscale_rgba(rgba, w, h, 480);
            if let Ok(png) = flux_media::encode_png_rgba(dw, dh, &small) {
                let _ = frame_app.emit(
                    "export-frame",
                    serde_json::json!({
                        "dataUrl": format!("data:image/png;base64,{}", b64(&png)),
                        "done": done,
                        "total": total,
                    }),
                );
            }
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            export_project_with_frames(&project, &opts, Some(progress), Some(frame_cb))
        }));
        match outcome {
            Ok(Ok(res)) => {
                exporting.store(false, Ordering::SeqCst);
                // A PNG sequence is an implementation detail of encoding; remove it
                // unless the caller explicitly asked to keep it (or there is no video).
                if !keep && res.video.is_some() {
                    let _ = std::fs::remove_dir_all(&res.frames_dir);
                }
                let _ = app.emit(
                    "export-done",
                    serde_json::json!({
                        "frames": res.frames,
                        "samples": res.samples,
                        "video": res.video.as_ref().map(|p| p.display().to_string()),
                        "wav": res.wav.as_ref().map(|p| p.display().to_string()),
                        "framesDir": if keep || res.video.is_none() {
                            Some(res.frames_dir.display().to_string())
                        } else {
                            None
                        },
                    }),
                );
            }
            Ok(Err(e)) => {
                exporting.store(false, Ordering::SeqCst);
                let _ = app.emit(
                    "export-error",
                    serde_json::json!({ "message": format!("{e:?}") }),
                );
            }
            Err(_) => {
                exporting.store(false, Ordering::SeqCst);
                let _ = app.emit(
                    "export-error",
                    serde_json::json!({ "message": "export thread panicked" }),
                );
            }
        }
    });

    Ok("export started".into())
}

#[tauri::command]
fn capabilities() -> serde_json::Value {
    serde_json::json!({
        "capture": flux_platform::capture_capabilities(),
        "integrations": flux_integrations::capability_report(),
    })
}

/// What the live-control listener knows. Every packet it accepts lands in one
/// counter, so a fader that appears to do nothing is a number to read rather
/// than a mystery: `unmapped` means the binding table or the node, `notApplied`
/// means the engine has no target for that kind yet, `droppedByReceiver` means
/// the stream outran the rate limiter.
#[tauri::command]
fn osc_status(state: tauri::State<AppState>) -> serde_json::Value {
    let g = state.osc.lock().unwrap();
    serde_json::json!({
        "port": g.port,
        "bound": g.bridge.is_some(),
        "bindError": g.bind_error,
        "stats": g.bridge.as_ref().map(|b| b.stats()).unwrap_or(serde_json::Value::Null),
    })
}

/// What the live audio capture knows: the last measurement, whether it can be
/// trusted, and the reason there is no measurement at all. The capture is owned
/// by the `CommandHost` that `SetLiveAudio` starts it on, so this reads the same
/// slot the command returns — the webview gets a read-out, not a second owner.
///
/// `live-audio` is not in the default build, so in this binary the honest answer
/// is `compiled: false`, said plainly rather than as a level of zero.
#[tauri::command]
fn live_audio_status(state: tauri::State<AppState>) -> serde_json::Value {
    state.host.lock().unwrap().live_audio_status()
}

#[tauri::command]
fn produce_generator_frame(
    generator_id: String,
    width: u32,
    height: u32,
    time_seconds: f64,
) -> Result<String, String> {
    // Direct producer preview for a generator type (used by the library peek).
    let rgba = produce_node(
        &generator_id,
        &Default::default(),
        width,
        height,
        time_seconds,
        1,
    )
    .map_err(|e| format!("{e:?}"))?;
    match rgba {
        Some(_) => Ok("producer".into()),
        None => Ok("analytic".into()),
    }
}

/// Render a graph to raw RGBA bytes (no PNG encode, no base64). Used for live
/// playback: the webview paints this into a canvas, which is far cheaper than a
/// PNG encode + data-URL decode per frame.
///
/// Returns the bytes as a binary IPC response (`ArrayBuffer` in JS). Returning a
/// `Vec<u8>` normally serialises to a JSON array of numbers — ~278 000 numbers
/// for a 352x198 frame — which was the real cause of choppy previews.
#[tauri::command]
fn render_preview_raw(
    state: tauri::State<AppState>,
    graph_id: String,
    time_seconds: f64,
    width: u32,
    height: u32,
    output_node_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let (project, mut graph) = {
        let host = state.host.lock().unwrap();
        (
            host.dispatcher.project.clone(),
            host.dispatcher.project.graphs.get(&graph_id).cloned(),
        )
    };
    let mut graph = graph.take().ok_or_else(|| "graph not found".to_string())?;
    if let Some(node_id) = output_node_id {
        if graph.node(&node_id).is_none() {
            return Err("output node not found".into());
        }
        graph.output_node = node_id;
    }
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    let mut guard = state.preview.lock().unwrap();
    if guard.is_none() {
        *guard = Some(PreviewRenderer::new().map_err(|e| format!("gpu init: {e:?}"))?);
    }
    let renderer = guard.as_mut().unwrap();
    let rgba = renderer
        .render_graph(&graph, &project, t, width, height)
        .map_err(|e| format!("render: {e:?}"))?;
    Ok(tauri::ipc::Response::new(rgba))
}

#[tauri::command]
fn build_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "gitHash": option_env!("FLUX_GIT_HASH").unwrap_or("unknown"),
    })
}

/// Render a chosen graph (or a specific node within it) to a PNG data URL.
/// Used by the Output node's inline thumbnail and the detached preview monitor;
/// `output_node_id` overrides which node is treated as the result, so each Output
/// node on the canvas can preview its own subgraph.
#[tauri::command]
fn render_graph_preview(
    state: tauri::State<AppState>,
    graph_id: String,
    time_seconds: f64,
    width: u32,
    height: u32,
    output_node_id: Option<String>,
) -> Result<String, String> {
    let (project, mut graph) = {
        let host = state.host.lock().unwrap();
        (
            host.dispatcher.project.clone(),
            host.dispatcher.project.graphs.get(&graph_id).cloned(),
        )
    };
    let mut graph = graph.take().ok_or_else(|| "graph not found".to_string())?;
    if let Some(node_id) = output_node_id {
        if graph.node(&node_id).is_none() {
            return Err("output node not found".into());
        }
        graph.output_node = node_id;
    }
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    let mut guard = state.preview.lock().unwrap();
    if guard.is_none() {
        *guard = Some(PreviewRenderer::new().map_err(|e| format!("gpu init: {e:?}"))?);
    }
    let renderer = guard.as_mut().unwrap();
    let rgba = renderer
        .render_graph(&graph, &project, t, width, height)
        .map_err(|e| format!("render: {e:?}"))?;
    let png =
        flux_media::encode_png_rgba(width, height, &rgba).map_err(|e| format!("png: {e:?}"))?;
    if png.len() > 4 * 1024 * 1024 {
        return Err("preview too large".into());
    }
    Ok(format!("data:image/png;base64,{}", b64(&png)))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Single instance: a second launch would open its own WebView2 profile and
    // both would race on the GPU device + user data, which is exactly how a
    // stale process kept serving an old inspector.
    if !try_acquire_single_instance() {
        std::process::exit(0);
    }

    let host = Arc::new(Mutex::new(CommandHost::new(Project::new("Untitled", 1))));
    let port = osc_port();
    tauri::Builder::default()
        .manage(AppState {
            host,
            preview: Mutex::new(None),
            exporting: Arc::new(AtomicBool::new(false)),
            osc: Mutex::new(OscRuntime {
                port,
                bridge: None,
                bind_error: None,
            }),
        })
        // Start listening for Live. A failure here is recorded and reported by
        // `osc_status`, never swallowed: an app that silently ignores the faders
        // is indistinguishable from one that has no listener at all.
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state = app.state::<AppState>();
            let runtime_port = state.osc.lock().unwrap().port;
            let emit = move |params: &[String], revision: u64| {
                let _ = app_handle.emit(
                    "live-control",
                    serde_json::json!({ "params": params, "revision": revision }),
                );
            };
            match OscBridge::start(runtime_port, state.host.clone(), emit) {
                Ok(bridge) => {
                    state.osc.lock().unwrap().bridge = Some(bridge);
                }
                Err(e) => {
                    eprintln!("flux-desktop: OSC listener could not bind port {runtime_port}: {e}");
                    state.osc.lock().unwrap().bind_error = Some(e.to_string());
                }
            }
            Ok(())
        })
        // The detached monitor is owned by the main editor window. Close it
        // before the editor exits so WebView2 does not leave a helper process
        // holding the Flux user-data profile open.
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                if let Some(monitor) = window.app_handle().get_webview_window("monitor") {
                    let _ = monitor.close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            open_monitor,
            toggle_monitor_fullscreen,
            render_preview,
            render_graph_preview,
            render_preview_raw,
            build_info,
            export_project_cmd,
            capabilities,
            osc_status,
            live_audio_status,
            produce_generator_frame
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                RunEvent::ExitRequested { .. } => {
                    // Close every owned webview before the runtime tears down.
                    // This makes normal shutdown deterministic even when the
                    // detached preview monitor is still open.
                    for window in app.webview_windows().values() {
                        let _ = window.close();
                    }
                }
                RunEvent::Exit => {
                    // WebView2 can keep helper processes alive after a window
                    // closes. Terminate only this Flux process tree so the
                    // detached preview and its renderer cannot outlive Flux.
                    #[cfg(target_os = "windows")]
                    {
                        let pid = std::process::id().to_string();
                        let _ = std::process::Command::new("taskkill")
                            .args(["/PID", &pid, "/T", "/F"])
                            .spawn();
                    }
                }
                _ => {}
            }
        });
}

fn _uses(_: GuiCommand) {}

/// Hold a named mutex for the life of the process so only one Flux instance runs
/// at a time. Returns false if another instance already holds it.
#[cfg(target_os = "windows")]
fn try_acquire_single_instance() -> bool {
    use std::ptr::null_mut;

    const ERROR_ALREADY_EXISTS: u32 = 183;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            lp_mutex_attributes: *mut std::ffi::c_void,
            b_initial_owner: i32,
            lp_name: *const u16,
        ) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
    }

    let name: Vec<u16> = "Local\\dev.flux.desktop.single"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(null_mut(), 1, name.as_ptr()) };
    if handle.is_null() {
        return true; // cannot determine; never hard-block startup
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        return false;
    }
    // Intentionally leak the handle: it must stay open for the whole process so
    // the mutex is released on exit.
    std::mem::forget(handle as usize);
    true
}

#[cfg(not(target_os = "windows"))]
fn try_acquire_single_instance() -> bool {
    true
}

#[cfg(test)]
mod tests {
    /// `build.rs` is the one place the webview bundle is produced, and a test
    /// cannot run it: cargo has already executed it before the test binary
    /// exists. So the invariant is read off the source, the way
    /// `capability_report_never_derives_verified_from_a_build_feature` reads the
    /// capability table. What these two gates cannot see is stated in the test
    /// that owns the shape: `smoke/I5-1-negative-child.log` (bundler exits
    /// nonzero -> `cargo build -p flux-desktop` exits 101) is the end-to-end
    /// proof, and the same probe against the previous `build.rs` exited 0.
    ///
    /// Ban 1: a bundle failure may never be reported as `cargo:warning` again.
    /// That is precisely how the defect shipped — both arms warned and fell
    /// through, so a green build proved nothing about which `app.bundle.js` was
    /// embedded, and the only trace was a line in
    /// `target/debug/build/flux-desktop-*/output`.
    #[test]
    fn ui_bundle_failure_is_never_reported_as_a_warning() {
        let script = include_str!("../build.rs");
        // Comment lines are excluded on purpose: build.rs *explains* the old
        // warning arms, so the string `cargo:warning` legitimately appears in
        // prose. What is banned is emitting it — a line of code.
        let warned: Vec<&str> = script
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//"))
            .filter(|line| line.contains("cargo:warning"))
            .collect();
        assert!(
            warned.is_empty(),
            "build.rs still emits an ungated warning: {warned:?}. A warning that the build \
             continues past is the defect this gate exists to keep out."
        );
        // Both arms must be *decisions about the child*, not prose about it: the
        // nonzero-exit arm and the spawn-failure arm each carry their own message,
        // and the exit status is inspected before anything is concluded.
        for needle in [
            "if !out.status.success()",
            "ui bundle failed",
            "could not start",
            "bundle_ui(&ui)",
        ] {
            assert!(
                script.contains(needle),
                "build.rs no longer contains {needle:?}: the bundle step is not gated the way \
                 item 1 left it (status inspected, both failure arms fatal, no `let _ =`)."
            );
        }
        assert!(
            !script.contains("let _ = bundle_ui"),
            "build.rs ignores the bundle step's result"
        );
    }

    /// Ban 2: every top-level UI source the bundler reads must be a *reason for
    /// cargo to re-run it*. At 63bc321 five were missing (`bundle.mjs`,
    /// `monitor.js`, `monitor.html`, `package.json`, `package-lock.json`), so
    /// editing the bundler or the monitor did not re-bundle at all — the build
    /// script never ran, and a green build embedded the old bundle. The gate
    /// enumerates the directory instead of restating the list, so a new UI file
    /// is caught the same way.
    #[test]
    fn every_ui_source_the_bundler_reads_is_watched() {
        let script = include_str!("../build.rs");
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
        let mut unwatched: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&ui).expect("apps/desktop/ui is not readable") {
            let path = entry.expect("ui directory entry").path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .expect("ui file name")
                .to_string_lossy()
                .to_string();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            // `mjs` and `cjs` are listed explicitly: matching only `js` would
            // skip `bundle.mjs` — the single most important input here — because
            // a file extension is not a substring match. Measured: the first
            // version of this gate missed it and reported four unwatched files
            // where there are five (smoke/I5-1-mut-watchset.log).
            if !matches!(ext, "js" | "mjs" | "cjs" | "html" | "json") {
                continue;
            }
            // Two exemptions, both load-bearing:
            //  * `app.bundle.js` is this script's own *output*. Watching an output
            //    makes every build re-run the build script, which is the cycle that
            //    hid the rerun asymmetry at 63bc321 (measured: two consecutive
            //    `cargo build -p flux-desktop` both re-ran with nothing touched).
            //  * `check-*.mjs` are gate scripts (item 2's bundle check); they are
            //    not inputs to a bundle.
            if name == "app.bundle.js" || name.starts_with("check-") {
                continue;
            }
            let want = format!("cargo:rerun-if-changed=../ui/{name}");
            if !script.contains(&want) {
                unwatched.push(name);
            }
        }
        assert!(
            unwatched.is_empty(),
            "build.rs does not watch {unwatched:?} in apps/desktop/ui. Editing those files does \
             not re-run the bundler, so `cargo build -p flux-desktop` embeds the previously \
             built app.bundle.js and reports success."
        );
    }
}
