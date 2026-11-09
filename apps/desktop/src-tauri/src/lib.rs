//! Flux desktop shell (Tauri 2).
//!
//! Rust is the authority: this shell exposes the engine's typed command surface
//! (`flux_app::commands::CommandHost`) over Tauri IPC. The webview only submits
//! requests and renders snapshots + preview frames; it never runs an independent
//! simulation or renderer.

use flux_app::commands::{CommandHost, GuiCommand, Request, Response};
use flux_app::export::{export_project, ExportOptions, PreviewRenderer};
use flux_app::producer::produce_node;
use flux_domain::{Project, RationalTime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

struct AppState {
    host: Mutex<CommandHost>,
    /// Cached GPU preview renderer (created lazily on first preview so a missing
    /// GPU doesn't break app startup). Reusing it avoids recreating the device
    /// and recompiling shaders on every frame.
    preview: Mutex<Option<PreviewRenderer>>,
    /// True while a background export is running (prevents concurrent GPU work).
    exporting: Arc<AtomicBool>,
}

#[tauri::command]
fn dispatch(state: tauri::State<AppState>, request: Request) -> Response {
    let mut host = state.host.lock().unwrap();
    host.handle(request)
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

/// Start an export on a background thread. Progress and completion are reported
/// through events (`export-progress`, `export-done`, `export-error`) so the
/// webview can keep playing the preview loop while frames are rendered.
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
    // Resolve relative output dirs against the process CWD so the reported path
    // is absolute and the user can find the file.
    let out_dir = {
        let p = std::path::PathBuf::from(&out_dir);
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir()
                .map(|d| d.join(&p))
                .unwrap_or(p)
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
        };
        match export_project(&project, &opts, Some(progress)) {
            Ok(res) => {
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
            Err(e) => {
                exporting.store(false, Ordering::SeqCst);
                let _ = app.emit(
                    "export-error",
                    serde_json::json!({ "message": format!("{e:?}") }),
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
    let host = CommandHost::new(Project::new("Untitled", 1));
    tauri::Builder::default()
        .manage(AppState {
            host: Mutex::new(host),
            preview: Mutex::new(None),
            exporting: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            render_preview,
            render_graph_preview,
            export_project_cmd,
            capabilities,
            produce_generator_frame
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn _uses(_: GuiCommand) {}
