//! Flux desktop shell (Tauri 2).
//!
//! Rust is the authority: this shell exposes the engine's typed command surface
//! (`flux_app::commands::CommandHost`) over Tauri IPC. The webview only submits
//! requests and renders snapshots + preview frames; it never runs an independent
//! simulation or renderer.

use flux_app::commands::{CommandHost, GuiCommand, Request, Response};
use flux_app::export::{export_project, ExportOptions};
use flux_app::producer::produce_node;
use flux_domain::{Project, RationalTime};
use std::sync::Mutex;

struct AppState {
    host: Mutex<CommandHost>,
}

#[tauri::command]
fn dispatch(state: tauri::State<AppState>, request: Request) -> Response {
    let mut host = state.host.lock().unwrap();
    host.handle(request)
}

/// Render a single preview frame of the active project at time `t` (seconds)
/// at reduced resolution; returns base64-free raw PNG bytes via a data URL path
/// handled on the JS side with a returned base64 *only over IPC* (bounded size).
#[tauri::command]
fn render_preview(
    state: tauri::State<AppState>,
    time_seconds: f64,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let project = {
        let host = state.host.lock().unwrap();
        host.dispatcher.project.clone()
    };
    let dir = std::env::temp_dir().join(format!("flux-preview-{}", std::process::id()));
    let t = RationalTime::new((time_seconds * 1_000_000.0) as i64, 1_000_000)
        .map_err(|e| e.to_string())?;
    // Render one frame range [t, t+1/fps) via the export path into a single PNG.
    let mut proj = project;
    proj.timeline.range = flux_domain::TimeRange::new(t, RationalTime::new(1, 60).unwrap());
    let res = export_project(
        &proj,
        &ExportOptions {
            out_dir: dir.clone(),
            project_name: "preview".into(),
            codec: flux_media::EncodeCodec::None,
            width,
            height,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .map_err(|e| format!("{e:?}"))?;
    let png = res.frames_dir.join("frame_000001.png");
    let bytes = std::fs::read(&png).map_err(|e| e.to_string())?;
    // Bound the payload; the preview is small (<= 960x540).
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("preview too large".into());
    }
    Ok(format!("data:image/png;base64,{}", b64(&bytes)))
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

#[tauri::command]
fn export_project_cmd(
    state: tauri::State<AppState>,
    out_dir: String,
    width: u32,
    height: u32,
) -> Result<String, String> {
    let project = {
        let host = state.host.lock().unwrap();
        host.dispatcher.project.clone()
    };
    let res = export_project(
        &project,
        &ExportOptions {
            out_dir: std::path::PathBuf::from(&out_dir),
            project_name: project.name.clone(),
            codec: flux_media::EncodeCodec::H264,
            width,
            height,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .map_err(|e| format!("{e:?}"))?;
    Ok(format!(
        "rendered {} frames, {} samples -> {}",
        res.frames,
        res.samples,
        res.video
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    ))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let host = CommandHost::new(Project::new("Untitled", 1));
    tauri::Builder::default()
        .manage(AppState {
            host: Mutex::new(host),
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            render_preview,
            export_project_cmd,
            capabilities,
            produce_generator_frame
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn _uses(_: GuiCommand) {}
